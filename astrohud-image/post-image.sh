#!/bin/bash
set -euo pipefail
script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source_image="$1/${IGconf_image_name}.img"
if [ -n "${ASTROHUD_IMAGE_OUTPUT:-}" ]; then
    python3 "$script_dir/image-artifact.py" publish "$source_image" "$ASTROHUD_IMAGE_OUTPUT"
    cp "$script_dir/package-sizes.tsv" "$ASTROHUD_IMAGE_OUTPUT.packages.tsv"
    cp "$IGconf_target_dir/manifest" "$ASTROHUD_IMAGE_OUTPUT.manifest"
    if [ "$(id -u)" = 0 ] && [ -n "${ASTROHUD_BUILD_UID:-}" ]; then
        chown "$ASTROHUD_BUILD_UID:$ASTROHUD_BUILD_GID" \
            "$ASTROHUD_IMAGE_OUTPUT" "$ASTROHUD_IMAGE_OUTPUT.sha256" \
            "$ASTROHUD_IMAGE_OUTPUT.json" "$ASTROHUD_IMAGE_OUTPUT.packages.tsv" "$ASTROHUD_IMAGE_OUTPUT.manifest"
    fi
fi
