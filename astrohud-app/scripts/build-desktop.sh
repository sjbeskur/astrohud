#!/bin/bash
set -euo pipefail
workspace_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$workspace_dir"
node astrohud-app/scripts/prepare-ui.mjs
docker build -t memory-lane-desktop-builder:bookworm -f astrohud-app/docker/Dockerfile astrohud-app/docker
docker run --rm --user "$(id -u):$(id -g)" \
    -e CARGO_HOME=/workspace/target/tauri-cargo-home \
    -e CARGO_TARGET_DIR=/workspace/target/tauri-desktop \
    -v "$workspace_dir:/workspace" memory-lane-desktop-builder:bookworm \
    cargo build --manifest-path astrohud-app/src-tauri/Cargo.toml
