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

# Build a new beta image from the saved master; refuses to overwrite output.
image-build output="target/astrohud-images/memory-lane-beta.img": pi-build
    sudo env "ASTROHUD_SERVER_URL=$ASTROHUD_SERVER_URL" bash astrohud-image/scripts/sanitize-clone.sh "$ASTROHUD_MASTER_IMAGE" "$1" "$ASTROHUD_OPERATOR_PUBKEY" target/pi-bookworm/aarch64-unknown-linux-gnu/release/astrohud-frame target/pi-bookworm/aarch64-unknown-linux-gnu/release/astrohud-provisioner

# Build the clean minbase image; requires RPI_IMAGE_GEN_DIR and host dependencies.
image-build-clean: pi-build
    bash astrohud-image/scripts/build-image.sh

# List disks so you can identify the removable SD card.
image-devices:
    lsblk -e 7 -o NAME,PATH,SIZE,TYPE,TRAN,RM,MOUNTPOINTS,MODEL

# Verify an image against its build checksum without writing a card.
image-verify image="target/astrohud-images/memory-lane-beta.img":
    sha256sum --check "$1.sha256"

# ERASE the explicit removable disk, write the image, and verify readback.
image-flash device image="target/astrohud-images/memory-lane-beta.img":
    sudo bash astrohud-image/scripts/flash-image.sh "$2" "$1"
