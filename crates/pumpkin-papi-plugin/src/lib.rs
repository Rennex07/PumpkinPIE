//! The PumpkinPAPI provider plugin.
//!
//! This crate is the Wasm component you drop into a server's `plugins`
//! directory. The protocol, the token scanner and the client live in
//! `pumpkin-papi`, which is a plain library with no `init-plugin` export.

pub mod command;
mod plugin;

pub use plugin::{PumpkinPapi, expansion_summary, registered, resolve_line, server};

/// Re-exported so the consumer-facing test can assert the provider's plugin name
/// and its permission node agree, without reaching into a private module.
pub use command::USE_PERMISSION;
