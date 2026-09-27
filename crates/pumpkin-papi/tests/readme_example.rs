//! Compiles the expansion example from the README, so the documentation cannot
//! drift away from the API without a test failing.
//!
//! If you change a signature in `client.rs`, this fails to build. That is the
//! point: the README is the thing a plugin author copies.

use pumpkin_papi::{answer, Cache, IpcMessage, PluginId, PapiClient, PapiError};

/// Stand-in for whatever a real expansion would look up.
fn rank_of(viewer: Option<&str>) -> Option<String> {
    let viewer = viewer?;
    Some(match viewer {
        "Steve" => "Admin",
        _ => "Member",
    }.to_string())
}

pub struct Ranks;

impl Ranks {
    fn register(&self) -> Result<(), PapiError> {
        PapiClient::new().register_expansion(
            "ranks",
            &["prefix", "suffix"],
            Cache::Ttl { ms: 5_000 },
        )?;
        Ok(())
    }

    fn handle(
        &self,
        _from: PluginId,
        message: IpcMessage,
    ) -> std::result::Result<IpcMessage, String> {
        answer(&message, |ctx| match ctx.name {
            "prefix" => rank_of(ctx.viewer),
            _ => None,
        })
        .map_err(|error| error.to_string())
    }
}

fn render() -> Result<(), PapiError> {
    let papi = PapiClient::new();

    let line = papi.set_placeholders(Some("Steve"), "%player_ping%ms %ranks_prefix%")?;
    println!("{}   unresolved: {:?}", line.text, line.unresolved);

    let online = papi.get_placeholder_value(None, "server_online")?;
    let all = papi.get_registered_placeholders()?;

    let lines = [("Steve", "%player_ping%ms"), ("Alex", "%player_ping%ms")];
    let results = papi.set_placeholders_batch(&lines)?;

    let ranks = Ranks;
    ranks.register()?;
    let _reply = ranks.handle(PluginId::from(String::new()), IpcMessage::new());

    let _ = (online, all, results);
    Ok(())
}

#[test]
fn the_readme_example_typechecks() {
    // Nothing to run: this exists so the signatures above must be real.
    let _ = render;
}
