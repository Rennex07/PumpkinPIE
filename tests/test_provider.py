"""End to end tests of the provider's IPC surface, with the host faked out.

`main` imports the Pumpkin SDK, so these need `pumpkin-api-py` installed the
same way the build does. Everything below the WIT layer, protocol decoding, the
registry and resolution, is the real code.
"""

import json
import unittest
from unittest import mock

import main
from main import PumpkinPapiPlugin

from .fakes import FakePlayer, FakeServer


def call(plugin, sender, payload):
    return json.loads(plugin.handle_ipc_message(sender, json.dumps(payload).encode()))


class ProviderTest(unittest.TestCase):
    def setUp(self):
        # The SDK's logging import is a stub that raises outside a component.
        patcher = mock.patch.object(main.logging, "log")
        self.logs = patcher.start()
        self.addCleanup(patcher.stop)
        self.plugin = PumpkinPapiPlugin()
        self.plugin._server = FakeServer(players=[FakePlayer("Steve")])

    def register(self, sender="Ranks", namespace="ranks", names=("prefix",)):
        return call(
            self.plugin,
            sender,
            {
                "op": "register_expansion",
                "namespace": namespace,
                "placeholders": list(names),
            },
        )

    def test_ping_reports_the_provider(self):
        reply = call(self.plugin, "Consumer", {"op": "ping"})
        self.assertTrue(reply["ok"])
        self.assertEqual(reply["name"], "PumpkinPAPI")
        self.assertEqual(reply["protocol"], 0)
        self.assertGreater(reply["placeholders"], 0)
        self.assertEqual(reply["expansions"], 0)

    def test_registered_placeholders_start_as_the_built_ins(self):
        reply = call(self.plugin, "Consumer", {"op": "get_registered_placeholders"})
        identifiers = [entry["id"] for entry in reply["placeholders"]]
        self.assertIn("player_name", identifiers)
        self.assertIn("server_online", identifiers)
        self.assertEqual(identifiers, sorted(identifiers))

    def test_set_placeholders_uses_the_viewer(self):
        reply = call(
            self.plugin,
            "Consumer",
            {
                "op": "set_placeholders",
                "text": "%player_name% is %player_ping% ms",
                "viewer": "Steve",
            },
        )
        self.assertEqual(reply["text"], "Steve is 42 ms")
        self.assertEqual(reply["unresolved"], [])

    def test_set_placeholders_falls_back_to_the_uuid(self):
        uuid = "069a79f4-44e9-4726-a5be-fca90e38aaf5"
        reply = call(
            self.plugin,
            "Consumer",
            {"op": "set_placeholders", "text": "%player_name%", "viewer": uuid},
        )
        self.assertEqual(reply["text"], "Steve")

    def test_set_placeholders_without_a_viewer_leaves_player_placeholders_empty(self):
        reply = call(
            self.plugin, "Consumer", {"op": "set_placeholders", "text": "[%player_name%]"}
        )
        self.assertEqual(reply["text"], "[]")

    def test_set_placeholders_reports_what_it_could_not_resolve(self):
        reply = call(
            self.plugin,
            "Consumer",
            {
                "op": "set_placeholders",
                "text": "%server_online% %nope% %player_name%",
                "viewer": "Steve",
            },
        )
        self.assertEqual(reply["text"], "1 %nope% Steve")
        self.assertEqual(reply["unresolved"], ["nope"])

    def test_get_placeholder_value_knows_a_single_id(self):
        reply = call(
            self.plugin,
            "Consumer",
            {"op": "get_placeholder_value", "id": "server_online", "viewer": "Steve"},
        )
        self.assertEqual(reply, {"ok": True, "known": True, "value": "1"})

    def test_get_placeholder_value_says_so_for_an_unknown_id(self):
        reply = call(self.plugin, "Consumer", {"op": "get_placeholder_value", "id": "nope"})
        self.assertEqual(reply, {"ok": True, "known": False})

    def test_an_expansion_registers_a_namespace_and_its_names(self):
        reply = self.register()
        self.assertEqual(reply["namespace"], "ranks")
        self.assertEqual(reply["placeholders"], ["prefix"])

        entries = call(self.plugin, "Ranks", {"op": "get_registered_placeholders"})
        entry = next(e for e in entries["placeholders"] if e["id"] == "ranks_prefix")
        self.assertEqual(entry["namespace"], "ranks")
        self.assertEqual(entry["source"], "Ranks")

    def test_registering_without_names_means_answering_anything(self):
        reply = call(
            self.plugin,
            "Ranks",
            {"op": "register_expansion", "namespace": "ranks", "placeholders": []},
        )
        self.assertEqual(reply["placeholders"], [])
        self.assertEqual(call(self.plugin, "Ranks", {"op": "ping"})["expansions"], 1)

    def test_a_reserved_namespace_is_refused(self):
        for namespace in ("player", "server", "papi"):
            with self.subTest(namespace=namespace):
                reply = self.register(namespace=namespace)
                self.assertFalse(reply["ok"])
                self.assertIn("reserved", reply["error"])

    def test_register_rejects_a_bad_request(self):
        for payload in (
            {"op": "register_expansion"},
            {"op": "register_expansion", "namespace": "ranks", "placeholders": "x"},
            {"op": "register_expansion", "namespace": "ranks", "placeholders": [7]},
            {"op": "register_expansion", "namespace": "my ranks"},
            {"op": "register_expansion", "namespace": "ranks", "placeholders": ["pre-fix"]},
        ):
            with self.subTest(payload=payload):
                self.assertFalse(call(self.plugin, "Ranks", payload)["ok"])

    def test_unregister_only_touches_the_calling_plugin(self):
        self.register(sender="Ranks")
        self.register(sender="Zones", namespace="zones", names=("name",))

        reply = call(self.plugin, "Ranks", {"op": "unregister_expansion"})

        self.assertEqual(reply["namespaces"], ["ranks"])
        self.assertEqual(call(self.plugin, "Zones", {"op": "ping"})["expansions"], 1)
        self.assertEqual(call(self.plugin, "Ranks", {"op": "ping"})["expansions"], 1)

    def test_an_expansion_owns_the_first_segment_of_an_id(self):
        self.register()
        # Outside a server the IPC import is a stub that raises, which is the
        # same path an expansion that failed to load takes.
        reply = call(
            self.plugin,
            "Consumer",
            {
                "op": "set_placeholders",
                "text": "hi %ranks_prefix%",
                "viewer": "Steve",
            },
        )
        self.assertEqual(reply["text"], "hi %ranks_prefix%")
        self.assertEqual(reply["unresolved"], ["ranks_prefix"])

    def test_the_callback_budget_is_shared_across_one_resolve(self):
        self.register(names=[f"name{index}" for index in range(20)])
        text = " ".join(f"%ranks_name{index}%" for index in range(20))
        reply = call(
            self.plugin,
            "Consumer",
            {"op": "set_placeholders", "text": text, "viewer": "Steve"},
        )
        # Every one of them stays in the text, but only the first
        # MAX_CALLBACKS_PER_RESOLVE reach the expansion.
        self.assertEqual(len(reply["unresolved"]), 20)
        spent = [
            logged
            for logged in self.logs.call_args_list
            if "budget" in str(logged)
        ]
        self.assertEqual(len(spent), 20 - main.MAX_CALLBACKS_PER_RESOLVE)

    def test_a_repeated_placeholder_costs_one_callback(self):
        self.register(names=("prefix",))
        text = " ".join(["%ranks_prefix%"] * 40)
        reply = call(
            self.plugin,
            "Consumer",
            {"op": "set_placeholders", "text": text, "viewer": "Steve"},
        )
        self.assertEqual(len(reply["unresolved"]), 1)
        self.assertEqual(
            [logged for logged in self.logs.call_args_list if "budget" in str(logged)],
            [],
        )

    def test_each_resolve_gets_a_fresh_callback_budget(self):
        self.register(names=("prefix",))
        for _ in range(main.MAX_CALLBACKS_PER_RESOLVE + 2):
            call(
                self.plugin,
                "Consumer",
                {"op": "set_placeholders", "text": "%ranks_prefix%"},
            )
        reply = call(
            self.plugin,
            "Consumer",
            {"op": "set_placeholders", "text": "%ranks_prefix%"},
        )
        self.assertEqual(reply["unresolved"], ["ranks_prefix"])

    def test_a_failing_getter_does_not_break_the_whole_text(self):
        class Exploding:
            def get_name(self):
                raise PermissionError("no permission for you")

            def __getattr__(self, name):
                raise PermissionError("no permission for you")

        self.plugin._server = FakeServer(players=[])
        self.plugin._server.online = [Exploding()]
        self.plugin._server.get_player_by_name = lambda name: self.plugin._server.online[0]
        reply = call(
            self.plugin,
            "Consumer",
            {
                "op": "set_placeholders",
                "text": "%player_name% %server_online%",
                "viewer": "Steve",
            },
        )
        self.assertEqual(reply["text"], "%player_name% 1")
        self.assertEqual(reply["unresolved"], ["player_name"])

    def test_set_placeholders_requires_text(self):
        self.assertFalse(call(self.plugin, "Consumer", {"op": "set_placeholders"})["ok"])

    def test_set_placeholders_rejects_a_non_string_text(self):
        reply = call(self.plugin, "Consumer", {"op": "set_placeholders", "text": 7})
        self.assertFalse(reply["ok"])

    def test_a_malformed_message_is_rejected_without_raising(self):
        reply = json.loads(self.plugin.handle_ipc_message("Consumer", b"{oops"))
        self.assertEqual(reply, {"ok": False, "error": "message is not valid JSON"})

    def test_an_unknown_op_is_reported(self):
        reply = call(self.plugin, "Consumer", {"op": "explode"})
        self.assertFalse(reply["ok"])
        self.assertIn("explode", reply["error"])


if __name__ == "__main__":
    unittest.main()
