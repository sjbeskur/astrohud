#!/bin/sh
set -eu

systemctl stop astrohud-provisioner.service
nmcli connection down astrohud-setup >/dev/null 2>&1 || true
nmcli connection delete astrohud-setup >/dev/null 2>&1 || true
rm -f /var/lib/astrohud/provisioning-required /var/lib/astrohud/setup-screen.ppm
systemctl start astrohud-wifi-guard.service
