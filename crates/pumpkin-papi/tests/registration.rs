//! Tests for the ways the plugin can register itself wrongly.
//!
//! These exist because both of these bugs produced exactly the same symptom
//! in game: `/papi` reported "Unknown command", which is what a client shows
//! for a command the player is not permitted to use.

use pumpkin_papi::command::use_permission;
use pumpkin_papi::protocol::PROVIDER;

#[test]
fn the_permission_node_is_namespaced_with_the_plugin_name() {
    // `Context::register_permission` rejects a node that does not start with
    // `{plugin name}:`. It compares case sensitively.
    let node = use_permission();
    assert!(
        node.starts_with(&format!("{PROVIDER}:")),
        "{node:?} must start with {PROVIDER:?}: or register_permission refuses it"
    );
    assert!(node.len() > PROVIDER.len() + 1, "{node:?} needs a key after the colon");
}

#[test]
fn the_metadata_name_matches_the_permission_namespace() {
    // The check above is only true if the plugin's own name is what the node
    // is built from, so pin the pair together.
    assert_eq!(PROVIDER, "PumpkinPAPI");
    assert_eq!(use_permission(), "PumpkinPAPI:use");
}
