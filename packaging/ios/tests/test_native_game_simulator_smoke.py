"""Exercise the real-game Simulator diagnostics script without requiring macOS."""
import os
import subprocess
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
SCRIPT = ROOT / "packaging/ios/tests/native_game_simulator_smoke.sh"
WORKFLOW = ROOT / ".github/workflows/ios-simulator.yml"


class NativeGameSimulatorSmokeTests(unittest.TestCase):
    def test_collects_game_file_log_and_screenshot_from_simulator(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            app = root / "HorizonGame.app"
            app.mkdir()
            container = root / "simulator-container"
            logs = container / "Documents/logs"
            logs.mkdir(parents=True)
            (logs / "latest.log").write_text("Horizon: missing resources\n", encoding="utf-8")
            fake_bin = root / "bin"
            fake_bin.mkdir()
            xcrun = fake_bin / "xcrun"
            xcrun.write_text(
                '#!/usr/bin/env bash\n'
                'if [[ "$1 $2" == "simctl get_app_container" ]]; then printf "%s\\n" "$FAKE_SIM_CONTAINER"; exit 0; fi\n'
                'if [[ "$1 $2" == "simctl io" ]]; then printf "PNG" > "${@: -1}"; exit 0; fi\n'
                'if [[ "$1 $2" == "simctl spawn" ]]; then printf "HorizonGame started\\n"; exit 0; fi\n'
                'exit 0\n',
                encoding="utf-8",
            )
            xcrun.chmod(0o755)
            env = {
                **os.environ,
                "PATH": str(fake_bin) + os.pathsep + os.environ["PATH"],
                "RUNNER_TEMP": str(root),
                "HORIZON_NATIVE_SIMULATOR_APP": str(app),
                "HORIZON_IOS_SIMULATOR_UDID": "TEST-UDID",
                "HORIZON_IOS_GAME_SIMULATOR_WAIT_SECONDS": "0",
                "FAKE_SIM_CONTAINER": str(container),
            }
            result = subprocess.run(["bash", str(SCRIPT)], env=env, capture_output=True, text=True, timeout=20)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual((root / "horizon-ios-game-simulator-game.log").read_text(), "Horizon: missing resources\n")
            self.assertTrue((root / "horizon-ios-game-simulator-screen.png").exists())
            self.assertTrue((root / "horizon-ios-game-simulator-console.log").exists())

    def test_workflow_uploads_game_log_for_black_screen_triage(self):
        self.assertIn("horizon-ios-game-simulator-game.log", WORKFLOW.read_text())


if __name__ == "__main__":
    unittest.main()
