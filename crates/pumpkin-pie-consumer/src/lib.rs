//! `PieConsumer`, a plugin that *asks* PumpkinPIE for placeholders.
//!
//! The other plugins in this workspace test the provider from the outside:
//! `pumpkin-pie-plugin` is the provider, and `pumpkin-pie-testexp` pushes
//! values towards it. Neither exercises the direction a real consumer uses,
//! which is a plugin calling [`PieClient`] and getting text back over IPC.
//! This one does, and it is the only way to find out that `set_placeholders`,
//! `set_placeholders_batch` and `get_placeholder_value` survive a round trip
//! through the wire format at all.
//!
//! Every check logs a line prefixed `CONSUMER-CHECK`, so a server log shows
//! which calls crossed the IPC boundary and what came back.

use pumpkin_pie::PieClient;
use pumpkin_plugin_api::command::{
    Command, CommandError, CommandNode, CommandSender, ConsumedArgs,
};
use pumpkin_plugin_api::commands::CommandHandler;
use pumpkin_plugin_api::permission::{Permission, PermissionDefault, PermissionLevel};
use pumpkin_plugin_api::text::TextComponent;
use pumpkin_plugin_api::{Context, Plugin, PluginMetadata, Result, Server};
use tracing::{error, info};

#[cfg(target_arch = "wasm32")]
use pumpkin_plugin_api::register_plugin;

/// The provider's plugin name, and so its IPC address. Listed in
/// `PluginMetadata::dependencies` so this plugin loads after it.
const PROVIDER: &str = "PumpkinPIE";

/// Prefix on every check line, so the log can be grepped for the calls that
/// actually crossed IPC.
const LOG_PREFIX: &str = "CONSUMER-CHECK";

/// Permission required by `/piecheck`.
///
/// Namespaced with this plugin's exact name, because
/// `Context::register_permission` refuses any other namespace, and a node that
/// was never registered denies the command to everyone, operators included. In
/// game that looks exactly like the command not existing.
const USE_PERMISSION: &str = "PieConsumer:use";

/// The consumer plugin. Holds no state of its own.
pub struct PieConsumer;

impl Plugin for PieConsumer {
    fn new() -> Self {
        Self
    }

    fn metadata(&self) -> PluginMetadata {
        PluginMetadata {
            name: "PieConsumer".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            authors: vec!["Rennex".to_string()],
            description: "Exercises the PumpkinPIE client API end to end".to_string(),
            dependencies: vec![PROVIDER.to_string()],
            permissions: vec![],
        }
    }

    fn on_load(&self, context: Context) -> Result<()> {
        if let Err(error) = context.register_permission(&Permission {
            node: USE_PERMISSION.to_string(),
            description: "Allows using /piecheck.".to_string(),
            default: PermissionDefault::Op(PermissionLevel::Two),
            children: Vec::new(),
        }) {
            error!("could not register {USE_PERMISSION}, /piecheck will be unusable: {error}");
        }
        context.register_command(command(), USE_PERMISSION);
        info!("PieConsumer {} ready", env!("CARGO_PKG_VERSION"));
        Ok(())
    }

    fn on_unload(&self, _context: Context) -> Result<()> {
        // Nothing was registered with the provider, so there is nothing to
        // release, and messaging a provider that has already unloaded traps the
        // guest. See the same note in `pumpkin-pie-testexp`.
        info!("PieConsumer unloaded");
        Ok(())
    }
}

fn command() -> Command {
    Command::new(
        &["piecheck".to_string()],
        "Run the PumpkinPIE client API checks",
    )
    .execute(CheckCommand)
    .then(CommandNode::literal("run").execute(CheckCommand))
}

struct CheckCommand;

