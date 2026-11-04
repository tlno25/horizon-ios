"""Source contracts for the native iOS controller and playback-session integration.

Host Rust tests validate the snapshot mapping. The native framework still
requires macOS/Xcode to prove ABI and Swift compilation.
"""
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[3]


class IOSGameControllerAudioTests(unittest.TestCase):
    def test_native_gamecontroller_bridge_is_iOS_specific(self):
        cargo = (ROOT / 'crates/horizon-gamecontroller/Cargo.toml').read_text()
        bridge = (ROOT / 'crates/horizon-gamecontroller/src/lib.rs').read_text()
        self.assertIn('target_os = "ios"', cargo)
        self.assertIn('objc2-game-controller', cargo)
        self.assertIn('GCController::controllers()', bridge)
        self.assertIn('GCController::current()', bridge)

    def test_gamepad_uses_apple_gamecontroller_snapshots_even_without_gilrs(self):
        source = (ROOT / 'crates/horizon-client/src/game/pad.rs').read_text()
        self.assertIn('horizon_gamecontroller::active()', source)
        self.assertIn('ios_snapshot_state(&snapshot)', source)
        self.assertIn('target_os = "android", target_os = "ios")))]', source)
        self.assertIn('Button::DPadUp, pad.dpad_up', source)
        self.assertIn('state.triggers = pad.triggers', source)

    def test_audio_session_tracks_play_and_return_to_launcher(self):
        audio = (ROOT / 'packaging/ios/GameLauncher/Sources/GameAudio.swift').read_text()
        present = (ROOT / 'packaging/ios/GameLauncher/Sources/Present.swift').read_text()
        self.assertIn('session.setCategory(.playback', audio)
        self.assertIn('.mixWithOthers', audio)
        self.assertIn('AVAudioSession.interruptionNotification', audio)
        self.assertIn('GameAudio.stop()', present)
        self.assertIn('GameAudio.start()', present)
        self.assertLess(present.index('GameAudio.stop()'), present.index('GameAudio.start()'))
        self.assertIn('launcherPresentationPending = true', present)
        self.assertIn('launcherPresentationPending = false', present)
        self.assertIn('root.presentedViewController is GameLauncherHostingController', present)


if __name__ == '__main__':
    unittest.main()
