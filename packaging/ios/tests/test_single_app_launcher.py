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
        self.assertIn('sha256: manifest.sha256.lowercased()', source)
        self.assertIn('actual != expected', source)
        self.assertIn('length != expected', source)
        self.assertIn('launcher-import', source)
        self.assertIn('launcher-start', source)
        self.assertIn('startGame()', source)
        self.assertNotIn('fileImporter(', source)
        self.assertNotIn('フォルダを選択', source)

    def test_launcher_supports_portrait_then_returns_to_landscape(self):
        game = GAME.read_text()
        presenter = (ROOT / 'packaging/ios/GameLauncher/Sources/Present.swift').read_text()
        plist = (ROOT / 'packaging/ios/GameInfo.plist').read_text()
        self.assertIn('ValidOrientations::LandscapeAndPortrait', game)
        self.assertIn('ValidOrientations::Landscape', LAUNCH.read_text())
        self.assertIn('requestGeometryUpdate', presenter)
        self.assertIn('UIInterfaceOrientationPortrait', plist)

    def test_offline_player_name_can_be_edited_in_swiftui(self):
        native = LAUNCH.read_text()
        swift = (ROOT / 'packaging/ios/GameLauncher/Sources/GameLauncherView.swift').read_text()
        self.assertIn('"launcher-player-get"', native)
        self.assertIn('"launcher-player"', native)
        self.assertIn('TextField("オフラインプレイヤー名"', swift)
        self.assertIn('Bridge.send("launcher-player ', swift)

    def test_simulator_end_to_end_probe_checks_native_callbacks_not_just_presentation(self):
        presenter = (ROOT / 'packaging/ios/GameLauncher/Sources/Present.swift').read_text()
        self.assertIn('verifyNativeLauncherBridge', presenter)
        for command in ('launcher-state', 'launcher-settings', 'launcher-player-get',
                        'launcher-option', 'launcher-start'):
            self.assertIn('Bridge.send("' + command, presenter)
        self.assertIn('horizon-ios-launcher-bridge-ready', presenter)
        smoke = (ROOT / 'packaging/ios/tests/native_game_simulator_smoke.sh').read_text()
        self.assertIn('horizon-ios-launcher-bridge-ready', smoke)

    def test_simulator_smoke_imports_a_synthetic_zip_through_rust_before_claiming_ready(self):
        presenter = (ROOT / 'packaging/ios/GameLauncher/Sources/Present.swift').read_text()
        self.assertIn('Bridge.send("launcher-import horizon-download-CIRESOURCE.zip")', presenter)
        self.assertIn('horizon-ios-launcher-import-ready', presenter)
        self.assertIn('afterImport["ready"] as? Bool == true', presenter)
        self.assertIn('launcher-start', presenter)
        self.assertLess(presenter.index('Bridge.send("launcher-start")'),
                        presenter.index('Bridge.send("launcher-import horizon-download-CIRESOURCE.zip")'))

    def test_simulator_refuses_play_after_import_without_minecraft_session(self):
        presenter = (ROOT / 'packaging/ios/GameLauncher/Sources/Present.swift').read_text()
        smoke = (ROOT / 'packaging/ios/tests/native_game_simulator_smoke.sh').read_text()
        self.assertIn('let deniedWithoutAccount = await Bridge.send("launcher-start")', presenter)
        self.assertIn('Sign in with an owning Minecraft account first', presenter)
        self.assertIn('stateAfterDenial["phase"] as? String == "waiting"', presenter)
        self.assertIn('horizon-ios-launcher-auth-guard-ready', presenter)
        self.assertIn('horizon-ios-launcher-auth-guard-ready', smoke)
        self.assertIn('Microsoft ownership gate', smoke)

    def test_game_action_bundles_swiftui_framework(self):
        workflow = WORKFLOW.read_text()
        self.assertIn('HorizonLauncherKit.framework', workflow)
        self.assertIn('--launcher-framework', workflow)
        self.assertTrue((ROOT / 'packaging/ios/GameLauncher/project.yml').exists())

if __name__ == '__main__':
    unittest.main()
