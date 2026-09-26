"""Stand-ins for the host objects the provider reads from."""

import enum

from papi.defaults import ResolveContext


class GameMode(enum.Enum):
    SURVIVAL = 0
    CREATIVE = 1


class Difficulty(enum.Enum):
    PEACEFUL = 0
    NORMAL = 1


class PermissionLevel(enum.Enum):
    ZERO = 0
    TWO = 2


class FakePlayer:
    def __init__(self, name="Steve", online=True):
        self.name = name
        self.online = online
        self.permissions = {"pumpkin-papi:use": True}

    def get_name(self):
        return self.name

    def get_id(self):
        return "069a79f4-44e9-4726-a5be-fca90e38aaf5"

    def get_ping(self):
        return 42

    def get_health(self):
        return 6.5

    def get_max_health(self):
        return 20.0

    def get_food_level(self):
        return 18

    def get_saturation(self):
        return 4.1992188

    def get_experience_level(self):
        return 7

    def get_experience_progress(self):
        return 0.25

    def get_gamemode(self):
        return GameMode.CREATIVE

    def get_permission_level(self):
        return PermissionLevel.TWO

    def get_position(self):
        return (1.23456, 64.0, -8.5)

    def get_yaw(self):
        return 90.04

    def get_pitch(self):
        return -12.0

    def get_locale(self):
        return "en_us"

    def get_ip(self):
        return "127.0.0.1"

    def get_team(self):
        return "red"

    def has_permission(self, node):
        return node in self.permissions


class FakeServer:
    def __init__(self, players=(), maximum=20):
        self.online = list(players)
        self.maximum = maximum

    def get_player_count(self):
        return len(self.online)

    def get_max_players(self):
        return self.maximum

    def get_motd(self):
        return "A Pumpkin server"

    def get_difficulty(self):
        return Difficulty.NORMAL

    def get_tps(self):
        return 19.98

    def get_mspt(self):
        return 4.26789

    def is_online_mode(self):
        return True

    def is_hardcore(self):
        return False

    def has_whitelist(self):
        return True

    def get_player_by_name(self, name):
        for player in self.online:
            if player.name == name and player.online:
                return player
        return None

    def get_player_by_uuid(self, uuid):
        for player in self.online:
            if player.get_id() == uuid and player.online:
                return player
        return None


def context(server=None, viewer=None, **kwargs):
    fields = {
        "server": server or FakeServer(),
        "viewer": viewer,
        "version": "0.1.0",
    }
    fields.update(kwargs)
    return ResolveContext(**fields)
