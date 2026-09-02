#!/bin/bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
image_dir="$(cd "${script_dir}/.." && pwd)"
workspace_dir="$(cd "${image_dir}/.." && pwd)"
image_gen_dir="${RPI_IMAGE_GEN_DIR:-}"
operator_key="${ASTROHUD_OPERATOR_PUBKEY:-}"
server_url="${ASTROHUD_SERVER_URL:-https://app.astrohud.com}"
expected_image_gen_commit="3f2c916086ad70197945bfc50ef953c1f6035f10"

if [ -z "$image_gen_dir" ] || [ ! -x "$image_gen_dir/rpi-image-gen" ]; then
    echo "Set RPI_IMAGE_GEN_DIR to the pinned rpi-image-gen v2.6.0 checkout." >&2
    exit 1
fi
if [ -z "$operator_key" ] || [ ! -f "$operator_key" ]; then
    echo "Set ASTROHUD_OPERATOR_PUBKEY to the public SSH key for beta support." >&2
    exit 1
fi
if ! [[ "$server_url" =~ ^https?://[^[:space:]\|]+$ ]]; then
    echo "ASTROHUD_SERVER_URL must be an HTTP or HTTPS URL without whitespace." >&2
    exit 1
fi

actual_image_gen_commit="$(git -C "$image_gen_dir" rev-parse HEAD)"
if [ "$actual_image_gen_commit" != "$expected_image_gen_commit" ]; then
    echo "rpi-image-gen must be pinned to v2.6.0 ($expected_image_gen_commit)." >&2
    exit 1
fi

frame_binary="$workspace_dir/target/pi-bookworm/aarch64-unknown-linux-gnu/release/astrohud-frame"
provisioner_binary="$workspace_dir/target/pi-bookworm/aarch64-unknown-linux-gnu/release/astrohud-provisioner"
for binary in "$frame_binary" "$provisioner_binary"; do
    if [ ! -x "$binary" ]; then
        echo "Missing ARM64 binary: $binary" >&2
        echo "Run astrohud-frame/scripts/build-pi-container.sh first." >&2
        exit 1
    fi
done

staging_dir="$(mktemp -d --tmpdir astrohud-image-source.XXXXXX)"
cleanup() {
    rm -rf "$staging_dir"
}
trap cleanup EXIT

cp -a "$image_dir/config" "$staging_dir/config"
cp -a "$image_dir/layer" "$staging_dir/layer"
install -m 0755 "$image_dir/post-build.sh" "$staging_dir/post-build.sh"
install -m 0644 "$operator_key" "$staging_dir/operator.pub"

overlay="$staging_dir/rootfs-overlay"
install -D -m 0755 "$frame_binary" "$overlay/usr/local/bin/astrohud-frame"
install -D -m 0755 "$provisioner_binary" "$overlay/usr/local/sbin/astrohud-provisioner"
install -D -m 0755 "$workspace_dir/astrohud-provisioner/deploy/astrohud-enter-setup.sh" "$overlay/usr/local/sbin/astrohud-enter-setup"
install -D -m 0755 "$workspace_dir/astrohud-provisioner/deploy/astrohud-exit-setup.sh" "$overlay/usr/local/sbin/astrohud-exit-setup"
install -D -m 0755 "$workspace_dir/astrohud-frame/deploy/astrohud-wifi-guard.sh" "$overlay/usr/local/sbin/astrohud-wifi-guard"
install -D -m 0644 "$workspace_dir/astrohud-frame/deploy/astrohud-frame.service" "$overlay/etc/systemd/system/astrohud-frame.service"
install -D -m 0644 "$workspace_dir/astrohud-provisioner/deploy/astrohud-provisioner.service" "$overlay/etc/systemd/system/astrohud-provisioner.service"
install -D -m 0644 "$workspace_dir/astrohud-frame/deploy/astrohud-wifi-guard.service" "$overlay/etc/systemd/system/astrohud-wifi-guard.service"
install -D -m 0644 "$workspace_dir/astrohud-frame/deploy/astrohud-wifi-guard.timer" "$overlay/etc/systemd/system/astrohud-wifi-guard.timer"
install -D -m 0644 "$workspace_dir/astrohud-frame/deploy/astrohud-journald.conf" "$overlay/etc/systemd/journald.conf.d/astrohud.conf"
install -D -m 0644 "$workspace_dir/astrohud-provisioner/deploy/astrohud-captive-dns.conf" "$overlay/etc/NetworkManager/dnsmasq-shared.d/astrohud-captive.conf"

sed -i "s|^Environment=ASTROHUD_SERVER_URL=.*|Environment=ASTROHUD_SERVER_URL=$server_url|" \
    "$overlay/etc/systemd/system/astrohud-frame.service" \
    "$overlay/etc/systemd/system/astrohud-provisioner.service"

install -d -m 0755 "$overlay/etc/astrohud"
{
    printf 'AstroHUD image\n'
    printf 'source_commit=%s\n' "$(git -C "$workspace_dir" rev-parse HEAD)"
    printf 'rpi_image_gen_commit=%s\n' "$actual_image_gen_commit"
    printf 'server_url=%s\n' "$server_url"
} > "$overlay/etc/astrohud/image-release"
chmod 0644 "$overlay/etc/astrohud/image-release"

"$image_gen_dir/rpi-image-gen" build \
    -S "$staging_dir" \
    -c astrohud-zero2w.yaml
