"""Confirm the Xcode bridge selects the matching Rust staticlib architecture."""
import os
import pathlib
import subprocess
import tempfile
import unittest

SCRIPT = pathlib.Path(__file__).parent.parent / 'build_rust.sh'


class RustTargetSelectionTests(unittest.TestCase):
    def invoke(self, platform, architectures):
        with tempfile.TemporaryDirectory() as root:
            home = pathlib.Path(root)
            cargo = home / '.cargo/bin/cargo'
            cargo.parent.mkdir(parents=True)
            cargo.write_text('#!/bin/sh\nprintf "%s\\n" "$*"\n')
            cargo.chmod(0o755)
            env = dict(os.environ, HOME=str(home), PLATFORM_NAME=platform,
                       ARCHS=architectures, PATH=f'{cargo.parent}:{os.environ["PATH"]}')
            return subprocess.run(['bash', str(SCRIPT)], env=env, text=True,
                                  capture_output=True, timeout=10)

    def test_intel_ios_simulator(self):
        output = self.invoke('iphonesimulator', 'x86_64')
        self.assertEqual(output.returncode, 0, output.stderr)
        self.assertIn('--target x86_64-apple-ios', output.stdout)

    def test_apple_silicon_ios_simulator(self):
        output = self.invoke('iphonesimulator', 'arm64')
        self.assertEqual(output.returncode, 0, output.stderr)
        self.assertIn('--target aarch64-apple-ios-sim', output.stdout)

    def test_device_always_arm64(self):
        output = self.invoke('iphoneos', 'arm64')
        self.assertEqual(output.returncode, 0, output.stderr)
        self.assertIn('--target aarch64-apple-ios', output.stdout)

    def test_unsupported_architecture_is_rejected(self):
        for platform, arch in [('iphoneos', 'x86_64'), ('iphonesimulator', 'i386'),
                               ('macosx', 'arm64')]:
            with self.subTest(platform=platform, arch=arch):
                output = self.invoke(platform, arch)
                self.assertNotEqual(output.returncode, 0)
