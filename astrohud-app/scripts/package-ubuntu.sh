#!/bin/bash
set -euo pipefail
workspace_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$workspace_dir"
docker build -t memory-lane-desktop-builder:ubuntu22 -f astrohud-app/docker/Dockerfile.ubuntu22 astrohud-app/docker
docker run --rm --user "$(id -u):$(id -g)" \
    -e CARGO_HOME=/workspace/target/tauri-cargo-home \
    -e CARGO_TARGET_DIR=/workspace/target/tauri-ubuntu22 \
    -e npm_config_cache=/workspace/target/tauri-npm-cache \
    -v "$workspace_dir:/workspace" memory-lane-desktop-builder:ubuntu22 \
    bash -c 'npm ci && npm run build -- --bundles deb'
version="$(node -p 'JSON.parse(require("fs").readFileSync("astrohud-app/package.json")).version')"
output="$workspace_dir/target/releases/desktop-v$version"
mkdir -p "$output"
cp target/tauri-ubuntu22/release/bundle/deb/*.deb "$output/"
cp astrohud-app/TESTING.txt "$output/README.txt"
cd "$output"
sha256sum ./*.deb README.txt > SHA256SUMS
tar -czf "../Memory-Lane-Desktop-${version}-ubuntu22-amd64.tar.gz" ./*.deb README.txt SHA256SUMS
printf 'Tester bundle: %s/../Memory-Lane-Desktop-%s-ubuntu22-amd64.tar.gz\n' "$output" "$version"
