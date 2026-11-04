"""Guardrails for the iOS account, official download, and native game gates.

These are source contracts, *not* a real iPhone/Microsoft login integration test.
"""
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[3]
SWIFT = ROOT / 'packaging/ios/GameLauncher/Sources'
NATIVE = ROOT / 'crates/horizon-client/src/game/ios_native_launcher.rs'


class MicrosoftGameIntegrationTests(unittest.TestCase):
    def test_oauth_device_flow_uses_keychain_and_entitlement(self):
        source = (SWIFT / 'MinecraftAccount.swift').read_text()
        for needle in ('/devicecode', '/token', 'user.auth.xboxlive.com',
                       'xsts.auth.xboxlive.com', 'authentication/login_with_xbox',
                       'entitlements/mcstore', 'minecraft/profile', 'kSecClassGenericPassword',
                       'kSecAttrAccessibleWhenUnlockedThisDeviceOnly', 'Task.checkCancellation()',
                       'verifyOwnership(token: saved.minecraftToken)'):
            self.assertIn(needle, source)
        self.assertNotIn('00000000441cc96b', source)
        self.assertNotIn('client_secret', source)

    def test_native_game_must_have_session_and_prepared_resources(self):
        source = NATIVE.read_text()
        self.assertIn('"launcher-session"', source)
        self.assertIn('AuthSession::parse(raw)', source)
        self.assertIn('Ok(()) if self.native_session.is_some()', source)
        self.assertIn('validate_prepared_resources', source)
        self.assertIn('auth::set_current(session)', source)
        self.assertIn('horizon_ios_launcher::start()', source)
        self.assertNotIn('log::info!("launcher-session', source)

    def test_official_mojang_download_is_separate_from_prepared_game_install(self):
        source = (SWIFT / 'OfficialMinecraftDownload.swift').read_text()
        view = (SWIFT / 'GameLauncherView.swift').read_text()
        for needle in ('version_manifest_v2.json', 'piston-meta.mojang.com',
                       'piston-data.mojang.com', 'resources.download.minecraft.net',
                       'libraries.minecraft.net', 'Insecure.SHA1', 'safeRelative',
                       'verifyForDownload', 'official-minecraft', 'Task.checkCancellation()'):
            self.assertIn(needle, source)
        self.assertIn('launcher-session', view)
        self.assertIn('account.sessionForLaunch()', view)
        self.assertIn('launcher-start', view)
        self.assertIn('JDK 25', view)
        self.assertNotIn('Bridge.send("launcher-start', source)

    def test_client_id_is_a_public_build_variable_not_a_secret(self):
        workflow = (ROOT / '.github/workflows/ios-simulator.yml').read_text()
        self.assertIn('vars.HORIZON_MSA_CLIENT_ID', workflow)
        self.assertIn('--microsoft-client-id', workflow)
        self.assertIn('HorizonMicrosoftClientID',
                      (ROOT / 'packaging/ios/GameInfo.plist').read_text())


if __name__ == '__main__':
    unittest.main()
