"""Simulator .app packaging remains independent from unsigned device IPA."""
import plistlib
import struct
import sys
import tempfile
import unittest
from pathlib import Path

IOS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(IOS))
from package_native_game_simulator import package_simulator_app


class NativeGameSimulatorTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        self.binary = self.root / "horizon-client"
        self.bundle = self.root / "HorizonGame.app"
        self.binary.write_bytes(struct.pack("<IIIIIII", 0xFEEDFACF, 0x01000007, 3, 2, 0, 0, 0) + bytes(60))
        self.binary.chmod(0o755)

    def test_creates_installable_simulator_bundle_without_vanilla_data(self):
        (self.root / "horizon-resources.json").write_text("private", encoding="utf-8")
        package_simulator_app(self.binary, IOS / "GameInfo.plist", self.bundle)
        self.assertEqual(
            {item.name for item in self.bundle.iterdir()},
            {"HorizonGame", "Info.plist", "PkgInfo"},
        )
        info = plistlib.loads((self.bundle / "Info.plist").read_bytes())
        self.assertEqual(info["CFBundleSupportedPlatforms"], ["iPhoneSimulator"])
        self.assertEqual(info["DTPlatformName"], "iphonesimulator")
        self.assertEqual(info["CFBundleIdentifier"], "com.example.horizonios.game")
        self.assertEqual(info["CFBundleExecutable"], "HorizonGame")
        self.assertEqual((self.bundle / "HorizonGame").read_bytes(), self.binary.read_bytes())
        self.assertFalse((self.bundle / "horizon-resources.json").exists())
        self.assertTrue((self.bundle / "HorizonGame").stat().st_mode & 0o111)

    def test_cannot_use_a_device_arm64_executable_in_intel_simulator(self):
        self.binary.write_bytes(struct.pack("<IIIIIII", 0xFEEDFACF, 0x0100000C, 0, 2, 0, 0, 0) + bytes(60))
        with self.assertRaisesRegex(ValueError, "x86_64.*Simulator"):
            package_simulator_app(self.binary, IOS / "GameInfo.plist", self.bundle)
        self.assertFalse(self.bundle.exists())

    def test_never_overwrites_existing_bundle_or_accepts_invalid_header(self):
        self.bundle.mkdir()
        (self.bundle / "keep.txt").write_text("important")
        with self.assertRaisesRegex(FileExistsError, "already exists"):
            package_simulator_app(self.binary, IOS / "GameInfo.plist", self.bundle)
        self.assertEqual((self.bundle / "keep.txt").read_text(), "important")
        self.bundle.rename(self.root / "previous.app")
        self.binary.write_bytes(b"invalid")
        with self.assertRaisesRegex(ValueError, "x86_64.*Simulator"):
            package_simulator_app(self.binary, IOS / "GameInfo.plist", self.bundle)


if __name__ == "__main__":
    unittest.main()
