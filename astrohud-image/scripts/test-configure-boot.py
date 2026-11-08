#!/usr/bin/env python3
import importlib.util
from pathlib import Path
import tempfile
import unittest

spec=importlib.util.spec_from_file_location('boot',Path(__file__).with_name('configure-boot.py'))
boot=importlib.util.module_from_spec(spec)
spec.loader.exec_module(boot)

class BootTests(unittest.TestCase):
    def test_quiet_boot_is_idempotent_and_preserves_root_and_serial(self):
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory); firmware=root/'boot/firmware';firmware.mkdir(parents=True)
            (firmware/'cmdline.txt').write_text('console=serial0,115200 console=tty1 root=PARTUUID=abc rootwait splash loglevel=7\n')
            (firmware/'config.txt').write_text('dtoverlay=vc4-kms-v3d\n[pi4]\narm_boost=1\n')
            boot.configure(root)
            first=(firmware/'cmdline.txt').read_text();config=(firmware/'config.txt').read_text()
            boot.configure(root)
            self.assertEqual((firmware/'cmdline.txt').read_text(),first)
            self.assertEqual((firmware/'config.txt').read_text(),config)
            self.assertIn('root=PARTUUID=abc',first);self.assertIn('console=serial0,115200',first)
            self.assertIn('console=tty3',first);self.assertNotIn('console=tty1',first)
            self.assertNotIn('splash',first);self.assertIn('logo.nologo',first)
            self.assertEqual(str((root/'etc/systemd/system/getty@tty1.service').readlink()),'/dev/null')
            self.assertIn('disable_splash=1',config)
    def test_missing_root_is_rejected_without_changing_cmdline(self):
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory);firmware=root/'boot/firmware';firmware.mkdir(parents=True)
            (firmware/'cmdline.txt').write_text('console=tty1\n');(firmware/'config.txt').write_text('')
            with self.assertRaises(ValueError):boot.configure(root)
            self.assertEqual((firmware/'cmdline.txt').read_text(),'console=tty1\n')

if __name__=='__main__':unittest.main()
