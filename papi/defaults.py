"""The provider's placeholder table.

An expansion claims a namespace, and every placeholder it answers is
`<namespace>_<name>`, the same rule Java PlaceholderAPI uses. `player`,
`server` and `papi` are reserved for the built-ins below.

Only getters that hand back plain values belong here. A getter that returns a
host resource, such as `get_world` or `get_display_name`, is left out until the
WIT offers a way to release the handle again.
"""

from dataclasses import dataclass


@dataclass(frozen=True)
class ResolveContext:
    """Everything a resolver is allowed to look at."""

    server: object
    viewer: object = None
    argument: str = None
    version: str = ""


@dataclass(frozen=True)
class Builtin:
    id: str
    description: str
    resolve: object
    takes_arg: bool = False


def _number(value, places=2):
    text = f"{float(value):.{places}f}"
    if "." in text:
        text = text.rstrip("0").rstrip(".")
    if text in ("", "-", "-0"):
        return "0"
    return text


def _enum(value):
    return str(getattr(value, "name", value)).lower()


def _player(context, method):
    if context.viewer is None:
        return None
    return getattr(context.viewer, method)()


def _string(context, method):
    value = _player(context, method)
    return "" if value is None else str(value)


def _player_number(context, method):
    value = _player(context, method)
    return "" if value is None else _number(value)


def _coordinate(index):
    def resolve(context):
        position = _player(context, "get_position")
        if position is None:
            return ""
        return _number(position[index])

    return resolve


def _has_permission(context):
    if context.viewer is None or not context.argument:
        return ""
    return "true" if context.viewer.has_permission(context.argument) else "false"


def _gamemode(context):
    mode = _player(context, "get_gamemode")
    return "" if mode is None else _enum(mode)


BUILTINS = (
    Builtin("player_name", "Name of the player the text is rendered for",
            lambda c: _string(c, "get_name")),
    Builtin("player_uuid", "Unique id of the player",
            lambda c: _string(c, "get_id")),
    Builtin("player_ping", "Latency of the player in milliseconds",
            lambda c: _string(c, "get_ping")),
    Builtin("player_health", "Current health of the player",
            lambda c: _player_number(c, "get_health")),
    Builtin("player_max_health", "Maximum health of the player",
            lambda c: _player_number(c, "get_max_health")),
    Builtin("player_gamemode", "Game mode of the player", _gamemode),
    Builtin("player_x", "X coordinate of the player", _coordinate(0)),
    Builtin("player_y", "Y coordinate of the player", _coordinate(1)),
    Builtin("player_z", "Z coordinate of the player", _coordinate(2)),
    Builtin("player_yaw", "Yaw of the player",
            lambda c: _number(_player(c, "get_yaw") or 0.0, 1)),
    Builtin("player_pitch", "Pitch of the player",
            lambda c: _number(_player(c, "get_pitch") or 0.0, 1)),
    Builtin("player_ip", "IP address of the player",
            lambda c: _string(c, "get_ip")),
    Builtin("player_has_permission", "Whether the player holds a permission node",
            _has_permission, takes_arg=True),
    Builtin("server_online", "Players currently online",
            lambda c: str(c.server.get_player_count())),
    Builtin("server_max_players", "Player slots on the server",
            lambda c: str(c.server.get_max_players())),
    Builtin("server_tps", "Ticks per second the server is running at",
            lambda c: _number(c.server.get_tps())),
    Builtin("server_mspt", "Milliseconds per tick the server averages",
            lambda c: _number(c.server.get_mspt())),
    Builtin("papi_version", "Version of PumpkinPAPI", lambda c: c.version),
)

BUILTIN_BY_ID = {builtin.id: builtin for builtin in BUILTINS}
