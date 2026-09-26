import unittest

from papi import tokens
from papi.defaults import BUILTINS, BUILTIN_BY_ID

from .fakes import FakePlayer, FakeServer, context


class BuiltinTest(unittest.TestCase):
    def test_every_id_is_a_token_the_scanner_accepts(self):
        for builtin in BUILTINS:
            with self.subTest(builtin=builtin.id):
                self.assertTrue(tokens.is_valid_identifier(builtin.id))
                self.assertIsNotNone(BUILTIN_BY_ID[builtin.id])

    def test_ids_are_unique(self):
        ids = [builtin.id for builtin in BUILTINS]
        self.assertEqual(sorted(ids), sorted(set(ids)))

    def test_the_index_covers_the_table(self):
        self.assertEqual(len(BUILTIN_BY_ID), len(BUILTINS))

    def test_player_values(self):
        ctx = context(viewer=FakePlayer())
        self.assertEqual(BUILTIN_BY_ID["player_name"].resolve(ctx), "Steve")
        self.assertEqual(BUILTIN_BY_ID["player_ping"].resolve(ctx), "42")
        self.assertEqual(BUILTIN_BY_ID["player_gamemode"].resolve(ctx), "creative")
        self.assertEqual(BUILTIN_BY_ID["player_uuid"].resolve(ctx), FakePlayer().get_id())
        self.assertEqual(BUILTIN_BY_ID["player_ip"].resolve(ctx), "127.0.0.1")
        self.assertEqual(BUILTIN_BY_ID["player_locale"].resolve(ctx), "en_us")

    def test_player_numbers_are_trimmed(self):
        ctx = context(viewer=FakePlayer())
        self.assertEqual(BUILTIN_BY_ID["player_max_health"].resolve(ctx), "20")
        self.assertEqual(BUILTIN_BY_ID["player_health"].resolve(ctx), "6.5")
        self.assertEqual(BUILTIN_BY_ID["player_y"].resolve(ctx), "64")
        self.assertEqual(BUILTIN_BY_ID["player_x"].resolve(ctx), "1.23")
        self.assertEqual(BUILTIN_BY_ID["player_z"].resolve(ctx), "-8.5")
        self.assertEqual(BUILTIN_BY_ID["player_pitch"].resolve(ctx), "-12")
        self.assertEqual(BUILTIN_BY_ID["player_yaw"].resolve(ctx), "90")

    def test_player_placeholders_are_empty_without_a_viewer(self):
        for identifier in ("player_name", "player_ping", "player_x", "player_uuid"):
            with self.subTest(identifier=identifier):
                self.assertEqual(BUILTIN_BY_ID[identifier].resolve(context()), "")

    def test_server_values(self):
        ctx = context(server=FakeServer(players=[FakePlayer()] * 5))
        self.assertEqual(BUILTIN_BY_ID["server_online"].resolve(ctx), "5")
        self.assertEqual(BUILTIN_BY_ID["server_max_players"].resolve(ctx), "20")
        self.assertEqual(BUILTIN_BY_ID["server_tps"].resolve(ctx), "19.98")
        self.assertEqual(BUILTIN_BY_ID["server_mspt"].resolve(ctx), "4.27")

    def test_has_permission_reads_the_argument(self):
        granted = context(viewer=FakePlayer(), argument="pumpkin-papi:use")
        denied = context(viewer=FakePlayer(), argument="some.other:node")
        without_argument = context(viewer=FakePlayer())
        self.assertEqual(BUILTIN_BY_ID["player_has_permission"].resolve(granted), "true")
        self.assertEqual(BUILTIN_BY_ID["player_has_permission"].resolve(denied), "false")
        self.assertEqual(
            BUILTIN_BY_ID["player_has_permission"].resolve(without_argument), ""
        )

    def test_papi_placeholders_report_the_provider_itself(self):
        ctx = context(expansions=2)
        self.assertEqual(BUILTIN_BY_ID["papi_version"].resolve(ctx), "0.1.0")
        self.assertEqual(BUILTIN_BY_ID["papi_protocol"].resolve(ctx), "0")
        self.assertEqual(BUILTIN_BY_ID["papi_expansions"].resolve(ctx), "2")


if __name__ == "__main__":
    unittest.main()
