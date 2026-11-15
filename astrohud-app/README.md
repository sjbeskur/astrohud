# Memory Lane desktop client draft

Tauri 2 desktop draft on `feature/tauri-client`, prepared for desktop iteration
before Android/iOS work. It bundles the real owner and sender interfaces from
`astrohud-rest/static`; the website source remains unchanged. The app has its
own Rust workspace so GTK/WebKit dependencies never enter Pi/server builds.

## Build and run on this Linux workstation

Requires Docker, Rust/Node on the development workstation, and runtime
GTK 3/WebKitGTK 4.1 libraries. Build dependencies stay in Docker. From the repo:

```sh
just client-build
just client-run
```

Debug builds default to `http://127.0.0.1:18116`; packaged release builds
default to `https://app.astrohud.com`. If the local API isn't already running:

```sh
ASTROHUD_DATA_DIR=target/client-demo-data \
  cargo run -p astrohud-rest --bin astrohud-rest -- 127.0.0.1:18116
```

Choose **Open sample frames → Create three sample frames → Open owner
controls**. This creates local sample frames/photos; it never enrolls a real
Pi. Use Home/Photos/Send/Invite/Settings normally. In Send, tap the large
**Choose photos** card to open the file picker. Review previews, use **Add more
photos** or **Remove**, then send with the count/destination button. New picks
append without duplicating the same file; cancelling keeps the current queue.
The local gallery shows the uploaded photo. Both owner and sender forms share
this control.

For an existing account, enter its server origin and paste the full owner
activation, owner recovery, or sender invitation URL. The link must belong to
the selected server. HTTPS is required except for loopback development.
Connecting to a real service performs real actions with that access: this
debug build starts with the local server to keep iteration separate. Sample
frames appear only when a loopback server is selected.

`ASTROHUD_CLIENT_SERVER_URL` can select the initial server. Server changes and
Disconnect discard the old session. The banner always identifies a desktop
draft and the connected server. Your browser's cookies are not imported.

## Package for Ubuntu testers

Run `just client-package` to build an optimized amd64 `.deb` inside Ubuntu
22.04 with its GTK/WebKit libraries. This avoids requiring a newer glibc on
the tester's machine. Docker and Node are required on the build workstation;
testers need neither Rust nor Node nor a development API.

The shareable archive is
`target/releases/Memory-Lane-Desktop-0.1.0-ubuntu22-amd64.tar.gz` and contains
the installer, checksums and [tester instructions](TESTING.txt). Release builds
open the live service by default. No private links, credentials or photos are
bundled. `sudo apt install ./Memory\ Lane_0.1.0_amd64.deb` installs the runtime
dependencies and an applications-menu entry. Automatic app updates are not
part of this preview. This package is for Intel/AMD 64-bit, not ARM devices.

## Shared interface and native transport

`scripts/prepare-ui.mjs` copies an explicit asset list into ignored `dist/`,
injects `native-bridge.js`, and adapts protected photo URLs into native fetches
and temporary blob URLs. Public invitation/recovery links still point to the
connected hosted server. Rebuild after changes; this Docker flow has no hot
reload. On machines with native development packages installed, `npm ci` and
`npm run dev` are also available.

Requests go through a Rust `reqwest` client. Its cookie jar stays in native
memory, never JS/localStorage or plaintext session files. The bridge permits
only GET/POST/DELETE under `/api/beta/` on the configured origin, with request/
response size bounds and redirects disabled. No arbitrary headers, filesystem
reads, remote scripts, or shell commands are exposed to the UI. A local CSP
and restricted window navigation keep the native bridge on bundled pages.

**Access clears when the app closes.** Reopen using your private link. This
is an explicit preview limitation, not production app authentication. Secure
OS credential persistence, revocable app-specific sessions, mobile link
registration, HEIC conversion, share extensions/intents, notifications and
background sending remain future milestones. No phone projects, signing or
store deployment are included. JPEG/PNG uploads use the existing API contract;
each file crosses the draft bridge in memory rather than a streaming native
uploader, so mobile upload memory/permissions still need their own spike.

## Verification

Native transport unit tests run in the build container:

```sh
docker run --rm --user "$(id -u):$(id -g)" \
  -e CARGO_HOME=/workspace/target/tauri-cargo-home \
  -e CARGO_TARGET_DIR=/workspace/target/tauri-desktop \
  -v "$PWD:/workspace" memory-lane-desktop-builder:bookworm \
  cargo test --manifest-path astrohud-app/src-tauri/Cargo.toml --lib --no-default-features
```

`scripts/test-desktop.py` tests the actual native WebKit window using the
official `tauri-driver`, a matching `WebKitWebDriver`, and a loopback test API.
It creates disposable demo data and covers sample setup, protected image
rendering, file selection/preview/upload, owner recovery, invited sender
upload, role isolation and clearing access after restart. It defaults to a
driver on port 4447; set `TAURI_TEST_DRIVER` to another loopback driver.
Set `TAURI_TEST_BINARY` to test an extracted release binary and set
`ASTROHUD_CLIENT_SERVER_URL=http://127.0.0.1:18116` in the driver's environment
so the release test uses disposable local data.
Use an isolated display so automated checks do not interact with a user's
desktop window. Test tools and screenshots live under ignored `target/` and
`/tmp`; they are not application dependencies.

The shared picker browser test lives at
`astrohud-rest/scripts/test-photo-picker.mjs`, independently of this app. It
checks both roles, add/remove/cancel/duplicates, destination labels and uploads
at phone and desktop widths. Set `PUPPETEER_MODULE` to the installed Puppeteer
module and run it against the loopback API.
