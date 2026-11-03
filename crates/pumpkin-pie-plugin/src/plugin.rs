//! The PumpkinPIE provider plugin.
//!
//! This crate is the Wasm component you drop into a server's `plugins/`
//! directory. It owns the expansion registry, answers `set_placeholders`, caches
//! what expansions allow it to, and exposes `/pie` for checking a config by hand.
//!
//! The protocol, the token scanner and the client live in `pumpkin-pie`, which
//! is a plain library. They are split because every plugin exports a symbol
//! called `init-plugin`, so a plugin that consumes PumpkinPIE cannot also link
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

use pumpkin_pie::builtins::{self, ResolveContext};
use pumpkin_pie::expansion::{Expansion, Registry};
use pumpkin_pie::protocol::{
    MAX_BATCH_LINES, MAX_ID_LENGTH, MAX_MESSAGE_BYTES, MAX_TEXT_LENGTH, ProtocolError,
    RegisteredPlaceholder, Request, ResolvedLine, Response, Success, decode_request, encode,
};
use pumpkin_pie::{IpcMessage, PluginId};
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
type CacheKey = (String, String, String, Option<String>);

/// One expansion's cached values, keyed by viewer and placeholder.
type ExpansionCache = HashMap<CacheKey, Cached>;

const MAX_CACHE_ENTRIES: usize = 4096;
const MAX_CACHE_BYTES: usize = 4 * MAX_MESSAGE_BYTES;

/// A cached expansion value, and when it was stored.
struct Cached {
    value: Option<String>,
    stored: Instant,
    ttl: Duration,
}

fn entry_size(key: &CacheKey, value: &Option<String>) -> usize {
    key.0.len()
        + key.1.len()
        + key.2.len()
        + key.3.as_ref().map_or(0, String::len)
        + value.as_ref().map_or(0, String::len)
}

fn cache_key(source: &str, viewer: &str, id: &str, argument: Option<&str>) -> CacheKey {
    (
        source.to_string(),
        viewer.to_string(),
        id.to_ascii_lowercase(),
        argument.map(str::to_string),
    )
}

fn cache_get(cache: &mut ExpansionCache, key: &CacheKey, now: Instant) -> Option<Option<String>> {
    let entry = cache.get(key)?;
    if now.saturating_duration_since(entry.stored) >= entry.ttl {
        cache.remove(key);
        return None;
    }
    Some(entry.value.clone())
}

fn cache_insert(
    cache: &mut ExpansionCache,
    key: CacheKey,
    value: Option<String>,
    ttl: Duration,
    now: Instant,
    max_entries: usize,
    max_bytes: usize,
) {
    cache.retain(|_, item| now.saturating_duration_since(item.stored) < item.ttl);
    cache.remove(&key);
    let size = entry_size(&key, &value);
    if max_entries == 0 || size > max_bytes {
        return;
    }
    let mut used: usize = cache
        .iter()
        .map(|(key, item)| entry_size(key, &item.value))
        .sum();
    while cache.len() >= max_entries || size > max_bytes.saturating_sub(used) {
        let Some(oldest) = cache
            .iter()
            .min_by_key(|(_, item)| item.stored)
            .map(|(key, _)| key.clone())
        else {
            break;
        };
        if let Some(item) = cache.remove(&oldest) {
            used -= entry_size(&oldest, &item.value);
        }
    }
    cache.insert(
        key,
        Cached {
            value,
            stored: now,
            ttl,
        },
    );
}

/// Everything the provider owns.
struct State {
    registry: Mutex<Registry>,
    /// Keyed by expansion source, then by viewer and placeholder.
    cache: Mutex<ExpansionCache>,
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
    try_resolve_line(viewer_name, text).unwrap_or_else(|_| ResolvedLine {
        text: text.to_string(),
        unresolved: Vec::new(),
    })
}

