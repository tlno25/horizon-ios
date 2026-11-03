"""Confirm the Xcode bridge selects the matching Rust staticlib architecture."""
import os
import pathlib
import subprocess
import tempfile
import unittest

SCRIPT = pathlib.Path(__file__).parent.parent / 'build_rust.sh'


class RustTargetSelectionTests(unittest.TestCase):
    def invoke(self, platform, architectures, configuration='Release'):
        with tempfile.TemporaryDirectory() as root:
            home = pathlib.Path(root)
            cargo = home / '.cargo/bin/cargo'
            cargo.parent.mkdir(parents=True)
            cargo.write_text('#!/bin/sh\nprintf "%s\\n" "$*"\n')
            cargo.chmod(0o755)
            env = dict(os.environ, HOME=str(home), PLATFORM_NAME=platform,
                       ARCHS=architectures, CONFIGURATION=configuration,
                       PATH=f'{cargo.parent}:{os.environ["PATH"]}')
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

    def test_debug_uses_fast_profile_to_reduce_ci_compile_time(self):
        output = self.invoke('iphoneos', 'arm64', 'Debug')
        self.assertEqual(output.returncode, 0, output.stderr)
        self.assertIn('--profile fast', output.stdout)
        self.assertNotIn('--release', output.stdout)

    def test_release_keeps_optimized_profile(self):
        output = self.invoke('iphoneos', 'arm64', 'Release')
        self.assertEqual(output.returncode, 0, output.stderr)
        self.assertIn('--release', output.stdout)
        self.assertNotIn('--profile fast', output.stdout)

    def test_xcode_debug_links_fast_rust_staticlib(self):
        project = (SCRIPT.parent / 'HorizonIOS.xcodeproj/project.pbxproj').read_text()
        debug = project.split('/* Target Debug */ = {', 1)[1].split('/* Target Release */ = {', 1)[0]
        release = project.split('/* Target Release */ = {', 1)[1]
        for target in ('aarch64-apple-ios', 'aarch64-apple-ios-sim', 'x86_64-apple-ios'):
            self.assertIn(f'{target}/fast', debug)
            self.assertIn(f'{target}/release', release)

    def test_unsupported_architecture_is_rejected(self):
        for platform, arch in [('iphoneos', 'x86_64'), ('iphonesimulator', 'i386'),
                               ('macosx', 'arm64')]:
            with self.subTest(platform=platform, arch=arch):
                output = self.invoke(platform, arch)
                self.assertNotEqual(output.returncode, 0)
