"""A native-game IPA must contain a SwiftUI launcher and wait for it before booting."""
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[3]
GAME = (ROOT / 'crates/horizon-client/src/game.rs')
LAUNCH = (ROOT / 'crates/horizon-client/src/game/ios_native_launcher.rs')
WORKFLOW = ROOT / '.github/workflows/ios-simulator.yml'

class SingleAppLauncherTests(unittest.TestCase):
    def test_game_loading_waits_for_swiftui_and_checks_resources_before_play(self):
        game = GAME.read_text()
        self.assertIn('horizon_ios_launcher::present()', game)
        self.assertIn('horizon_ios_launcher::waiting()', game)
        self.assertIn('poll_native_launcher()', game)
        self.assertTrue(LAUNCH.exists())
        source = LAUNCH.read_text()
        self.assertIn('"launcher-start"', source)
        self.assertIn('"launcher-import"', source)
        self.assertIn('import_archive_checked', source)
        self.assertIn('validate_prepared_resources', source)

    def test_one_tap_download_and_same_sandbox_handoff(self):
        source = (ROOT / 'packaging/ios/GameLauncher/Sources/GameLauncherView.swift').read_text()
        self.assertIn('HorizonResourcesManifestURL', source)
        self.assertIn('ゲームファイルをダウンロード', source)
        self.assertIn('manifest.validate(expectedVersion: version)', source)
        self.assertIn('actual == manifest.sha256.lowercased()', source)
        self.assertIn('launcher-import', source)
        self.assertIn('launcher-start', source)
        self.assertIn('startGame()', source)
        self.assertNotIn('fileImporter(', source)
        self.assertNotIn('フォルダを選択', source)

    def test_game_action_bundles_swiftui_framework(self):
        workflow = WORKFLOW.read_text()
        self.assertIn('HorizonLauncherKit.framework', workflow)
        self.assertIn('--launcher-framework', workflow)
        self.assertTrue((ROOT / 'packaging/ios/GameLauncher/project.yml').exists())

if __name__ == '__main__':
    unittest.main()
