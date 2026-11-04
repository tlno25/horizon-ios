"""Confirm unsupported iOS narration stays disabled without a startup error."""
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]


class IOSNarratorPolicyTests(unittest.TestCase):
    def test_unsupported_ios_narrator_does_not_spawn_failure_logging_thread(self):
        source = (ROOT / 'crates/horizon-narrator/src/lib.rs').read_text()
        self.assertIn('#[cfg(not(target_os = "ios"))]', source)
        self.assertIn('speech::Thread::spawn', source)
        self.assertIn('iOS does not have a speech backend yet.', source)
        self.assertIn('mod speech;', source)

    def test_ios_quick_checks_avoid_narrator_regressions(self):
        workflow = (ROOT / '.github/workflows/ios-quick-checks.yml').read_text()
        self.assertEqual(workflow.count("'crates/horizon-narrator/**'"), 2)
        self.assertIn('cargo test --locked -p horizon-narrator --lib', workflow)
        self.assertIn('cargo check --locked -p horizon-narrator --target aarch64-apple-ios', workflow)


if __name__ == '__main__':
    unittest.main()
