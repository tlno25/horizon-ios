"""Regression coverage for device-target IPA structure, independent of Xcode."""

import tempfile
import unittest
from pathlib import Path
from zipfile import ZipFile, ZipInfo

from verify_unsigned_ipa import validate


class UnsignedIpaTests(unittest.TestCase):
    def create_ipa(self, files):
        context = tempfile.TemporaryDirectory()
        self.addCleanup(context.cleanup)
        ipa = Path(context.name) / 'horizon-ios-unsigned.ipa'
        with ZipFile(ipa, 'w') as bundle:
            for name, contents in files.items():
                info = ZipInfo(name)
                info.create_system = 3
                info.external_attr = (0o100755 if name.endswith('/HorizonIOS') else 0o100644) << 16
                bundle.writestr(info, contents)
        return ipa

    def test_correct_payload_is_valid(self):
        ipa = self.create_ipa({
            'Payload/HorizonIOS.app/HorizonIOS': b'\xfe\xed\xfa\xcf',
            'Payload/HorizonIOS.app/Info.plist': b'bplist00',
        })
        self.assertEqual(validate(ipa), 2)

    def test_simulator_zip_or_wrong_path_is_rejected(self):
        for files in (
            {'HorizonIOS.app/HorizonIOS': b'bin', 'HorizonIOS.app/Info.plist': b'plist'},
            {'Payload/HorizonIOS.app/Info.plist': b'plist'},
            {'Payload/HorizonIOS.app/HorizonIOS': b'bin'},
            {'Payload/Other.app/HorizonIOS': b'bin', 'Payload/Other.app/Info.plist': b'plist'},
        ):
            with self.subTest(files=files), self.assertRaises(ValueError):
                validate(self.create_ipa(files))

    def test_rejects_duplicate_and_traversal_entries(self):
        for name in ('Payload/../evil', '/Payload/HorizonIOS.app/evil'):
            with self.subTest(name=name), self.assertRaises(ValueError):
                validate(self.create_ipa({
                    'Payload/HorizonIOS.app/HorizonIOS': b'bin',
                    'Payload/HorizonIOS.app/Info.plist': b'plist',
                    name: b'bad',
                }))
        ipa = self.create_ipa({
            'Payload/HorizonIOS.app/HorizonIOS': b'bin',
            'Payload/HorizonIOS.app/Info.plist': b'plist',
        })
        with ZipFile(ipa, 'a') as bundle:
            import warnings
            with warnings.catch_warnings():
                warnings.simplefilter('ignore', UserWarning)
                bundle.writestr('Payload/HorizonIOS.app/HorizonIOS', b'duplicate')
        with self.assertRaises(ValueError):
            validate(ipa)

    def test_rejects_non_executable_binary(self):
        ipa = self.create_ipa({'Payload/HorizonIOS.app/HorizonIOS': b'bin', 'Payload/HorizonIOS.app/Info.plist': b'plist'})
        with ZipFile(ipa, 'r') as bundle:
            info = bundle.getinfo('Payload/HorizonIOS.app/HorizonIOS')
            info.external_attr = (0o100644 << 16)
            # Repackage a binary without an executable bit.
            contents = {entry.filename: bundle.read(entry) for entry in bundle.infolist()}
        with ZipFile(ipa, 'w') as bundle:
            for name, payload in contents.items():
                item = ZipInfo(name)
                item.create_system = 3
                item.external_attr = (0o100644 << 16)
                bundle.writestr(item, payload)
        with self.assertRaisesRegex(ValueError, 'executable permissions'):
            validate(ipa)
