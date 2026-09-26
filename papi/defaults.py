"""The provider's placeholder table.

An expansion claims a namespace, and every placeholder it answers is
`<namespace>_<name>`, the same rule Java PlaceholderAPI uses. `player`,
`server` and `papi` are reserved for the built-ins below.

Only getters that hand back plain values belong here. A getter that returns a
host resource, such as `get_world` or `get_display_name`, is left out until the
WIT offers a way to release the handle again.
"""

from dataclasses import dataclass

from .protocol import PROTOCOL_VERSION


@dataclass(frozen=True)
class ResolveContext:
    """Everything a resolver is allowed to look at."""

    server: object
    viewer: object = None
    argument: str = None
    version: str = ""
    expansions: int = 0


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


def _bool(value):
    return "true" if value else "false"


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


def _online_percent(context):
    maximum = context.server.get_max_players()
    if not maximum:
        return "0"
    return _number(context.server.get_player_count() / maximum * 100, 1)


def _team(context):
    team = _player(context, "get_team")
    return "" if team is None else str(team)


def _experience_progress(context):
    progress = _player(context, "get_experience_progress")
    return "" if progress is None else _number(progress * 100, 1)


def _gamemode(context):
    mode = _player(context, "get_gamemode")
    return "" if mode is None else _enum(mode)


def _permission_level(context):
    level = _player(context, "get_permission_level")
    return "" if level is None else _enum(level)


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
    Builtin("player_food", "Food level of the player, 0 to 20",
            lambda c: _string(c, "get_food_level")),
    Builtin("player_saturation", "Saturation of the player",
            lambda c: _player_number(c, "get_saturation")),
    Builtin("player_experience_level", "Experience level of the player",
            lambda c: _string(c, "get_experience_level")),
    Builtin("player_experience_progress", "Progress to the next level in percent",
            _experience_progress),
    Builtin("player_gamemode", "Game mode of the player", _gamemode),
    Builtin("player_permission_level", "Permission level of the player",
            _permission_level),
    Builtin("player_x", "X coordinate of the player", _coordinate(0)),
    Builtin("player_y", "Y coordinate of the player", _coordinate(1)),
    Builtin("player_z", "Z coordinate of the player", _coordinate(2)),
    Builtin("player_yaw", "Yaw of the player",
            lambda c: _number(_player(c, "get_yaw") or 0.0, 1)),
    Builtin("player_pitch", "Pitch of the player",
            lambda c: _number(_player(c, "get_pitch") or 0.0, 1)),
    Builtin("player_locale", "Locale of the player's client",
            lambda c: _string(c, "get_locale")),
    Builtin("player_ip", "IP address of the player",
            lambda c: _string(c, "get_ip")),
    Builtin("player_team", "Scoreboard team of the player", _team),
    Builtin("player_has_permission", "Whether the player holds a permission node",
            _has_permission, takes_arg=True),
    Builtin("server_online", "Players currently online",
            lambda c: str(c.server.get_player_count())),
    Builtin("server_max_players", "Player slots on the server",
            lambda c: str(c.server.get_max_players())),
    Builtin("server_online_percent", "How full the server is in percent",
            _online_percent),
    Builtin("server_motd", "Message of the day", lambda c: c.server.get_motd()),
    Builtin("server_difficulty", "Difficulty of the server",
            lambda c: _enum(c.server.get_difficulty())),
    Builtin("server_tps", "Ticks per second the server is running at",
            lambda c: _number(c.server.get_tps())),
    Builtin("server_mspt", "Milliseconds per tick the server averages",
            lambda c: _number(c.server.get_mspt())),
    Builtin("server_online_mode", "Whether the server authenticates with Mojang",
            lambda c: _bool(c.server.is_online_mode())),
    Builtin("server_hardcore", "Whether the server is hardcore",
            lambda c: _bool(c.server.is_hardcore())),
    Builtin("server_whitelist", "Whether the server has a whitelist",
            lambda c: _bool(c.server.has_whitelist())),
    Builtin("papi_version", "Version of PumpkinPAPI", lambda c: c.version),
    Builtin("papi_protocol", "Protocol version PumpkinPAPI speaks",
            lambda c: str(PROTOCOL_VERSION)),
    Builtin("papi_expansions", "Expansions registered with PumpkinPAPI",
            lambda c: str(c.expansions)),
)

BUILTIN_BY_ID = {builtin.id: builtin for builtin in BUILTINS}
