#!/usr/bin/env python3
"""Host-only release packaging; the private Ed25519 key never goes to a Pi."""
import argparse
import datetime
import hashlib
import json
from pathlib import Path
import re
import shutil
import subprocess
import tempfile
from urllib.parse import urlsplit

TARGET = 'aarch64-unknown-linux-gnu'
MAX_BINARY = 64 * 1024 * 1024


def https(value):
    url = urlsplit(value)
    if url.scheme != 'https' or not url.hostname or url.username or url.password or url.fragment or url.query:
        raise ValueError('base URL must use HTTPS without credentials, query or fragment')
    return value.rstrip('/')


def artifact(path, base):
    data = path.read_bytes()
    if not 64 <= len(data) <= MAX_BINARY:
        raise ValueError(f'invalid binary size: {path}')
    if data[:6] != b'\x7fELF\x02\x01' or data[18:20] != b'\xb7\x00':
        raise ValueError(f'expected an ARM64 ELF executable: {path}')
    return dict(url=f'{base}/{path.name}', bytes=len(data), sha256=hashlib.sha256(data).hexdigest())


def package(args):
    if not re.fullmatch(r'(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)', args.version):
        raise ValueError('version must be a stable MAJOR.MINOR.PATCH release')
    if not 1 <= args.sequence <= (2**64 - 1) or not 1 <= args.expires_days <= 90:
        raise ValueError('sequence must be positive; metadata lifetime must be 1–90 days')
    base = https(args.base_url)
    release = dict(schema=1, version=args.version, sequence=args.sequence,
                   expires_unix=int((datetime.datetime.now(datetime.timezone.utc) + datetime.timedelta(days=args.expires_days)).timestamp()),
                   target=TARGET,
                   frame=artifact(args.bin_dir / 'astrohud-frame', base),
                   provisioner=artifact(args.bin_dir / 'astrohud-provisioner', base))
    payload = json.dumps(release, separators=(',', ':'), ensure_ascii=True)
    with tempfile.TemporaryDirectory(prefix='memory-lane-sign-') as tmp:
        payload_path = Path(tmp) / 'payload'
        signature_path = Path(tmp) / 'signature'
        payload_path.write_bytes(payload.encode('utf-8'))
        subprocess.run(['openssl', 'pkeyutl', '-sign', '-rawin', '-inkey', str(args.key),
                        '-in', str(payload_path), '-out', str(signature_path)], check=True)
        signature = signature_path.read_bytes()
    if len(signature) != 64:
        raise ValueError('signing key must be Ed25519')
    # Refuse to overwrite a published bundle accidentally.
    args.output.mkdir(parents=True, exist_ok=False)
    for name in ('astrohud-frame', 'astrohud-provisioner'):
        shutil.copyfile(args.bin_dir / name, args.output / name)
        # Detect accidental file changes between hashing and copying.
        expected = release['frame' if name == 'astrohud-frame' else 'provisioner']
        if artifact(args.output / name, base) != expected:
            raise ValueError('binary changed while packaging')
    (args.output / 'stable.json').write_text(json.dumps(dict(payload=payload, signature=signature.hex()), indent=2) + '\n')
    return release


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--version', required=True)
    parser.add_argument('--sequence', type=int, required=True)
    parser.add_argument('--expires-days', type=int, default=30)
    parser.add_argument('--base-url', required=True, help='Immutable release asset directory URL')
    parser.add_argument('--key', type=Path, required=True, help='Private Ed25519 PEM key (outside repo)')
    parser.add_argument('--bin-dir', type=Path, default=Path('target/pi-bookworm/aarch64-unknown-linux-gnu/release'))
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    try:
        release = package(args)
    except (ValueError, OSError, subprocess.CalledProcessError) as error:
        parser.exit(1, f'release packaging failed: {error}\n')
    print(f"Packaged {release['version']} (sequence {release['sequence']}) in {args.output}")


if __name__ == '__main__':
    main()
