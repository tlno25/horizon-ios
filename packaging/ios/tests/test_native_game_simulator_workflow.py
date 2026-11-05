"""The expensive game Simulator build must remain manual and isolated."""
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
WORKFLOW = ROOT / ".github/workflows/ios-simulator.yml"


class GameSimulatorWorkflowTests(unittest.TestCase):
    def test_opt_in_target_compiles_true_game_for_arm_simulator(self):
        body = WORKFLOW.read_text()
        self.assertIn("- game-simulator", body)
        self.assertIn("if: ${{ inputs.build_target == 'game-simulator' }}", body)
        self.assertIn("--target aarch64-apple-ios-sim -p horizon-client", body)
        self.assertIn("package_native_game_simulator.py", body)
        self.assertIn("name: horizon-ios-game-simulator-app", body)
        self.assertIn("native_game_simulator_smoke.sh", body)

    def test_native_game_is_manual_default_and_old_device_target_is_opt_in(self):
        body = WORKFLOW.read_text()
        self.assertIn("default: game", body)
        self.assertIn("- device", body)
        self.assertIn("if: ${{ inputs.build_target == 'game' }}", body)
        self.assertNotIn("  push:", body)


if __name__ == "__main__":
    unittest.main()
