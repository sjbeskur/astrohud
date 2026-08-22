# AstroHUD

AstroHUD currently contains a small proof of concept for sending an image to an
Actix WebSocket service and displaying it in a browser through WebAssembly.

The project is being rehabilitated as the foundation for a channel-based
digital picture frame. See [VISION.md](VISION.md) for the product thesis,
boundaries, and proof-of-concept milestones.

## Current components

- `astrohud-rest`: Actix server, static display page, and WebSocket relay
- `astrohud-client`: command-line image sender
- `astrohud-frame`: native SDL2 frame with an offline, size-bounded disk cache
- `astrohud-provisioner`: native setup-AP and captive Wi-Fi provisioning service
- `astroview_wasm`: browser display client

The original WebSocket path remains a transport demonstration. The newer
vertical slice persists channel images locally, and the native frame caches
them offline. Authentication, pairing, and hosted media storage are not yet
implemented.

## Prerequisites

- A current stable Rust toolchain
- [`wasm-pack`](https://rustwasm.github.io/wasm-pack/installer/) when rebuilding
  the browser module

## Build

Check the Rust workspace:

```sh
cargo check --workspace
```

Rebuild the browser module directly into the server's static directory:

```sh
wasm-pack build astroview_wasm \
  --target web \
  --out-dir ../astrohud-rest/static \
  --out-name astroview_wasm
```

## Run the current demo

Start the service from any directory:

```sh
cargo run --package astrohud-rest -- 0.0.0.0:8080
```

The rehabilitation branch currently exposes three browser views:

- `http://localhost:8080/sender.html` creates channels and publishes photos.
- `http://localhost:8080/frame.html` displays the persistent manifest for the
  proof-of-concept frame named `Grandma's kitchen`.
- `http://localhost:8080/` preserves the original WebSocket/WASM transport
  demo. It derives its WebSocket endpoint from the page URL, so remote viewers
  can use the host's LAN address without rebuilding the WASM module.

Persistent proof-of-concept data defaults to `astrohud-rest/data`. Set
`ASTROHUD_DATA_DIR` to use another directory:

```sh
ASTROHUD_DATA_DIR=/tmp/astrohud cargo run --package astrohud-rest -- 127.0.0.1:8080
```

Send an image:

```sh
cargo run --package astrohud-client -- 127.0.0.1:8080 path/to/photo.jpg
```

The command-line sender and root view still use the original transient
WebSocket broadcast. The responsive sender and frame views use the new
persistent channel API.

## Native frame MVP

`astrohud-frame` is the replacement path for running a frame without Chromium.
It synchronizes the existing frame manifest, downloads JPEG and PNG images
atomically, retains the newest images within a configurable disk budget, and
starts from its saved manifest when the server is unavailable.

Build it on a Raspberry Pi or Linux workstation:

```sh
cargo build --release --package astrohud-frame
```

Run a desktop/headless development smoke test with SDL's dummy display:

```sh
SDL_VIDEODRIVER=dummy cargo run --package astrohud-frame -- \
  --windowed \
  --server http://127.0.0.1:8080
```

Run directly on the Pi's console display after stopping any compositor:

```sh
SDL_VIDEODRIVER=kmsdrm ./target/release/astrohud-frame \
  --server http://192.168.50.144:8080 \
  --frame demo-frame \
  --cache-dir /var/lib/astrohud
```

See [`astrohud-frame/README.md`](astrohud-frame/README.md) for dependencies,
configuration, cache semantics, and the physical-Pi validation checklist.

## Device provisioning MVP

`astrohud-provisioner` supplies the no-mobile-app first-boot path. An
unprovisioned frame broadcasts a protected, per-device `AstroHUD-XXXXXX`
network, displays a scannable Wi-Fi QR setup card, and serves a local Wi-Fi
selection page. It tests candidate credentials before atomically replacing the
active NetworkManager profile and its recovery backup. See
[`astrohud-provisioner/README.md`](astrohud-provisioner/README.md) for the state
model, security boundaries, and deployment files.

## Local vertical-slice API

- `GET /api/health`
- `GET|POST /api/channels`
- `POST /api/photos` using multipart fields `channel_id` and `photo`
- `GET /api/frames/{frame_id}/manifest`
- `GET /media/{storage_key}`

New channels are automatically subscribed to the single `demo-frame` fixture.
This deliberate shortcut will be replaced by explicit frame pairing and
subscription management in the next milestone.

## Repository state

The current branch predates the picture-frame product direction. Rehabilitation
work should keep the transport demo operational while the first persistent
vertical slice is introduced.
