#!/bin/bash
set -euo pipefail

if [ "$(id -u)" -ne 0 ]; then
    echo "Run this script with sudo." >&2
    exit 1
fi
if [ "$#" -lt 3 ] || [ "$#" -gt 5 ]; then
    echo "usage: sudo sanitize-clone.sh <original.img> <sanitized.img> <operator.pub> [astrohud-frame] [astrohud-provisioner]" >&2
    exit 1
fi

source_image="$(readlink -f "$1")"
output_image="$(readlink -m "$2")"
operator_key="$(readlink -f "$3")"
frame_binary=""
if [ "$#" -eq 4 ]; then
    frame_binary="$(readlink -f "$4")"
fi
provisioner_binary=""
if [ "$#" -eq 5 ]; then
    frame_binary="$(readlink -f "$4")"
    provisioner_binary="$(readlink -f "$5")"
fi
expected_size=15931539456
old_user=sbeskur
support_user=astrohud-support
script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
image_dir="$(cd "${script_dir}/.." && pwd)"

if [ ! -f "$source_image" ]; then
    echo "Source image not found: $source_image" >&2
    exit 1
fi
if [ ! -f "$operator_key" ]; then
    echo "Operator public key not found: $operator_key" >&2
    exit 1
fi
if [ -n "$frame_binary" ] && [ ! -f "$frame_binary" ]; then
    echo "Frame binary not found: $frame_binary" >&2
    exit 1
fi
if [ -n "$frame_binary" ] \
    && ! file -b "$frame_binary" | grep -q 'ARM aarch64'; then
    echo "Frame binary is not an ARM64 executable: $frame_binary" >&2
    exit 1
fi
if [ -n "$provisioner_binary" ] && [ ! -f "$provisioner_binary" ]; then
    echo "Provisioner binary not found: $provisioner_binary" >&2
    exit 1
fi
if [ -n "$provisioner_binary" ] \
    && ! file -b "$provisioner_binary" | grep -q 'ARM aarch64'; then
    echo "Provisioner binary is not an ARM64 executable: $provisioner_binary" >&2
    exit 1
fi
if [ "$source_image" = "$output_image" ]; then
    echo "Refusing to modify the original image in place." >&2
    exit 1
fi
if [ -e "$output_image" ]; then
    echo "Refusing to overwrite existing output: $output_image" >&2
    exit 1
fi
if [ "$(stat -c %s "$source_image")" -ne "$expected_size" ]; then
    echo "Source image does not have the validated AstroHUD card size." >&2
    exit 1
fi
if ! grep -qE '^ssh-(ed25519|rsa|ecdsa) ' "$operator_key"; then
    echo "Operator key does not look like an OpenSSH public key." >&2
    exit 1
fi

install -d -m 0755 "$(dirname "$output_image")"
chmod 0600 "$source_image"
cp --reflink=auto --sparse=always "$source_image" "$output_image"
chmod 0600 "$output_image"

mount_dir="$(mktemp -d --tmpdir astrohud-clone-root.XXXXXX)"
loop_device=""
boot_mounted=false
root_mounted=false

cleanup() {
    set +e
    if $boot_mounted; then
        umount "$mount_dir/boot/firmware"
    fi
    if $root_mounted; then
        umount "$mount_dir"
    fi
    if [ -n "$loop_device" ]; then
        losetup --detach "$loop_device"
    fi
    rm -rf "$mount_dir"
}
trap cleanup EXIT

loop_device="$(losetup --find --show --partscan "$output_image")"
udevadm settle
boot_partition="${loop_device}p1"
root_partition="${loop_device}p2"
if [ ! -b "$boot_partition" ] || [ ! -b "$root_partition" ]; then
    echo "Sanitized image did not expose the expected two partitions." >&2
    exit 1
fi

e2fsck -pf "$root_partition" || {
    status=$?
    if [ "$status" -gt 1 ]; then
        exit "$status"
    fi
}
tune2fs -m 0 "$root_partition"
mount "$root_partition" "$mount_dir"
root_mounted=true
install -d -m 0755 "$mount_dir/boot/firmware"
mount "$boot_partition" "$mount_dir/boot/firmware"
boot_mounted=true

