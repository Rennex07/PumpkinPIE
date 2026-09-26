"""PumpkinPAPI, a placeholder provider other Pumpkin plugins resolve through.

Modelled on Java PlaceholderAPI. Expansions claim a namespace and answer for
`<namespace>_<name>`, the provider answers the `player`, `server` and `papi`
namespaces itself, and a placeholder nobody can resolve is left in the text as
written.

Protocol version 0, all messages are UTF-8 JSON objects over Pumpkin's
inter-plugin IPC. The provider's plugin name is its address.

    -> {"op": "set_placeholders", "text": "%player_name%", "viewer": "Steve"}
    <- {"ok": true, "text": "Steve", "unresolved": []}

    -> {"op": "register_expansion", "namespace": "ranks", "placeholders": ["prefix"]}
    <- {"ok": true, "namespace": "ranks", "placeholders": ["prefix"]}
    -> {"op": "set_placeholders", "text": "%ranks_prefix%", "viewer": "Steve"}
    -> {"op": "on_request", "namespace": "ranks", "id": "ranks_prefix",
        "name": "prefix", "viewer": "Steve", "argument": null}
    <- {"ok": true, "value": "Admin"}

`papi/protocol.py` lists the operations, `papi/client.py` is the Python client.
"""

from dataclasses import replace

from pumpkin_api import (
    Plugin,
    command,
    context,
    logging,
    metadata,
    permission,
    register_plugin,
    text,
)
from wit_world.imports import ipc

from papi import protocol, registry, tokens
from papi.defaults import BUILTIN_BY_ID, BUILTINS, ResolveContext

PROVIDER = protocol.PROVIDER
VERSION = "0.1.0"
USE_PERMISSION = f"{PROVIDER}:use"

# An expansion is asked synchronously, so a misbehaving one could otherwise ask
# for placeholders forever. A single set_placeholders call gets this many
# callbacks before the rest of its placeholders are left alone.
MAX_CALLBACKS_PER_RESOLVE = 16

MAX_SUGGESTIONS = 20

USAGE = (
    "Usage: /papi parse <text> to resolve placeholders, "
    "/papi list to show every placeholder, "
    "/papi expansions to list the registered expansions."
)


