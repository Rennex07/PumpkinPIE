//! The PumpkinPAPI provider plugin.
//!
//! This crate is the Wasm component you drop into a server's `plugins/`
//! directory. It owns the expansion registry, answers `set_placeholders`, caches
//! what expansions allow it to, and exposes `/papi` for checking a config by hand.
//!
//! The protocol, the token scanner and the client live in `pumpkin-papi`, which
//! is a plain library. They are split because every plugin exports a symbol
//! called `init-plugin`, so a plugin that consumes PumpkinPAPI cannot also link
//! a crate that exports one.
//!
//! The `Plugin` trait hands every callback `&self`, and command handlers must be
//! `'static`, so the state lives in a [`OnceLock`] rather than on the plugin
//! value. Nothing here holds a lock across a host call, because the host may
//! deliver an IPC callback back into this plugin before the call returns.

use std::cell::Cell;
use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard, OnceLock};
use std::time::{Duration, Instant};

use pumpkin_papi::builtins::{self, ResolveContext};
use pumpkin_papi::expansion::{Expansion, Registry};
use pumpkin_papi::protocol::{
    MAX_BATCH_LINES, MAX_ID_LENGTH, MAX_TEXT_LENGTH, ProtocolError, RegisteredPlaceholder, Request,
    ResolvedLine, Response, Success, decode_request, encode,
};
use pumpkin_papi::{IpcMessage, PluginId};
use pumpkin_plugin_api::{
    Context, Player, Plugin, PluginMetadata, Result, Server, register_plugin,
};
use tracing::{info, warn};

/// Version reported by `ping`.
const VERSION: &str = env!("CARGO_PKG_VERSION");

/// An expansion is asked synchronously, so a misbehaving one could otherwise
/// ask for placeholders forever. This is the budget for one resolved line, not
/// one message, so a batch of a thousand lines is not penalised for being large.
const MAX_CALLBACKS_PER_LINE: usize = 16;

/// What one expansion's value is cached against.
type CacheKey = (String, String);

/// One expansion's cached values, keyed by viewer and placeholder.
type ExpansionCache = HashMap<CacheKey, Cached>;

/// A cached expansion value, and when it was stored.
struct Cached {
    value: Option<String>,
    stored: Instant,
}

/// Everything the provider owns.
struct State {
    registry: Mutex<Registry>,
    /// Keyed by expansion source, then by viewer and placeholder.
    cache: Mutex<HashMap<String, ExpansionCache>>,
    server: OnceLock<Server>,
}

static STATE: OnceLock<State> = OnceLock::new();

/// The provider's state.
///
/// # Panics
/// Panics if called before `on_load`, which the host does not do.
fn state() -> &'static State {
    STATE.get_or_init(|| State {
        registry: Mutex::new(Registry::new()),
        cache: Mutex::new(HashMap::new()),
        server: OnceLock::new(),
    })
}

/// A poisoned lock is a bug, not a reason to stop resolving placeholders.
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// The server handle `on_load` was given.
///
/// # Panics
/// Panics if called before `on_load`.
pub fn server() -> &'static Server {
    state()
        .server
        .get()
        .expect("the server handle is only valid after on_load")
}

/// Resolves one line, returning the text and what stayed unresolved.
pub fn resolve_line(viewer_name: Option<&str>, text: &str) -> ResolvedLine {
    let server = server();
    let viewer = viewer_name.and_then(|name| server.get_player_by_name(name));
    let viewer_key = viewer_name.unwrap_or_default().to_string();
    let budget = Cell::new(MAX_CALLBACKS_PER_LINE);

    let result = pumpkin_papi::tokens::substitute(text, |id, argument| {
        resolve_one(viewer.as_ref(), &viewer_key, id, argument, &budget)
    });
    ResolvedLine {
        text: result.text,
        unresolved: result.unresolved,
    }
}

