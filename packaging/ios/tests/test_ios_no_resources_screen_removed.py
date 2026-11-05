"""iOS must never render the old misleading no-game-files screen."""
import importlib.util
from pathlib import Path
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[3]
CHECK = Path(__file__).with_name('assert_no_legacy_ios_screen.py')
spec = importlib.util.spec_from_file_location('assert_no_legacy_ios_screen', CHECK)
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


class IOSLegacyScreenRemovalTests(unittest.TestCase):
    def test_entire_legacy_screen_modules_excluded_from_ios(self):
        game = (ROOT / 'crates/horizon-client/src/game.rs').read_text()
        library = (ROOT / 'crates/horizon-client/src/lib.rs').read_text()
        self.assertIn('#[cfg(not(target_os = "ios"))]\nmod no_resources;', game)
        self.assertIn('#[cfg(not(target_os = "ios"))]\npub mod no_resources;', library)
        self.assertIn('horizon_ios_launcher::bundled()', game)
        self.assertIn('failed to present its bundled SwiftUI launcher', game)
        self.assertIn('return no_resources::show', game)  # Retained for desktop only.
        self.assertIn('#[cfg(not(target_os = "ios"))]\n        if !options.offscreen', game)

    def test_ios_no_longer_has_first_run_screen_copy(self):
        screen = (ROOT / 'crates/horizon-client/src/no_resources.rs').read_text()
        for marker in ('Horizon on iPhone needs game files', 'IOS_TITLE',
                       'IOS_PARAGRAPHS', 'Horizon launcher is missing',
                       'layout_for_platform'):
            self.assertNotIn(marker, screen)

    def test_missing_launcher_cannot_automatically_start_the_renderer(self):
        launcher = (ROOT / 'crates/horizon-ios-launcher/src/lib.rs').read_text()
        self.assertIn('#[cfg(not(target_os = "ios"))]\n        start();', launcher)
        self.assertIn('cannot load or present the bundled SwiftUI launcher', launcher)

    def test_banned_screen_text_is_detected_in_binaries(self):
        with tempfile.TemporaryDirectory() as folder:
            binary = Path(folder) / 'Game'
            binary.write_bytes(b'fake MachO data')
            module.assert_no_legacy_screen(binary)
            for marker in module.BANNED:
                binary.write_bytes(b'fake MachO ' + marker + b' trailing bytes')
                with self.subTest(marker=marker), self.assertRaisesRegex(ValueError, 'obsolete'):
                    module.assert_no_legacy_screen(binary)

    def test_apple_builds_reject_legacy_screen_binaries_before_packaging(self):
        workflow = (ROOT / '.github/workflows/ios-simulator.yml').read_text()
        self.assertGreaterEqual(workflow.count('assert_no_legacy_ios_screen.py'), 2)


if __name__ == '__main__':
    unittest.main()