for required in \
    etc/os-release \
    usr/local/bin/astrohud-frame \
    usr/local/sbin/astrohud-provisioner \
    etc/systemd/system/astrohud-frame.service \
    etc/systemd/system/astrohud-provisioner.service; do
    if [ ! -e "$mount_dir/$required" ]; then
        echo "Clone does not look like the working AstroHUD appliance: missing /$required" >&2
        exit 1
    fi
done
if ! grep -qE '^VERSION_ID="?13"?$' "$mount_dir/etc/os-release"; then
    echo "Clone is not the validated Debian 13 appliance." >&2
    exit 1
fi

if [ -n "$frame_binary" ]; then
    install -o root -g root -m 0755 "$frame_binary" \
        "$mount_dir/usr/local/bin/astrohud-frame"
fi
if [ -n "$provisioner_binary" ]; then
    install -o root -g root -m 0755 "$provisioner_binary" \
        "$mount_dir/usr/local/sbin/astrohud-provisioner"
fi

if ! grep -q "^${old_user}:" "$mount_dir/etc/passwd"; then
    echo "Expected maintenance account was not found in the clone." >&2
    exit 1
fi
if grep -q "^${support_user}:" "$mount_dir/etc/passwd"; then
    echo "Support account already exists; refusing an ambiguous rename." >&2
    exit 1
fi
account_uid="$(awk -F: -v user="$old_user" '$1 == user { print $3 }' "$mount_dir/etc/passwd")"
account_gid="$(awk -F: -v user="$old_user" '$1 == user { print $4 }' "$mount_dir/etc/passwd")"
if [ -z "$account_uid" ] || [ -z "$account_gid" ]; then
    echo "Could not determine the maintenance account ownership." >&2
    exit 1
fi

for account_file in etc/passwd etc/shadow etc/group etc/gshadow; do
    sed -i "s/${old_user}/${support_user}/g" "$mount_dir/$account_file"
done
for optional_account_file in etc/subuid etc/subgid; do
    if [ -f "$mount_dir/$optional_account_file" ]; then
        sed -i "s/${old_user}/${support_user}/g" "$mount_dir/$optional_account_file"
    fi
done
rm -rf "$mount_dir/home/$old_user"
install -d -o "$account_uid" -g "$account_gid" -m 0700 \
    "$mount_dir/home/$support_user/.ssh"
install -o "$account_uid" -g "$account_gid" -m 0600 "$operator_key" \
    "$mount_dir/home/$support_user/.ssh/authorized_keys"

install -d -m 0755 "$mount_dir/etc/ssh/sshd_config.d"
cat > "$mount_dir/etc/ssh/sshd_config.d/01-astrohud-support.conf" <<'EOF'
PermitRootLogin no
ChallengeResponseAuthentication no
PasswordAuthentication no
GSSAPIAuthentication no
UsePAM yes
PubkeyAuthentication yes
AuthenticationMethods publickey
EOF
printf '%s ALL=(ALL:ALL) NOPASSWD: ALL\n' "$support_user" \
    > "$mount_dir/etc/sudoers.d/010_astrohud-support"
chmod 0440 "$mount_dir/etc/sudoers.d/010_astrohud-support"
rm -f "$mount_dir/etc/sudoers.d/010_pi-nopasswd"
rm -f "$mount_dir/etc/systemd/system/getty@tty1.service.d/autologin.conf"
rm -f "$mount_dir/var/lib/AccountsService/users/$old_user"

install -m 0644 "$image_dir/clone/astrohud-regenerate-ssh-keys.service" \
    "$mount_dir/etc/systemd/system/astrohud-regenerate-ssh-keys.service"
install -d -m 0755 "$mount_dir/etc/systemd/system/multi-user.target.wants"
ln -s ../astrohud-regenerate-ssh-keys.service \
    "$mount_dir/etc/systemd/system/multi-user.target.wants/astrohud-regenerate-ssh-keys.service"

printf '%s\n' 'astrohud-??????' > "$mount_dir/etc/hostname"
sed -i 's/astrohud-dev/astrohud-??????/g' "$mount_dir/etc/hosts"
truncate -s 0 "$mount_dir/etc/machine-id"
rm -f "$mount_dir/var/lib/dbus/machine-id"
find "$mount_dir/etc/ssh" -maxdepth 1 -type f -name 'ssh_host_*' -delete

