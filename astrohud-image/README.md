# Memory Lane appliance image

The default workflow builds a fresh ARM64 Debian appliance for Raspberry Pi
Zero 2 W. It installs the native viewer, Wi-Fi setup wizard, NetworkManager,
required graphics/firmware, recovery guard, and key-only operator SSH access.
It includes no desktop, browser, compiler, kernel headers, preallocated photo
cache, or disk-backed swap. The earlier cloned beta image remains a fallback.

The official `rpi-image-gen` v2.6.0 source is pinned to commit
`3f2c916086ad70197945bfc50ef953c1f6035f10`. The clean base uses its ARM64 slim
layer, rather than the broader multi-architecture suite. Build dependencies
and ARM emulation live in a Docker tool image; no build packages are installed
on the workstation.

## Build and inspect

Host requirements: Docker, `just`, Python 3, and an operator SSH public key.
Compression also needs `zstd` on the host. Run from the repository:

```sh
just                         # List commands; no build or flash
just image-check             # Application, boot-config and artifact tests
just image-build             # Cross-compile applications and build clean image
just image-size              # File, partition, allocated and free bytes
just image-verify            # Check the raw image checksum
just image-compress          # Produce .img.zst plus its own checksum
just image-devices           # Identify a removable SD card
just image-flash /dev/sdg     # ERASES the selected card, writes, verifies readback
```

`image-build-clean` is also available as an explicit name for the clean build.
For faster application-only iterations after a successful clean build, use
`just image-repack <new-output.img>`. It reuses the cached OS filesystem,
refreshes application binaries and service files, reruns sealing checks, and
packages a new image. It does not install OS updates; use `image-build` after
changing OS packages, base layers, or target hardware.
The default artifact is `target/astrohud-images/memory-lane-clean.img`.
Use an explicit output path to keep multiple versions; existing images are
never overwritten:

```sh
just image-build target/astrohud-images/memory-lane-test.img
just image-size target/astrohud-images/memory-lane-test.img
just image-verify target/astrohud-images/memory-lane-test.img
just image-compress target/astrohud-images/memory-lane-test.img
just image-flash /dev/sdg target/astrohud-images/memory-lane-test.img
```

Each clean build publishes an `.img.sha256`, `.img.json` size report, and
`.img.packages.tsv` package-size inventory, and `.img.manifest` package-version
inventory alongside the raw image. Compression
keeps the raw image and creates `.img.zst` and `.img.zst.sha256`. Verify either
file with `just image-verify <filename>`. The flash recipe accepts the raw
`.img`; the compressed file is for distribution, not a smaller on-card layout.

The image builder uses a privileged Docker container for mmdebstrap's mount
namespaces and registers ARM64 binfmt emulation on x86 hosts. It mounts the
repository at `/workspace`; it does not run flashing commands. Inspect those
commands in `scripts/build-clean-container.sh` and `scripts/container-entry.sh`.
The Docker build can take longer the first time; host tools and package caches
are retained under ignored `target/image-work` for subsequent builds.

`ASTROHUD_OPERATOR_PUBKEY` defaults to `~/.ssh/id_ed25519.pub` and
`ASTROHUD_SERVER_URL` defaults to `https://app.astrohud.com`. Only the supplied
public key is copied into the build. The image's support account is
`astrohud-support`, with key-only SSH and passwordless sudo for attended beta
support. Review that maintenance policy before a broader release.

The default wireless regulatory domain is US, and the system timezone is UTC.
Set these deployment-specific values in `config/astrohud-zero2w.yaml` before
building images for another region.

## Size and first boot

The boot partition is sized at 200% of its content and the root partition at
150%, instead of reserving a fixed 4 GiB root or cloning an entire card. Exact
sizes depend on the resolved Debian/Raspberry Pi package versions; inspect the
resulting artifact rather than treating a target size as a guarantee.

On first boot, `memory-lane-expand-root.service` extends the existing second
partition and its ext4 filesystem into the card's remaining space. The start
sector and existing data are preserved. It records completion only after both
steps succeed, and retries after a failed boot. The expanded filesystem holds
photos as well as the OS. The viewer's default 1 GiB cache limit is a usage
limit, not a preallocated file in the image.

The resize service supports this image's standard two-partition, ext4 root
layout. It must be validated on a spare physical Pi/card before distributing
this image. Removing large swap allocations also needs a memory-pressure test
on the 512 MiB Pi Zero 2 W. The image contains no application updater yet.

The post-build hook clears package download/index caches and rejects leaked
runtime identities, device credentials, Wi-Fi profiles, photos, assigned
machine IDs, and SSH host keys. Each Pi generates its own identity and SSH
host keys on boot. Services are enabled after the overlay has installed their
unit files. The build records a source revision in `/etc/astrohud/image-release`;
package versions are recorded in the published manifest. Package repositories
are not snapshot-pinned, so this is not a bit-for-bit reproducible build.

## Display and setup behavior

Quiet boot configuration hides the firmware splash, reduces boot messages,
moves the kernel console off the viewer's terminal, and masks the tty1 login
prompt. Logs remain available over SSH. The native viewer paints a centered
Memory Lane / Starting up card once graphics are ready, before loading its
cache. Photos and the setup QR replace it when available. The earliest
firmware/kernel interval remains blank; no splash daemon or artificial delay
is added.

The Pi-hosted phone setup uses a Wi-Fi → Password → Finish wizard, with Back
and Show password controls and no decorative vertical color bars. It scans
fresh before opening the AP. After entering home Wi-Fi details, wait for the
TV to confirm Wi-Fi was saved, reconnect the phone to the internet, and tap
Continue setup. A timer does not force navigation or assume a connection
succeeded. The owner then names the frame through the private device-bound
setup link. Normal setup needs no copied activation link or typed claim code.

The current viewer supports centered, cropped photo backgrounds with blur and
slight darkening, next-image preparation,
owner-selected photos, timed pins, and slideshow intervals. These controls
require matching REST/web changes; rebuilding the image does not deploy the
hosted website.

## Existing beta clone fallback

This path retains the older, larger Debian installation and requires a saved
master image, which is not included in the repository:

```sh
just image-build-beta target/astrohud-images/memory-lane-beta-fallback.img
just image-flash /dev/sdg target/astrohud-images/memory-lane-beta-fallback.img
```

`ASTROHUD_MASTER_IMAGE` defaults to
`target/astrohud-images/astrohud-original-2026-08-30.img`.
The sanitizer works on a new output copy, strips device/Wi-Fi/photo/log state,
replaces the maintenance account with key-only support access, and scrubs free
space. It never modifies the master file. Do not sanitize an assigned appliance
in place.

## Native image-generator workflow

For a supported native Debian/Raspberry Pi ARM64 build host, install the
pinned generator's documented host dependencies, then run:

```sh
just pi-build
RPI_IMAGE_GEN_DIR=/path/to/rpi-image-gen \
ASTROHUD_OPERATOR_PUBKEY=/path/to/operator.pub \
ASTROHUD_IMAGE_OUTPUT="$PWD/target/astrohud-images/memory-lane-clean.img" \
bash astrohud-image/scripts/build-image.sh
```

Use `just image-devices` to recheck the target before every flash. If the card
is busy, close file manager windows and terminals using it, then retry. No
image data is written until its mounted filesystems have been unmounted.
