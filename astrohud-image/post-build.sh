#!/bin/sh
set -eu

rootfs="$1"
script_dir="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
boot_config="$script_dir/configure-boot.py"
if [ ! -f "$boot_config" ]; then boot_config="$script_dir/scripts/configure-boot.py"; fi
python3 "$boot_config" "$rootfs"

# The source overlay is applied after YAML customize-hooks. Enable these only
# now that the actual unit files are installed.
systemctl --root="$rootfs" enable astrohud-regenerate-ssh-keys.service NetworkManager astrohud-frame astrohud-provisioner astrohud-wifi-guard.timer memory-lane-expand-root.service
# NetworkManager also enables its wait-online unit; disable it afterwards.
systemctl --root="$rootfs" disable systemd-networkd systemd-networkd-wait-online NetworkManager-wait-online.service 2>/dev/null || true
# NetworkManager supplies normal DNS; the setup AP uses its separate dnsmasq.
# NetworkManager writes this file when the household connection becomes active.
rm -f "$rootfs/etc/resolv.conf"
ln -s /run/NetworkManager/resolv.conf "$rootfs/etc/resolv.conf"

# Do not ship package caches, swap allocations, or build tools. Keep package
# metadata and copyright notices for servicing and license attribution.
find "$rootfs/var/cache/apt/archives" -type f -name '*.deb' -delete
find "$rootfs/var/lib/apt/lists" -type f -delete
chroot "$rootfs" dpkg-query -W '-f=${Package}\t${Installed-Size}\n' | sort > "$script_dir/package-sizes.tsv"
if grep -Eq '^(chromium[^[:space:]]*|build-essential|gcc-[0-9]+-aarch64-linux-gnu|g\+\+|linux-headers-[^[:space:]]*|dphys-swapfile)[[:space:]]' "$script_dir/package-sizes.tsv"; then
    echo 'Unexpected browser, development tool or swap package in clean image' >&2
    exit 1
fi
for swap in swapfile var/swap; do
    if [ -e "$rootfs/$swap" ]; then echo "Unexpected allocated swap: /$swap" >&2; exit 1; fi
done

require_file() {
    if [ ! -f "$rootfs/$1" ]; then
        echo "AstroHUD image verification failed: missing /$1" >&2
        exit 1
    fi
}

for required in \
    usr/local/bin/astrohud-frame \
    usr/local/sbin/astrohud-provisioner \
    etc/systemd/system/astrohud-frame.service \
    etc/systemd/system/astrohud-provisioner.service \
    etc/systemd/system/astrohud-wifi-guard.timer \
    usr/local/sbin/memory-lane-expand-root \
    etc/systemd/system/memory-lane-expand-root.service \
    etc/systemd/system/astrohud-regenerate-ssh-keys.service \
    etc/astrohud/image-release; do
    require_file "$required"
done

for forbidden in \
    etc/astrohud/device.json \
    etc/astrohud/manufacturing.json \
    etc/astrohud/reset.json \
    var/lib/astrohud/reset-in-progress \
    etc/astrohud/wifi-profile.nmconnection \
    etc/NetworkManager/system-connections/astrohud-wifi.nmconnection \
    var/lib/astrohud/device-credential \
    var/lib/astrohud/manifest.json \
    var/lib/astrohud/setup-screen.ppm \
    var/lib/astrohud/media; do
    if [ -e "$rootfs/$forbidden" ]; then
        echo "AstroHUD image verification failed: runtime state exists at /$forbidden" >&2
        exit 1
    fi
done

if [ -s "$rootfs/etc/machine-id" ]; then
    echo "AstroHUD image verification failed: machine ID is already populated" >&2
    exit 1
fi

if find "$rootfs/etc/ssh" -maxdepth 1 -type f -name 'ssh_host_*_key' | grep -q .; then
    echo "AstroHUD image verification failed: SSH host keys are already populated" >&2
    exit 1
fi

chroot "$rootfs" /usr/local/bin/astrohud-frame --help >/dev/null
echo "Memory Lane image verification passed: application loads and appliance state is sealed"
