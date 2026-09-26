import unittest

from papi import protocol


class DecodeTest(unittest.TestCase):
    def test_reads_a_request(self):
        request = protocol.decode(b'{"op":"resolve","text":"%player_name%"}')
        self.assertEqual(request["op"], "resolve")
        self.assertEqual(request["text"], "%player_name%")

    def test_rejects_a_missing_or_empty_op(self):
        for message in (b"{}", b'{"op":""}', b'{"op":7}', b'{"op":null}'):
            with self.subTest(message=message):
                with self.assertRaises(protocol.ProtocolError):
                    protocol.decode(message)

    def test_rejects_a_payload_that_is_not_an_object(self):
        for message in (b"[]", b'"ping"', b"7"):
            with self.subTest(message=message):
                with self.assertRaises(protocol.ProtocolError):
                    protocol.decode(message)

    def test_rejects_malformed_json(self):
        with self.assertRaises(protocol.ProtocolError):
            protocol.decode(b"{not json")

    def test_rejects_invalid_utf8(self):
        with self.assertRaises(protocol.ProtocolError):
            protocol.decode(b'{"op":"\xff"}')

    def test_rejects_an_oversized_message(self):
        oversized = b'{"op":"resolve","text":"' + b"a" * protocol.MAX_MESSAGE_BYTES + b'"}'
        with self.assertRaises(protocol.ProtocolError):
            protocol.decode(oversized)


class ResponseTest(unittest.TestCase):
    def test_ok_carries_its_fields(self):
        self.assertEqual(
            protocol.decode_response(protocol.ok(text="Steve")),
            {"ok": True, "text": "Steve"},
        )

    def test_error_carries_the_message(self):
        self.assertEqual(
            protocol.decode_response(protocol.error("nope")),
            {"ok": False, "error": "nope"},
        )

    def test_rejects_a_response_without_ok(self):
        with self.assertRaises(protocol.ProtocolError):
            protocol.decode_response(b'{"text":"Steve"}')

    def test_rejects_a_non_boolean_ok(self):
        with self.assertRaises(protocol.ProtocolError):
            protocol.decode_response(b'{"ok":"yes"}')


class ReadStrTest(unittest.TestCase):
    def test_returns_the_default_when_absent(self):
        self.assertIsNone(protocol.read_str({}, "viewer"))
        self.assertEqual(protocol.read_str({}, "viewer", default=""), "")

    def test_rejects_a_non_string(self):
        with self.assertRaises(protocol.ProtocolError):
            protocol.read_str({"viewer": 7}, "viewer")

    def test_rejects_a_string_over_the_limit(self):
        with self.assertRaises(protocol.ProtocolError):
            protocol.read_str({"id": "a" * 200}, "id", 128)

    def test_required_rejects_an_empty_string(self):
        with self.assertRaises(protocol.ProtocolError):
            protocol.read_required_str({"text": ""}, "text")
        with self.assertRaises(protocol.ProtocolError):
            protocol.read_required_str({}, "text")


if __name__ == "__main__":
    unittest.main()
