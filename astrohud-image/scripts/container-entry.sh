#!/bin/bash
set -euo pipefail
# Namespace mounts are required by mmdebstrap. On x86, register the static
# ARM64 interpreter before entering the target chroot. No SD devices are used.
if [ "$(uname -m)" != aarch64 ]; then
    if ! mountpoint -q /proc/sys/fs/binfmt_misc; then
        mount -t binfmt_misc binfmt_misc /proc/sys/fs/binfmt_misc
    fi
    update-binfmts --enable qemu-aarch64
    if ! grep -q '^enabled' /proc/sys/fs/binfmt_misc/qemu-aarch64; then
        echo 'ARM64 emulation is unavailable.' >&2
        exit 1
    fi
fi
git config --global --add safe.directory /workspace
exec bash /workspace/astrohud-image/scripts/build-image.sh "$@"
