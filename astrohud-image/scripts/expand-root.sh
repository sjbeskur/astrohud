#!/bin/bash
# Runs on the appliance only, after local filesystems are mounted.
set -euo pipefail
root_device="$(readlink -f "$(findmnt -nro SOURCE /)")"
parent="$(lsblk -dnro PKNAME "$root_device" | xargs)"
partition="$(cat "/sys/class/block/${root_device##*/}/partition")"
filesystem="$(findmnt -nro FSTYPE /)"
if [ "$partition" != 2 ] || [ "$filesystem" != ext4 ] || [ -z "$parent" ]; then
    echo "Unexpected root layout; refusing expansion: $root_device ($filesystem)" >&2
    exit 1
fi
# growpart preserves the start sector and filesystem, extending only partition 2.
result=""
if result="$(growpart "/dev/$parent" "$partition" 2>&1)"; then
    printf '%s\n' "$result"
elif [[ "$result" == NOCHANGE:* ]]; then
    printf '%s\n' "$result"
else
    printf '%s\n' "$result" >&2
    exit 1
fi
resize2fs "$root_device"
state_dir="${STATE_DIRECTORY:-/var/lib/memory-lane}"
install -d -m 0755 "$state_dir"
printf 'expanded\n' > "$state_dir/root-expanded"