/// Resolves a single placeholder, or `None` to leave it alone.
fn resolve_one(
    viewer: Option<&Player>,
    viewer_key: &str,
    id: &str,
    argument: Option<&str>,
    budget: &Cell<usize>,
) -> Option<String> {
    let server = server();

    if let Some(builtin) = builtins::find(id) {
        let context = ResolveContext {
            server,
            viewer,
            argument,
        };
        return (builtin.resolve)(&context);
    }

    let expansion = lock(&state().registry).owner_of(id)?.clone();
    let key: CacheKey = (viewer_key.to_string(), id.to_ascii_lowercase());
    if let Some(cached) = cached(&expansion, &key) {
        return cached;
    }

    if budget.get() == 0 {
        warn!("callback budget spent, leaving {id} alone");
        return None;
    }
    budget.set(budget.get() - 1);

    let (namespace, name) = id.split_once('_')?;
    let request = Request::OnRequest {
        namespace: namespace.to_string(),
        id: id.to_string(),
        // Lowercased to match how `Registry::add` stores names. Sending it as
        // written meant `%myexp_COUNT%` reached the expansion as `COUNT`, which
        // no `match` arm for the registered `count` would ever hit, so the
        // placeholder silently stayed in the text.
        name: name.to_ascii_lowercase(),
        viewer: viewer.map(|player| player.get_name()),
        argument: argument.map(str::to_string),
    };
    let value = ask_expansion(&expansion.source, &request);
    store(&expansion, key, value.clone());
    value
}

/// Reads a cached value if it is still inside its expansion's TTL.
fn cached(expansion: &Expansion, key: &CacheKey) -> Option<Option<String>> {
    if expansion.ttl_ms() == 0 {
        return None;
    }
    let mut all = lock(&state().cache);
    let entry = all.get_mut(&expansion.source)?.get_mut(key)?;
    if entry.stored.elapsed() > Duration::from_millis(expansion.ttl_ms()) {
        return None;
    }
    Some(entry.value.clone())
}

/// Stores a value if the expansion asked for a TTL.
fn store(expansion: &Expansion, key: CacheKey, value: Option<String>) {
    if expansion.ttl_ms() == 0 {
        return;
    }
    lock(&state().cache)
        .entry(expansion.source.clone())
        .or_default()
        .insert(
            key,
            Cached {
                value,
                stored: Instant::now(),
            },
        );
}

/// Every placeholder the provider knows, built in and contributed.
pub fn registered() -> Vec<RegisteredPlaceholder> {
    let mut entries: Vec<RegisteredPlaceholder> = builtins::BUILTINS
        .iter()
        .map(|builtin| RegisteredPlaceholder {
            id: builtin.id.to_string(),
            description: builtin.description.to_string(),
            namespace: namespace_of(builtin.id).to_string(),
            source: pumpkin_papi::PROVIDER.to_string(),
        })
        .collect();

    for expansion in lock(&state().registry).all() {
        for name in &expansion.names {
            entries.push(RegisteredPlaceholder {
                id: format!("{}_{}", expansion.namespace, name),
                description: String::new(),
                namespace: expansion.namespace.clone(),
                source: expansion.source.clone(),
            });
        }
    }
    entries.sort_by(|left, right| left.id.cmp(&right.id));
    entries.dedup_by(|left, right| left.id == right.id);
    entries
}

/// One line per registered expansion, for `/papi`.
pub fn expansion_summary() -> Vec<String> {
    lock(&state().registry)
        .all()
        .map(|expansion| {
            let names = if expansion.names.is_empty() {
                "answers anything".to_string()
            } else {
                expansion
                    .names
                    .iter()
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(", ")
            };
            format!(
                "%{}%  from {}: {names}",
                expansion.namespace, expansion.source
            )
        })
        .collect()
}

