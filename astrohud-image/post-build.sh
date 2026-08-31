#!/bin/sh
set -eu

rootfs="$1"

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
    etc/astrohud/image-release; do
    require_file "$required"
done

for forbidden in \
    etc/astrohud/device.json \
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

echo "AstroHUD image verification passed: appliance state is sealed"
