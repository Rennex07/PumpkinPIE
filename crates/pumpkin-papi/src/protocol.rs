//! The wire format for PumpkinPAPI's inter-plugin IPC.
//!
//! Every message is a UTF-8 JSON object. A request carries an `op`, a response
//! carries `ok`, and a failed response also carries `error`. The operations are
//! named after the Java PlaceholderAPI methods they mirror.
//!
//! Unknown fields are ignored rather than rejected, so a plugin built against
//! an older revision of this file still works against a newer provider.

use serde::{Deserialize, Serialize};

/// Version of the protocol. A consumer should call [`Request::Ping`] and check
/// this before relying on anything else.
pub const PROTOCOL_VERSION: u16 = 0;

/// The provider's plugin name, which is also its IPC address.
pub const PROVIDER: &str = "PumpkinPAPI";

/// Largest message accepted in either direction, in bytes.
pub const MAX_MESSAGE_BYTES: usize = 1 << 20;

/// Longest text one [`Request::SetPlaceholders`] will resolve.
pub const MAX_TEXT_LENGTH: usize = 32768;

/// Longest namespace, placeholder id, viewer name or argument.
pub const MAX_ID_LENGTH: usize = 128;

/// Most lines one [`Request::SetPlaceholdersBatch`] may carry.
pub const MAX_BATCH_LINES: usize = 8192;

/// Longest cache TTL an expansion may ask for, in milliseconds.
///
/// A plugin that caches for long enough serves stale ranks, so the provider
/// clamps whatever is asked for to this.
pub const MAX_CACHE_TTL_MS: u64 = 60_000;

/// Namespaces the provider answers itself, which an expansion cannot claim.
pub const RESERVED_NAMESPACES: [&str; 3] = ["player", "server", "papi"];

/// Something a request asked for that cannot be honoured.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ProtocolError {
    #[error("message is larger than {MAX_MESSAGE_BYTES} bytes")]
    TooLarge,
    #[error("message is not valid UTF-8")]
    NotUtf8,
    #[error("message is not valid JSON")]
    NotJson,
    #[error("'{field}' is longer than {limit}")]
    TooLong { field: String, limit: usize },
    #[error("'{namespace}' is not a valid namespace")]
    BadNamespace { namespace: String },
    #[error("'{name}' is not a valid placeholder name")]
    BadName { name: String },
    #[error("'{namespace}' is reserved by the provider")]
    Reserved { namespace: String },
}

/// How long the provider may reuse an expansion's values.
///
/// Mirrors Java PlaceholderAPI's `Cacheable`: an expansion that opts in gets
/// its values cached per viewer, and one that does not is asked every time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Cache {
    /// Ask every time. The default.
    #[default]
    Never,
    /// Reuse a value for this many milliseconds, capped by the provider.
    Ttl { ms: u64 },
}

impl Cache {
    /// This request's TTL, clamped to [`MAX_CACHE_TTL_MS`].
    #[must_use]
    pub fn effective_ttl_ms(self) -> u64 {
        match self {
            Self::Never => 0,
            Self::Ttl { ms } => ms.min(MAX_CACHE_TTL_MS),
        }
    }
}

/// One line of text to resolve, and who it is for.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Line {
    /// Player name or uuid the placeholders are resolved for, or [`None`] to
    /// resolve without a player.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub viewer: Option<String>,
    /// The text, with `%placeholder%` tokens in it.
    pub text: String,
}

/// One resolved line, and the placeholders that stayed in it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedLine {
    /// The text with every resolvable placeholder replaced.
    pub text: String,
    /// Ids that could not be resolved, in the order they first appear.
    pub unresolved: Vec<String>,
}

/// A request from a consumer to the provider.
///
/// Serialises as one JSON object with an `op` field, so serde rejects an
/// operation it has never heard of rather than letting it reach the provider.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Request {
    /// Ask whether the provider is loaded and which versions it speaks.
    Ping,
    /// List every placeholder the provider knows, built in and contributed.
    GetRegisteredPlaceholders,
    /// Claim a namespace for the calling plugin.
    RegisterExpansion {
        namespace: String,
        #[serde(default)]
        placeholders: Vec<String>,
        #[serde(default)]
        cache: Cache,
    },
    /// Give up every namespace the calling plugin claimed.
    UnregisterExpansion,
    /// Resolve every placeholder in one line.
    SetPlaceholders {
        text: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        viewer: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        argument: Option<String>,
    },
    /// Resolve many lines in one message, which is what a tab list or a
    /// scoreboard refreshing every player actually wants.
    SetPlaceholdersBatch { requests: Vec<Line> },
    /// Resolve a single placeholder by id.
    GetPlaceholderValue {
        id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        viewer: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        argument: Option<String>,
    },
    /// Sent by the provider to the expansion that owns a namespace, once for
    /// each of its placeholders the provider cannot answer itself.
    OnRequest {
        namespace: String,
        id: String,
        name: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        viewer: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        argument: Option<String>,
    },
}

