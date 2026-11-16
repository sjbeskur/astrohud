#!/usr/bin/env python3
"""Exercise setup DNS against a real dnsmasq and an unresponsive upstream.

Requires dnsmasq and permission to bind loopback UDP sockets; uses high ports
and never changes workstation DNS, DHCP, routes or network interfaces.
"""
import os
from pathlib import Path
import shutil
import socket
import struct
import subprocess
import time
import unittest

CONFIG = Path(os.environ.get('MEMORY_LANE_CAPTIVE_DNS_CONFIG',
    str(Path(__file__).resolve().parents[1]/'deploy/astrohud-captive-dns.conf')))


class CaptiveDnsTests(unittest.TestCase):
    def setUp(self):
        if not shutil.which('dnsmasq'):
            self.skipTest('dnsmasq is required')
        self.upstream = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
        self.upstream.bind(('127.0.0.1', 0))
        self.addCleanup(self.upstream.close)
        with socket.socket(socket.AF_INET, socket.SOCK_DGRAM) as reservation:
            reservation.bind(('127.0.0.1', 0))
            self.port = reservation.getsockname()[1]
        self.daemon = subprocess.Popen([
            'dnsmasq', '--no-daemon', '--conf-file='+str(CONFIG),
            '--port='+str(self.port), '--listen-address=127.0.0.1',
            '--bind-interfaces', '--no-hosts', '--no-resolv', '--pid-file=',
            '--server=127.0.0.1#'+str(self.upstream.getsockname()[1]),
            '--log-facility=-',
        ], stdout=subprocess.DEVNULL, stderr=subprocess.PIPE)
        self.addCleanup(self.stop_daemon)
        deadline = time.monotonic()+3
        while time.monotonic() < deadline:
            if self.daemon.poll() is not None:
                self.fail(self.daemon.stderr.read().decode())
            try:
                self.query('ready.example', 1, timeout=0.1)
                break
            except (TimeoutError, ConnectionRefusedError):
                time.sleep(0.02)
        else:
            self.fail('dnsmasq did not start')

    def stop_daemon(self):
        if self.daemon.poll() is None:
            self.daemon.terminate()
        self.daemon.communicate(timeout=3)

    def query(self, name, kind, timeout=0.75):
        labels = b''.join(bytes([len(label)])+label.encode() for label in name.split('.'))+b'\0'
        packet = struct.pack('!6H', 0x4d4c, 0x0100, 1, 0, 0, 0)+labels+struct.pack('!2H', kind, 1)
        with socket.socket(socket.AF_INET, socket.SOCK_DGRAM) as client:
            client.settimeout(timeout)
            start = time.monotonic()
            client.sendto(packet, ('127.0.0.1', self.port))
            reply, _ = client.recvfrom(4096)
            elapsed = time.monotonic()-start
        identifier, flags, _, answers, _, _ = struct.unpack('!6H', reply[:12])
        self.assertEqual(identifier, 0x4d4c)
        self.assertEqual(flags & 0xf, 0, 'expected a successful local DNS response')
        self.assertTrue(flags & 0x8000)
        return reply, answers, elapsed

    def test_probe_records_answer_locally_without_internet(self):
        for kind, label in [(1, 'A'), (28, 'AAAA'), (64, 'SVCB'), (65, 'HTTPS')]:
            with self.subTest(record=label):
                reply, answers, elapsed = self.query(label.lower()+'.captive.example', kind)
                if kind == 1:
                    self.assertEqual(answers, 1)
                    self.assertEqual(reply[-4:], socket.inet_aton('10.42.0.1'))
                else:
                    self.assertEqual(answers, 0)
                print(f'{label}: local response in {elapsed*1000:.1f} ms', flush=True)
        self.upstream.setblocking(False)
        with self.assertRaises(BlockingIOError, msg='setup DNS must not forward probes upstream'):
            self.upstream.recvfrom(4096)


if __name__ == '__main__':
    unittest.main()
