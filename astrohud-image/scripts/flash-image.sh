#!/bin/bash
# Explicit invocation authorizes erasing the supplied removable disk.
set -euo pipefail

if [ "$#" -ne 2 ]; then
    echo "Usage: sudo $0 <completed.img> <removable-whole-disk>" >&2
    exit 1
fi
if [ "$(id -u)" -ne 0 ]; then
    echo "Run this script with sudo." >&2
    exit 1
fi
image="$(readlink -f "$1")"
disk="$(readlink -f "$2")"
if [ ! -f "$image" ] || [ ! -f "$image.sha256" ]; then
    echo "A completed image and its .sha256 file are required." >&2
    exit 1
fi
if [ ! -b "$disk" ] || [ "$(lsblk -dn -o TYPE "$disk" | xargs)" != disk ] \
    || [ "$(lsblk -dn -o RM "$disk" | xargs)" != 1 ]; then
    echo "Refusing to write: target must be a removable whole disk." >&2
    exit 1
fi
image_bytes="$(stat -c %s "$image")"
if [ "$(blockdev --getsize64 "$disk")" -lt "$image_bytes" ]; then
    echo "The image is larger than the target disk." >&2
    exit 1
fi
if [ -n "$(losetup -j "$image")" ]; then
    echo "The image is still attached to a loop device; finish the build first." >&2
    exit 1
fi
expected="$(awk 'NR == 1 { print $1 }' "$image.sha256")"
actual="$(sha256sum "$image" | cut -d ' ' -f 1)"
if [ "$expected" != "$actual" ]; then
    echo "Image checksum verification failed." >&2
    exit 1
fi
mapfile -t children < <(lsblk -nrpo NAME "$disk")
# Refuse a system disk even if it is reported as removable.
for child in "${children[@]}"; do
    while IFS= read -r mountpoint; do
        case "$mountpoint" in
            /|/boot|/boot/efi|/home|/usr|/var)
                echo "Refusing to write a disk containing $mountpoint." >&2
                exit 1 ;;
        esac
    done < <(findmnt -rn -S "$child" -o TARGET)
done
lsblk -o NAME,SIZE,MODEL,MOUNTPOINTS "$disk"
for ((index=${#children[@]}-1; index>=0; index--)); do
    child="${children[index]}"
    if findmnt -rn -S "$child" >/dev/null; then
        if ! umount --all-targets "$child"; then
            echo "The card is busy. Close file manager windows and terminals using it, then retry." >&2
            echo "No image data has been written. To inspect open files: sudo fuser -vm $child" >&2
            exit 1
        fi
    fi
    if swapon --show=NAME --noheadings | grep -Fxq "$child"; then
        echo "Refusing to overwrite active swap: $child" >&2
        exit 1
    fi
done
echo "Erasing $disk and writing $image"
dd if="$image" of="$disk" bs=4M conv=fsync status=progress
blockdev --flushbufs "$disk"
echo "Verifying the complete written image…"
cmp -n "$image_bytes" "$image" "$disk"
echo "Verified. The SD card is ready to remove and boot in the Pi."
