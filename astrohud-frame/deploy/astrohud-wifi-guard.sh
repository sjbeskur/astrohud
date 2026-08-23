#!/bin/sh
set -eu

connection_id="astrohud-wifi"
interface="wlan0"
backup="/etc/astrohud/wifi-profile.nmconnection"
profile="/etc/NetworkManager/system-connections/astrohud-wifi.nmconnection"
provisioning_marker="/var/lib/astrohud/provisioning-required"

log() {
    logger -t astrohud-wifi-guard -- "$1"
}

# Setup mode intentionally owns wlan0. The provisioner removes this marker only
# after it has committed and backed up a working household connection.
if [ -e "${provisioning_marker}" ]; then
    exit 0
fi

if [ ! -s "${backup}" ]; then
    log "profile backup is missing or empty: ${backup}"
    exit 1
fi

if [ ! -s "${profile}" ]; then
    install -o root -g root -m 0600 "${backup}" "${profile}"
    nmcli connection reload
    log "restored missing NetworkManager profile from the protected backup"
fi

if ! nmcli -g connection.uuid connection show "${connection_id}" >/dev/null 2>&1; then
    install -o root -g root -m 0600 "${backup}" "${profile}"
    nmcli connection reload
    log "replaced an unreadable NetworkManager profile from the protected backup"
fi

nmcli radio wifi on
nmcli device set "${interface}" managed yes

if nmcli -t -f DEVICE,STATE device status | grep -Fqx "${interface}:connected"; then
    exit 0
fi

log "${interface} is disconnected; requesting reconnection"
if ! nmcli connection up "${connection_id}" ifname "${interface}"; then
    log "reconnection attempt failed; the timer will retry"
fi

exit 0
