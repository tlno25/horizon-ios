"""The experimental IPA contains a real arm64 game executable and no Mojang data."""
import os
import plistlib
import struct
import sys
import tempfile
import unittest
import zipfile
from pathlib import Path

IOS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(IOS))
from package_native_game_ipa import package


class NativeGameIpaTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.binary = self.root / "horizon-client"
        # 64-bit little-endian Mach-O executable for arm64 (CPU subtype all).
        self.binary.write_bytes(struct.pack("<IIIIIII", 0xFEEDFACF, 0x0100000C, 0, 2, 0, 0, 0) + b"\0" * 64)
        self.binary.chmod(0o755)
        self.output = self.root / "out" / "Horizon-game-unsigned.ipa"

    def test_valid_game_archive_has_correct_bundle_and_no_vanilla_files(self):
        (self.root / "horizon-resources.json").write_text("private", encoding="utf-8")
        package(self.binary, IOS / "GameInfo.plist", self.output)
        with zipfile.ZipFile(self.output) as ipa:
            prefix = "Payload/HorizonGame.app/"
            self.assertEqual(set(ipa.namelist()), {prefix + name for name in ("HorizonGame", "Info.plist", "PkgInfo")})
            info = plistlib.loads(ipa.read(prefix + "Info.plist"))
            self.assertEqual(info["CFBundleExecutable"], "HorizonGame")
            self.assertEqual(info["CFBundlePackageType"], "APPL")
            self.assertEqual(info["MinimumOSVersion"], "16.0")
            self.assertTrue(info["UIFileSharingEnabled"])
            self.assertTrue(info["LSSupportsOpeningDocumentsInPlace"])
            self.assertEqual(ipa.read(prefix + "HorizonGame"), self.binary.read_bytes())
            self.assertTrue((ipa.getinfo(prefix + "HorizonGame").external_attr >> 16) & 0o111)
            self.assertNotIn("horizon-resources.json", "".join(ipa.namelist()))

    def test_rejects_wrong_architecture_and_incomplete_binary(self):
        for contents in (b"", b"MZ" + b"\0" * 30,
                         struct.pack("<IIIIIII", 0xFEEDFACF, 0x01000007, 0, 2, 0, 0, 0)):
            with self.subTest(data=contents[:8]):
                self.binary.write_bytes(contents)
                with self.assertRaisesRegex(ValueError, "arm64 Mach-O"):
                    package(self.binary, IOS / "GameInfo.plist", self.output)
                self.assertFalse(self.output.exists())

    def test_does_not_silently_bundle_non_executable_or_bad_plist(self):
        self.binary.chmod(0o644)
        with self.assertRaisesRegex(ValueError, "executable"):
            package(self.binary, IOS / "GameInfo.plist", self.output)
        self.binary.chmod(0o755)
        bad_info = self.root / "bad.plist"
        bad_info.write_bytes(plistlib.dumps({"CFBundleExecutable": "NotHorizonGame"}))
        with self.assertRaisesRegex(ValueError, "metadata"):
            package(self.binary, bad_info, self.output)


if __name__ == "__main__":
    unittest.main()
