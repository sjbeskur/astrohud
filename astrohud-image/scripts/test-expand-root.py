#!/usr/bin/env python3
"""Exercise first-boot decisions with fake disk commands; never access a disk."""
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

SCRIPT = Path(__file__).with_name('expand-root.sh')


class ExpansionTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.root = Path(self.directory.name)
        binary = self.root / 'bin'
        binary.mkdir()
        stub = '''#!/usr/bin/env python3
import os,sys,json
from pathlib import Path
name=Path(sys.argv[0]).name
with open(os.environ['COMMAND_LOG'],'a') as log:log.write(json.dumps([name]+sys.argv[1:])+'\\n')
if name=='findmnt':print('/dev/mmcblk0p2' if sys.argv[-2]=='SOURCE' else os.environ.get('ROOT_FS','ext4'))
elif name=='lsblk':print('mmcblk0')
elif name=='cat':print(os.environ.get('PARTITION','2'))
elif name=='growpart':
 print(os.environ.get('GROW_RESULT','CHANGED: partition=2'))
 sys.exit(int(os.environ.get('GROW_EXIT','0')))
elif name=='resize2fs':sys.exit(int(os.environ.get('RESIZE_EXIT','0')))
'''
        for name in ['findmnt', 'lsblk', 'cat', 'growpart', 'resize2fs']:
            path = binary / name
            path.write_text(stub)
            path.chmod(0o755)
        self.environment = os.environ.copy()
        self.environment.update(PATH=str(binary)+':'+os.environ['PATH'],
                                STATE_DIRECTORY=str(self.root/'state'), COMMAND_LOG=str(self.root/'commands'))

    def tearDown(self):
        self.directory.cleanup()

    def run_script(self, **overrides):
        result = subprocess.run(['bash', str(SCRIPT)], env=self.environment | overrides,
                                capture_output=True, text=True)
        marker = (self.root/'state/root-expanded').exists()
        commands = (self.root/'commands').read_text()
        return result, marker, commands

    def test_success_marks_complete_after_resize(self):
        result, marker, commands = self.run_script()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertTrue(marker)
        self.assertIn('["growpart", "/dev/mmcblk0", "2"]', commands)
        self.assertIn('["resize2fs", "/dev/mmcblk0p2"]', commands)

    def test_already_full_partition_still_resizes_filesystem(self):
        result, marker, commands = self.run_script(GROW_EXIT='1', GROW_RESULT='NOCHANGE: partition 2 is full')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertTrue(marker)
        self.assertIn('resize2fs', commands)

    def test_partition_failure_does_not_resize_or_mark_complete(self):
        result, marker, commands = self.run_script(GROW_EXIT='2', GROW_RESULT='FAILED: cannot grow')
        self.assertNotEqual(result.returncode, 0)
        self.assertFalse(marker)
        self.assertNotIn('resize2fs', commands)

    def test_resize_failure_remains_retryable(self):
        result, marker, _ = self.run_script(RESIZE_EXIT='1')
        self.assertNotEqual(result.returncode, 0)
        self.assertFalse(marker)

    def test_unexpected_layout_is_never_modified(self):
        result, marker, commands = self.run_script(PARTITION='1')
        self.assertNotEqual(result.returncode, 0)
        self.assertFalse(marker)
        self.assertNotIn('growpart', commands)


if __name__ == '__main__':
    unittest.main()
