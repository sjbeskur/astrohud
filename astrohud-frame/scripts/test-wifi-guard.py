#!/usr/bin/env python3
"""Exercise the actual watchdog with isolated profiles and fake NetworkManager."""
import os
from pathlib import Path
import subprocess
import tempfile
import unittest


SCRIPT = Path(__file__).resolve().parents[1] / 'deploy/astrohud-wifi-guard.sh'


class WifiGuardTests(unittest.TestCase):
    def run_guard(self, connected=False, reconnect_fails=False, provisioning=False):
        with tempfile.TemporaryDirectory(prefix='wifi-guard-test-') as directory:
            root = Path(directory)
            source = SCRIPT.read_text()
            for name, path in {
                'backup': '/etc/astrohud/wifi-profile.nmconnection',
                'profile': '/etc/NetworkManager/system-connections/astrohud-wifi.nmconnection',
                'marker': '/var/lib/astrohud/provisioning-required',
            }.items():
                replacement = root / name
                source = source.replace(path, str(replacement))
                if name != 'marker' or provisioning:
                    replacement.touch()
                if name != 'marker':
                    replacement.write_text('test profile')
            script = root / 'guard.sh'
            script.write_text(source)
            nmcli = root / 'nmcli'
            nmcli.write_text('''#!/bin/sh
printf '%s\\n' "$*" >> "$GUARD_TEST_CALLS"
case "$*" in
  '-g connection.uuid connection show astrohud-wifi') printf 'test-uuid\\n' ;;
  '-t -f DEVICE,STATE device status') printf '%s\\n' "$GUARD_TEST_STATE" ;;
  'connection up astrohud-wifi ifname wlan0') exit "$GUARD_TEST_CONNECT_EXIT" ;;
esac
''')
            nmcli.chmod(0o755)
            (root / 'grep').symlink_to('/usr/bin/grep')
            # Intentionally no logger, and no host NetworkManager in PATH.
            calls = root / 'calls'
            environment = dict(os.environ, PATH=str(root),
                               GUARD_TEST_CALLS=str(calls),
                               GUARD_TEST_STATE='wlan0:connected' if connected else 'wlan0:disconnected',
                               GUARD_TEST_CONNECT_EXIT='1' if reconnect_fails else '0')
            result = subprocess.run(['/bin/sh', str(script)], env=environment,
                                    capture_output=True, text=True, timeout=5)
            return result, calls.read_text() if calls.exists() else ''

    def test_missing_logger_does_not_block_reconnection(self):
        result, calls = self.run_guard()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn('connection up astrohud-wifi ifname wlan0', calls)
        self.assertIn('requesting reconnection', result.stderr)

    def test_failed_reconnection_reports_retry_without_logger(self):
        result, calls = self.run_guard(reconnect_fails=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn('connection up astrohud-wifi ifname wlan0', calls)
        self.assertIn('the timer will retry', result.stderr)

    def test_connected_device_does_not_reconnect(self):
        result, calls = self.run_guard(connected=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertNotIn('connection up', calls)

    def test_setup_mode_leaves_network_manager_alone(self):
        result, calls = self.run_guard(provisioning=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(calls, '')


if __name__ == '__main__':
    unittest.main()
