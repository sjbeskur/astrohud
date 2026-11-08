#!/bin/bash
set -euo pipefail
workspace_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$workspace_dir"
output="${1:-target/astrohud-images/memory-lane-clean.img}"
if [ "$#" -gt 0 ]; then shift; fi
operator_key="${ASTROHUD_OPERATOR_PUBKEY:-${HOME}/.ssh/id_ed25519.pub}"
if [ ! -f "$operator_key" ]; then
    echo 'Set ASTROHUD_OPERATOR_PUBKEY to your operator SSH public key.' >&2
    exit 1
fi
output="$(realpath -m "$output")"
case "$output" in "$workspace_dir"/target/*) ;; *) echo 'Clean output must be under target/.' >&2; exit 1;; esac
if [ -e "$output" ]; then echo "Refusing to overwrite $output" >&2; exit 1; fi
install -d "$(dirname "$output")" target/image-input target/image-work
install -m 0644 "$operator_key" target/image-input/operator.pub
# Install host build dependencies inside a disposable container, not the workstation.
docker build -f astrohud-image/docker/Dockerfile -t memory-lane-image-builder:2.6.0 astrohud-image/docker
docker run --rm --privileged \
    --volume "$workspace_dir:/workspace" \
    --env ASTROHUD_OPERATOR_PUBKEY=/workspace/target/image-input/operator.pub \
    --env "ASTROHUD_SERVER_URL=${ASTROHUD_SERVER_URL:-https://app.astrohud.com}" \
    --env "ASTROHUD_IMAGE_OUTPUT=/workspace/${output#"$workspace_dir"/}" \
    --env "ASTROHUD_BUILD_UID=$(id -u)" --env "ASTROHUD_BUILD_GID=$(id -g)" \
    memory-lane-image-builder:2.6.0 "$@"
