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
        self.launcher = self.root / "HorizonLauncherKit.framework"
        self.launcher.mkdir()
        launcher_binary = self.launcher / "HorizonLauncherKit"
        launcher_binary.write_bytes(struct.pack("<IIIIIII", 0xFEEDFACF, 0x0100000C, 0, 6, 0, 0, 0) + b"\0" * 64)
        launcher_binary.chmod(0o755)
        (self.launcher / "Info.plist").write_bytes(plistlib.dumps({
            "CFBundleExecutable": "HorizonLauncherKit", "CFBundlePackageType": "FMWK"}))

    def test_valid_game_archive_has_correct_bundle_and_no_vanilla_files(self):
        (self.root / "horizon-resources.json").write_text("private", encoding="utf-8")
        package(self.binary, IOS / "GameInfo.plist", self.output,
                launcher_framework=self.launcher)
        with zipfile.ZipFile(self.output) as ipa:
            prefix = "Payload/HorizonGame.app/"
            self.assertEqual(set(ipa.namelist()), {
                prefix + name for name in ("HorizonGame", "Info.plist", "PkgInfo")
            } | {
                prefix + "Frameworks/HorizonLauncherKit.framework/" + name
                for name in ("HorizonLauncherKit", "Info.plist")
            })
            info = plistlib.loads(ipa.read(prefix + "Info.plist"))
            self.assertEqual(info["CFBundleExecutable"], "HorizonGame")
            self.assertEqual(info["CFBundleDisplayName"], "Horizon")
            self.assertEqual(info["CFBundleName"], "Horizon")
            self.assertEqual(info["CFBundleShortVersionString"], "0.2.0")
            self.assertEqual(info["CFBundleIdentifier"], "com.example.horizonios.game")
            self.assertEqual(info["CFBundlePackageType"], "APPL")
            self.assertEqual(info["MinimumOSVersion"], "16.0")
            self.assertTrue(info["UIFileSharingEnabled"])
            self.assertTrue(info["LSSupportsOpeningDocumentsInPlace"])
            self.assertEqual(ipa.read(prefix + "HorizonGame"), self.binary.read_bytes())
            self.assertTrue((ipa.getinfo(prefix + "HorizonGame").external_attr >> 16) & 0o111)
            self.assertNotIn("horizon-resources.json", "".join(ipa.namelist()))

    def test_generated_engine_tables_can_be_packaged_without_mojang_assets(self):
        engine = self.root / "engine.zip"
        with zipfile.ZipFile(engine, "w") as z:
            for name in ("horizon-resources.json", "facts.json", "registries.json", "commands.json"):
                z.writestr(name, "{}")
        package(self.binary, IOS / "GameInfo.plist", self.output,
                launcher_framework=self.launcher, engine_data=engine)
        with zipfile.ZipFile(self.output) as ipa:
            self.assertEqual(ipa.read("Payload/HorizonGame.app/engine-data.zip"), engine.read_bytes())
            self.assertNotIn("horizon-resources.json", ipa.namelist())

    def test_rejects_original_game_assets_in_engine_tables(self):
        engine = self.root / "bad-engine.zip"
        with zipfile.ZipFile(engine, "w") as z:
            for name in ("horizon-resources.json", "facts.json", "registries.json", "commands.json"):
                z.writestr(name, "{}")
            z.writestr("assets/minecraft/textures/block/stone.png", b"not allowed")
        with self.assertRaisesRegex(ValueError, "generated-only"):
            package(self.binary, IOS / "GameInfo.plist", self.output,
                    launcher_framework=self.launcher, engine_data=engine)
        self.assertFalse(self.output.exists())

    def test_never_packages_an_ios_game_without_the_swiftui_launcher(self):
        with self.assertRaisesRegex(ValueError, "requires a SwiftUI launcher"):
            package(self.binary, IOS / "GameInfo.plist", self.output)
        self.assertFalse(self.output.exists())

    def test_embeds_native_swiftui_launcher_without_resources(self):
        package(self.binary, IOS / "GameInfo.plist", self.output,
                launcher_framework=self.launcher)
        with zipfile.ZipFile(self.output) as ipa:
            base = "Payload/HorizonGame.app/Frameworks/HorizonLauncherKit.framework/"
            self.assertIn(base + "HorizonLauncherKit", ipa.namelist())
            self.assertIn(base + "Info.plist", ipa.namelist())
            self.assertTrue((ipa.getinfo(base + "HorizonLauncherKit").external_attr >> 16) & 0o111)
            self.assertNotIn("horizon-resources.json", "".join(ipa.namelist()))

    def test_rejects_framework_with_invalid_bundle_metadata(self):
        (self.launcher / "Info.plist").unlink()
        with self.assertRaisesRegex(ValueError, "launcher framework"):
            package(self.binary, IOS / "GameInfo.plist", self.output,
                    launcher_framework=self.launcher)
        self.assertFalse(self.output.exists())

    def test_rejects_wrong_framework_architecture_or_type(self):
        dylib = self.launcher / "HorizonLauncherKit"
        for kind, cpu in ((6, 0x01000007), (2, 0x0100000C), (6, 0)):
            with self.subTest(kind=kind, cpu=cpu):
                dylib.write_bytes(struct.pack("<IIIIIII", 0xFEEDFACF, cpu, 0, kind, 0, 0, 0))
                with self.assertRaisesRegex(ValueError, "arm64 iPhoneOS SwiftUI launcher"):
                    package(self.binary, IOS / "GameInfo.plist", self.output,
                            launcher_framework=self.launcher)
                self.assertFalse(self.output.exists())

    def test_rejects_wrong_architecture_and_incomplete_binary(self):
        for contents in (b"", b"MZ" + b"\0" * 30,
                         struct.pack("<IIIIIII", 0xFEEDFACF, 0x01000007, 0, 2, 0, 0, 0)):
            with self.subTest(data=contents[:8]):
                self.binary.write_bytes(contents)
                with self.assertRaisesRegex(ValueError, "arm64 Mach-O"):
                    package(self.binary, IOS / "GameInfo.plist", self.output,
                            launcher_framework=self.launcher)
                self.assertFalse(self.output.exists())

    def test_does_not_silently_bundle_non_executable_or_bad_plist(self):
        self.binary.chmod(0o644)
        with self.assertRaisesRegex(ValueError, "executable"):
            package(self.binary, IOS / "GameInfo.plist", self.output,
                    launcher_framework=self.launcher)
        self.binary.chmod(0o755)
        bad_info = self.root / "bad.plist"
        bad_info.write_bytes(plistlib.dumps({"CFBundleExecutable": "NotHorizonGame"}))
        with self.assertRaisesRegex(ValueError, "metadata"):
            package(self.binary, bad_info, self.output,
                    launcher_framework=self.launcher)


if __name__ == "__main__":
    unittest.main()
