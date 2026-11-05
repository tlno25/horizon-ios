"""Keep CI preparation aligned with Horizon's security-pinned Mojang metadata."""
import json
import sys
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT / "tools"))
from android_prepare import approved_version_manifest


class ApprovedVersionManifestTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.pins = json.loads((ROOT / "research-lock.json").read_text())["minecraft"]

    def test_current_approved_manifest_is_used(self):
        current = self.pins["accepted_manifest_sha1"][0]
        url = "https://piston-meta.mojang.com/v1/packages/approved/26.3.json"
        self.assertEqual(approved_version_manifest({"sha1": current, "url": url}, "26.3", self.pins),
                         (url, current))

    def test_republished_unreviewed_manifest_uses_original_pinned_copy(self):
        uri, checksum = approved_version_manifest(
            {"sha1": "80608ff6753705b6a82e2cb86a5937d4d233abd4", "url": "https://untrusted.example/26.3.json"},
            "26.3", self.pins)
        self.assertEqual(checksum, self.pins["manifest_sha1"])
        self.assertEqual(uri, f"https://piston-meta.mojang.com/v1/packages/{checksum}/26.3.json")

    def test_original_pin_is_allowed(self):
        pin = self.pins["manifest_sha1"]
        uri = f"https://piston-meta.mojang.com/v1/packages/{pin}/26.3.json"
        self.assertEqual(approved_version_manifest({"sha1": pin, "url": uri}, "26.3", self.pins),
                         (uri, pin))


if __name__ == "__main__":
    unittest.main()
