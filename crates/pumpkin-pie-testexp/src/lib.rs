//! `TestExp`, a placeholder expansion for PumpkinPIE.
//!
//! This plugin exists to prove the expansion path works end to end: it claims
//! two namespaces, answers the provider's `on_request` callbacks, and logs every
//! one of them as a `TESTEXP-ON-REQUEST` line so the log alone shows whether the
//! provider is asking once per resolve or serving the second namespace from its
//! cache.
//!
//! Two namespaces rather than one, because `register_expansion` takes a single
//! [`Cache`] for a whole namespace. `testexp` is uncached and its values must be
//! recomputed every time; `testexpcached` asks for a TTL. Mixing both in one
//! namespace is not expressible.
//!
//! | Placeholder | Value |
//! |:--|:--|
//! | `%testexp_greeting%` | a constant |
//! | `%testexp_count%` | how many times this plugin has answered, as a number |
//! | `%testexpcached%` | a constant, on the uncached namespace |
//! | `%testexpcached_value%` | a constant, on the TTL namespace |

use std::sync::atomic::{AtomicU64, Ordering};

use pumpkin_pie::{Cache, IpcMessage, PieClient, PieError, PluginId, RequestContext, answer};
use pumpkin_plugin_api::{Context, Plugin, PluginMetadata, Result};
use tracing::info;

#[cfg(target_arch = "wasm32")]
use pumpkin_plugin_api::register_plugin;

/// The provider's plugin name, and therefore its IPC address. Listed in
/// `PluginMetadata::dependencies` so this plugin loads after it.
const PROVIDER: &str = "PumpkinPIE";

/// Prefix on every request log, so the server log can be grepped for exactly
/// the callbacks the provider made.
const LOG_PREFIX: &str = "TESTEXP-ON-REQUEST";

/// How long the provider may reuse `%testexpcached_value%`, in milliseconds.
/// The provider clamps this to its own maximum, so 60s is as long as it goes.
const CACHED_TTL_MS: u64 = 60_000;

const GREETING: &str = "Hello from TestExp";
const CACHED: &str = "cached value (uncached namespace)";
const CACHED_VALUE: &str = "cached value (TTL namespace)";

/// How many placeholders this plugin has answered since it loaded.
///
/// `handle_ipc_message` takes `&self`, so the count cannot live on the plugin
/// value the way `new()` would suggest.
static ANSWERED: AtomicU64 = AtomicU64::new(0);

/// What one `on_request` is worth, after logging it.
fn resolve(context: RequestContext<'_>) -> Option<String> {
    let value = match context.name {
        "greeting" => Some(GREETING.to_string()),
        // Bumped before it is read, so the first answer logs 1 rather than 0.
        "count" => Some((ANSWERED.fetch_add(1, Ordering::Relaxed) + 1).to_string()),
        "cached" => Some(CACHED.to_string()),
        // `%testexpcached_value%` routes here: the namespace is everything
        // before the first underscore, and a namespace may no longer contain
        // one, so the two cannot be confused.
        "value" => Some(CACHED_VALUE.to_string()),
        _ => None,
    };

    info!(
        "{LOG_PREFIX} namespace={} id={} name={} viewer={} argument={} answered={} total={}",
        context.namespace,
        context.id,
        context.name,
        context.viewer.unwrap_or("<none>"),
        context.argument.unwrap_or("<none>"),
        value.as_deref().unwrap_or("<declined>"),
        ANSWERED.load(Ordering::Relaxed),
    );

    value
}

/// The expansion plugin. Holds no state of its own; see [`ANSWERED`].
pub struct TestExp;

impl TestExp {
    /// Claims both namespaces, refusing to start if either is refused.
    fn register(&self) -> std::result::Result<(), PieError> {
        let pie = PieClient::new();

        let uncached =
            pie.register_expansion("testexp", &["greeting", "count", "cached"], Cache::Never)?;
        let cached = pie.register_expansion(
            "testexpcached",
            &["value"],
            Cache::Ttl { ms: CACHED_TTL_MS },
        )?;

        info!("{PROVIDER} accepted testexp: {uncached:?}");
        info!("{PROVIDER} accepted testexpcached: {cached:?}");
        Ok(())
    }
}

impl Plugin for TestExp {
    fn new() -> Self {
        Self
    }

