"""The iOS-native browser bridge must be checked before expensive macOS builds."""
import unittest
from pathlib import Path

WORKFLOW = Path(__file__).resolve().parents[3] / '.github/workflows/ios-quick-checks.yml'


class AppleOpenQuickCheckTests(unittest.TestCase):
    def test_changes_to_apple_browser_bridge_trigger_ubuntu_quick_checks(self):
        self.assertIn("'crates/horizon-apple-open/**'", WORKFLOW.read_text())

    def test_quick_checks_compile_uikit_bridge_for_ios_without_macos_sdk(self):
        workflow = WORKFLOW.read_text()
        self.assertIn('targets: aarch64-apple-ios', workflow)
        self.assertIn('cargo test --locked -p horizon-apple-open', workflow)
        self.assertIn('cargo check --locked -p horizon-apple-open --target aarch64-apple-ios', workflow)


if __name__ == '__main__':
    unittest.main()
