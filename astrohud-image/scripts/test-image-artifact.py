#!/usr/bin/env python3
import importlib.util
from pathlib import Path
import shutil
import struct
import subprocess
import tempfile
import unittest

spec = importlib.util.spec_from_file_location('artifact', Path(__file__).with_name('image-artifact.py'))
artifact = importlib.util.module_from_spec(spec)
spec.loader.exec_module(artifact)


class ArtifactTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.root = Path(self.directory.name)
        self.image = self.root / 'test image.img'
        data = bytearray(2 * 1024 * 1024)
        data[510:512] = b'\x55\xaa'
        data[450] = 0x83
        struct.pack_into('<II', data, 454, 2048, 2048)
        sb = 1024 * 1024 + 1024
        struct.pack_into('<I', data, sb + 4, 1024)
        struct.pack_into('<I', data, sb + 12, 512)
        struct.pack_into('<H', data, sb + 56, 0xef53)
        self.image.write_bytes(data)

    def tearDown(self):
        self.directory.cleanup()

    def test_sizes_are_distinct_from_allocated_and_free_space(self):
        report = artifact.inspect(self.image)
        self.assertEqual(report['bytes'], 2 * 1024 * 1024)
        self.assertEqual(report['partitions'][0]['bytes'], 1024 * 1024)
        self.assertEqual(report['partitions'][0]['allocated_bytes'], 512 * 1024)
        self.assertEqual(report['partitions'][0]['free_bytes'], 512 * 1024)

    def test_partition_outside_image_is_rejected(self):
        with self.image.open('r+b') as handle:
            handle.seek(458)
            handle.write(struct.pack('<I', 100000))
        with self.assertRaisesRegex(ValueError, 'extends beyond'):
            artifact.inspect(self.image)

    def test_checksum_is_for_chosen_file_and_detects_changes(self):
        artifact.checksum(self.image)
        artifact.verify(self.image)
        with self.image.open('r+b') as handle:
            handle.write(b'changed')
        with self.assertRaisesRegex(ValueError, 'does not match'):
            artifact.verify(self.image)

    def test_publish_refuses_overwrite_and_preserves_source(self):
        before = artifact.digest(self.image)
        output = self.root / 'release.img'
        artifact.publish(self.image, output)
        self.assertEqual(artifact.digest(output), before)
        artifact.verify(output)
        with self.assertRaises(FileExistsError):
            artifact.publish(self.image, output)
        self.assertEqual(artifact.digest(self.image), before)
        self.assertEqual(artifact.digest(output), before)
        self.assertTrue(Path(str(output) + '.json').is_file())

    @unittest.skipUnless(shutil.which('zstd'), 'zstd is not installed')
    def test_compressed_artifact_roundtrips_and_has_own_checksum(self):
        artifact.checksum(self.image)
        artifact.compress(self.image)
        compressed = Path(str(self.image) + '.zst')
        artifact.verify(compressed)
        result = subprocess.run(['zstd', '-dc', str(compressed)], check=True, capture_output=True)
        self.assertEqual(result.stdout, self.image.read_bytes())
        self.assertLess(compressed.stat().st_size, self.image.stat().st_size)
        with self.assertRaises(FileExistsError):
            artifact.compress(self.image)


if __name__ == '__main__':
    unittest.main()
