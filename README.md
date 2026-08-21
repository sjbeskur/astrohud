# AstroHUD

AstroHUD currently contains a small proof of concept for sending an image to an
Actix WebSocket service and displaying it in a browser through WebAssembly.

The project is being rehabilitated as the foundation for a channel-based
digital picture frame. See [VISION.md](VISION.md) for the product thesis,
boundaries, and proof-of-concept milestones.

## Current components

- `astrohud-rest`: Actix server, static display page, and WebSocket relay
- `astrohud-client`: command-line image sender
- `astroview_wasm`: browser display client

This is still a transport demonstration. It does not yet persist images,
authenticate clients, model channels, or cache media offline.

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
