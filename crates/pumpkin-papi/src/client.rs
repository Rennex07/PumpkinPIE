//! Consumer and expansion side of the PumpkinPAPI protocol.
//!
//! Consuming placeholders:
//!
//! ```no_run
//! use pumpkin_papi::{PapiClient, PapiError};
//!
//! # fn render() -> Result<(), PapiError> {
//! let papi = PapiClient::new();
//! let line = papi.set_placeholders(Some("Steve"), "%player_ping%ms")?;
//! # let _ = line.text;
//! # Ok(())
//! # }
//! ```
//!
//! Providing them:
//!
//! ```no_run
//! use pumpkin_papi::{Cache, IpcMessage, PapiClient, PapiError, answer};
//!
//! # fn on_load() -> Result<(), PapiError> {
//! PapiClient::new().register_expansion("ranks", &["prefix"], Cache::Ttl { ms: 5_000 })?;
//! # Ok(())
//! # }
//!
//! # fn handle(message: IpcMessage) -> Result<IpcMessage, String> {
//! answer(&message, |ctx| Some("Admin".to_string())).map_err(|e| e.to_string())
//! # }
//! ```

use crate::protocol::{
    Cache, Line, MAX_ID_LENGTH, MAX_TEXT_LENGTH, ProtocolError, RegisteredPlaceholder, Request,
    ResolvedLine, Response, Success, decode_request, decode_response, encode,
};

/// Why a call to the provider did not produce a value.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PapiError {
    /// The provider is not loaded, the name is wrong, or the plugin messaged
    /// itself. The host reports all three the same way.
    #[error("{provider} is not reachable: it is not loaded, or the name is wrong")]
    Unreachable { provider: String },
    /// The provider answered, and refused.
    #[error("{provider} refused the request: {message}")]
    Refused { provider: String, message: String },
    /// The provider answered with something this client cannot read.
    #[error("{provider} sent a reply this client cannot read: {message}")]
    Malformed { provider: String, message: String },
    /// The provider answered, but not with the operation that was asked for.
    #[error("{provider} answered {operation} with the wrong response")]
    Mismatched {
        provider: String,
        operation: &'static str,
    },
    /// The request could not be encoded, so it was never sent.
    #[error("request could not be encoded: {0}")]
    Unencodable(#[source] ProtocolError),
}

/// What a caller needs to answer an `on_request` message.
#[derive(Debug, Clone, Copy)]
pub struct RequestContext<'a> {
    /// The namespace the provider asked about.
    pub namespace: &'a str,
    /// The full placeholder id, `namespace_name`.
    pub id: &'a str,
    /// The part after the namespace.
    pub name: &'a str,
    /// Name of the player the text is being rendered for, if any.
    pub viewer: Option<&'a str>,
    /// The token's argument, if it carried one.
    pub argument: Option<&'a str>,
}

/// What `register_expansion` actually applied, after the provider clamped the
/// TTL it was asked for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Registered {
    /// The names the provider now advertises for the namespace, lowercased.
    pub placeholders: Vec<String>,
    /// The cache the provider is actually applying, which may be shorter than
    /// the one requested. Read this rather than assuming you got what you asked
    /// for: asking for an hour comes back as `Ttl { ms: 60_000 }`.
    pub cache: Cache,
}

/// Talks to one PumpkinPAPI provider over Pumpkin's inter-plugin IPC.
///
/// Holds no state, so it is cheap to keep one around for the life of a plugin.
#[derive(Debug, Clone)]
pub struct PapiClient {
    provider: String,
}

impl Default for PapiClient {
    fn default() -> Self {
        Self::new()
    }
}

impl PapiClient {
    /// A client for the provider with the default name.
    #[must_use]
    pub fn new() -> Self {
        Self::with_provider(crate::protocol::PROVIDER)
    }

    /// A client for a provider registered under a different name.
    #[must_use]
    pub fn with_provider(provider: impl Into<String>) -> Self {
        Self {
            provider: provider.into(),
        }
    }

