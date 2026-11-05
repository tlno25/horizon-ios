"""The publisher can preconfigure one-tap HTTPS provisioning without bundling assets."""
import plistlib
import sys
import tempfile
import unittest
from pathlib import Path

IOS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(IOS))
from configure_game_plist import prepare


class GameManifestConfigurationTests(unittest.TestCase):
    def test_managed_https_manifest_is_saved_only_in_generated_game_plist(self):
        with tempfile.TemporaryDirectory() as directory:
            target = Path(directory) / 'Configured.plist'
            source = IOS / 'GameInfo.plist'
            original = source.read_bytes()
            prepare(source, target, 'https://assets.example.org/26.3/game-files.json', '11111111-2222-4333-8444-555555555555')
            info = plistlib.loads(target.read_bytes())
            self.assertEqual(info['HorizonResourcesManifestURL'], 'https://assets.example.org/26.3/game-files.json')
            self.assertEqual(info['HorizonMicrosoftClientID'], '11111111-2222-4333-8444-555555555555')
            self.assertEqual(source.read_bytes(), original)
            self.assertFalse(any(name.name.startswith('horizon-resources') for name in target.parent.iterdir()))

    def test_manual_game_build_has_unique_numeric_bundle_version(self):
        with tempfile.TemporaryDirectory() as directory:
            target = Path(directory) / 'Configured.plist'
            prepare(IOS / 'GameInfo.plist', target, '', build_number='1234')
            self.assertEqual(plistlib.loads(target.read_bytes())['CFBundleVersion'], '1234')
            for invalid in ('0', '-1', '1.2', 'abc', '1234567890'):
                with self.subTest(invalid=invalid), self.assertRaisesRegex(ValueError, 'build number'):
                    prepare(IOS / 'GameInfo.plist', target, '', build_number=invalid)
            # Developer builds without a CI number retain the template version.
            prepare(IOS / 'GameInfo.plist', target, '')
            self.assertEqual(plistlib.loads(target.read_bytes())['CFBundleVersion'], '1')

    def test_empty_configuration_is_explicitly_allowed_but_unsafe_links_are_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            target = Path(directory) / 'Configured.plist'
            prepare(IOS / 'GameInfo.plist', target, '')
            self.assertEqual(plistlib.loads(target.read_bytes())['HorizonResourcesManifestURL'], '')
            self.assertEqual(plistlib.loads(target.read_bytes())['HorizonMicrosoftClientID'], '')
            for link in ['http://example.org/files.json', 'ftp://example.org/files.json',
                         'https://name:secret@example.org/manifest.json',
                         'https://example.org/manifest.json#private']:
                with self.subTest(link=link), self.assertRaisesRegex(ValueError, 'HTTPS'):
                    prepare(IOS / 'GameInfo.plist', target, link)

    def test_public_client_id_validation(self):
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / 'Info.plist'
            for value in ('not-a-guid', '00000000441cc96b', 'client:secret'):
                with self.subTest(value=value), self.assertRaisesRegex(ValueError, 'GUID'):
                    prepare(IOS / 'GameInfo.plist', output, '', value)


if __name__ == '__main__':
    unittest.main()
