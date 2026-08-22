#!/bin/sh
set -eu

install -d -o root -g root -m 0755 /var/lib/astrohud
install -o root -g root -m 0600 /dev/null /var/lib/astrohud/provisioning-required
systemctl restart astrohud-provisioner.service
