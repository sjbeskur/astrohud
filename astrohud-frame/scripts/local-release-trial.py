#!/usr/bin/env python3
"""Install/rollback a reversible trial on the reference Pi. Never resets Wi-Fi.

Stage the frame, provisioner and guard in /tmp/memory-lane-release-trial, then
run as root: install FRAME_SHA256 PROVISIONER_SHA256 GUARD_SHA256.
Rollback restores the service overrides that preceded this trial.
"""
import hashlib
import json
import os
from pathlib import Path
import struct
import subprocess
import sys
import time

ROOT = Path('/opt/memory-lane/local-tests/release-0.2')
RECORD = ROOT / 'original.json'
NAMES = ['astrohud-frame', 'astrohud-provisioner', 'astrohud-wifi-guard']
UNITS = [name + '.service' for name in NAMES]
DROPINS = [Path('/etc/systemd/system') / (unit + '.d') / '99-local-release-trial.conf' for unit in UNITS]
MARKER = '# Memory Lane release/0.2.x local trial\n'


def systemctl(*arguments):
    return subprocess.run(['systemctl', *arguments], check=True, capture_output=True, text=True).stdout.strip()


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def atomic(path, data, mode):
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_name(path.name + '.trial-new')
    with temporary.open('xb') as stream:
        os.fchmod(stream.fileno(), mode)
        stream.write(data)
        stream.flush()
        os.fsync(stream.fileno())
    os.replace(temporary, path)
    descriptor = os.open(path.parent, os.O_RDONLY | os.O_DIRECTORY)
    try:
        os.fsync(descriptor)
    finally:
        os.close(descriptor)


def restore():
    previous = json.loads(RECORD.read_text())
    for path in DROPINS:
        if path.exists():
            assert path.read_text().startswith(MARKER), 'Unrecognized service override'
        original = previous[str(path)]
        if original is None:
            path.unlink(missing_ok=True)
        else:
            atomic(path, original.encode(), 0o644)
    systemctl('daemon-reload')
    for unit in UNITS[:2]:
        systemctl('restart', unit)
        systemctl('is-active', '--quiet', unit)


def main():
    assert os.geteuid() == 0, 'Run as root on the reference Pi'
    assert Path('/proc/device-tree/model').read_text().startswith('Raspberry Pi Zero 2 W'), 'Wrong board'
    if sys.argv[1:] == ['rollback']:
        restore()
        print(json.dumps({'trial': 'rolled_back', 'previous_viewer_and_provisioner_active': True}))
        return
    assert len(sys.argv) == 5 and sys.argv[1] == 'install', 'Expected install plus three SHA256 hashes, or rollback'
    state = Path('/opt/memory-lane/state.json')
    assert json.loads(state.read_text()).get('pending') is None, 'Application update in progress'
    assert not Path('/etc/astrohud/reset.json').exists(), 'Factory reset in progress'
    assert not Path('/var/lib/astrohud/provisioning-required').exists(), 'Wi-Fi setup in progress'
    assert not Path('/var/lib/astrohud/reset-in-progress').exists(), 'Factory reset in progress'
    artifacts = []
    for index, (name, expected) in enumerate(zip(NAMES, sys.argv[2:])):
        data = (Path('/tmp/memory-lane-release-trial') / name).read_bytes()
        assert hashlib.sha256(data).hexdigest() == expected, 'Artifact checksum mismatch: ' + name
        if index < 2:
            assert data[:6] == b'\x7fELF\x02\x01' and struct.unpack_from('<H', data, 18)[0] == 183, 'Expected ARM64 ELF'
        artifacts.append(data)
    protected = [Path('/etc/machine-id'), Path('/etc/astrohud/device.json'),
                 Path('/etc/astrohud/manufacturing.json'), Path('/var/lib/astrohud/device-credential'), state,
                 Path('/usr/local/bin/astrohud-frame').resolve(), Path('/usr/local/sbin/astrohud-provisioner').resolve(),
                 Path('/usr/local/sbin/astrohud-wifi-guard').resolve()]
    protected += list(Path('/etc/NetworkManager/system-connections').glob('*'))
    before = {path: digest(path) for path in protected if path.is_file()}
    ROOT.mkdir(parents=True, exist_ok=True)
    if not RECORD.exists():
        assert all(not path.exists() for path in DROPINS), 'Trial override exists without rollback record'
        atomic(RECORD, json.dumps({str(path): None for path in DROPINS}).encode(), 0o600)
    else:
        assert all(not path.exists() or path.read_text().startswith(MARKER) for path in DROPINS), 'Unrecognized trial override'
    try:
        atomic(ROOT / 'local-release-trial.py', Path(__file__).read_bytes(), 0o755)
        for name, data, dropin in zip(NAMES, artifacts, DROPINS):
            atomic(ROOT / name, data, 0o755)
            settings = MARKER + '[Service]\nExecStart=\nExecStart=' + str(ROOT / name) + '\n'
            if name == 'astrohud-frame':
                settings += 'Environment=ASTROHUD_DISPLAY_IDLE=observe\n'
            atomic(dropin, settings.encode(), 0o644)
        systemctl('daemon-reload')
        for unit in UNITS[:2]:
            systemctl('restart', unit)
        time.sleep(4)
        for name, unit in zip(NAMES[:2], UNITS[:2]):
            systemctl('is-active', '--quiet', unit)
            pid = systemctl('show', '--property=MainPID', '--value', unit)
            assert os.readlink('/proc/' + pid + '/exe') == str(ROOT / name), 'Trial executable is not running'
        assert {path: digest(path) for path in before} == before, 'Signed releases, identity, Wi-Fi or updater state changed'
    except Exception:
        restore()
        raise
    print(json.dumps({'trial': 'installed', 'viewer_and_provisioner_active': True,
                      'display_idle': 'observe', 'guard_override_installed': True,
                      'signed_releases_identity_wifi_updater_preserved': True}))


if __name__ == '__main__':
    main()