/// One placeholder as reported by [`Request::GetRegisteredPlaceholders`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegisteredPlaceholder {
    /// The full identifier, `namespace_name`.
    pub id: String,
    /// One line about what it is. Empty for contributed names, which have no
    /// room for a description on the wire.
    #[serde(default)]
    pub description: String,
    pub namespace: String,
    /// Plugin that provides it, or the provider for the built ins.
    pub source: String,
}

/// The body of a successful response, one variant per operation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Success {
    Ping {
        protocol: u16,
        name: String,
        version: String,
        placeholders: usize,
        expansions: usize,
    },
    Registered {
        placeholders: Vec<RegisteredPlaceholder>,
    },
    RegisteredExpansion {
        namespace: String,
        placeholders: Vec<String>,
        /// The TTL the provider actually applied, after clamping.
        cache: Cache,
    },
    Unregistered {
        namespaces: Vec<String>,
    },
    Resolved(ResolvedLine),
    ResolvedBatch {
        results: Vec<ResolvedLine>,
    },
    Value {
        known: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        value: Option<String>,
    },
    OnRequest {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        value: Option<String>,
    },
}

/// A response from the provider or an expansion.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Response {
    pub ok: bool,
    #[serde(flatten, default, skip_serializing_if = "Option::is_none")]
    pub success: Option<Success>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl Response {
    /// A successful response.
    #[must_use]
    pub const fn success(success: Success) -> Self {
        Self { ok: true, success: Some(success), error: None }
    }

    /// A failed response.
    #[must_use]
    pub fn failure(message: impl Into<String>) -> Self {
        Self { ok: false, success: None, error: Some(message.into()) }
    }

    /// Turns a [`ProtocolError`] into a failed response.
    #[must_use]
    pub fn from_error(error: ProtocolError) -> Self {
        Self::failure(error.to_string())
    }

    /// The successful body, or the reason there isn't one.
    pub fn into_success(self) -> Result<Success, String> {
        if !self.ok {
            return Err(self.error.unwrap_or_else(|| "unknown error".to_string()));
        }
        self.success.ok_or_else(|| "response carried no body".to_string())
    }
}

/// Encodes a value to bytes, refusing anything oversized.
pub fn encode<T: Serialize>(value: &T) -> Result<Vec<u8>, ProtocolError> {
    let bytes = serde_json::to_vec(value).map_err(|_| ProtocolError::NotJson)?;
    if bytes.len() > MAX_MESSAGE_BYTES {
        return Err(ProtocolError::TooLarge);
    }
    Ok(bytes)
}

fn parse<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> Result<T, ProtocolError> {
    if bytes.len() > MAX_MESSAGE_BYTES {
        return Err(ProtocolError::TooLarge);
    }
    let text = std::str::from_utf8(bytes).map_err(|_| ProtocolError::NotUtf8)?;
    serde_json::from_str(text).map_err(|_| ProtocolError::NotJson)
}

/// Decodes a request, checking its size first.
pub fn decode_request(bytes: &[u8]) -> Result<Request, ProtocolError> {
    parse(bytes)
}

/// Decodes a response, checking its size first.
pub fn decode_response(bytes: &[u8]) -> Result<Response, ProtocolError> {
    parse(bytes)
}

/// Whether `namespace` may be claimed by an expansion.
pub fn check_namespace(namespace: &str) -> Result<&str, ProtocolError> {
    if namespace.is_empty() || namespace.starts_with(|c: char| c.is_ascii_digit()) {
        return Err(ProtocolError::BadNamespace { namespace: namespace.to_string() });
    }
    if !namespace.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_') {
        return Err(ProtocolError::BadNamespace { namespace: namespace.to_string() });
    }
    if RESERVED_NAMESPACES.contains(&namespace) {
        return Err(ProtocolError::Reserved { namespace: namespace.to_string() });
    }
    Ok(namespace)
}

/// Whether `name` is usable as the part of a placeholder id after the namespace.
pub fn check_name(name: &str) -> Result<&str, ProtocolError> {
    if name.is_empty() {
        return Err(ProtocolError::BadName { name: name.to_string() });
    }
    if !name.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_') {
        return Err(ProtocolError::BadName { name: name.to_string() });
    }
    Ok(name)
}
