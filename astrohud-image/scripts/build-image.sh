#!/bin/bash
set -euo pipefail

image_args=()
image_overrides=()
if [ "${1:-}" = --image-only ]; then image_args=(-i); shift; fi
if [ "$#" -ne 0 ]; then echo "Usage: $0 [--image-only]" >&2; exit 1; fi

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
image_dir="$(cd "${script_dir}/.." && pwd)"
workspace_dir="$(cd "${image_dir}/.." && pwd)"
image_gen_dir="${RPI_IMAGE_GEN_DIR:-}"
operator_key="${ASTROHUD_OPERATOR_PUBKEY:-}"
server_url="${ASTROHUD_SERVER_URL:-https://app.astrohud.com}"
output="${ASTROHUD_IMAGE_OUTPUT:-$workspace_dir/target/astrohud-images/memory-lane-clean.img}"
if [ -e "$output" ]; then echo "Refusing to overwrite $output" >&2; exit 1; fi
export ASTROHUD_IMAGE_OUTPUT="$output"
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
install -m 0755 "$image_dir/post-image.sh" "$staging_dir/post-image.sh"
install -m 0644 "$image_dir/scripts/image-artifact.py" "$staging_dir/image-artifact.py"
install -m 0755 "$image_dir/scripts/configure-boot.py" "$staging_dir/configure-boot.py"
install -m 0644 "$operator_key" "$staging_dir/operator.pub"

overlay="$staging_dir/rootfs-overlay"
install -D -m 0644 "$image_dir/clone/astrohud-regenerate-ssh-keys.service" "$overlay/etc/systemd/system/astrohud-regenerate-ssh-keys.service"
install -D -m 0755 "$image_dir/scripts/expand-root.sh" "$overlay/usr/local/sbin/memory-lane-expand-root"
install -D -m 0644 "$image_dir/deploy/memory-lane-expand-root.service" "$overlay/etc/systemd/system/memory-lane-expand-root.service"
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
    printf 'Memory Lane clean image\n'
    printf 'source_commit=%s\n' "$(git -C "$workspace_dir" rev-parse HEAD)"
    if [ -n "$(git -C "$workspace_dir" status --porcelain)" ]; then
        printf 'source_dirty=true\n'
    else
        printf 'source_dirty=false\n'
    fi
    printf 'rpi_image_gen_commit=%s\n' "$actual_image_gen_commit"
    printf 'frame_sha256=%s\n' "$(sha256sum "$frame_binary" | cut -d ' ' -f 1)"
    printf 'provisioner_sha256=%s\n' "$(sha256sum "$provisioner_binary" | cut -d ' ' -f 1)"
    printf 'server_url=%s\n' "$server_url"
} > "$overlay/etc/astrohud/image-release"
chmod 0644 "$overlay/etc/astrohud/image-release"

install -d "$workspace_dir/target/image-work"
if [ "${#image_args[@]}" -ne 0 ]; then
    previous_env="$workspace_dir/target/image-work/bootstrap/final.env"
    cached_root="$(sed -n 's/^IGconf_target_path=//p' "$previous_env" | tr -d '"')"
    case "$cached_root" in "$workspace_dir"/target/image-work/chroot-*/filesystem) ;; *) echo 'No matching clean build filesystem; run image-build first.' >&2; exit 1;; esac
    test -d "$cached_root"
    cached_context="${cached_root%/filesystem}"
    image_overrides=(-- "IGconf_artefact_context=${cached_context##*/}")
    # Reuse only the local build root, refresh the applications, then recheck
    # that it is sealed. This never operates on a mounted customer SD card.
    cp -a "$overlay/." "$cached_root/"
    "$staging_dir/post-build.sh" "$cached_root"
fi
"$image_gen_dir/rpi-image-gen" build "${image_args[@]}" \
    -S "$staging_dir" \
    -B "$workspace_dir/target/image-work" \
    -c astrohud-zero2w.yaml "${image_overrides[@]}"
