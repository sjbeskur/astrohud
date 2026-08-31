# AstroHUD native frame

`astrohud-frame` is a native SDL2 viewer for the Raspberry Pi Zero 2 W. It
replaces Chromium, labwc, and browser-side polling on the display device while
continuing to use `astrohud-rest` as the source of the frame manifest and media.

## MVP behavior

- Fetches an authenticated device manifest every five seconds when a device
  credential is configured. It retains the legacy frame-ID endpoint for local
  development without a credential.
- Downloads JPEG and PNG images into a persistent on-device cache.
- Writes downloads and the manifest to temporary files before atomically
  renaming them, so an interruption cannot expose partial state.
- Keeps manifest order and retains the newest files within a 1 GiB default
  budget. If one image alone exceeds the cap, it is kept so the frame can still
  display something.
- Loads the saved manifest and cached images before attempting the network.
- Applies EXIF orientation and letterboxes each image against a dark background.
- Retries network synchronization indefinitely while the cached slideshow
  continues.

The cache contains only server-supplied display files. It is not the source of
truth and may be deleted and reconstructed at any time.

## Build dependencies

The Rust `sdl2` crate builds a small bundled SDL configured for DRM/KMS and
OpenGL ES. It intentionally excludes X11, Wayland, audio, joystick, and sensor
support. Building on Raspberry Pi OS Lite requires a C toolchain and the
DRM/KMS development headers:

```sh
sudo apt install \
  build-essential cmake pkg-config \
  libdrm-dev libgbm-dev libegl-dev libgles-dev libudev-dev
```

Then build the release binary:

```sh
cargo build --release --package astrohud-frame
```

### Cross-compile for the Pi

The Pi does not need Rust or any build tools. The recommended build uses the
provided Debian Bookworm container, which also caps the binary's glibc
requirements below Raspberry Pi OS Trixie:

```sh
astrohud-frame/scripts/build-pi-container.sh
```

The deployable binary is written to
`target/pi-bookworm/aarch64-unknown-linux-gnu/release/astrohud-frame`. SDL2 is
linked into the executable; the Pi only needs its normal DRM, GBM, EGL, and
GLES runtime libraries.

`scripts/build-pi.sh` is the underlying direct cross-build for workstations
whose glibc sysroot is known to be no newer than the target appliance.

## Configuration

Command-line arguments override environment variables.

| Argument | Environment | Default |
| --- | --- | --- |
| `--server URL` | `ASTROHUD_SERVER_URL` | `http://127.0.0.1:8080` |
| `--frame ID` | `ASTROHUD_FRAME_ID` | `demo-frame` |
| `--credential-file PATH` | `ASTROHUD_DEVICE_CREDENTIAL_FILE` | unset |
| `--cache-dir PATH` | `ASTROHUD_FRAME_CACHE_DIR` | `./astrohud-frame-data` |
| `--cache-mib N` | `ASTROHUD_CACHE_MIB` | `1024` |
| `--sync-seconds N` | `ASTROHUD_SYNC_SECONDS` | `5` |
| `--slide-seconds N` | `ASTROHUD_SLIDE_SECONDS` | `12` |
| `--windowed` | — | fullscreen |

HTTPS is supported. When a credential file is configured, its trimmed contents
are sent as a bearer credential for both manifest and media requests. Keep that
file readable only by the frame service account. A public deployment must use
HTTPS so the credential is encrypted in transit.

## Test without a display

The bundled SDL includes its dummy backend for automated smoke testing:

```sh
SDL_VIDEODRIVER=dummy timeout 5s \
  cargo run --package astrohud-frame -- \
  --windowed --server http://127.0.0.1:8080
```

## Physical Pi validation

Do the first run manually from a local or SSH-accessible console. Stop labwc or
any other compositor first because only one process can own the DRM display.

```sh
SDL_VIDEODRIVER=kmsdrm ./target/release/astrohud-frame \
  --server http://192.168.50.144:8080 \
  --frame demo-frame \
  --cache-dir /var/lib/astrohud
```

Validate these behaviors before replacing the existing kiosk startup:

1. A newly uploaded photo appears within the configured sync interval.
2. Portrait and landscape images have the correct orientation and aspect ratio.
3. Restarting the process with the server stopped immediately shows cached
   photos.
4. An interrupted download does not create a visible or permanent partial file.
5. Memory remains stable through several slideshow cycles.

`deploy/astrohud-frame.service` is a starting systemd unit. Install it only
after the manual DRM/KMS test succeeds on the physical monitor.

For that unit, create its unprivileged service account and install the binary:

```sh
sudo useradd --system --home-dir /var/lib/astrohud \
  --shell /usr/sbin/nologin astrohud
sudo install -m 0755 \
  target/pi-bookworm/aarch64-unknown-linux-gnu/release/astrohud-frame \
  /usr/local/bin/astrohud-frame
sudo install -m 0644 astrohud-frame/deploy/astrohud-frame.service \
  /etc/systemd/system/astrohud-frame.service
```

Before enabling it, disable the existing tty1 browser-kiosk startup so labwc
and the native viewer do not compete for the same DRM device. Then reload
systemd and enable the native service:

```sh
sudo systemctl daemon-reload
sudo systemctl enable --now astrohud-frame.service
```

To return to the preserved tty1 browser kiosk:

```sh
sudo systemctl disable --now astrohud-frame.service
sudo systemctl start getty@tty1.service
```

## Wi-Fi recovery guard

For an unattended frame, store its NetworkManager profile under the connection
ID `astrohud-wifi`, set it to reconnect indefinitely, and keep a root-only
backup outside NetworkManager's profile directory. The deployment includes a
timer that restores a missing or unreadable profile and requests reconnection
once per minute.

The backup contains the Wi-Fi credential. Create it only on the Pi, keep it
mode `0600`, and never add it to the repository:

```sh
sudo nmcli connection modify "Wi-Fi connection 1" \
  connection.id astrohud-wifi \
  connection.autoconnect yes \
  connection.autoconnect-retries 0 \
  802-11-wireless.powersave 2
sudo install -d -o root -g root -m 0700 /etc/astrohud
sudo install -o root -g root -m 0600 \
  "/etc/NetworkManager/system-connections/Wi-Fi connection 1.nmconnection" \
  /etc/astrohud/wifi-profile.nmconnection
sudo mv "/etc/NetworkManager/system-connections/Wi-Fi connection 1.nmconnection" \
  /etc/NetworkManager/system-connections/astrohud-wifi.nmconnection
sudo chmod 0600 \
  /etc/NetworkManager/system-connections/astrohud-wifi.nmconnection
sudo nmcli connection reload
```

Install `deploy/astrohud-wifi-guard.sh` as
`/usr/local/sbin/astrohud-wifi-guard`, install the matching service and timer
under `/etc/systemd/system`, then enable the timer. The optional bounded
`deploy/astrohud-journald.conf` drop-in preserves up to 32 MiB or seven days of
logs so the cause of a future outage survives a reboot.

## Deliberately deferred

- Lightweight WebSocket/SSE change notifications
- Automatic first-boot pairing and credential rotation
- Checksums and byte sizes in the manifest
- Server-created, resolution-aware display variants
- Cache observability and remote diagnostics
- Cross-fades or other transitions
