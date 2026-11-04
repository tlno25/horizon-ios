"""Check the SwiftUI launcher can reach the real Rust importer and validated settings."""
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
HEADER = ROOT / 'packaging/ios/HorizonIOS/horizon_ios_bridge.h'
RUST = ROOT / 'crates/horizon-ios/src/lib.rs'
PROJECT = ROOT / 'packaging/ios/HorizonIOS.xcodeproj/project.pbxproj'


class LauncherBridgeTests(unittest.TestCase):
    def test_settings_have_ios_c_bridge_entrypoints(self):
        header, rust = HEADER.read_text(), RUST.read_text()
        for symbol in ('horizon_ios_launcher_settings_json',
                       'horizon_ios_launcher_set_setting',
                       'horizon_ios_launcher_set_player_name'):
            self.assertIn(symbol, header)
            self.assertIn('fn ' + symbol, rust)

    def test_download_verifies_https_sha256_before_native_import(self):
        source = (ROOT / 'packaging/ios/HorizonIOS/LauncherView.swift').read_text()
        self.assertIn('url.scheme?.lowercased() == "https"', source)
        self.assertIn('hash.utf8.count == 64', source)
        self.assertIn('digest.finalize()', source)
        self.assertIn('guard calculated == hash', source)
        self.assertIn('horizon_ios_import_resources(', source)
        self.assertIn('didWriteData', source)
        self.assertIn('func cancel()', source)
        self.assertIn('maximumBytes', source)
        self.assertIn('willPerformHTTPRedirection', source)

    def test_swiftui_launcher_is_in_xcode_build(self):
        source = ROOT / 'packaging/ios/HorizonIOS/LauncherView.swift'
        self.assertTrue(source.exists())
        project = PROJECT.read_text()
        self.assertIn('LauncherView.swift', project)
        self.assertIn('AA0000000000000000000019);', project)
        self.assertIn('LauncherView(', (ROOT / 'packaging/ios/HorizonIOS/HomeView.swift').read_text())
        self.assertIn('URLSessionDownloadDelegate', source.read_text())
        self.assertIn('SHA256', source.read_text())


if __name__ == '__main__':
    unittest.main()
