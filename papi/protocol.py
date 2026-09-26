"""Wire format for PumpkinPAPI's inter-plugin IPC.

Requests and responses are UTF-8 JSON objects. Everything arriving over IPC is
untrusted, so `decode` checks the size, the encoding and the shape before the
payload reaches the registry.

The operations are named after the Java PlaceholderAPI methods they mirror, so
the two read the same way:

    Java                              Here
    setPlaceholders                   set_placeholders
    getPlaceholderValue               get_placeholder_value
    getRegisteredPlaceholders         get_registered_placeholders
    registerPlaceholderExpansion      register_expansion
    unregisterPlaceholderExpansion    unregister_expansion
    onRequest                         on_request  (sent back to the expansion)
"""

import json

PROTOCOL_VERSION = 0

# The provider's plugin name, which is also its IPC address.
PROVIDER = "PumpkinPAPI"

MAX_MESSAGE_BYTES = 65536
MAX_TEXT_LENGTH = 32768
MAX_ID_LENGTH = 128
MAX_DESCRIPTION_LENGTH = 256
MAX_PLACEHOLDERS_PER_EXPANSION = 256
MAX_NAMESPACES = 256

OP_PING = "ping"
OP_GET_REGISTERED_PLACEHOLDERS = "get_registered_placeholders"
OP_REGISTER_EXPANSION = "register_expansion"
OP_UNREGISTER_EXPANSION = "unregister_expansion"
OP_SET_PLACEHOLDERS = "set_placeholders"
OP_GET_PLACEHOLDER_VALUE = "get_placeholder_value"
OP_ON_REQUEST = "on_request"

RESERVED_NAMESPACES = frozenset({"player", "server", "papi"})


class ProtocolError(ValueError):
    """Raised when an IPC message cannot be understood."""


def encode(payload):
    return json.dumps(payload, separators=(",", ":")).encode("utf-8")


def ok(**fields):
    return encode({"ok": True, **fields})


def error(message):
    return encode({"ok": False, "error": message})


def _load(message):
    if len(message) > MAX_MESSAGE_BYTES:
        raise ProtocolError(f"message is larger than {MAX_MESSAGE_BYTES} bytes")
    try:
        text = message.decode("utf-8")
    except UnicodeDecodeError as exc:
        raise ProtocolError("message is not valid UTF-8") from exc
    try:
        payload = json.loads(text)
    except ValueError as exc:
        raise ProtocolError("message is not valid JSON") from exc
    if not isinstance(payload, dict):
        raise ProtocolError("payload must be a JSON object")
    return payload


def decode(message):
    """Decode a request, which must name the operation it wants."""
    payload = _load(message)
    op = payload.get("op")
    if not isinstance(op, str) or not op:
        raise ProtocolError("request is missing a string 'op'")
    return payload


def decode_response(message):
    """Decode a response, which must say whether it succeeded."""
    payload = _load(message)
    if not isinstance(payload.get("ok"), bool):
        raise ProtocolError("response is missing a boolean 'ok'")
    return payload


def read_str(payload, key, limit=MAX_TEXT_LENGTH, default=None):
    """Read an optional string field, rejecting anything that is not one."""
    value = payload.get(key)
    if value is None:
        return default
    if not isinstance(value, str):
        raise ProtocolError(f"'{key}' must be a string")
    if len(value) > limit:
        raise ProtocolError(f"'{key}' is longer than {limit} characters")
    return value


def read_required_str(payload, key, limit=MAX_TEXT_LENGTH):
    value = read_str(payload, key, limit)
    if not value:
        raise ProtocolError(f"'{key}' is required")
    return value
