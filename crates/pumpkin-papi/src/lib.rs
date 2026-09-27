//! PumpkinPAPI, a placeholder provider for Pumpkin.
//!
//! One crate that is both the provider plugin and the client other plugins
//! depend on. As a `cdylib` it loads into a server's `plugins` directory; as an
//! `rlib` it gives a plugin [`PapiClient`].
//!
//! The model follows Java PlaceholderAPI. An expansion claims a namespace and
//! answers for `<namespace>_<name>`, the provider answers the reserved
//! `player`, `server` and `papi` namespaces itself, and a placeholder nobody
//! can resolve is left in the text as written.
//!
//! Consume placeholders:
//!
//! ```no_run
//! use pumpkin_papi::PapiClient;
//!
//! let papi = PapiClient::new();
//! let line = papi.set_placeholders(Some("Steve"), "%player_ping%ms %ranks_prefix%").unwrap();
//! println!("{}", line.text);
//! ```
//!
//! Provide them, which is the `PlaceholderExpansion` equivalent:
//!
//! ```no_run
//! use pumpkin_papi::{Cache, IpcMessage, PapiClient, PapiError, answer};
//!
//! # fn on_load() -> Result<(), PapiError> {
//! PapiClient::new().register_expansion("ranks", &["prefix", "suffix"], Cache::Ttl { ms: 5_000 })?;
//! # Ok(())
//! # }
//!
//! # fn handle(message: IpcMessage) -> Result<IpcMessage, String> {
//! answer(&message, |ctx| match ctx.name {
//!     "prefix" => Some("Admin".to_string()),
//!     _ => None,
//! })
//! .map_err(|error| error.to_string())
//! # }
//! ```
//!
//! The protocol is JSON over Pumpkin's inter-plugin IPC, so a plugin written
//! against it in any language with a Pumpkin SDK can reach the same provider.

pub mod builtins;
pub mod client;
pub mod command;
pub mod expansion;
pub mod plugin;
pub mod protocol;
pub mod tokens;

pub use client::{PapiClient, PapiError, RequestContext, answer};
pub use expansion::{Expansion, Registry};
pub use plugin::PumpkinPapi;
pub use protocol::{Cache, Line, PROTOCOL_VERSION, PROVIDER, Request, Response, Success};
pub use tokens::{Substituted, Token};

/// The WIT message types, under the names the plugin API uses them by.
///
/// `pumpkin-plugin-api` keeps its generated `wit` module private, so these are
/// the aliases it uses instead: `PluginId` is a `String` and `IpcMessage` is a
/// `Vec<u8>`.
pub type PluginId = String;

/// See [`PluginId`].
pub type IpcMessage = Vec<u8>;
