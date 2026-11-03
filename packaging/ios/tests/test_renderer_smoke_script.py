"""Exercise the Simulator diagnostic shell script without needing a Mac."""

import json
import os
import pathlib
import subprocess
import tempfile
import unittest

SCRIPT = pathlib.Path(__file__).parent / "simulator_renderer_smoke.sh"
MOCK_XCRUN = '''#!/usr/bin/env python3
import json,os,pathlib,sys
args=sys.argv[1:]
if os.environ.get('MOCK_RECORD_COMMANDS'):
    with open(os.environ['MOCK_RECORD_COMMANDS'], 'a', encoding='utf-8') as record:
        record.write(' '.join(args) + '\\n')
if os.environ.get('HORIZON_IOS_SIMULATOR_REUSE_BOOT') == '1' and args[:2] in (['simctl','boot'], ['simctl','bootstatus']):
    sys.exit('No simulator boot/bootstatus call allowed on reused boot')
runner=pathlib.Path(os.environ['RUNNER_TEMP'])
app_data=runner/'simulator-container'
status=app_data/'Library/Caches/horizon-ios-graphics.json'
flag=app_data/'Library/Caches/horizon-ios-renderer-smoke.flag'
if args[:2]==['simctl','get_app_container']:
    print(app_data)
elif args[:2]==['simctl','launch']:
    assert os.environ.get('SIMCTL_CHILD_HORIZON_IOS_RENDERER_SMOKE')=='1'
    assert not status.exists(), 'Old cached result was not cleared'
    assert flag.read_text().strip()=='1', 'No explicit in-container smoke request'
    status.parent.mkdir(parents=True,exist_ok=True)
    status.write_text(json.dumps({'state':2,'error':'','engine':1,
          'player':'Player00001','core_load':0,
          'renderer_preview':int(os.environ['MOCK_PREVIEW']),
          'smoke_mode':int(os.environ.get('MOCK_ACK', '1')),
          'renderer_stage':'Horizon renderer: creating the pipelines'}))
elif args[:3]==['simctl','io',os.environ['HORIZON_IOS_SIMULATOR_UDID']]:
    pathlib.Path(args[-1]).write_bytes(b'png mock')
elif args[:3]==['simctl','spawn',os.environ['HORIZON_IOS_SIMULATOR_UDID']]:
    print('Mock Horizon native renderer log')
elif args[:2] in (['simctl','boot'],['simctl','bootstatus'],
                 ['simctl','install'],['simctl','shutdown']):
    pass
else:
    sys.exit(f'Unexpected xcrun command: {args}')
'''


class RendererSmokeScriptTests(unittest.TestCase):
    def run_mock(self, state, ack=1, reuse_boot=False):
        with tempfile.TemporaryDirectory() as work:
            root = pathlib.Path(work)
            app = root / 'derived/Build/Products/Debug-iphonesimulator/HorizonIOS.app'
            app.mkdir(parents=True)
            mock = root / 'bin/xcrun'
            mock.parent.mkdir()
            mock.write_text(MOCK_XCRUN)
            mock.chmod(0o755)
            status = root / 'simulator-container/Library/Caches/horizon-ios-graphics.json'
            status.parent.mkdir(parents=True)
            status.write_text(json.dumps({'renderer_preview':3}))
            env = dict(os.environ, PATH=str(mock.parent) + os.pathsep + os.environ['PATH'],
                       DERIVED_DATA=str(root/'derived'), RUNNER_TEMP=str(root),
                       HORIZON_IOS_SIMULATOR_UDID='fake-iphone', MOCK_PREVIEW=str(state), MOCK_ACK=str(ack),
                       MOCK_RECORD_COMMANDS=str(root/'commands.log'))
            if reuse_boot:
                env['HORIZON_IOS_SIMULATOR_REUSE_BOOT'] = '1'
            process = subprocess.run(['bash', str(SCRIPT)], env=env,
                                     capture_output=True, text=True, timeout=10)
            diagnostic = root/'horizon-ios-boot-renderer-status.json'
            self.assertTrue(diagnostic.exists(), process.stderr)
            self.assertEqual(json.loads(diagnostic.read_text())['renderer_preview'], state)
            self.assertTrue((root/'horizon-ios-boot-renderer-screenshot.png').exists())
            self.assertIn('Mock Horizon', (root/'horizon-ios-boot-renderer-native.log').read_text())
            if reuse_boot:
                self.assertNotIn('simctl boot', (root/'commands.log').read_text())
            return process

    def test_reuse_boot_skips_simulator_boot_and_bootstatus(self):
        result = self.run_mock(3, reuse_boot=True)
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_success_checks_a_real_renderer_frame(self):
        result = self.run_mock(3)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn('presented a frame', result.stdout)

    def test_inactive_diagnostic_mode_cannot_pass_even_if_preview_state_is_ready(self):
        result = self.run_mock(3, ack=0)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('diagnostic mode', result.stderr)

    def test_failed_renderer_keeps_diagnostics_without_falsely_passing(self):
        result = self.run_mock(4)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('did not present', result.stderr)


if __name__ == '__main__':
    unittest.main()
