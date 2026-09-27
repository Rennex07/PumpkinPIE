//! The `/papi` command, for checking a config by hand.

use pumpkin_plugin_api::command::{
    Arg, ArgumentType, Command, CommandError, CommandNode, CommandSender, CommandSuggestion,
    CommandSuggestions, ConsumedArgs, StringType, SuggestionRequest,
};
use pumpkin_plugin_api::commands::{CommandHandler, CommandSuggestionHandler};
use pumpkin_plugin_api::permission::{Permission, PermissionDefault, PermissionLevel};
use pumpkin_plugin_api::text::{NamedColor, TextComponent};
use pumpkin_plugin_api::{Context, Server};
use tracing::warn;

use pumpkin_papi::builtins;

use crate::plugin;

/// Permission required by `/papi`.
///
/// It has to be namespaced with the provider's exact plugin name, because
/// `Context::register_permission` refuses any other namespace and a node that
/// was never registered denies everyone, operators included.
pub const USE_PERMISSION: &str = "PumpkinPAPI:use";

/// The permission node `/papi` is gated on.
#[must_use]
pub const fn use_permission() -> &'static str {
    USE_PERMISSION
}

const USAGE: &str = "Usage: /papi parse <text> to resolve placeholders, \
                     /papi expansions to list the registered expansions.";

/// Most completions offered at once.
const MAX_SUGGESTIONS: usize = 20;

/// Registers `/papi` and the permission it needs.
pub fn register(context: Context) {
    if let Err(error) = context.register_permission(&Permission {
        node: USE_PERMISSION.to_string(),
        description: "Allows using /papi.".to_string(),
        default: PermissionDefault::Op(PermissionLevel::Two),
        children: Vec::new(),
    }) {
        // Swallowing this leaves the node unregistered, which denies the command
        // to everyone while looking exactly like the command not existing.
        warn!("could not register {USE_PERMISSION}, /papi will be unusable: {error}");
    }

    context.register_command(root(), USE_PERMISSION);
}

fn root() -> Command {
    Command::new(&["papi".to_string()], "Resolve and inspect placeholders")
        .execute(PapiCommand)
        .then(
            CommandNode::literal("parse")
                .execute(PapiCommand)
                .suggest(PapiSuggestions)
                .then(
                    CommandNode::argument("text", &ArgumentType::String(StringType::Greedy))
                        .execute(PapiCommand)
                        .suggest(PapiSuggestions),
                ),
        )
        .then(
            CommandNode::literal("expansions")
                .execute(PapiCommand)
                .suggest(PapiSuggestions),
        )
}

/// The single handler behind every `/papi` branch.
struct PapiCommand;

impl CommandHandler for PapiCommand {
    fn handle(
        &self,
        sender: CommandSender,
        _server: Server,
        args: ConsumedArgs,
    ) -> std::result::Result<i32, CommandError> {
        let requested = match args.get_value("text") {
            Arg::Simple(value) | Arg::Msg(value) => value,
            _ => String::new(),
        };
        if requested.is_empty() {
            return help(&sender);
        }

        let viewer = sender.as_player().map(|player| player.get_name());
        let line = plugin::resolve_line(viewer.as_deref(), &requested);
        sender.send_message(TextComponent::text(&line.text));

        if !line.unresolved.is_empty() {
            let note = TextComponent::text(&format!("unresolved: {}", line.unresolved.join(", ")));
            sender.send_error(note.color_named(NamedColor::Red));
        }
        Ok(i32::try_from(line.unresolved.len()).unwrap_or(i32::MAX))
    }
}

/// Bare `/papi`, or `/papi` with nothing to parse.
fn help(sender: &CommandSender) -> std::result::Result<i32, CommandError> {
    sender.send_message(TextComponent::text(USAGE));
    for builtin in builtins::BUILTINS {
        sender.send_message(TextComponent::text(&format!(
            "%{}%  {}",
            builtin.id, builtin.description
        )));
    }
    for entry in plugin::expansion_summary() {
        sender.send_message(TextComponent::text(&entry));
    }
    Ok(0)
}

/// Offers `%placeholder%` completions for the token the cursor sits in.
struct PapiSuggestions;

impl CommandSuggestionHandler for PapiSuggestions {
    fn suggest(
        &self,
        _sender: CommandSender,
        _server: Server,
        request: SuggestionRequest,
    ) -> CommandSuggestions {
        let cursor = (request.cursor as usize).min(request.input.len());
        let typed = &request.input[..cursor];
        let start = typed.rfind('%').map_or(typed.len(), |index| index + 1);
        let prefix = typed[start..].to_ascii_lowercase();

        let values = plugin::registered()
            .into_iter()
            .filter(|entry| entry.id.starts_with(&prefix))
            .take(MAX_SUGGESTIONS)
            .map(|entry| CommandSuggestion {
                value: format!("%{}%", entry.id),
                tooltip: Some(TextComponent::text(&entry.description)),
            })
            .collect();

        CommandSuggestions {
            start: start as u32,
            length: prefix.len() as u32,
            values,
        }
    }
}
/// Both of these mistakes produced the same symptom in game: `/papi` reported
/// "Unknown command", which is what a client shows for a command the player may
/// not use, and the log stayed clean.
#[cfg(test)]
mod tests {
    use super::USE_PERMISSION;
    use pumpkin_papi::PROVIDER;

    #[test]
    fn the_permission_node_is_namespaced_with_the_plugin_name() {
        // `Context::register_permission` rejects a node that does not start with
        // `{plugin name}:`, compared case sensitively.
        assert!(
            USE_PERMISSION.starts_with(&format!("{PROVIDER}:")),
            "{USE_PERMISSION:?} must start with {PROVIDER:?}: or register_permission refuses it"
        );
        assert!(
            USE_PERMISSION.len() > PROVIDER.len() + 1,
            "{USE_PERMISSION:?} needs a key after the colon"
        );
    }

    #[test]
    fn the_metadata_name_matches_the_permission_namespace() {
        // The check above only holds if the plugin's own name is what the node
        // is built from, so pin the pair together.
        assert_eq!(PROVIDER, "PumpkinPAPI");
        assert_eq!(USE_PERMISSION, "PumpkinPAPI:use");
    }
}
