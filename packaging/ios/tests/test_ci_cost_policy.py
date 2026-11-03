"""Prevent expensive macOS CI from being re-enabled on every iOS commit."""
from pathlib import Path
import re
import unittest

ROOT = Path(__file__).resolve().parents[3]
WORKFLOWS = ROOT / '.github/workflows'


def events(text):
    # Find only the top-level 'on:' mapping, not job or step properties.
    section = re.search(r'(?m)^on:\s*\n(?P<body>(?:^[ \t]+.*\n|^\s*\n)*)', text)
    return set(re.findall(r'^  ([a-z_]+):', section.group('body'), re.M)) if section else set()


class CIWorkflowsDoNotRunMacOSAutomatically(unittest.TestCase):
    def test_all_ios_macos_workflows_are_manual_only(self):
        for path in sorted(WORKFLOWS.glob('ios-*.yml')):
            text = path.read_text()
            if re.search(r'(?m)^\s+runs-on:\s*macos-', text):
                with self.subTest(workflow=path.name):
                    self.assertEqual(events(text), {'workflow_dispatch'})

    def test_quick_checks_stay_on_ubuntu_and_cancel_superseded_runs(self):
        text = (WORKFLOWS / 'ios-quick-checks.yml').read_text()
        self.assertIn('ubuntu-latest', text)
        self.assertNotRegex(text, r'(?m)^\s+runs-on:\s*macos-')
        self.assertTrue({'push', 'pull_request'} <= events(text))
        self.assertIn('cancel-in-progress: true', text)

    def test_manual_build_does_not_default_to_two_macos_jobs(self):
        text = (WORKFLOWS / 'ios-simulator.yml').read_text()
        self.assertIn('default: device', text)
        self.assertIn('inputs.build_target == \'device\'', text)
        self.assertIn('inputs.build_target == \'simulator\'', text)
        self.assertIn('if: ${{ inputs.renderer_probe }}', text)
        self.assertIn('cancel-in-progress: false', text)

    def test_readme_describes_only_manual_macos_builds(self):
        text = (ROOT / 'packaging/ios/README.md').read_text()
        self.assertNotIn('workflow also runs automatically', text)
        self.assertNotIn('unsigned real-device IPA is now built by default', text)
        self.assertIn('manually', text.lower())


if __name__ == '__main__':
    unittest.main()
