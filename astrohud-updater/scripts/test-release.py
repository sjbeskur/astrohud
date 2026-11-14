#!/usr/bin/env python3
"""Exercise real OpenSSL signing without a production key or network."""
import argparse
import importlib.util
import json
from pathlib import Path
import subprocess
import tempfile
import unittest

spec = importlib.util.spec_from_file_location('release', Path(__file__).with_name('release.py'))
release = importlib.util.module_from_spec(spec)
spec.loader.exec_module(release)


class PackagingTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        self.key = self.root / 'test-only.pem'
        subprocess.run(['openssl', 'genpkey', '-algorithm', 'ED25519', '-out', str(self.key)], check=True)
        binary = bytearray(64)
        binary[:6] = b'\x7fELF\x02\x01'
        binary[18] = 183
        for name in ('astrohud-frame', 'astrohud-provisioner'):
            (self.root / name).write_bytes(binary)
        self.args = argparse.Namespace(version='0.6.0', sequence=6, expires_days=30,
            base_url='https://example/releases/v0.6.0', bin_dir=self.root, key=self.key,
            output=self.root / 'bundle')

    def test_signature_covers_exact_payload_and_copied_binaries(self):
        manifest = release.package(self.args)
        envelope = json.loads((self.args.output / 'stable.json').read_text())
        payload = self.root / 'payload'
        signature = self.root / 'signature'
        public = self.root / 'public.pem'
        payload.write_bytes(envelope['payload'].encode())
        signature.write_bytes(bytes.fromhex(envelope['signature']))
        subprocess.run(['openssl', 'pkey', '-in', str(self.key), '-pubout', '-out', str(public)], check=True)
        command = ['openssl', 'pkeyutl', '-verify', '-pubin', '-inkey', str(public),
                   '-rawin', '-in', str(payload), '-sigfile', str(signature)]
        subprocess.run(command, check=True, capture_output=True)
        self.assertEqual(manifest, json.loads(envelope['payload']))
        payload.write_bytes(payload.read_bytes() + b' ')
        self.assertNotEqual(subprocess.run(command, capture_output=True).returncode, 0)
        self.assertEqual((self.args.output / 'astrohud-frame').read_bytes(), (self.root / 'astrohud-frame').read_bytes())
        with self.assertRaises(FileExistsError):
            release.package(self.args)

    def test_wrong_architecture_refused_before_packaging(self):
        (self.root / 'astrohud-frame').write_bytes(b'\x7fELF\x02\x01' + bytes(58))
        with self.assertRaises(ValueError):
            release.package(self.args)
        self.assertFalse(self.args.output.exists())

    def test_release_parameters_and_urls(self):
        for url in ('http://example/a', 'https://user:secret@example/a', 'https://example/a#x'):
            with self.assertRaises(ValueError):
                release.https(url)
        self.args.version = '../unsafe'
        with self.assertRaises(ValueError):
            release.package(self.args)
        self.args.version = '0.6.0'
        self.args.sequence = 0
        with self.assertRaises(ValueError):
            release.package(self.args)


if __name__ == '__main__':
    unittest.main()
