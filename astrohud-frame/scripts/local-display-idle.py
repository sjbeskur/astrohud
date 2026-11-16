#!/usr/bin/env python3
"""Run on the local reference Pi as root. Signed installed releases stay intact.

Stage the ARM64 frame at /tmp/memory-lane-display-idle first, then invoke
with observe|auto and its SHA-256. Invoke with rollback to restore the unit.
"""
import hashlib
import json
import os
from pathlib import Path
import struct
import subprocess
import sys
import time

SERVICE = 'astrohud-frame.service'
DIRECTORY = Path('/opt/memory-lane/local-tests/display-idle')
BINARY = DIRECTORY/'astrohud-frame'
METADATA = DIRECTORY/'original.json'
DROPIN = Path('/etc/systemd/system/astrohud-frame.service.d/99-local-display-idle.conf')
MARKER = '# Memory Lane display-idle local test\n'


def systemctl(*args):
    return subprocess.run(['systemctl', *args], check=True, capture_output=True, text=True).stdout.strip()


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def atomic(path, data, mode):
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_name(path.name+'.local-new')
    with temporary.open('xb') as stream:
        os.fchmod(stream.fileno(), mode)
        stream.write(data)
        stream.flush()
        os.fsync(stream.fileno())
    os.replace(temporary, path)
    fd = os.open(path.parent, os.O_RDONLY | os.O_DIRECTORY)
    try:
        os.fsync(fd)
    finally:
        os.close(fd)


def rollback():
    original = json.loads(METADATA.read_text())
    if original['dropin'] is None:
        if DROPIN.exists():
            assert DROPIN.read_text().startswith(MARKER), 'Unrecognized override; refusing removal'
            DROPIN.unlink()
    else:
        atomic(DROPIN, original['dropin'].encode(), 0o644)
    systemctl('daemon-reload')
    systemctl('restart', SERVICE)
    systemctl('is-active', '--quiet', SERVICE)
    print(json.dumps({'local_display_test': 'rolled back', 'viewer_active': True}))


assert os.geteuid() == 0, 'Use sudo on the local reference Pi'
model = Path('/proc/device-tree/model').read_text().strip('\0')
assert model.startswith('Raspberry Pi Zero 2 W'), 'Only the reference Pi Zero 2 W is supported by this helper'
mode = sys.argv[1] if len(sys.argv) > 1 else ''
if mode == 'rollback':
    rollback()
    sys.exit(0)
assert mode in ('observe', 'auto'), 'Expected observe, auto or rollback'
expected = sys.argv[2]
data = Path('/tmp/memory-lane-display-idle').read_bytes()
assert hashlib.sha256(data).hexdigest() == expected, 'Artifact checksum mismatch'
assert data[:6] == b'\x7fELF\x02\x01' and struct.unpack_from('<H', data, 18)[0] == 183, 'Expected ARM64 ELF'
state = Path('/opt/memory-lane/state.json')
assert json.loads(state.read_text()).get('pending') is None, 'An application update is in progress'
protected = [Path('/etc/machine-id'), Path('/etc/astrohud/device.json'), Path('/var/lib/astrohud/device-credential'), state]
protected += list(Path('/etc/NetworkManager/system-connections').glob('*'))
before = {p: digest(p) for p in protected if p.is_file()}
installed = Path('/usr/local/bin/astrohud-frame').resolve()
installed_hash = digest(installed)
DIRECTORY.mkdir(parents=True, exist_ok=True, mode=0o755)
if not METADATA.exists():
    original = DROPIN.read_text() if DROPIN.exists() else None
    assert original is None, 'A display-idle test override already exists without its rollback record'
    atomic(METADATA, json.dumps({'dropin': original, 'installed_path': str(installed), 'installed_sha256': installed_hash}).encode(), 0o600)
elif DROPIN.exists():
    assert DROPIN.read_text().startswith(MARKER), 'Unrecognized local test override'
atomic(BINARY, data, 0o755)
try:
    atomic(DROPIN, (MARKER+'[Service]\nExecStart=\nExecStart='+str(BINARY)+'\nEnvironment=ASTROHUD_DISPLAY_IDLE='+mode+'\nStandardOutput=journal\nStandardError=journal\n').encode(), 0o644)
    systemctl('daemon-reload')
    systemctl('restart', SERVICE)
    time.sleep(3)
    systemctl('is-active', '--quiet', SERVICE)
    pid = systemctl('show', '--property=MainPID', '--value', SERVICE)
    assert os.readlink('/proc/'+pid+'/exe') == str(BINARY), 'Prototype is not the running viewer'
    assert {p: digest(p) for p in before} == before, 'Identity, Wi-Fi or updater state changed'
    assert digest(installed) == installed_hash, 'Installed release artifact changed'
    systemctl('is-active', '--quiet', 'astrohud-provisioner.service')
except Exception:
    rollback()
    raise
print(json.dumps({'mode': mode, 'viewer_active': True, 'pid': int(pid), 'prototype_sha256': expected,
                  'installed_release_unchanged': True, 'identity_networks_and_updater_state_preserved': True}))
