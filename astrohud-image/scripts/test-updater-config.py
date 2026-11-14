#!/usr/bin/env python3
"""Keep secrets and insecure channel settings out of distributable images."""
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

spec = importlib.util.spec_from_file_location(
    "updater_config", Path(__file__).with_name("validate-updater-config.py"))
config = importlib.util.module_from_spec(spec)
spec.loader.exec_module(config)


class UpdaterConfigTests(unittest.TestCase):
    def test_only_public_https_channel_and_verification_key_are_accepted(self):
        valid = {"manifest_url": "https://updates.example/stable.json", "public_key": "ab" * 32}
        invalid = [dict(valid, private_key="secret"), dict(valid, public_key="short")]
        invalid += [dict(valid, manifest_url=url) for url in (
            "http://example/stable.json", "https://user:secret@example/stable.json",
            "https://example/stable.json?token=secret", "https://example/stable.json#fragment",
            "https://example/stable.json\n", "https:///stable.json")]
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "config.json"
            path.write_text(json.dumps(valid))
            self.assertEqual(config.validate(path), valid)
            for value in invalid:
                with self.subTest(value=value):
                    path.write_text(json.dumps(value))
                    with self.assertRaises(ValueError):
                        config.validate(path)


if __name__ == "__main__":
    unittest.main()
