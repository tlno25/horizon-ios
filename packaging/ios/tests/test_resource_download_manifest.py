"""Prepare publisher manifests from authorized ZIP archives without shipping assets."""
import hashlib
import json
import sys
import tempfile
import unittest
import zipfile
from pathlib import Path

IOS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(IOS))
from create_resource_download_manifest import generate_manifest


class ResourceDownloadManifestTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.archive = self.root / 'authorized-resources.zip'
        self.output = self.root / 'game-files.json'

    def make_fixture(self, missing=None, version='26.3'):
        files = {
            'horizon-resources.json': json.dumps({'schema': 1, 'minecraft': version}),
            'facts.json': '{}',
            'block-tags.json': '{}',
            'registries.json': '{}',
            'commands.json': '{}',
            'assets/minecraft/font/placeholder.json': '{}',
            'assets/minecraft/textures/placeholder.png': 'not-real-minecraft-data',
        }
        files.pop(missing, None)
        with zipfile.ZipFile(self.archive, 'w', zipfile.ZIP_DEFLATED) as zip:
            for name, data in files.items():
                zip.writestr(name, data)

    def test_generates_exact_hash_size_and_version_for_one_tap_download(self):
        self.make_fixture()
        generated = generate_manifest(self.archive, self.output, 'https://assets.example.test/horizon-26.3.zip')
        self.assertEqual(generated['minecraft'], '26.3')
        self.assertEqual(generated['url'], 'https://assets.example.test/horizon-26.3.zip')
        self.assertEqual(generated['sha256'], hashlib.sha256(self.archive.read_bytes()).hexdigest())
        self.assertEqual(generated['size'], self.archive.stat().st_size)
        self.assertEqual(json.loads(self.output.read_text()), generated)

    def test_rejects_incomplete_or_wrong_version_prepared_assets(self):
        for missing in ['facts.json', 'assets/minecraft/textures/placeholder.png']:
            self.make_fixture(missing=missing)
            with self.subTest(missing=missing), self.assertRaisesRegex(ValueError, 'missing'):
                generate_manifest(self.archive, self.output, 'https://example.test/game.zip')
            self.assertFalse(self.output.exists())

    def test_rejects_wrong_engine_version_and_keeps_existing_manifest(self):
        self.make_fixture(version='26.4')
        with self.assertRaisesRegex(ValueError, 'expected 26.3'):
            generate_manifest(self.archive, self.output, 'https://example.test/game.zip', '26.3')
        self.assertFalse(self.output.exists())
        self.make_fixture()
        generate_manifest(self.archive, self.output, 'https://example.test/game.zip', '26.3')
        old = self.output.read_bytes()
        with self.assertRaises(FileExistsError):
            generate_manifest(self.archive, self.output, 'https://example.test/other.zip', '26.3')
        self.assertEqual(self.output.read_bytes(), old)

    def test_rejects_archive_symlinks_before_publishing(self):
        self.make_fixture()
        link = zipfile.ZipInfo('assets/minecraft/textures/linked.png')
        link.create_system = 3
        link.external_attr = (0o120777 << 16)
        with zipfile.ZipFile(self.archive, 'a') as zip:
            zip.writestr(link, b'../../outside')
        with self.assertRaisesRegex(ValueError, 'symlink'):
            generate_manifest(self.archive, self.output, 'https://assets.example.test/game.zip')
        self.assertFalse(self.output.exists())

    def test_rejects_untrusted_urls_and_bad_zips(self):
        self.make_fixture()
        for url in ['http://assets.example.test/game.zip', 'https://name:password@example.test/game.zip',
                    'https://example.test/game.zip#anything']:
            with self.subTest(url=url), self.assertRaisesRegex(ValueError, 'HTTPS'):
                generate_manifest(self.archive, self.output, url)
        self.archive.write_bytes(b'not-a-zip')
        with self.assertRaisesRegex(ValueError, 'ZIP'):
            generate_manifest(self.archive, self.output, 'https://assets.example.test/game.zip')
        self.assertFalse(self.output.exists())


if __name__ == '__main__':
    unittest.main()
