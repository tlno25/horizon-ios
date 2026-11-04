"""A rebuilt simulator game should not be necessary to re-test a screenshot."""
import unittest
from pathlib import Path

WORKFLOW = Path(__file__).resolve().parents[3] / ".github/workflows/ios-simulator.yml"


class NativeGameSimulatorSmokeWorkflowTests(unittest.TestCase):
    def test_can_opt_in_to_smoke_using_an_existing_game_simulator_artifact(self):
        workflow = WORKFLOW.read_text()
        self.assertIn("- game-smoke", workflow)
        self.assertIn("game-simulator-smoke:", workflow)
        self.assertIn("if: ${{ inputs.build_target == 'game-smoke' }}", workflow)
        self.assertIn("run-id: ${{ inputs.source_game_simulator_run }}", workflow)
        self.assertIn("github-token: ${{ github.token }}", workflow)
        self.assertIn("name: horizon-ios-game-simulator-app", workflow)
        self.assertIn("native_game_simulator_smoke.sh", workflow)

    def test_smoke_only_path_preserves_read_only_artifact_permissions(self):
        workflow = WORKFLOW.read_text()
        self.assertIn("  actions: read", workflow)
        self.assertNotIn("  push:", workflow)


if __name__ == "__main__":
    unittest.main()
