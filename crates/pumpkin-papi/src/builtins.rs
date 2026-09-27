//! The placeholders the provider answers itself.
//!
//! A placeholder earns a place here only when it is vanilla state a third party
//! has no better source for. Everything else is meant to arrive as an
//! expansion.
//!
//! Only getters that hand back plain values belong here. A getter that returns
//! a host resource, such as `get_world` or `get_display_name`, is left out until
//! the WIT offers a way to release the handle again.

use pumpkin_plugin_api::{Player, Server};

/// What a resolver is allowed to look at.
pub struct ResolveContext<'a> {
    /// The server, for the `server_` namespace.
    pub server: &'a Server,
    /// The player the text is being rendered for, if any.
    pub viewer: Option<&'a Player>,
    /// The token's argument, if it carried one.
    pub argument: Option<&'a str>,
}

/// A built in placeholder.
pub struct Builtin {
    /// The identifier between the percent signs.
    pub id: &'static str,
    /// One line about what it is, shown by `/papi`.
    pub description: &'static str,
    /// Computes the value, or [`None`] to leave the placeholder alone.
    pub resolve: fn(&ResolveContext<'_>) -> Option<String>,
    /// Whether the placeholder takes an argument.
    pub takes_arg: bool,
}

/// Trims a float for display, so `20.00` reads as `20` and `-0.00` as `0`.
fn number(value: f64, places: u32) -> String {
    let text = format!("{:.*}", places as usize, value);
    let trimmed = text.trim_end_matches('0').trim_end_matches('.');
    if trimmed.is_empty() || trimmed == "-" {
        "0".to_string()
    } else {
        trimmed.to_string()
    }
}

/// Every built in placeholder, in the order `/papi` lists them.
pub static BUILTINS: &[Builtin] = &[
    Builtin {
        id: "player_name",
        description: "The viewer's name",
        resolve: |context| Some(context.viewer?.get_name()),
        takes_arg: false,
    },
    Builtin {
        id: "player_uuid",
        description: "The viewer's unique id",
        resolve: |context| Some(context.viewer?.get_id().to_string()),
        takes_arg: false,
    },
    Builtin {
        id: "player_ip",
        description: "The viewer's IP address",
        resolve: |context| Some(context.viewer?.get_ip()),
        takes_arg: false,
    },
    Builtin {
        id: "player_gamemode",
        description: "The viewer's game mode",
        resolve: |context| {
            // The WIT enum has no `Display`, so its `Debug` name is the case.
            Some(format!("{:?}", context.viewer?.get_gamemode()).to_lowercase())
        },
        takes_arg: false,
    },
    Builtin {
        id: "player_team",
        description: "The viewer's scoreboard team",
        resolve: |context| Some(context.viewer?.get_team()?.to_string()),
        takes_arg: false,
    },
    Builtin {
        id: "player_ping",
        description: "The viewer's latency in milliseconds",
        resolve: |context| Some(context.viewer?.get_ping().to_string()),
        takes_arg: false,
    },
    Builtin {
        id: "player_health",
        description: "The viewer's health",
        resolve: |context| Some(number(context.viewer?.get_health() as f64, 1)),
        takes_arg: false,
    },
    Builtin {
        id: "player_max_health",
        description: "The viewer's maximum health",
        resolve: |context| Some(number(context.viewer?.get_max_health() as f64, 1)),
        takes_arg: false,
    },
    Builtin {
        id: "player_yaw",
        description: "The viewer's yaw",
        resolve: |context| Some(number(context.viewer?.get_yaw() as f64, 1)),
        takes_arg: false,
    },
    Builtin {
        id: "player_pitch",
        description: "The viewer's pitch",
        resolve: |context| Some(number(context.viewer?.get_pitch() as f64, 1)),
        takes_arg: false,
    },
    Builtin {
        id: "player_x",
        description: "The viewer's X coordinate",
        resolve: |context| {
            let (x, _, _) = context.viewer?.get_position();
            Some(number(x, 2))
        },
        takes_arg: false,
    },
    Builtin {
        id: "player_y",
        description: "The viewer's Y coordinate",
        resolve: |context| {
            let (_, y, _) = context.viewer?.get_position();
            Some(number(y, 2))
        },
        takes_arg: false,
    },
    Builtin {
        id: "player_z",
        description: "The viewer's Z coordinate",
        resolve: |context| {
            let (_, _, z) = context.viewer?.get_position();
            Some(number(z, 2))
        },
        takes_arg: false,
    },
    Builtin {
        id: "player_has_permission",
        description: "Whether the viewer holds a permission node",
        resolve: |context| {
            let node = context.argument?;
            Some(context.viewer?.has_permission(node).to_string())
        },
        takes_arg: true,
    },
    Builtin {
        id: "server_online",
        description: "Players currently online",
        resolve: |context| Some(context.server.get_player_count().to_string()),
        takes_arg: false,
    },
    Builtin {
        id: "server_max_players",
        description: "Player slots on the server",
        resolve: |context| Some(context.server.get_max_players().to_string()),
        takes_arg: false,
    },
];

/// Looks a built in up by id, ignoring case.
#[must_use]
pub fn find(id: &str) -> Option<&'static Builtin> {
    BUILTINS.iter().find(|builtin| builtin.id.eq_ignore_ascii_case(id))
}

/// How many built ins there are, for `/papi` and `ping`.
#[must_use]
pub const fn count() -> usize {
    BUILTINS.len()
}
