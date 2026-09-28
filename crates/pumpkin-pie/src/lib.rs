//! PumpkinPIE, the Placeholder Integration Engine for Pumpkin.
//!
//! Two crates, because every Pumpkin plugin exports a symbol called
//! `init-plugin` and a plugin cannot link a crate that exports one:
//!
//! - `pumpkin_pie` is this crate, a plain `rlib` holding the protocol, the
//!   token scanner and the client a plugin uses as [`PieClient`].
//! - `pumpkin_pie_plugin` is the `cdylib` you drop into a server's `plugins`
//!   directory. Depend on this crate, never on that one.
//!
//! The model follows Java PlaceholderAPI. An expansion claims a namespace and
//! answers for `<namespace>_<name>`, the provider answers the reserved
//! `player`, `server` and `pie` namespaces itself, and a placeholder nobody
//! can resolve is left in the text as written.
//!
//! Consume placeholders:
//!
//! ```no_run
//! use pumpkin_pie::PieClient;
//!
//! let pie = PieClient::new();
//! let line = pie.set_placeholders(Some("Steve"), "%player_ping%ms %ranks_prefix%").unwrap();
//! println!("{}", line.text);
//! ```
//!
//! Provide them, which is the `PlaceholderExpansion` equivalent:
//!
//! ```no_run
//! use pumpkin_pie::{Cache, IpcMessage, PieClient, PieError, answer};
//!
//! # fn on_load() -> Result<(), PieError> {
//! PieClient::new().register_expansion("ranks", &["prefix", "suffix"], Cache::Ttl { ms: 5_000 })?;
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
pub mod expansion;
pub mod protocol;
pub mod tokens;

pub use client::{PieClient, PieError, Registered, RequestContext, answer};
pub use expansion::{Expansion, Registry};
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
