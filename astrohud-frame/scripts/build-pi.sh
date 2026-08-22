#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
workspace_dir="$(cd "${script_dir}/../.." && pwd)"

export CC_aarch64_unknown_linux_gnu=aarch64-linux-gnu-gcc
export CXX_aarch64_unknown_linux_gnu=aarch64-linux-gnu-g++
export CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=aarch64-linux-gnu-gcc
export PKG_CONFIG_ALLOW_CROSS=1
export SDL2_TOOLCHAIN="${workspace_dir}/astrohud-frame/cmake/aarch64-linux-gnu.cmake"

cd "${workspace_dir}"
cargo build --locked --release \
    --package astrohud-frame \
    --package astrohud-provisioner \
    --target aarch64-unknown-linux-gnu