pub(crate) fn try_resolve_line(
    viewer_name: Option<&str>,
    text: &str,
) -> Result<ResolvedLine, ProtocolError> {
    let server = server();
    let viewer = viewer_name.and_then(|name| server.get_player_by_name(name));
    let viewer_key = viewer_name.unwrap_or_default().to_string();
    let budget = Cell::new(MAX_CALLBACKS_PER_LINE);

    let result = pumpkin_pie::tokens::try_substitute(text, MAX_MESSAGE_BYTES, |id, argument| {
        resolve_one(viewer.as_ref(), &viewer_key, id, argument, &budget)
    })
    .map_err(|_| ProtocolError::TooLarge)?;
    Ok(ResolvedLine {
        text: result.text,
        unresolved: result.unresolved,
    })
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
    let key = cache_key(&expansion.source, viewer_key, id, argument);
    if let Some(cached) = cached(&expansion, &key) {
        return cached;
    }

    if budget.get() == 0 {
        warn!("callback budget spent, leaving {id} alone");
        return None;
    }
    budget.set(budget.get() - 1);

    let request = callback_request(
        &expansion,
        id,
        viewer.map(|player| player.get_name()),
        argument,
    )?;
    let value = ask_expansion(&expansion.source, &request);
    store(&expansion, key, value.clone());
    value
}

fn callback_request(
    expansion: &Expansion,
    id: &str,
    viewer: Option<String>,
    argument: Option<&str>,
) -> Option<Request> {
    let (_, name) = id.split_once('_')?;
    Some(Request::OnRequest {
        namespace: expansion.namespace.clone(),
        id: id.to_string(),
        // Lowercased to match how `Registry::add` stores names. Sending it as
        // written meant `%myexp_COUNT%` reached the expansion as `COUNT`, which
        // no `match` arm for the registered `count` would ever hit, so the
        // placeholder silently stayed in the text.
        name: name.to_ascii_lowercase(),
        viewer,
        argument: argument.map(str::to_string),
    })
}

/// Reads a cached value if it is still inside its expansion's TTL.
fn cached(expansion: &Expansion, key: &CacheKey) -> Option<Option<String>> {
    if expansion.ttl_ms() == 0 {
        return None;
    }
    cache_get(&mut lock(&state().cache), key, Instant::now())
}