    fn metadata(&self) -> PluginMetadata {
        PluginMetadata {
            name: "TestExp".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            authors: vec!["Rennex".to_string()],
            description: "Test expansion for PumpkinPIE: registers a namespace and answers \
                          its on_request callbacks, logging every one of them."
                .to_string(),
            dependencies: vec![PROVIDER.to_string()],
            permissions: Vec::new(),
        }
    }

    fn on_load(&self, _context: Context) -> Result<()> {
        self.register().map_err(|error| error.to_string())?;
        info!("TestExp {} ready", env!("CARGO_PKG_VERSION"));
        Ok(())
    }

    fn on_unload(&self, _context: Context) -> Result<()> {
        // Deliberately does not unregister. Plugins unload in load order, so
        // PumpkinPIE's store is already gone by the time this runs, and
        // messaging it traps the guest: the log showed
        // "Wasm plugin store driver stopped" straight after
        // "PumpkinPIE unloaded" with no line from here at all.
        //
        // Nothing needs cleaning up anyway. The provider drops a source's
        // namespaces and cached values when it gets `unregister_expansion`,
        // and a provider that is unloading anyway throws the whole registry
        // away with it.
        info!("TestExp unloaded");
        Ok(())
    }

    fn handle_ipc_message(
        &self,
        _sender: PluginId,
        message: IpcMessage,
    ) -> std::result::Result<IpcMessage, String> {
        answer(&message, resolve).map_err(|error| error.to_string())
    }
}

// The plugin's entry point is the `init-plugin` export a component needs, and
// nothing on a host build wants it. Gating it to `wasm32` is what lets the
// `#[cfg(test)]` module below link: without the gate the test binary would
// carry the export too, and fail at link time.
#[cfg(target_arch = "wasm32")]
register_plugin!(TestExp);

#[cfg(test)]
mod tests {
    use super::*;

    use pumpkin_pie::protocol::{decode_response, encode};
    use pumpkin_pie::{Request, Success};

    /// Runs one `on_request` through the plugin and returns the value it sent
    /// back, so these cover `answer` and [`resolve`] together.
    fn ask(namespace: &str, name: &str) -> Option<String> {
        let request = Request::OnRequest {
            namespace: namespace.to_string(),
            id: format!("{namespace}_{name}"),
            name: name.to_string(),
            viewer: Some("Steve".to_string()),
            argument: None,
        };
        let encoded = encode(&request).expect("the request encodes");
        let reply = answer(&encoded, resolve).expect("the reply encodes");
        match decode_response(&reply)
            .expect("the reply decodes")
            .into_success()
        {
            Ok(Success::OnRequest { value }) => value,
            other => panic!("expected an on_request reply, got {other:?}"),
        }
    }

    #[test]
    fn every_registered_name_answers() {
        assert_eq!(ask("testexp", "greeting").as_deref(), Some(GREETING));
        assert_eq!(ask("testexp", "cached").as_deref(), Some(CACHED));
    }

    /// `testexpcached` has no underscore, so `%testexpcached_value%` reaches it
    /// with `namespace` intact and `name` spelled `value`.
    #[test]
    fn the_ttl_namespace_answers_under_its_own_namespace() {
        assert_eq!(ask("testexpcached", "value").as_deref(), Some(CACHED_VALUE));
    }

    #[test]
    fn an_unknown_name_is_declined() {
        assert_eq!(ask("testexp", "nonsense"), None);
    }

    /// The only test that touches `count`, so the shared counter cannot be
    /// moved by another test running alongside it.
    #[test]
    fn count_climbs_by_one_per_answer() {
        let first: u64 = ask("testexp", "count")
            .expect("count answers")
            .parse()
            .expect("a number");
        let second: u64 = ask("testexp", "count")
            .expect("count answers")
            .parse()
            .expect("a number");
        assert_eq!(second, first + 1);
    }

    /// The host loads the provider first because of this list, and refuses to
    /// load TestExp at all if the name is wrong.
    #[test]
    fn the_metadata_names_the_provider_as_a_dependency() {
        let metadata = TestExp::new().metadata();
        assert_eq!(metadata.name, "TestExp");
        assert_eq!(metadata.dependencies, vec![PROVIDER.to_string()]);
    }
}