/// Handles one decoded request.
fn handle(sender: &str, request: Request) -> Response {
    match request {
        Request::Ping => Response::success(Success::Ping {
            protocol: pumpkin_papi::PROTOCOL_VERSION,
            name: pumpkin_papi::PROVIDER.to_string(),
            version: VERSION.to_string(),
            placeholders: builtins::count(),
            expansions: lock(&state().registry).len(),
        }),
        Request::GetRegisteredPlaceholders => Response::success(Success::Registered {
            placeholders: registered(),
        }),
        Request::RegisterExpansion {
            namespace,
            placeholders,
            cache,
        } => {
            forget(sender);
            let claimed = {
                let mut registry = lock(&state().registry);
                registry
                    .add(&namespace, sender, placeholders, cache)
                    .cloned()
            };
            match claimed {
                Ok(expansion) => Response::success(Success::RegisteredExpansion {
                    namespace: expansion.namespace,
                    placeholders: expansion.names.into_iter().collect(),
                    cache: expansion.cache,
                }),
                Err(error) => Response::from_error(error),
            }
        }
        Request::UnregisterExpansion => {
            let namespaces = lock(&state().registry).drop_source(sender);
            for namespace in &namespaces {
                info!("{sender} released {namespace}");
            }
            lock(&state().cache).remove(sender);
            Response::success(Success::Unregistered { namespaces })
        }
        Request::SetPlaceholders { text, viewer, .. } => {
            if let Err(error) = check_text(&text) {
                return Response::from_error(error);
            }
            Response::success(Success::Resolved(resolve_line(viewer.as_deref(), &text)))
        }
        Request::SetPlaceholdersBatch { requests } => {
            if requests.len() > MAX_BATCH_LINES {
                return Response::failure(format!("at most {MAX_BATCH_LINES} lines per batch"));
            }
            let results = requests
                .iter()
                .filter(|line| check_text(&line.text).is_ok())
                .map(|line| resolve_line(line.viewer.as_deref(), &line.text))
                .collect();
            Response::success(Success::ResolvedBatch { results })
        }
        Request::GetPlaceholderValue {
            id,
            viewer,
            argument,
        } => {
            if id.len() > MAX_ID_LENGTH {
                return Response::from_error(ProtocolError::TooLong {
                    field: "id".to_string(),
                    limit: MAX_ID_LENGTH,
                });
            }
            let budget = Cell::new(MAX_CALLBACKS_PER_LINE);
            let viewer_key = viewer.clone().unwrap_or_default();
            let resolved = viewer
                .as_deref()
                .and_then(|name| server().get_player_by_name(name));
            match resolve_one(
                resolved.as_ref(),
                &viewer_key,
                &id,
                argument.as_deref(),
                &budget,
            ) {
                Some(value) => Response::success(Success::Value {
                    known: true,
                    value: Some(value),
                }),
                None => Response::success(Success::Value {
                    known: false,
                    value: None,
                }),
            }
        }
        Request::OnRequest { .. } => Response::failure("the provider does not provide expansions"),
    }
}

/// The reserved namespace a built in belongs to.
fn namespace_of(id: &str) -> &str {
    id.split_once('_').map_or(id, |(namespace, _)| namespace)
}

fn check_text(text: &str) -> Result<(), ProtocolError> {
    if text.len() > MAX_TEXT_LENGTH {
        return Err(ProtocolError::TooLong {
            field: "text".to_string(),
            limit: MAX_TEXT_LENGTH,
        });
    }
    Ok(())
}

/// Drops anything cached for `sender`, which a re-register invalidates.
fn forget(sender: &str) {
    lock(&state().cache).remove(sender);
}

/// Asks the plugin that owns an expansion for one of its placeholders.
fn ask_expansion(source: &str, request: &Request) -> Option<String> {
    let bytes = encode(request).ok()?;
    let reply = match pumpkin_plugin_api::ipc::send_ipc_message(source, &bytes) {
        Ok(Ok(reply)) => reply,
        Ok(Err(message)) => {
            warn!("{source} refused an on_request: {message}");
            return None;
        }
        Err(()) => {
            warn!("{source} did not answer an on_request");
            return None;
        }
    };
    match pumpkin_papi::protocol::decode_response(&reply)
        .ok()?
        .into_success()
    {
        Ok(Success::OnRequest { value }) => value,
        Ok(_) => {
            warn!("{source} answered an on_request with the wrong response");
            None
        }
        Err(message) => {
            warn!("{source} failed an on_request: {message}");
            None
        }
    }
}

/// The provider plugin. Holds no state of its own; see [`state`].
pub struct PumpkinPapi;

impl Plugin for PumpkinPapi {
    fn new() -> Self {
        Self
    }

    fn metadata(&self) -> PluginMetadata {
        PluginMetadata {
            name: pumpkin_papi::PROVIDER.to_string(),
            version: VERSION.to_string(),
            authors: vec!["Rennex".to_string()],
            description: "Placeholder provider that other Pumpkin plugins resolve through"
                .to_string(),
            dependencies: Vec::new(),
            permissions: Vec::new(),
        }
    }

    fn on_load(&self, context: Context) -> Result<()> {
        let _ = state().server.set(context.get_server());
        crate::command::register(context);
        info!(
            "PumpkinPAPI {VERSION} ready with {} built in placeholders",
            builtins::count()
        );
        Ok(())
    }

    fn on_unload(&self, _context: Context) -> Result<()> {
        info!("PumpkinPAPI unloaded");
        Ok(())
    }

    fn handle_ipc_message(
        &self,
        sender: PluginId,
        message: IpcMessage,
    ) -> std::result::Result<IpcMessage, String> {
        let request = match decode_request(&message) {
            Ok(request) => request,
            Err(error) => {
                return Ok(encode(&Response::from_error(error)).unwrap_or_default());
            }
        };
        let response = handle(&sender, request);
        Ok(encode(&response).unwrap_or_default())
    }
}

register_plugin!(PumpkinPapi);
