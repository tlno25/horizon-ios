"""Ensure native game builds stay opt-in and never change the default shell IPA."""
import re
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
WORKFLOW = ROOT / ".github" / "workflows" / "ios-simulator.yml"


class NativeGameWorkflowTests(unittest.TestCase):
    def test_explicit_game_target_builds_true_client_not_diagnostic_shell(self):
        workflow = WORKFLOW.read_text()
        self.assertIn("- game", workflow)
        self.assertIn("if: ${{ inputs.build_target == 'game' }}", workflow)
        self.assertIn("-p horizon-client --no-default-features --features audio", workflow)
        self.assertIn("package_native_game_ipa.py", workflow)
        self.assertIn("name: horizon-ios-native-game-ipa", workflow)

    def test_device_game_uses_available_arm64_mac_runner(self):
        import yaml
        workflow = yaml.safe_load(WORKFLOW.read_text())
        job = workflow["jobs"]["game-build"]
        self.assertEqual(job["runs-on"], "macos-15")
        steps = str(job["steps"])
        self.assertIn("--target aarch64-apple-ios", steps)
        self.assertIn("build_game_launcher.sh iphoneos arm64", steps)
        self.assertIn("Horizon-game-unsigned.ipa", steps)

    def test_launcher_only_ci_validates_swift_without_recompiling_all_rust(self):
        workflow = WORKFLOW.read_text()
        self.assertIn("- game-launcher", workflow)
        self.assertIn("launcher-only-build:", workflow)
        self.assertIn('build_game_launcher.sh iphonesimulator x86_64', workflow)
        self.assertIn('_horizon_launcher_present', workflow)
        import yaml
        launcher = yaml.safe_load(workflow)['jobs']['launcher-only-build']
        self.assertEqual(launcher['runs-on'], 'macos-26-intel')

    def test_native_game_simulator_uses_available_apple_silicon_runner(self):
        import yaml
        workflow = yaml.safe_load(WORKFLOW.read_text())
        simulator = workflow["jobs"]["game-simulator-build"]
        self.assertEqual(simulator["runs-on"], "macos-15")
        job = str(simulator["steps"])
        self.assertIn("aarch64-apple-ios-sim", job)
        self.assertIn("iphonesimulator arm64", job)
        self.assertIn("--architecture arm64", job)
        smoke = workflow["jobs"]["game-simulator-smoke"]
        self.assertEqual(smoke["runs-on"], "macos-15")

    def test_default_device_build_remains_unchanged_and_game_never_autoruns(self):
        workflow = WORKFLOW.read_text()
        self.assertIn("default: device", workflow)
        self.assertNotRegex(workflow, r"(?m)^\s+push:\s*$")
        self.assertRegex(workflow, r"(?m)^  game-build:\s*$")


if __name__ == "__main__":
    unittest.main()
