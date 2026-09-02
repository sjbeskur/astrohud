# AstroHUD

AstroHUD currently contains a small proof of concept for sending an image to an
Actix WebSocket service and displaying it in a browser through WebAssembly.

The project is being rehabilitated as the foundation for a channel-based
digital picture frame. See [VISION.md](VISION.md) for the product thesis,
boundaries, and proof-of-concept milestones. The living
[product burndown](PRODUCT_BURNDOWN.md) tracks the Pi appliance, hosted
software/user engagement, and mobile application as separate delivery streams.
The current delivery target is the three-household friendly beta described in
[`MVP.md`](MVP.md).

## Current components

- `astrohud-rest`: Actix server, static display page, and WebSocket relay
- `astrohud-client`: command-line image sender
- `astrohud-frame`: native SDL2 frame with an offline, size-bounded disk cache
- `astrohud-provisioner`: native setup-AP and captive Wi-Fi provisioning service
- `astroview_wasm`: browser display client

The original WebSocket path remains a transport demonstration. The friendly
beta path now isolates households, claims first-boot devices, and gives owners
revocable private sender links. Images are persisted locally and the native
frame caches them offline. Device-authenticated media sync and hosted storage
are not yet implemented.

AstroHUD's shared visual tokens and interface rules are documented in
[`UI_THEME.md`](UI_THEME.md). They adapt the operational design language from
the Advanced Data Machines company homepage while keeping photographs primary.

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

- `http://localhost:8080/` creates channels and publishes photos. The explicit
  `/sender.html` path serves the same interface.
- `http://localhost:8080/frame.html` displays the persistent manifest for the
  proof-of-concept frame named `Grandma's kitchen`.
- `http://localhost:8080/wasm_index.html` preserves the original WebSocket/WASM
  transport demo. It derives its WebSocket endpoint from the page URL, so
  remote viewers can use the host's LAN address without rebuilding the WASM
  module.

Persistent proof-of-concept data defaults to `astrohud-rest/data`. Set
`ASTROHUD_DATA_DIR` to use another directory:

```sh
ASTROHUD_DATA_DIR=/tmp/astrohud cargo run --package astrohud-rest -- 127.0.0.1:8080
```

### Try the friendly-beta onboarding flow locally

With the server running, open `/device-simulator.html` and choose **Continue
onboarding**. The device-bound page waits for enrollment, asks for the place
name, creates owner access, claims the frame, and continues to the owner page
in the same browser. No household identifier, owner activation link, or copied
claim code is part of the normal owner's journey.

The operator command remains available for recovery and attended support:

```sh
cargo run --package astrohud-rest --bin astrohud-admin -- \
  create-household "Tester household" "Primary owner" \
  target/activation-cards/tester-household.png
```

The optional recovery PNG and its activation URL are private credentials.
Protect them like a password until delivered to the owner.

Once the frame is connected, create a sender link from the owner page. Open
that link in a private browser window to exercise the recipient experience:
the destination is fixed by the invitation, the sender previews a JPEG or PNG,
and the server derives the household and channel rather than trusting form
fields. The claimed device simulator then downloads and displays the newest
photo using its own credential. Disabling the sender link on the owner page
takes effect immediately.

Use `ASTROHUD_PUBLIC_URL` when the server is accessed through a different local
hostname. Set `ASTROHUD_SECURE_COOKIES=true` only when it is served through
HTTPS.

When no endpoint argument is supplied, the service listens on
`0.0.0.0:$PORT`. This is the launch mode used by hosted services such as
Render; the explicit endpoint commands above remain available for local use.

### Host the friendly beta on Render

Create one web service from the repository. Leave **Root Directory** blank so
Cargo can read the workspace manifest, while building and starting only the
server package:

- Build command: `cargo build --release --package astrohud-rest`
- Start command: `./target/release/astrohud-rest`
- Health check path: `/api/health`

Attach a persistent disk at `/var/data`; both the SQLite database and uploaded
photos must survive service restarts. Configure these environment variables:

```text
ASTROHUD_DATA_DIR=/var/data/astrohud
ASTROHUD_PUBLIC_URL=https://app.astrohud.com
ASTROHUD_SECURE_COOKIES=true
```

Render supplies `PORT` automatically. Do not set it manually.

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
active NetworkManager profile and its recovery backup. Once online, it
generates an appliance-only device credential, enrolls with the server,
continues the same phone at the public onboarding page, and creates owner
access when the owner names the place. A high-entropy, single-use bootstrap
token binds the browser to that appliance; its short claim code remains only
as an attended support fallback. The setup screen disappears when the owner
claims the frame. See
[`astrohud-provisioner/README.md`](astrohud-provisioner/README.md) for the state
model, security boundaries, and deployment files.

A physical recovery/factory-reset button is planned but not implemented. The
proposed part, GPIO wiring, hold behavior, security boundaries, and hardware
test procedure are documented in [`RESET_BUTTON.md`](RESET_BUTTON.md).

## Local vertical-slice API

- `GET /api/health`
- `GET|POST /api/channels`
- `POST /api/photos` using multipart fields `channel_id` and `photo`
- `GET /api/frames/{frame_id}/manifest`
- `GET /media/{storage_key}`

The friendly-beta routes add owner sessions, device enrollment and claiming,
revocable sender invitations, invitation-scoped uploads, and
device-authenticated manifest and media delivery under `/api/beta`. The legacy
manifest remains available for the original demo frame, but `/media` now serves
only demo-household photos; beta-household files require the claimed device's
credential.

Channels created through the legacy demo API are automatically subscribed to
the single `demo-frame` fixture. The friendly-beta claim flow instead creates
an explicit household-scoped frame subscription.

## Repository state

The current branch predates the picture-frame product direction. Rehabilitation
work should keep the transport demo operational while the first persistent
vertical slice is introduced.
