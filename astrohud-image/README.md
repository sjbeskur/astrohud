# AstroHUD appliance image

This definition builds a sealed ARM64 image for Raspberry Pi Zero 2 W with
official `rpi-image-gen` v2.6.0. It installs the native frame, Wi-Fi
provisioner, recovery guard, required runtime packages, and operator-only SSH
support. The output contains no device identity, credential, Wi-Fi profile,
photo, cache manifest, machine ID, or SSH host key.

The beta image uses a locked local account named `astrohud-support`, accepts
only the operator public key supplied at build time, and permits that account
to use passwordless `sudo` for attended appliance support. Replace this
maintenance policy before a broader release.

## Build

First cross-compile the application binaries:

```sh
astrohud-frame/scripts/build-pi-container.sh
```

Check out the pinned official image generator and install its documented host
dependencies. Then run:

```sh
RPI_IMAGE_GEN_DIR=/path/to/rpi-image-gen \
ASTROHUD_OPERATOR_PUBKEY=/path/to/operator.pub \
ASTROHUD_SERVER_URL=http://192.168.50.144:8080 \
astrohud-image/scripts/build-image.sh
```

The URL is intentionally a build input. Use the local server for attended LAN
testing and the future HTTPS Render origin for tester images.

The post-build check fails if any appliance runtime state or reusable host
identity leaks into the root filesystem. Do not use an assigned Pi as an image
source and do not "seal" an assigned appliance in place.

## Friendly-beta clone path

For the two-unit attended beta, `scripts/sanitize-clone.sh` can transform a
powered-off image of the working appliance into a separate golden image. It
never modifies its input image. The sanitizer renames the maintenance account
to `astrohud-support`, replaces its home with only the supplied public key,
removes all device/Wi-Fi/photo/log identity, arranges unique SSH host-key
generation, and fills filesystem free space with zeros so deleted secrets are
not recoverable from the distributed copy.

Run it only against the verified master file, never a block device:

```sh
sudo astrohud-image/scripts/sanitize-clone.sh \
  target/astrohud-images/astrohud-original-2026-08-30.img \
  target/astrohud-images/astrohud-tester-golden.img \
  /home/sbeskur/.ssh/id_ed25519.pub \
  target/pi-bookworm/aarch64-unknown-linux-gnu/release/astrohud-frame \
  target/pi-bookworm/aarch64-unknown-linux-gnu/release/astrohud-provisioner
```

The optional final arguments replace the frame viewer and provisioner in the
sanitized output with the tested ARM64 builds. Use them whenever application
code has changed since the source card was captured.

## First boot

The image starts without a household Wi-Fi profile. The provisioner generates
the identity and credential on the Pi, starts the protected setup network, and
shows the Wi-Fi QR card. After Wi-Fi succeeds it enrolls with the configured
server and replaces the setup card with the short claim code. Claiming removes
that card and allows the authenticated frame slideshow to appear.