    /// The plugin name this client sends to.
    #[must_use]
    pub fn provider(&self) -> &str {
        &self.provider
    }

    fn send(&self, request: &Request) -> Result<Success, PapiError> {
        let bytes = encode(request).map_err(PapiError::Unencodable)?;
        let reply = match pumpkin_plugin_api::ipc::send_ipc_message(&self.provider, &bytes) {
            Ok(Ok(reply)) => reply,
            Ok(Err(message)) => {
                return Err(PapiError::Refused {
                    provider: self.provider.clone(),
                    message,
                });
            }
            Err(()) => {
                return Err(PapiError::Unreachable {
                    provider: self.provider.clone(),
                });
            }
        };
        let response: Response = decode_response(&reply).map_err(|error| PapiError::Malformed {
            provider: self.provider.clone(),
            message: error.to_string(),
        })?;
        response
            .into_success()
            .map_err(|message| PapiError::Refused {
                provider: self.provider.clone(),
                message,
            })
    }

    /// Resolves every placeholder in `text`.
    ///
    /// `viewer` is a player name or uuid, or [`None`] to render without a
    /// player, in which case player placeholders come back empty.
    ///
    /// # Errors
    /// Returns [`PapiError`] if the provider cannot be reached or refuses.
    pub fn set_placeholders(
        &self,
        viewer: Option<&str>,
        text: &str,
    ) -> Result<ResolvedLine, PapiError> {
        let success = self.send(&Request::SetPlaceholders {
            text: truncate(text, MAX_TEXT_LENGTH)?,
            viewer: viewer
                .map(|name| truncate(name, MAX_ID_LENGTH))
                .transpose()?,
            argument: None,
        })?;
        match success {
            Success::Resolved(line) => Ok(line),
            _ => Err(PapiError::Mismatched {
                provider: self.provider.clone(),
                operation: "set_placeholders",
            }),
        }
    }

    /// Resolves many lines in one message.
    ///
    /// This is what a tab list or scoreboard refreshing every player should
    /// call: one message per refresh instead of one per player.
    ///
    /// # Errors
    /// Returns [`PapiError`] if the provider cannot be reached or refuses.
    pub fn set_placeholders_batch(
        &self,
        lines: &[(&str, &str)],
    ) -> Result<Vec<ResolvedLine>, PapiError> {
        let requests = lines
            .iter()
            .map(|(viewer, text)| Line {
                viewer: Some((*viewer).to_string()),
                text: (*text).to_string(),
            })
            .collect();
        let success = self.send(&Request::SetPlaceholdersBatch { requests })?;
        match success {
            Success::ResolvedBatch { results } => Ok(results),
            _ => Err(PapiError::Mismatched {
                provider: self.provider.clone(),
                operation: "set_placeholders_batch",
            }),
        }
    }

    /// One placeholder's value, or [`None`] when it is not registered.
    ///
    /// # Errors
    /// Returns [`PapiError`] if the provider cannot be reached or refuses.
    pub fn get_placeholder_value(
        &self,
        viewer: Option<&str>,
        id: &str,
    ) -> Result<Option<String>, PapiError> {
        let success = self.send(&Request::GetPlaceholderValue {
            id: truncate(id, MAX_ID_LENGTH)?,
            viewer: viewer
                .map(|name| truncate(name, MAX_ID_LENGTH))
                .transpose()?,
            argument: None,
        })?;
        match success {
            Success::Value { value, .. } => Ok(value),
            _ => Err(PapiError::Mismatched {
                provider: self.provider.clone(),
                operation: "get_placeholder_value",
            }),
        }
    }

    /// Every placeholder the provider knows, built in and contributed.
    ///
    /// # Errors
    /// Returns [`PapiError`] if the provider cannot be reached or refuses.
    pub fn get_registered_placeholders(&self) -> Result<Vec<RegisteredPlaceholder>, PapiError> {
        let success = self.send(&Request::GetRegisteredPlaceholders)?;
        match success {
            Success::Registered { placeholders } => Ok(placeholders),
            _ => Err(PapiError::Mismatched {
                provider: self.provider.clone(),
                operation: "get_registered_placeholders",
            }),
        }
    }

