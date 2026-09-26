import unittest

from papi import tokens


def resolve_all(**values):
    def resolve(identifier, argument):
        return values.get(identifier)

    return resolve


class SubstituteTest(unittest.TestCase):
    def test_replaces_a_single_token(self):
        result, unresolved = tokens.substitute(
            "hello %player_name%", resolve_all(player_name="Steve")
        )
        self.assertEqual(result, "hello Steve")
        self.assertEqual(unresolved, [])

    def test_replaces_every_occurrence(self):
        result, _ = tokens.substitute(
            "%player_ping% ms and %player_ping% ms again",
            resolve_all(player_ping="42"),
        )
        self.assertEqual(result, "42 ms and 42 ms again")

    def test_keeps_unresolved_tokens_and_reports_them_once(self):
        result, unresolved = tokens.substitute(
            "%known% %missing% %missing%", resolve_all(known="yes")
        )
        self.assertEqual(result, "yes %missing% %missing%")
        self.assertEqual(unresolved, ["missing"])

    def test_doubled_percent_signs_are_not_an_escape(self):
        result, unresolved = tokens.substitute(
            "%%player_name%%", resolve_all(player_name="Steve")
        )
        self.assertEqual(result, "%Steve%")
        self.assertEqual(unresolved, [])

    def test_passes_the_argument_to_the_resolver(self):
        seen = []

        def resolve(identifier, argument):
            seen.append((identifier, argument))
            return "granted" if argument == "pumpkin-papi:use" else ""

        result, _ = tokens.substitute(
            "%player_has_permission:pumpkin-papi:use%", resolve
        )
        self.assertEqual(result, "granted")
        self.assertEqual(seen, [("player_has_permission", "pumpkin-papi:use")])

    def test_leaves_a_lone_percent_alone(self):
        for source in ("50% off", "%", "% ", "%player_name", "a % b %", "100%%"):
            with self.subTest(source=source):
                result, unresolved = tokens.substitute(
                    source, resolve_all(player_name="Steve")
                )
                self.assertEqual(result, source)
                self.assertEqual(unresolved, [])

    def test_text_without_a_token_is_untouched(self):
        result, unresolved = tokens.substitute("just words", resolve_all())
        self.assertEqual(result, "just words")
        self.assertEqual(unresolved, [])

    def test_a_resolver_returning_empty_string_still_counts_as_resolved(self):
        result, unresolved = tokens.substitute(
            "[%player_name%]", resolve_all(player_name="")
        )
        self.assertEqual(result, "[]")
        self.assertEqual(unresolved, [])

    def test_an_expansion_placeholder_resolves_like_any_other(self):
        result, _ = tokens.substitute(
            "%luckperms_prefix%Steve", resolve_all(luckperms_prefix="Admin ")
        )
        self.assertEqual(result, "Admin Steve")


class FindTest(unittest.TestCase):
    def test_finds_ids_and_arguments_in_order_without_duplicates(self):
        self.assertEqual(
            tokens.find("%a% %b:x% %a%"),
            [("a", None), ("b", "x")],
        )

    def test_ignores_text_that_is_not_a_token(self):
        self.assertEqual(tokens.find("100% of %"), [])


class IdentifierTest(unittest.TestCase):
    def test_accepts_normal_ids(self):
        self.assertTrue(tokens.is_valid_identifier("player_name"))
        self.assertTrue(tokens.is_valid_identifier("Ranks2"))

    def test_rejects_anything_a_token_cannot_carry(self):
        for bad in ("", "1abc", "a b", "a-b", "a:b", "%a%"):
            with self.subTest(bad=bad):
                self.assertFalse(tokens.is_valid_identifier(bad))


if __name__ == "__main__":
    unittest.main()
