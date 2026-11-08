set shell := ["bash", "-eu", "-o", "pipefail", "-c"]
set positional-arguments := true

export ASTROHUD_OPERATOR_PUBKEY := env("ASTROHUD_OPERATOR_PUBKEY", env("HOME") / ".ssh/id_ed25519.pub")
export ASTROHUD_MASTER_IMAGE := env("ASTROHUD_MASTER_IMAGE", "target/astrohud-images/astrohud-original-2026-08-30.img")
export ASTROHUD_SERVER_URL := env("ASTROHUD_SERVER_URL", "https://app.astrohud.com")

# List available commands (does not build or flash).
default:
    @just --list

# Cross-compile both Pi applications using Docker.
pi-build:
    bash astrohud-frame/scripts/build-pi-container.sh

# Test the application and offline boot configuration.
image-check:
    cargo test -p astrohud-frame -p astrohud-provisioner -p astrohud-rest
    python3 astrohud-image/scripts/test-configure-boot.py
    python3 astrohud-image/scripts/test-image-artifact.py
    python3 astrohud-image/scripts/test-expand-root.py

# Build the default clean image.
image-build output="target/astrohud-images/memory-lane-clean.img": (image-build-clean output)

# Build the older beta clone as a fallback; refuses to overwrite output.
image-build-beta output="target/astrohud-images/memory-lane-beta.img": pi-build
    sudo env "ASTROHUD_SERVER_URL=$ASTROHUD_SERVER_URL" bash astrohud-image/scripts/sanitize-clone.sh "$ASTROHUD_MASTER_IMAGE" "$1" "$ASTROHUD_OPERATOR_PUBKEY" target/pi-bookworm/aarch64-unknown-linux-gnu/release/astrohud-frame target/pi-bookworm/aarch64-unknown-linux-gnu/release/astrohud-provisioner

# Build a clean, size-derived image in Docker (requires privileged build namespaces).
image-build-clean output="target/astrohud-images/memory-lane-clean.img": pi-build
    bash astrohud-image/scripts/build-clean-container.sh "$1"

# Refresh apps and repackage the previous clean OS build; no OS package updates.
image-repack output="target/astrohud-images/memory-lane-clean.img": pi-build
    bash astrohud-image/scripts/build-clean-container.sh "$1" --image-only

# Inspect file, partition, and allocated filesystem sizes without mounting.
image-size image="target/astrohud-images/memory-lane-clean.img":
    python3 astrohud-image/scripts/image-artifact.py info "$1"

# Verify and produce a smaller .img.zst download; preserves the raw flash image.
image-compress image="target/astrohud-images/memory-lane-clean.img":
    python3 astrohud-image/scripts/image-artifact.py compress "$1"

# List disks so you can identify the removable SD card.
image-devices:
    lsblk -e 7 -o NAME,PATH,SIZE,TYPE,TRAN,RM,MOUNTPOINTS,MODEL

# Verify an image against its build checksum without writing a card.
image-verify image="target/astrohud-images/memory-lane-clean.img":
    python3 astrohud-image/scripts/image-artifact.py verify "$1"

# ERASE the explicit removable disk, write the image, and verify readback.
image-flash device image="target/astrohud-images/memory-lane-clean.img":
    sudo bash astrohud-image/scripts/flash-image.sh "$2" "$1"
