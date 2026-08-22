#!/bin/sh
set -eu

systemctl stop astrohud-provisioner.service
nmcli connection down astrohud-setup >/dev/null 2>&1 || true
nmcli connection delete astrohud-setup >/dev/null 2>&1 || true
rm -f /var/lib/astrohud/provisioning-required
systemctl start astrohud-wifi-guard.service
