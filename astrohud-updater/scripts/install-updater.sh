#!/bin/bash
# Attended bootstrap for an existing ARM64 appliance. Does not restart apps.
set -euo pipefail
if [ "$#" -ne 2 ]; then
    echo "Usage: sudo $0 /path/to/memory-lane /path/to/updater-config.json" >&2
    exit 1
fi
if [ "$(id -u)" != 0 ] || [ "$(uname -m)" != aarch64 ]; then
    echo 'Bootstrap requires root on the ARM64 appliance.' >&2
    exit 1
fi
script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
deploy_dir="$script_dir/../deploy"
# Validate the executable before changing any existing installation.
"$1" --version
install -d -m 0755 /opt/memory-lane /usr/local/bin
install -m 0755 "$1" /usr/local/bin/memory-lane.bootstrap-new
mv -f /usr/local/bin/memory-lane.bootstrap-new /usr/local/bin/memory-lane
install -m 0644 "$2" /etc/memory-lane-updater.json.bootstrap-new
mv -f /etc/memory-lane-updater.json.bootstrap-new /etc/memory-lane-updater.json
for unit in memory-lane-update-check.service memory-lane-update-check.timer memory-lane-update-recover.service memory-lane-update-agent.service; do
    install -m 0644 "$deploy_dir/$unit" "/etc/systemd/system/$unit"
done
for unit in astrohud-frame.service astrohud-provisioner.service; do
    install -d -m 0755 "/etc/systemd/system/$unit.d"
    install -m 0644 "$deploy_dir/10-update-recovery.conf" "/etc/systemd/system/$unit.d/10-update-recovery.conf"
done
systemctl daemon-reload
systemctl enable memory-lane-update-recover.service
systemctl start memory-lane-update-recover.service
/usr/local/bin/memory-lane init
systemctl enable --now memory-lane-update-check.timer
systemctl enable --now memory-lane-update-agent.service
/usr/local/bin/memory-lane status
