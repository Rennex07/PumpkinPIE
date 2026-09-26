"""Consumer side of the PumpkinPAPI protocol, shaped like Java PlaceholderAPI.

A plugin that only consumes placeholders:

    from papi.client import PapiClient

    papi = PapiClient()
    line, unresolved = papi.set_placeholders(player, "%player_ping% ms")

A plugin that also provides them registers an expansion, which is the
PlaceholderAPI `PlaceholderExpansion` equivalent:

    papi.register_expansion("ranks", ["prefix", "suffix"])
    papi.on_request(self.rank)
    ...
    def rank(self, namespace, name, viewer, argument):
        return RANKS.get(viewer, "Member")

`dependencies` in the plugin metadata should list the provider, so the consumer
is only loaded once the provider is ready to answer.
"""

from wit_world.imports import ipc

from papi import protocol


class PapiError(RuntimeError):
    """Raised when the provider cannot be reached or refuses a request."""


def _unwrap(reply):
    """`send_ipc_message` answers with the inner result, which may be wrapped."""
    if isinstance(reply, bytes):
        return reply
    return getattr(reply, "value", None)


class PapiClient:
    """Talks to one PumpkinPAPI provider over Pumpkin inter-plugin IPC."""

    def __init__(self, provider: str = protocol.PROVIDER):
        self.provider = provider
        self._on_request = None

    def _call(self, op, **fields):
        request = protocol.encode({"op": op, **fields})
        try:
            reply = ipc.send_ipc_message(self.provider, request)
        except Exception as exc:
            # The host reports an unknown plugin, a plugin that is not loaded
            # yet, and a plugin messaging itself the same way.
            raise PapiError(f"{self.provider} is not reachable: {exc}") from exc
        message = _unwrap(reply)
        if not isinstance(message, bytes):
            raise PapiError(f"{self.provider} answered with no message")
        try:
            payload = protocol.decode_response(message)
        except protocol.ProtocolError as exc:
            raise PapiError(f"{self.provider} sent an unreadable reply: {exc}") from exc
        if not payload["ok"]:
            raise PapiError(str(payload.get("error", "unknown error")))
        return payload

    def ping(self):
        return self._call(protocol.OP_PING)

    def get_registered_placeholders(self):
        """Every placeholder the provider knows, built ins and expansions alike."""
        return self._call(protocol.OP_GET_REGISTERED_PLACEHOLDERS)["placeholders"]

    def set_placeholders(self, viewer, text, argument=None):
        """Return `text` with its placeholders resolved, plus the unresolved ids.

        `viewer` is a player name or uuid, or None to render without a player.
        """
        payload = self._call(
            protocol.OP_SET_PLACEHOLDERS,
            text=text,
            viewer=viewer if isinstance(viewer, str) else _name_of(viewer),
            argument=argument,
        )
        return payload["text"], payload["unresolved"]

    def get_placeholder_value(self, identifier, viewer=None, argument=None):
        """One placeholder's value, or None when it cannot be resolved."""
        payload = self._call(
            protocol.OP_GET_PLACEHOLDER_VALUE,
            id=identifier,
            viewer=viewer if isinstance(viewer, str) else _name_of(viewer),
            argument=argument,
        )
        if not payload["known"]:
            return None
        return payload["value"]

    def register_expansion(self, namespace, placeholders=()):
        """Claim `namespace` for this plugin. Later calls replace the name list."""
        return self._call(
            protocol.OP_REGISTER_EXPANSION,
            namespace=namespace,
            placeholders=list(placeholders),
        )

    def unregister_expansion(self):
        """Give up every namespace this plugin claimed."""
        return self._call(protocol.OP_UNREGISTER_EXPANSION)["namespaces"]

    def on_request(self, callback):
        """Answer `on_request` messages for the namespaces this plugin claimed.

        `callback(namespace, name, viewer, argument)` returns the value as a
        string, or None to leave the placeholder unresolved.
        """
        self._on_request = callback
        return callback

    def handle_ipc_message(self, sender: str, message: bytes) -> bytes:
        """Wire this into the plugin's own `handle_ipc_message`."""
        try:
            request = protocol.decode(message)
        except protocol.ProtocolError as exc:
            return protocol.error(str(exc))
        if request["op"] != protocol.OP_ON_REQUEST:
            return protocol.error(f"unknown op '{request['op']}'")
        if not callable(self._on_request):
            return protocol.error("this plugin registered no on_request handler")

        namespace = protocol.read_str(request, "namespace", protocol.MAX_ID_LENGTH, "")
        name = protocol.read_str(request, "name", protocol.MAX_ID_LENGTH, "")
        viewer = protocol.read_str(request, "viewer", protocol.MAX_ID_LENGTH, "")
        argument = protocol.read_str(request, "argument", protocol.MAX_ID_LENGTH)
        try:
            value = self._on_request(namespace, name, viewer, argument)
        except Exception as exc:
            return protocol.error(str(exc))
        return protocol.ok(value=value)


def _name_of(viewer):
    if viewer is None:
        return ""
    get_name = getattr(viewer, "get_name", None)
    return get_name() if callable(get_name) else str(viewer)