rm -f "$mount_dir/etc/astrohud/device.json"
rm -f "$mount_dir/etc/astrohud/wifi-profile.nmconnection"
rm -f "$mount_dir/var/lib/astrohud/device-credential"
rm -f "$mount_dir/var/lib/astrohud/manifest.json"
rm -f "$mount_dir/var/lib/astrohud/setup-screen.ppm"
rm -f "$mount_dir/var/lib/astrohud/provisioning-required"
rm -rf "$mount_dir/var/lib/astrohud/media"
rm -rf "$mount_dir/var/lib/astrohud/cache-before-secure-delivery"

if [ -d "$mount_dir/etc/NetworkManager/system-connections" ]; then
    find "$mount_dir/etc/NetworkManager/system-connections" \
        -mindepth 1 -maxdepth 1 -type f -delete
fi
rm -rf "$mount_dir/var/lib/NetworkManager"
rm -rf "$mount_dir/var/lib/dhcp"
rm -f "$mount_dir/var/lib/systemd/random-seed"
rm -f "$mount_dir/root/.bash_history"
find "$mount_dir/var/log" -type f -exec truncate -s 0 {} +
find "$mount_dir/tmp" -mindepth 1 -delete
find "$mount_dir/var/tmp" -mindepth 1 -delete

for forbidden in \
    etc/astrohud/device.json \
    etc/astrohud/wifi-profile.nmconnection \
    var/lib/astrohud/device-credential \
    var/lib/astrohud/manifest.json \
    var/lib/astrohud/media \
    var/lib/astrohud/cache-before-secure-delivery; do
    if [ -e "$mount_dir/$forbidden" ]; then
        echo "Sanitization failed: /$forbidden still exists." >&2
        exit 1
    fi
done
if grep -q "$old_user" \
    "$mount_dir/etc/passwd" "$mount_dir/etc/shadow" \
    "$mount_dir/etc/group" "$mount_dir/etc/gshadow"; then
    echo "Sanitization failed: old maintenance account remains." >&2
    exit 1
fi

set +e
dd if=/dev/zero of="$mount_dir/boot/firmware/.astrohud-zero-free" \
    bs=16M status=none
set -e
boot_zero_bytes="$(stat -c %s "$mount_dir/boot/firmware/.astrohud-zero-free")"
if [ "$boot_zero_bytes" -lt 1048576 ]; then
    echo "Could not scrub free space in the boot filesystem." >&2
    exit 1
fi
sync
rm -f "$mount_dir/boot/firmware/.astrohud-zero-free"

set +e
dd if=/dev/zero of="$mount_dir/.astrohud-zero-free" bs=64M status=progress
set -e
root_zero_bytes="$(stat -c %s "$mount_dir/.astrohud-zero-free")"
if [ "$root_zero_bytes" -lt 1073741824 ]; then
    echo "Could not scrub free space in the root filesystem." >&2
    exit 1
fi
sync
rm -f "$mount_dir/.astrohud-zero-free"
sync

umount "$mount_dir/boot/firmware"
boot_mounted=false
umount "$mount_dir"
root_mounted=false
tune2fs -m 5 "$root_partition"
e2fsck -pf "$root_partition" || {
    status=$?
    if [ "$status" -gt 1 ]; then
        exit "$status"
    fi
}
fsck.vfat -a "$boot_partition" || {
    status=$?
    if [ "$status" -gt 1 ]; then
        exit "$status"
    fi
}
losetup --detach "$loop_device"
loop_device=""

if [ -n "${SUDO_UID:-}" ] && [ -n "${SUDO_GID:-}" ]; then
    chown "$SUDO_UID:$SUDO_GID" "$output_image"
fi
sha256sum "$output_image" > "$output_image.sha256"
if [ -n "${SUDO_UID:-}" ] && [ -n "${SUDO_GID:-}" ]; then
    chown "$SUDO_UID:$SUDO_GID" "$output_image.sha256"
fi

echo "Sanitized AstroHUD clone ready: $output_image"
cat "$output_image.sha256"