class PumpkinPapiPlugin(Plugin):
    def __init__(self):
        super().__init__()
        self._server = None
        self._registry = registry.Registry()

    def metadata(self) -> metadata.PluginMetadata:
        # `permissions` stays empty on purpose: a non empty list makes the host
        # prompt for it at load time. The node is registered in `on_load`.
        return metadata.PluginMetadata(
            name=PROVIDER,
            version=VERSION,
            authors=["Rennex"],
            description="Placeholder provider that other Pumpkin plugins resolve through.",
            dependencies=[],
            permissions=[],
        )

    def on_load(self, ctx: context.Context) -> None:
        self._server = ctx.get_server()
        ctx.register_permission(
            permission.Permission(
                node=USE_PERMISSION,
                description="Allows using /papi.",
                default=permission.PermissionDefault_Op(permission.PermissionLevel.TWO),
                children=[],
            )
        )
        self._register_commands(ctx)
        logging.log(
            logging.Level.INFO,
            f"{PROVIDER} {VERSION} ready with {len(BUILTINS)} built in placeholders",
        )

    def on_unload(self, ctx: context.Context) -> None:
        logging.log(logging.Level.INFO, f"{PROVIDER} unloaded")

    def _register_commands(self, ctx):
        """Wire the /papi tree by hand.

        `Plugin.register_command` never attaches `extra_nodes` to the root, and
        `then` consumes the child node, so every node is wired before it is
        attached and the root is registered last.
        """
        handler = self.register_command_handler(self._handle_command)
        suggest = self.register_command_suggestion_handler(self._suggest_command)

        text_argument = command.CommandNode.argument(
            "text", command.ArgumentType_String(command.StringType.GREEDY)
        )
        text_argument.execute_with_handler_id(handler)
        text_argument.suggest_with_handler_id(suggest)

        branches = []
        for name in ("parse", "list", "expansions"):
            literal = command.CommandNode.literal(name)
            literal.execute_with_handler_id(handler)
            literal.suggest_with_handler_id(suggest)
            if name == "parse":
                literal.then(text_argument)
            branches.append(literal)

        root = command.Command(["papi"], "Resolve and inspect placeholders")
        root.execute_with_handler_id(handler)
        root.suggest_with_handler_id(suggest)
        for literal in branches:
            root.then(literal)

        ctx.register_command(root, USE_PERMISSION)

    def _handle_command(self, sender, srv, args) -> int:
        requested = args.get_value("text")
        if not requested.value:
            self._send_usage(sender)
            return 0

        viewer = ""
        player = sender.as_player()
        if player is not None:
            viewer = player.get_name()

        resolved, unresolved = self._set_placeholders(requested.value, viewer, None)
        sender.send_message(text.TextComponent.text(resolved))
        if unresolved:
            sender.send_error(
                text.TextComponent.text(f"Unresolved: {', '.join(unresolved)}")
            )
        return len(unresolved)

    def _send_usage(self, sender):
        sender.send_message(text.TextComponent.text(USAGE))
        for builtin in BUILTINS:
            sender.send_message(
                text.TextComponent.text(f"%{builtin.id}%  {builtin.description}")
            )
        for namespace in self._registry.namespaces():
            expansion = self._registry.get(namespace)
            names = ", ".join(f"%{namespace}_{name}%" for name in sorted(expansion.names))
            sender.send_message(
                text.TextComponent.text(f"%{namespace}%  from {expansion.source}: {names}")
            )

    def _suggest_command(self, sender, srv, request) -> command.CommandSuggestions:
        """Suggest `%id%` for the token the cursor sits in."""
        typed = request.input[: request.cursor]
        start = typed.rfind("%") + 1
        prefix = typed[start:].lower()
        values = []
        for identifier in self._placeholder_ids():
            if not identifier.startswith(prefix):
                continue
            values.append(
                command.CommandSuggestion(
                    value=f"%{identifier}%",
                    tooltip=text.TextComponent.text(self._describe_id(identifier)),
                )
            )
            if len(values) == MAX_SUGGESTIONS:
                break
        return command.CommandSuggestions(
            start=len(typed) - len(prefix),
            length=len(prefix),
            values=values,
        )

    def _placeholder_ids(self):
        identifiers = [builtin.id for builtin in BUILTINS]
        for namespace in self._registry.namespaces():
            for name in sorted(self._registry.get(namespace).names):
                identifiers.append(f"{namespace}_{name}")
        return sorted(identifiers)

    def _describe_id(self, identifier):
        builtin = BUILTIN_BY_ID.get(identifier)
        if builtin is not None:
            return builtin.description
        expansion = self._registry.owner_of(identifier)
        if expansion is None:
            return ""
        return f"from {expansion.source}"

    def _set_placeholders(self, source, viewer, argument):
        """Substitute every placeholder in `source`; returns (text, unresolved)."""
        budget = [MAX_CALLBACKS_PER_RESOLVE]
        cache = {}
        base = ResolveContext(
            server=self._server,
            viewer=self._viewer(viewer),
            argument=argument,
            version=VERSION,
        )

        def resolve_one(identifier, token_argument):
            # Repeating a placeholder repeats its value, not the round trip: an
            # expansion asked twice for the same thing is only asked once.
            key = (identifier.lower(), token_argument)
            if key not in cache:
                cache[key] = self._resolve_one(base, identifier, token_argument, budget)
            return cache[key]

        return tokens.substitute(source, resolve_one)

    def _viewer(self, name):
        if not name:
            return None
        player = self._server.get_player_by_name(name)
        if player is None:
            return self._server.get_player_by_uuid(name)
        return player

    def _resolve_one(self, base, identifier, argument, budget):
        ctx = replace(base, argument=argument)
        try:
            builtin = BUILTIN_BY_ID.get(identifier.lower())
            if builtin is not None:
                return builtin.resolve(ctx)
            expansion = self._registry.owner_of(identifier)
            if expansion is None:
                return None
            if budget[0] <= 0:
                logging.log(
                    logging.Level.WARN,
                    f"callback budget spent, leaving {identifier} alone",
                )
                return None
            budget[0] -= 1
            return self._ask_expansion(expansion, identifier, ctx)
        except Exception as exc:
            # A permission gated getter or a misbehaving expansion must not take
            # the whole text down with it.
            logging.log(logging.Level.WARN, f"{identifier} failed to resolve: {exc}")
            return None

    def _ask_expansion(self, expansion, identifier, ctx):
        viewer = None
        if ctx.viewer is not None:
            viewer = ctx.viewer.get_name()
        namespace, _, name = identifier.lower().partition("_")
        request = protocol.encode(
            {
                "op": protocol.OP_ON_REQUEST,
                "namespace": namespace,
                "id": identifier,
                "name": name,
                "viewer": viewer,
                "argument": ctx.argument,
            }
        )
        try:
            reply = ipc.send_ipc_message(expansion.source, request)
        except Exception as exc:
            raise RuntimeError(f"{expansion.source} did not answer: {exc}") from exc
        if not isinstance(reply, bytes):
            reply = getattr(reply, "value", None)
        if not isinstance(reply, bytes):
            raise RuntimeError(f"{expansion.source} answered with no bytes")
        payload = protocol.decode_response(reply)
        if not payload["ok"]:
            raise RuntimeError(payload.get("error", "unknown error"))
        value = payload.get("value")
        return None if value is None else str(value)

    def handle_ipc_message(self, sender: str, message: bytes) -> bytes:
        try:
            request = protocol.decode(message)
        except protocol.ProtocolError as exc:
            return protocol.error(str(exc))

        op = request["op"]
        try:
            if op == protocol.OP_PING:
                return protocol.ok(
                    protocol=protocol.PROTOCOL_VERSION,
                    name=PROVIDER,
                    version=VERSION,
                    placeholders=len(BUILTINS),
                    expansions=len(self._registry),
                )
            if op == protocol.OP_GET_REGISTERED_PLACEHOLDERS:
                return protocol.ok(placeholders=self._get_registered_placeholders())
            if op == protocol.OP_REGISTER_EXPANSION:
                return self._register_expansion(sender, request)
            if op == protocol.OP_UNREGISTER_EXPANSION:
                return protocol.ok(namespaces=self._registry.drop_source(sender))
            if op == protocol.OP_SET_PLACEHOLDERS:
                return self._set_placeholders_request(request)
            if op == protocol.OP_GET_PLACEHOLDER_VALUE:
                return self._get_placeholder_value(request)
        except protocol.ProtocolError as exc:
            return protocol.error(str(exc))
        except Exception as exc:
            logging.log(logging.Level.ERROR, f"{op} from {sender} failed: {exc}")
            return protocol.error("internal error")
        return protocol.error(f"unknown op '{op}'")

    def _get_registered_placeholders(self):
        entries = [
            {
                "id": builtin.id,
                "description": builtin.description,
                "namespace": "papi",
                "source": PROVIDER,
            }
            for builtin in BUILTINS
        ]
        for namespace in self._registry.namespaces():
            expansion = self._registry.get(namespace)
            for name in sorted(expansion.names):
                entries.append(
                    {
                        "id": f"{namespace}_{name}",
                        "description": "",
                        "namespace": namespace,
                        "source": expansion.source,
                    }
                )
        return sorted(entries, key=lambda entry: entry["id"])

    def _register_expansion(self, sender, request):
        namespace = protocol.read_required_str(
            request, "namespace", protocol.MAX_ID_LENGTH
        )
        names = request.get("placeholders", [])
        if not isinstance(names, list):
            raise protocol.ProtocolError("'placeholders' must be a list")
        if len(names) > protocol.MAX_PLACEHOLDERS_PER_EXPANSION:
            raise protocol.ProtocolError(
                f"at most {protocol.MAX_PLACEHOLDERS_PER_EXPANSION} names at a time"
            )
        for name in names:
            if not isinstance(name, str):
                raise protocol.ProtocolError("each placeholder name must be a string")
        expansion = self._registry.add(namespace, sender, names)
        if len(self._registry) > protocol.MAX_NAMESPACES:
            raise protocol.ProtocolError(
                f"the provider holds at most {protocol.MAX_NAMESPACES} expansions"
            )
        return protocol.ok(
            namespace=expansion.namespace,
            placeholders=sorted(expansion.names),
        )

    def _set_placeholders_request(self, request):
        source = protocol.read_required_str(request, "text")
        viewer = protocol.read_str(request, "viewer", protocol.MAX_ID_LENGTH, "")
        argument = protocol.read_str(request, "argument", protocol.MAX_ID_LENGTH)
        resolved, unresolved = self._set_placeholders(source, viewer, argument)
        return protocol.ok(text=resolved, unresolved=unresolved)

    def _get_placeholder_value(self, request):
        identifier = protocol.read_required_str(
            request, "id", protocol.MAX_ID_LENGTH
        )
        viewer = protocol.read_str(request, "viewer", protocol.MAX_ID_LENGTH, "")
        argument = protocol.read_str(request, "argument", protocol.MAX_ID_LENGTH)
        base = ResolveContext(
            server=self._server,
            viewer=self._viewer(viewer),
            version=VERSION,
        )
        value = self._resolve_one(base, identifier, argument, [MAX_CALLBACKS_PER_RESOLVE])
        if value is None:
            return protocol.ok(known=False)
        return protocol.ok(known=True, value=value)
