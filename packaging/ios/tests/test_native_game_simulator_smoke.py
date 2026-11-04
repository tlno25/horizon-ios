"""Exercise the real-game Simulator diagnostics script without requiring macOS."""
import os
import subprocess
import tempfile
import threading
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
SCRIPT = ROOT / "packaging/ios/tests/native_game_simulator_smoke.sh"
WORKFLOW = ROOT / ".github/workflows/ios-simulator.yml"
FIRST_FRAME = "[12:01:01] [Render thread/INFO]: Horizon renderer: presenting the first frame\n"


class NativeGameSimulatorSmokeTests(unittest.TestCase):
    def run_smoke(self, root: Path, *, log: str, delay_marker: bool = False,
                  launcher: bool = False, launcher_ready: bool = False,
                  bridge_ready: bool = False, import_ready: bool = False,
                  auth_guard_ready: bool = False, timeout: int = 2):
        app = root / "HorizonGame.app"
        app.mkdir()
        if launcher:
            (app / "Frameworks/HorizonLauncherKit.framework").mkdir(parents=True)
        container = root / "simulator-container"
        logs = container / "Documents/logs"
        logs.mkdir(parents=True)
        game_log = logs / "latest.log"
        game_log.write_text(log, encoding="utf-8")
        fake_bin = root / "bin"
        fake_bin.mkdir()
        xcrun = fake_bin / "xcrun"
        xcrun.write_text(
            '#!/usr/bin/env bash\n'
            'if [[ "$1 $2" == "simctl get_app_container" ]]; then printf "%s\\n" "$FAKE_SIM_CONTAINER"; exit 0; fi\n'
            'if [[ "$1 $2" == "simctl io" ]]; then\n'
            '  if grep -Fq "presenting the first frame" "$FAKE_SIM_CONTAINER/Documents/logs/latest.log"; then\n'
            '    printf "FRAME_READY" > "${@: -1}";\n'
            '  else\n'
            '    printf "EARLY_CAPTURE" > "${@: -1}";\n'
            '  fi\n'
            '  exit 0;\n'
            'fi\n'
            'if [[ "$1 $2" == "simctl launch" && "$FAKE_SWIFTUI_PRESENT" == "1" ]]; then\n'
            '  mkdir -p "$FAKE_SIM_CONTAINER/Library/Caches"\n'
            '  printf "presented" > "$FAKE_SIM_CONTAINER/Library/Caches/horizon-ios-launcher-presented"\n'
            '  if [[ "$FAKE_SWIFTUI_BRIDGE" == "1" ]]; then\n'
            '    printf "ready" > "$FAKE_SIM_CONTAINER/Library/Caches/horizon-ios-launcher-bridge-ready"\n'
            '  fi\n'
            '  if [[ "$FAKE_SWIFTUI_IMPORT" == "1" && -f "$FAKE_SIM_CONTAINER/Documents/horizon-download-CIRESOURCE.zip" ]]; then\n'
            '    printf "ready" > "$FAKE_SIM_CONTAINER/Library/Caches/horizon-ios-launcher-import-ready"\n'
            '    if [[ "$FAKE_SWIFTUI_AUTH_GUARD" == "1" ]]; then\n'
            '      printf "ready" > "$FAKE_SIM_CONTAINER/Library/Caches/horizon-ios-launcher-auth-guard-ready"\n'
            '    fi\n'
            '  fi\n'
            'fi\n'
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
            "HORIZON_IOS_GAME_SIMULATOR_READY_TIMEOUT_SECONDS": str(timeout),
            "HORIZON_IOS_GAME_SIMULATOR_SETTLE_SECONDS": "0",
            "FAKE_SIM_CONTAINER": str(container),
            "FAKE_SWIFTUI_PRESENT": "1" if launcher_ready else "0",
            "FAKE_SWIFTUI_BRIDGE": "1" if bridge_ready else "0",
            "FAKE_SWIFTUI_IMPORT": "1" if import_ready else "0",
            "FAKE_SWIFTUI_AUTH_GUARD": "1" if auth_guard_ready else "0",
        }
        timer = None
        if delay_marker:
            timer = threading.Timer(0.3, lambda: game_log.write_text(log + FIRST_FRAME, encoding="utf-8"))
            timer.start()
        try:
            result = subprocess.run(["bash", str(SCRIPT)], env=env, capture_output=True, text=True, timeout=12)
        finally:
            if timer:
                timer.join(timeout=2)
        return result

    def test_waits_for_delayed_first_frame_before_capturing_screen(self):
        # This failed in CI: Metal shader initialization took 43s, but the
        # screenshot was captured after just 20s, before the first frame.
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            result = self.run_smoke(root, log="Horizon renderer: creating the pipelines\n", delay_marker=True)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual((root / "horizon-ios-game-simulator-screen.png").read_bytes(), b"FRAME_READY")
            self.assertIn("presenting the first frame", (root / "horizon-ios-game-simulator-game.log").read_text())
            self.assertTrue((root / "horizon-ios-game-simulator-console.log").exists())

    def test_missing_first_frame_fails_but_still_captures_diagnostics(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            result = self.run_smoke(root, log="Horizon renderer: creating the pipelines\n", timeout=0)
            self.assertNotEqual(result.returncode, 0, result.stdout)
            self.assertIn("first frame", result.stderr)
            self.assertEqual((root / "horizon-ios-game-simulator-screen.png").read_bytes(), b"EARLY_CAPTURE")
            self.assertEqual((root / "horizon-ios-game-simulator-game.log").read_text(), "Horizon renderer: creating the pipelines\n")
            self.assertTrue((root / "horizon-ios-game-simulator-console.log").exists())

    def test_preexisting_frame_presents_and_collects_logs(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            result = self.run_smoke(root, log=FIRST_FRAME, timeout=0)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual((root / "horizon-ios-game-simulator-screen.png").read_bytes(), b"FRAME_READY")
            self.assertEqual((root / "horizon-ios-game-simulator-game.log").read_text(), FIRST_FRAME)

    def test_swiftui_launcher_is_success_without_first_metal_frame(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            result = self.run_smoke(root, log="Horizon iOS launcher startup\n",
                                    launcher=True, launcher_ready=True, bridge_ready=True,
                                    import_ready=True, auth_guard_ready=True, timeout=2)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertIn("SwiftUI launcher", result.stdout)
            self.assertTrue((root / "horizon-ios-game-simulator-screen.png").exists())

    def test_simulator_import_probe_detects_broken_checked_zip_installer(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            result = self.run_smoke(root, log="SwiftUI bridge working\n",
                                    launcher=True, launcher_ready=True, bridge_ready=True,
                                    import_ready=False, timeout=0)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("ZIP", result.stderr)
            self.assertTrue((root / "horizon-ios-game-simulator-screen.png").exists())

    def test_valid_prepared_zip_still_rejects_play_without_microsoft_ownership(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            result = self.run_smoke(root, log="SwiftUI bridge and ZIP ready\n",
                                    launcher=True, launcher_ready=True, bridge_ready=True,
                                    import_ready=True, auth_guard_ready=False, timeout=0)
            self.assertNotEqual(result.returncode, 0, result.stdout)
            self.assertIn("ownership gate", result.stderr)
            self.assertTrue((root / "horizon-ios-game-simulator-screen.png").exists())

    def test_simulator_import_fixture_contains_only_synthetic_resources(self):
        import zipfile
        from io import BytesIO
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            result = self.run_smoke(root, log="Bridge and synthetic ZIP\n",
                                    launcher=True, launcher_ready=True, bridge_ready=True,
                                    import_ready=True, auth_guard_ready=True, timeout=1)
            self.assertEqual(result.returncode, 0, result.stderr)
            archive = root / "simulator-container/Documents/horizon-download-CIRESOURCE.zip"
            self.assertTrue(archive.exists(), "Simulator test must create a ZIP inside the app sandbox")
            with zipfile.ZipFile(archive) as zip:
                self.assertEqual(zip.read("horizon-resources.json"), b'{"schema":7,"minecraft":"26.3"}')
                self.assertIn('assets/minecraft/textures/fixture', zip.namelist())
                self.assertIn('facts.json', zip.namelist())

    def test_swiftui_presented_but_unresponsive_rust_bridge_fails(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            result = self.run_smoke(root, log="SwiftUI only\n",
                                    launcher=True, launcher_ready=True, bridge_ready=False, timeout=0)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("bridge", result.stderr.lower())
            self.assertTrue((root / "horizon-ios-game-simulator-screen.png").exists())

    def test_unpresented_swiftui_launcher_fails_instead_of_claiming_success(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            result = self.run_smoke(root, log="Horizon game starting\n", launcher=True, timeout=0)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("SwiftUI", result.stderr)
            self.assertTrue((root / "horizon-ios-game-simulator-screen.png").exists())

    def test_workflow_uploads_game_log_for_black_screen_triage(self):
        self.assertIn("horizon-ios-game-simulator-game.log", WORKFLOW.read_text())


if __name__ == "__main__":
    unittest.main()
