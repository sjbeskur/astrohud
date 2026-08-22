#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
workspace_dir="$(cd "${script_dir}/../.." && pwd)"
image_name="astrohud-frame-cross:bookworm"

docker build \
  --file "${workspace_dir}/astrohud-frame/docker/Dockerfile.cross" \
  --tag "${image_name}" \
  "${workspace_dir}"

docker run --rm \
  --user "$(id -u):$(id -g)" \
  --env CARGO_HOME=/tmp/cargo-home \
  --env CARGO_TARGET_DIR=/workspace/target/pi-bookworm \
  --volume "${workspace_dir}:/workspace" \
  "${image_name}" \
  /workspace/astrohud-frame/scripts/build-pi.sh
