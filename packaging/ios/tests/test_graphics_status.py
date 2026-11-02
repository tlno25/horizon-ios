import unittest

from verify_graphics_status import validate


class GraphicsStatusTests(unittest.TestCase):
    def test_metal_ready(self):
        self.assertEqual(validate({"state": 2, "error": ""}), "Metal ready")

    def test_metal_failed_with_native_message(self):
        with self.assertRaisesRegex(ValueError, "Metal initialization: No compatible"):
            validate({"state": 3, "error": "Metal initialization: No compatible adapter"})

    def test_status_never_reported_as_ready(self):
        for code in (0, 1, 4, -1, "2", None):
            with self.subTest(code=code), self.assertRaises(ValueError):
                validate({"state": code, "error": ""})

    def test_bad_payloads(self):
        for value in (None, [], {}, {"state": 2}, {"state": True, "error": ""}):
            with self.subTest(value=value), self.assertRaises(ValueError):
                validate(value)


if __name__ == "__main__":
    unittest.main()
