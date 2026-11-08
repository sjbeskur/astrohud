#!/usr/bin/env python3
"""Inspect and package regular image files without mounting or touching a card."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import struct
import subprocess
import tempfile


def digest(path):
    with path.open('rb') as handle:
        return hashlib.file_digest(handle, 'sha256').hexdigest()


def verify(path):
    checksum = Path(str(path) + '.sha256')
    words = checksum.read_text().split()
    if not words or len(words[0]) != 64 or any(c not in '0123456789abcdefABCDEF' for c in words[0]):
        raise ValueError('Invalid SHA-256 file')
    if digest(path) != words[0].lower():
        raise ValueError(f'Checksum does not match: {path}')
    print(f'Checksum OK: {path}')


def checksum(path):
    Path(str(path) + '.sha256').write_text(f'{digest(path)}  {path.name}\n')


def inspect(path):
    report = {'file': path.name, 'bytes': path.stat().st_size}
    if path.suffix == '.zst':
        report['format'] = 'zstd'
        return report
    with path.open('rb') as handle:
        mbr = handle.read(512)
        if len(mbr) != 512 or mbr[510:512] != b'\x55\xaa':
            raise ValueError('Expected an MBR disk image')
        partitions = []
        for number in range(1, 5):
            entry = mbr[446+(number-1)*16:462+(number-1)*16]
            start, sectors = struct.unpack_from('<II', entry, 8)
            if not sectors:
                continue
            if (start + sectors) * 512 > report['bytes']:
                raise ValueError('Partition extends beyond the image')
            item = {'number': number, 'type': hex(entry[4]), 'bytes': sectors*512}
            if entry[4] == 0x83:
                handle.seek(start*512+1024)
                sb = handle.read(1024)
                if len(sb) == 1024 and struct.unpack_from('<H', sb, 56)[0] == 0xef53:
                    blocks = struct.unpack_from('<I', sb, 4)[0]
                    free = struct.unpack_from('<I', sb, 12)[0]
                    if struct.unpack_from('<I', sb, 96)[0] & 0x80:
                        blocks += struct.unpack_from('<I', sb, 336)[0] << 32
                        free += struct.unpack_from('<I', sb, 344)[0] << 32
                    exponent = struct.unpack_from('<I', sb, 24)[0]
                    if exponent > 6:
                        raise ValueError('Invalid ext4 block size')
                    block_size = 1024 << exponent
                    if free > blocks or blocks*block_size > sectors*512:
                        raise ValueError('Invalid ext4 allocation counts')
                    item.update(filesystem='ext4', allocated_bytes=(blocks-free)*block_size,
                                free_bytes=free*block_size)
            partitions.append(item)
        report['partitions'] = partitions
    return report


def publish(source, output):
    output.parent.mkdir(parents=True, exist_ok=True)
    report = inspect(source)
    temporary = None
    try:
        with tempfile.NamedTemporaryFile(dir=output.parent, prefix='.image-', delete=False) as handle:
            temporary = Path(handle.name)
        subprocess.run(['cp', '--reflink=auto', '--sparse=always', '--', str(source), str(temporary)], check=True)
        temporary.chmod(0o644)
        # Hard-link publication refuses to overwrite a previous image, atomically.
        os.link(temporary, output)
        report['file'] = output.name
        checksum(output)
        Path(str(output) + '.json').write_text(json.dumps(report, indent=2) + '\n')
    finally:
        if temporary is not None:
            temporary.unlink(missing_ok=True)
    print(json.dumps(report, indent=2))


def compress(path):
    verify(path)
    output = Path(str(path) + '.zst')
    temporary = None
    try:
        with tempfile.NamedTemporaryFile(dir=output.parent, prefix='.compress-', delete=False) as handle:
            temporary = Path(handle.name)
        subprocess.run(['zstd', '-T0', '-10', '--quiet', '--force', '-o', str(temporary), '--', str(path)], check=True)
        subprocess.run(['zstd', '--test', '--quiet', str(temporary)], check=True)
        temporary.chmod(0o644)
        os.link(temporary, output)
        checksum(output)
    finally:
        if temporary is not None:
            temporary.unlink(missing_ok=True)
    print(f'{path.stat().st_size:,} → {output.stat().st_size:,} bytes: {output}')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('action', choices=['info', 'verify', 'compress', 'publish'])
    parser.add_argument('image', type=Path)
    parser.add_argument('output', type=Path, nargs='?')
    parser.add_argument('--json', action='store_true', help='Print machine-readable size data')
    args = parser.parse_args()
    if not args.image.is_file() or args.image.is_symlink():
        parser.error('Input must be a regular image file, not a device or symlink')
    if args.action == 'info':
        report = inspect(args.image)
        if args.json:
            print(json.dumps(report, indent=2))
        else:
            print(f"{args.image}: {report['bytes']/2**30:.2f} GiB ({report['bytes']:,} bytes)")
            for partition in report.get('partitions', []):
                detail = ''
                if 'allocated_bytes' in partition:
                    detail = f", allocated {partition['allocated_bytes']/2**20:.1f} MiB, free {partition['free_bytes']/2**20:.1f} MiB"
                print(f"  Partition {partition['number']}: {partition['bytes']/2**20:.1f} MiB{detail}")
    elif args.action == 'verify':
        verify(args.image)
    elif args.action == 'compress':
        if Path(str(args.image) + '.zst').exists():
            parser.error('Compressed output already exists')
        compress(args.image)
    else:
        if args.output is None:
            parser.error('publish requires an output filename')
        publish(args.image, args.output)


if __name__ == '__main__':
    main()
