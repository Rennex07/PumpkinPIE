import unittest

from papi import registry
from papi.protocol import ProtocolError


class NamespaceTest(unittest.TestCase):
    def test_accepts_a_lowercase_namespace(self):
        self.assertTrue(registry.is_valid_namespace("luckperms"))

    def test_rejects_a_malformed_or_numeric_namespace(self):
        for bad in ("", "2ranks", "my-ranks", "my ranks", "ranks!"):
            with self.subTest(bad=bad):
                self.assertFalse(registry.is_valid_namespace(bad))

    def test_accepts_a_lowercase_name(self):
        self.assertTrue(registry.is_valid_name("prefix"))

    def test_rejects_a_malformed_name(self):
        for bad in ("", "pre-fix", "pre fix"):
            with self.subTest(bad=bad):
                self.assertFalse(registry.is_valid_name(bad))


class RegistryTest(unittest.TestCase):
    def setUp(self):
        self.registry = registry.Registry()

    def test_starts_empty(self):
        self.assertEqual(len(self.registry), 0)
        self.assertEqual(self.registry.namespaces(), [])

    def test_add_normalises_the_namespace_and_names(self):
        expansion = self.registry.add("Ranks", "RanksPlugin", ["Prefix", "SUFFIX"])
        self.assertEqual(expansion.namespace, "ranks")
        self.assertEqual(expansion.names, frozenset({"prefix", "suffix"}))
        self.assertEqual(self.registry.namespaces(), ["ranks"])

    def test_a_reserved_namespace_is_refused(self):
        for namespace in ("player", "server", "papi"):
            with self.subTest(namespace=namespace):
                with self.assertRaises(ProtocolError):
                    self.registry.add(namespace, "Ranks")

    def test_a_name_may_repeat_a_reserved_word_without_colliding(self):
        # Built ins all live in the reserved namespaces, so `ranks_player_name`
        # cannot shadow one and is allowed.
        expansion = self.registry.add("ranks", "Ranks", ["player_name"])
        self.assertEqual(expansion.names, frozenset({"player_name"}))

    def test_a_malformed_namespace_or_name_is_refused(self):
        with self.assertRaises(ProtocolError):
            self.registry.add("my ranks", "Ranks")
        with self.assertRaises(ProtocolError):
            self.registry.add("ranks", "Ranks", ["pre-fix"])

    def test_registering_the_same_namespace_twice_replaces_the_names(self):
        self.registry.add("ranks", "Ranks", ["prefix", "suffix"])
        expansion = self.registry.add("ranks", "Ranks", ["prefix"])
        self.assertEqual(len(self.registry), 1)
        self.assertEqual(expansion.names, frozenset({"prefix"}))

    def test_an_empty_name_list_means_the_expansion_answers_anything(self):
        expansion = self.registry.add("ranks", "Ranks")
        self.assertEqual(expansion.names, frozenset())
        self.assertIsNotNone(self.registry.owner_of("ranks_whatever"))

    def test_owner_of_splits_on_the_first_underscore(self):
        self.registry.add("ranks", "Ranks", ["prefix"])
        self.assertIsNotNone(self.registry.owner_of("ranks_prefix"))
        self.assertIsNotNone(self.registry.owner_of("RANKS_prefix"))
        self.assertIsNone(self.registry.owner_of("ranks"))
        self.assertIsNone(self.registry.owner_of("other_prefix"))

    def test_drop_source_only_removes_that_contributor(self):
        self.registry.add("ranks", "Ranks", ["prefix"])
        self.registry.add("zones", "Zones", ["name"])
        self.registry.add("quests", "Ranks", ["stage"])

        self.assertEqual(self.registry.drop_source("Ranks"), ["quests", "ranks"])
        self.assertEqual(self.registry.namespaces(), ["zones"])

    def test_drop_source_of_an_unknown_contributor_removes_nothing(self):
        self.registry.add("ranks", "Ranks", ["prefix"])
        self.assertEqual(self.registry.drop_source("Other"), [])
        self.assertEqual(len(self.registry), 1)

    def test_describe_is_sorted_and_carries_the_owner(self):
        self.registry.add("zones", "Zones", ["name"])
        self.registry.add("ranks", "Ranks", ["suffix", "prefix"])
        self.assertEqual(
            self.registry.describe(),
            [
                {
                    "namespace": "ranks",
                    "source": "Ranks",
                    "placeholders": ["prefix", "suffix"],
                },
                {"namespace": "zones", "source": "Zones", "placeholders": ["name"]},
            ],
        )


if __name__ == "__main__":
    unittest.main()