/// Stores a value if the expansion asked for a TTL.
fn store(expansion: &Expansion, key: CacheKey, value: Option<String>) {
    if expansion.ttl_ms() == 0 {
        return;
    }
    cache_insert(
        &mut lock(&state().cache),
        key,
        value,
        Duration::from_millis(expansion.ttl_ms()),
        Instant::now(),
        MAX_CACHE_ENTRIES,
        MAX_CACHE_BYTES,
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
            source: pumpkin_pie::PROVIDER.to_string(),
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

/// One line per registered expansion, for `/pie`.
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
            protocol: pumpkin_pie::PROTOCOL_VERSION,
            name: pumpkin_pie::PROVIDER.to_string(),
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
            forget(sender);
            Response::success(Success::Unregistered { namespaces })
        }
        Request::SetPlaceholders { text, viewer, .. } => {
            if let Err(error) = check_text(&text) {
                return Response::from_error(error);
            }
            match try_resolve_line(viewer.as_deref(), &text) {
                Ok(line) => Response::success(Success::Resolved(line)),
                Err(error) => Response::from_error(error),
            }
        }
        Request::SetPlaceholdersBatch { requests } => {
            if requests.len() > MAX_BATCH_LINES {
                return Response::failure(format!("at most {MAX_BATCH_LINES} lines per batch"));
            }
            resolve_batch(
                &requests,
                |line| try_resolve_line(line.viewer.as_deref(), &line.text),
                MAX_MESSAGE_BYTES,
            )
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

fn resolve_batch(
    requests: &[pumpkin_pie::protocol::Line],
    mut resolve: impl FnMut(&pumpkin_pie::protocol::Line) -> Result<ResolvedLine, ProtocolError>,
    limit: usize,
) -> Response {
    for line in requests {
        if let Err(error) = check_text(&line.text) {
            return Response::from_error(error);
        }
    }
    // Reserve space for the response wrapper and separators. Each line is
    // serialized separately to account for JSON escaping as well as text.
    let mut remaining = limit.saturating_sub(64);
    let mut results = Vec::new();
    for line in requests {
        let result = match resolve(line) {
            Ok(result) => result,
            Err(error) => return Response::from_error(error),
        };
        let size = match encode(&result) {
            Ok(bytes) => bytes.len().saturating_add(1),
            Err(error) => return Response::from_error(error),
        };
        if size > remaining {
            return Response::from_error(ProtocolError::TooLarge);
        }
        remaining -= size;
        results.push(result);
    }
    Response::success(Success::ResolvedBatch { results })
}

fn encode_response(response: &Response) -> Vec<u8> {
    encode(response).unwrap_or_else(|_| {
        // This fixed error is always below the protocol limit.
        br#"{"ok":false,"error":"response is too large"}"#.to_vec()
    })
}

/// Drops anything cached for `sender`, which a re-register invalidates.
fn forget(sender: &str) {
    lock(&state().cache).retain(|key, _| key.0 != sender);
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
    match pumpkin_pie::protocol::decode_response(&reply)
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

pub struct PumpkinPie;

impl Plugin for PumpkinPie {
    fn new() -> Self {
        Self
    }

    fn metadata(&self) -> PluginMetadata {
        PluginMetadata {
            name: pumpkin_pie::PROVIDER.to_string(),
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
            "PumpkinPIE {VERSION} ready with {} built in placeholders",
            builtins::count()
        );
        Ok(())
    }

    fn on_unload(&self, _context: Context) -> Result<()> {
        info!("PumpkinPIE unloaded");
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
        Ok(encode_response(&response))
    }
}

register_plugin!(PumpkinPie);

#[cfg(test)]
mod tests {
    use super::*;
    use pumpkin_pie::protocol::{Cache, Line, decode_response};

    #[test]
    fn cache_distinguishes_arguments_and_negative_values() {
        let mut cache = ExpansionCache::new();
        let now = Instant::now();
        let ttl = Duration::from_secs(10);
        for (arg, value) in [
            (None, Some("none")),
            (Some(""), None),
            (Some("gold"), Some("10")),
            (Some("silver"), Some("50")),
        ] {
            cache_insert(
                &mut cache,
                cache_key("exp", "Steve", "RANKS_value", arg),
                value.map(str::to_string),
                ttl,
                now,
                10,
                1000,
            );
        }
        for (arg, value) in [
            (None, Some("none")),
            (Some(""), None),
            (Some("gold"), Some("10")),
            (Some("silver"), Some("50")),
        ] {
            assert_eq!(
                cache_get(
                    &mut cache,
                    &cache_key("exp", "Steve", "ranks_value", arg),
                    now
                ),
                Some(value.map(str::to_string))
            );
        }
    }

    #[test]
    fn cache_evicts_expired_and_oldest_entries_and_limits_bytes() {
        let mut cache = ExpansionCache::new();
        let now = Instant::now();
        let ttl = Duration::from_secs(2);
        let a = cache_key("s", "", "a", None);
        let b = cache_key("s", "", "b", None);
        let c = cache_key("s", "", "c", None);
        cache_insert(&mut cache, a.clone(), Some("x".into()), ttl, now, 2, 100);
        cache_insert(
            &mut cache,
            b.clone(),
            None,
            ttl,
            now + Duration::from_millis(1),
            2,
            100,
        );
        cache_insert(
            &mut cache,
            c.clone(),
            None,
            ttl,
            now + Duration::from_millis(2),
            2,
            100,
        );
        assert!(!cache.contains_key(&a));
        assert!(cache.contains_key(&b));
        cache_insert(
            &mut cache,
            a.clone(),
            None,
            ttl,
            now + ttl + Duration::from_millis(2),
            2,
            100,
        );
        assert!(!cache.contains_key(&c));
        assert_eq!(
            cache_get(&mut cache, &b, now + ttl + Duration::from_millis(1)),
            None
        );
        cache_insert(
            &mut cache,
            b.clone(),
            Some("too large".into()),
            ttl,
            now + ttl,
            2,
            3,
        );
        assert!(!cache.contains_key(&b));
    }

    #[test]
    fn cache_evicts_old_entries_to_fit_byte_budget() {
        let mut cache = ExpansionCache::new();
        let now = Instant::now();
        let ttl = Duration::from_secs(5);
        let a = cache_key("s", "", "a", None);
        let b = cache_key("s", "", "b", None);
        cache_insert(&mut cache, a.clone(), Some("123".into()), ttl, now, 10, 7);
        cache_insert(
            &mut cache,
            b.clone(),
            Some("456".into()),
            ttl,
            now + Duration::from_millis(1),
            10,
            7,
        );
        assert!(!cache.contains_key(&a));
        assert!(cache.contains_key(&b));
    }

    #[test]
    fn cache_removes_an_expired_entry_on_lookup() {
        let mut cache = ExpansionCache::new();
        let now = Instant::now();
        let ttl = Duration::from_secs(1);
        let key = cache_key("s", "viewer", "s_value", None);
        cache_insert(&mut cache, key.clone(), None, ttl, now, 2, 100);
        assert_eq!(cache_get(&mut cache, &key, now), Some(None));
        assert_eq!(cache_get(&mut cache, &key, now + ttl), None);
        assert!(!cache.contains_key(&key));
    }

    #[test]
    fn callback_uses_registered_normalized_namespace() {
        let mut registry = Registry::new();
        let expansion = registry.add("Ranks", "source", [], Cache::Never).unwrap();
        let request = callback_request(expansion, "RANKS_Prefix", None, None).unwrap();
        assert!(
            matches!(request, Request::OnRequest { namespace, name, id, .. } if namespace == "ranks" && name == "prefix" && id == "RANKS_Prefix")
        );
    }

    #[test]
    fn batch_rejects_oversized_middle_line_before_resolving() {
        let lines = vec![
            Line {
                viewer: Some("a".into()),
                text: "first".into(),
            },
            Line {
                viewer: Some("b".into()),
                text: "x".repeat(MAX_TEXT_LENGTH + 1),
            },
            Line {
                viewer: Some("c".into()),
                text: "last".into(),
            },
        ];
        let mut calls = 0;
        let response = resolve_batch(
            &lines,
            |_| {
                calls += 1;
                unreachable!()
            },
            MAX_MESSAGE_BYTES,
        );
        assert!(!response.ok);
        assert_eq!(calls, 0);
        assert!(!decode_response(&encode_response(&response)).unwrap().ok);
    }

    #[test]
    fn valid_batch_preserves_viewer_order_and_cardinality() {
        let lines: Vec<Line> = ["a", "b", "c"]
            .into_iter()
            .map(|name| Line {
                viewer: Some(name.into()),
                text: "x".into(),
            })
            .collect();
        let response = resolve_batch(
            &lines,
            |line| {
                Ok(ResolvedLine {
                    text: line.viewer.clone().unwrap(),
                    unresolved: vec![],
                })
            },
            MAX_MESSAGE_BYTES,
        );
        let Success::ResolvedBatch { results } = response.into_success().unwrap() else {
            panic!("expected batch");
        };
        assert_eq!(
            results
                .iter()
                .map(|line| line.text.as_str())
                .collect::<Vec<_>>(),
            ["a", "b", "c"]
        );
    }

    #[test]
    fn batch_limits_aggregate_encoded_size_and_returns_valid_failure() {
        let lines = vec![
            Line {
                viewer: None,
                text: "x".into()
            };
            3
        ];
        let response = resolve_batch(
            &lines,
            |_| {
                Ok(ResolvedLine {
                    text: "\"".repeat(20),
                    unresolved: vec![],
                })
            },
            150,
        );
        assert!(!response.ok);
        assert!(!decode_response(&encode_response(&response)).unwrap().ok);
        let oversized = Response::success(Success::Value {
            known: true,
            value: Some("x".repeat(MAX_MESSAGE_BYTES)),
        });
        assert!(!decode_response(&encode_response(&oversized)).unwrap().ok);
    }
}