impl CommandHandler for CheckCommand {
    fn handle(
        &self,
        sender: CommandSender,
        _server: Server,
        _args: ConsumedArgs,
    ) -> std::result::Result<i32, CommandError> {
        let pie = PieClient::new();
        let mut failures = 0;

        // The console has no player, so a name is used to exercise the
        // provider's per-viewer cache key and the player built-ins answering
        // for someone who is not online.
        let viewer = sender
            .as_player()
            .map(|player| player.get_name())
            .unwrap_or_else(|| "blockrnc".to_string());

        // A mix of a built in, a registered expansion, and a placeholder nobody
        // provides.
        let mixed = "%server_max_players% %testexp_greeting% %nope_nope%";
        match pie.set_placeholders(Some(&viewer), mixed) {
            Ok(line) => {
                info!(
                    "{LOG_PREFIX} set_placeholders sent={mixed:?} text={:?} unresolved={:?}",
                    line.text, line.unresolved
                );
                report(&sender, "set_placeholders", &line.text);
            }
            Err(error) => {
                failures += 1;
                error!("{LOG_PREFIX} set_placeholders failed: {error}");
            }
        }

        // The same call with no viewer, which is the shape a plugin uses for
        // server-wide text.
        match pie.set_placeholders(None, "%server_online% online") {
            Ok(line) => {
                info!(
                    "{LOG_PREFIX} set_placeholders_no_viewer text={:?}",
                    line.text
                );
                report(&sender, "no viewer", &line.text);
            }
            Err(error) => {
                failures += 1;
                error!("{LOG_PREFIX} set_placeholders no viewer failed: {error}");
            }
        }

        // One message for several lines, which is the whole reason batching
        // exists.
        let lines = [
            (viewer.as_str(), "%testexp_greeting%"),
            (viewer.as_str(), "%testexp_count%"),
            (viewer.as_str(), "%testexpcached_value%"),
        ];
        match pie.set_placeholders_batch(&lines) {
            Ok(results) => {
                if results.len() != lines.len() {
                    failures += 1;
                    error!(
                        "{LOG_PREFIX} batch returned {} of {} lines",
                        results.len(),
                        lines.len()
                    );
                }
                for (index, line) in results.iter().enumerate() {
                    info!("{LOG_PREFIX} batch[{index}] text={:?}", line.text);
                    report(&sender, "batch", &line.text);
                }
            }
            Err(error) => {
                failures += 1;
                error!("{LOG_PREFIX} set_placeholders_batch failed: {error}");
            }
        }

        // One value three ways: a built in, a registered expansion, and
        // something nobody claims, which has to be `None` rather than an error.
        for id in ["server_max_players", "testexp_greeting", "nope_nope"] {
            match pie.get_placeholder_value(Some(&viewer), id) {
                Ok(value) => {
                    info!("{LOG_PREFIX} get_placeholder_value id={id} value={value:?}");
                    report(&sender, id, value.as_deref().unwrap_or("<none>"));
                }
                Err(error) => {
                    failures += 1;
                    error!("{LOG_PREFIX} get_placeholder_value id={id} failed: {error}");
                }
            }
        }

        // The full list, which is how a plugin discovers what it can use.
        match pie.get_registered_placeholders() {
            Ok(entries) => {
                let mut namespaces: Vec<&str> = entries
                    .iter()
                    .map(|entry| entry.namespace.as_str())
                    .collect();
                namespaces.sort_unstable();
                namespaces.dedup();
                info!(
                    "{LOG_PREFIX} registered count={} namespaces={namespaces:?}",
                    entries.len()
                );
                report(
                    &sender,
                    "registered",
                    &format!("{} placeholders", entries.len()),
                );
            }
            Err(error) => {
                failures += 1;
                error!("{LOG_PREFIX} get_registered_placeholders failed: {error}");
            }
        }

        // A plugin that registered nothing must not be able to unregister
        // somebody else's namespaces.
        match pie.unregister_expansion() {
            Ok(namespaces) => {
                info!("{LOG_PREFIX} unregister returned {namespaces:?}, expected empty");
                if !namespaces.is_empty() {
                    failures += 1;
                    error!("{LOG_PREFIX} unregister claimed namespaces it never registered");
                }
            }
            Err(error) => {
                failures += 1;
                error!("{LOG_PREFIX} unregister failed: {error}");
            }
        }

        if failures == 0 {
            info!("{LOG_PREFIX} all checks passed");
            report(&sender, "result", "all checks passed");
        } else {
            error!("{LOG_PREFIX} {failures} check(s) failed");
        }
        Ok(failures)
    }
}

fn report(sender: &CommandSender, label: &str, value: &str) {
    sender.send_message(TextComponent::text(&format!("{label}: {value}")));
}

// The plugin's entry point is the `init-plugin` export a component needs, and
// nothing on a host build wants it. Gating it to `wasm32` also keeps
// `pumpkin-pie`'s own copy of that symbol from colliding with this crate's
// when the test harness links.
#[cfg(target_arch = "wasm32")]
register_plugin!(PieConsumer);
