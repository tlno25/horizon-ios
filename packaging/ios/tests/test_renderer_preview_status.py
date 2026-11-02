"""The boot renderer is not the Minecraft game world."""
import unittest

from verify_renderer_preview import validate


class BootPreviewStatusTests(unittest.TestCase):
    def test_completed_horizon_renderer_frame(self):
        self.assertEqual(validate({
            'state': 2, 'error': '', 'engine': 1, 'core_load': 0,
            'renderer_preview': 3, 'smoke_mode': 1, 'player': 'Player00001'
        }), 'Horizon boot renderer presented a frame')

    def test_no_private_minecraft_data_is_required_for_boot_preview(self):
        self.assertIn('Horizon', validate({
            'state': 2, 'error': '', 'engine': 1, 'core_load': 0,
            'renderer_preview': 3, 'smoke_mode': 1, 'player': 'Alex'
        }))

    def test_rejects_probe_fallback_and_incomplete_preview(self):
        for preview in (0, 1, 2, 4, -1, '3', None, True):
            with self.subTest(preview=preview), self.assertRaises(ValueError):
                validate({
                    'state': 2, 'error': 'GPU failed', 'engine': 1,
                    'player': 'Player00001', 'renderer_preview': preview, 'smoke_mode': 1,
                    'core_load': 0
                })

    def test_cannot_report_success_without_app_acknowledging_diagnostic_mode(self):
        for ack in (0, None, True, "1", -1):
            with self.subTest(ack=ack), self.assertRaises(ValueError):
                validate({'state': 2, 'error': '', 'engine': 1,
                          'renderer_preview': 3, 'smoke_mode': ack,
                          'player': 'Player00001'})

    def test_rejects_invalid_metal_and_profile(self):
        for payload in ({'state': 3, 'engine': 1, 'renderer_preview': 3, 'smoke_mode': 1},
                        {'state': 2, 'engine': 0, 'renderer_preview': 3, 'smoke_mode': 1},
                        {'state': 2, 'engine': 1, 'renderer_preview': 3, 'smoke_mode': 1}):
            with self.subTest(payload=payload), self.assertRaises(ValueError):
                validate(payload)
