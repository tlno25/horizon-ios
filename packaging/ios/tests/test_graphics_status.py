import unittest

from verify_graphics_status import validate


class GraphicsStatusTests(unittest.TestCase):
    def test_metal_ready(self):
        self.assertEqual(validate({"state": 2, "error": "", "engine": 1, "player": "Player00001", "core_load": 0}), "Metal ready")

    def test_renderer_preview_state_must_be_valid(self):
        for status in ("3", None, True, -1, 5):
            with self.subTest(status=status), self.assertRaises(ValueError):
                validate({"state": 2, "error": "", "engine": 1, "player": "Player00001", "renderer_preview": status})

    def test_terrain_preview_status_is_optional_and_bounded(self):
        base = {"state": 2, "error": "", "engine": 1, "player": "Player00001"}
        self.assertEqual(validate(base), "Metal ready")
        for terrain in range(7):
            with self.subTest(valid=terrain):
                self.assertEqual(validate({**base, "terrain_preview": terrain}), "Metal ready")
        for terrain in ("4", None, True, -1, 7, []):
            with self.subTest(invalid=terrain), self.assertRaisesRegex(ValueError, "terrain_preview"):
                validate({**base, "terrain_preview": terrain})

    def test_core_load_status_must_be_numeric_when_reported(self):
        for status in ("2", None, True, 7, -1):
            with self.subTest(status=status), self.assertRaises(ValueError):
                validate({"state": 2, "error": "", "engine": 1, "player": "Player00001", "core_load": status})

    def test_engine_profile_must_be_initialized_before_gpu_smoke_passes(self):
        for engine, player in ((0, ""), (3, "Player00001"), (1, ""), (2, "")):
            with self.subTest(engine=engine, player=player), self.assertRaises(ValueError):
                validate({"state": 2, "error": "", "engine": engine, "player": player})

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