    /// Claims `namespace` for this plugin, answering for `names`.
    ///
    /// Registering again replaces the name list and the cache setting. A
    /// namespace with no names means the expansion answers whatever it is asked
    /// for. `cache` says whether the provider may reuse its values, mirroring
    /// Java PlaceholderAPI's `Cacheable`.
    ///
    /// # Errors
    /// Returns [`PapiError`] if the provider cannot be reached, refuses, or the
    /// namespace or a name is malformed or reserved. A namespace containing an
    /// underscore is malformed, because the namespace of `%a_b%` is everything
    /// before the first underscore.
    pub fn register_expansion(
        &self,
        namespace: &str,
        names: &[&str],
        cache: Cache,
    ) -> Result<Registered, PapiError> {
        let success = self.send(&Request::RegisterExpansion {
            namespace: truncate(namespace, MAX_ID_LENGTH)?,
            placeholders: names
                .iter()
                .map(|name| truncate(name, MAX_ID_LENGTH))
                .collect::<Result<Vec<_>, _>>()?,
            cache,
        })?;
        match success {
            Success::RegisteredExpansion {
                placeholders,
                cache,
                ..
            } => Ok(Registered {
                placeholders,
                cache,
            }),
            _ => Err(PapiError::Mismatched {
                provider: self.provider.clone(),
                operation: "register_expansion",
            }),
        }
    }

    /// Gives up every namespace this plugin claimed.
    ///
    /// # Errors
    /// Returns [`PapiError`] if the provider cannot be reached or refuses.
    pub fn unregister_expansion(&self) -> Result<Vec<String>, PapiError> {
        let success = self.send(&Request::UnregisterExpansion)?;
        match success {
            Success::Unregistered { namespaces } => Ok(namespaces),
            _ => Err(PapiError::Mismatched {
                provider: self.provider.clone(),
                operation: "unregister_expansion",
            }),
        }
    }
}

fn truncate(value: &str, limit: usize) -> Result<String, PapiError> {
    if value.len() > limit {
        return Err(PapiError::Unencodable(ProtocolError::TooLong {
            field: value.to_string(),
            limit,
        }));
    }
    Ok(value.to_string())
}

/// Turns an incoming `on_request` message into a reply.
///
/// An expansion author wires this into `Plugin::handle_ipc_message` and returns
/// [`None`] from `resolve` to decline, which leaves the placeholder in the
/// text. Anything that is not an `on_request` message is answered as a refusal
/// so a misrouted message is visible rather than silent.
///
/// The provider calls this **synchronously from inside its own IPC handler**, so
/// do not call back into the provider from `resolve`. Reaching for a
/// [`PapiClient`] there re-enters the provider while it is still inside the call
/// that invoked you, and it will deadlock.
///
/// # Errors
/// Returns [`ProtocolError`] if the message cannot be read or the reply cannot
/// be encoded.
pub fn answer(
    message: &[u8],
    resolve: impl FnOnce(RequestContext<'_>) -> Option<String>,
) -> Result<Vec<u8>, ProtocolError> {
    let reply = match decode_request(message) {
        Ok(Request::OnRequest {
            namespace,
            id,
            name,
            viewer,
            argument,
        }) => {
            let value = resolve(RequestContext {
                namespace: &namespace,
                id: &id,
                name: &name,
                viewer: viewer.as_deref(),
                argument: argument.as_deref(),
            });
            Response::success(Success::OnRequest { value })
        }
        Ok(other) => {
            let op = serde_json::to_value(other)
                .ok()
                .and_then(|value| {
                    value
                        .get("op")
                        .and_then(|op| op.as_str())
                        .map(str::to_string)
                })
                .unwrap_or_else(|| "an unnamed operation".to_string());
            Response::failure(format!("unknown op '{op}'"))
        }
        Err(error) => Response::from_error(error),
    };
    encode(&reply)
}
