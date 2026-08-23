# AstroHUD desktop handoff

## Release consolidation (2026-08-23)

The repository now has a clean release boundary on `master`:

- annotated `v0.1.0` points to `922dbe6`, the original WebSocket/WASM
  prototype at the former `master` tip;
- `feature/ui-refresh` was merged into `master` with the explicit merge commit
  `dede9f8`; and
- annotated `v0.2.0` points to `dede9f8`, the validated native picture-frame,
  provisioning, UI, location-tab, and product-planning POC.

`feature/picture-frame-poc` and `refactor/cleanup_rm_vestigials` were already
ancestors of `feature/ui-refresh`, so no duplicate merges were required. The
feature branches remain available for history. The merged workspace passed
`cargo test --workspace` and
`cargo clippy --workspace --all-targets -- -D warnings` before `v0.2.0` was
created. `master`, `v0.1.0`, and `v0.2.0` were subsequently confirmed on
`origin`.

The original developer worktree remains on `feature/ui-refresh` at `6ecae82`.
It contained a pre-existing uncommitted
`astrohud-rest/static/wasm_index.html` edit throughout the release work, and
that edit was never staged or included in `v0.2.0`. At the final handoff check,
the worktree was clean and that file matched the branch, so the earlier local
edit had been resolved outside the isolated release operation. Resume new work
from `master`.

## Native SDL frame deployment (2026-08-22)

The Pi Zero 2 W at `192.168.50.57` now runs the native `astrohud-frame`
viewer instead of Chromium/labwc. It was cross-compiled on the workstation in
the Bookworm build container; Rust and other build tools were not installed on
the Pi.

- Binary: `/usr/local/bin/astrohud-frame`
- Unit: `/etc/systemd/system/astrohud-frame.service`
- Cache: `/var/lib/astrohud` (display images plus the saved manifest)
- Source: `http://192.168.50.144:8080`, frame `demo-frame`
- State: `astrohud-frame.service` enabled, active, and verified after a Pi reboot
- Display: DRM reports the app owns a `1920x1080` XR24 framebuffer on `HDMI-A-1`
- Memory after reboot: approximately 219 MiB used / 195 MiB available, with the
  viewer at approximately 85 MiB RSS. The Chromium baseline was approximately
  314 MiB used / 100 MiB available and was using substantial swap.

Rebuild with:

```sh
./astrohud-frame/scripts/build-pi-container.sh
```

The deployable artifact is
`target/pi-bookworm/aarch64-unknown-linux-gnu/release/astrohud-frame`. The
currently deployed UI/location build has SHA-256
`6cc3eae1ff8d67d640d28ec8b88d889131419c77b253eb7285f5addcec35ab13`
and a maximum glibc requirement of `GLIBC_2.34`.

To roll back to the preserved browser kiosk:

```sh
sudo systemctl disable --now astrohud-frame.service
sudo systemctl start getty@tty1.service
```

The original labwc autostart remains in place, and its deployment-time backup
is `~/.config/labwc/autostart.chromium-backup-20260822` on the Pi.

The REST server on the workstation is still started manually. The current
debug build is running as `target/debug/astrohud-rest 0.0.0.0:8080`; a future
manual start can use
`cargo run --package astrohud-rest -- 0.0.0.0:8080`. It will not survive a
workstation reboot until a separate service is added. Cached photos continue to
display while it is unavailable.

### Wi-Fi recovery and maintenance access

On 2026-08-22 the Pi fell off Wi-Fi after running overnight. A cold boot did
not restore connectivity because NetworkManager had no persistent wireless
profile on disk. Recreating the connection through `nmtui` restored
`192.168.50.57`. The earlier disconnect's root cause could not be recovered
because journald had no persistent journal.

The replacement connection is now the persistent profile `astrohud-wifi`, with
autoconnect enabled, unlimited autoconnect retries, and Wi-Fi power saving
disabled. Its keyfile is
`/etc/NetworkManager/system-connections/astrohud-wifi.nmconnection`. A root-only
recovery copy lives at `/etc/astrohud/wifi-profile.nmconnection`; it contains
the Wi-Fi credential and must never be copied into the repository.

`astrohud-wifi-guard.timer` runs at boot and once per minute. It restores a
missing or unreadable profile from that backup and asks NetworkManager to
reconnect when `wlan0` is disconnected. The recovery path was tested by moving
the live profile aside: the guard restored it byte-for-byte without dropping
the active connection. A subsequent reboot also passed; the guard explicitly
activated Wi-Fi at its first 30-second run, then the frame returned at `.57`.

Journald now uses bounded persistent storage: at most 32 MiB and seven days,
configured by `/etc/systemd/journald.conf.d/astrohud.conf`. Future network
failures can be inspected after reboot with `journalctl -b -1`.

### Setup-AP provisioning MVP

The native `astrohud-provisioner` is installed at
`/usr/local/sbin/astrohud-provisioner`; its enabled systemd unit exits dormant
when a household profile or recovery backup exists. Normal-mode reboot was
verified with Wi-Fi, the guard timer, provisioner, and SDL viewer together.

This Pi's non-secret device identity is `MSCKK8`, and its setup SSID is
`AstroHUD-MSCKK8`. The separate random setup password exists only in the
root-owned `/etc/astrohud/device.json`. Retrieve manufacturing label/QR data
locally with `sudo astrohud-provisioner --print-label`; never put that output
in logs or the repository.

Setup mode uses NetworkManager shared mode plus `dnsmasq-base`, the protected
setup AP, captive DNS at `10.42.0.1`, and a native HTTP portal. It scans nearby
networks, tests a candidate while the AP is down, then atomically updates the
active `astrohud-wifi` profile and protected backup. A failure recreates the
setup AP. The Wi-Fi guard ignores `wlan0` while the provisioning marker exists.

Local maintenance commands:

```sh
sudo astrohud-enter-setup  # intentionally disconnects household Wi-Fi
sudo astrohud-exit-setup   # cancels and restores the protected profile
```

The production reset button is intentionally deferred until hardware is
available. The proposed SparkFun panel-mount switch, GPIO17-to-ground wiring,
medium-hold Wi-Fi recovery, long-hold factory reset, and validation checklist
are captured in [`RESET_BUTTON.md`](RESET_BUTTON.md). Nothing currently watches
a GPIO pin or performs a factory reset.

The complete attended flow was verified on 2026-08-22 with a ten-minute
automatic rollback armed: the phone discovered `AstroHUD-MSCKK8`, received an
address, opened the captive portal, selected the household network, and caused
the frame to return at `.57`. The active profile and protected backup compared
byte-for-byte, the setup marker and volatile AP keyfile were removed, image
sync resumed, and the rollback was canceled. A final reboot from the
portal-created profile passed with Wi-Fi, guard timer, dormant provisioner, and
SDL viewer all healthy.

The native service was changed from `Restart=always` to `Restart=on-failure`.
Pressing Escape now provides a deliberate maintenance exit, while unexpected
failures still restart the viewer.

### ADM-aligned UI refresh

Branch `feature/ui-refresh` adapts the operational design language from
`~/repos/nemesis/adm-website` branch `feature/company-homepage-refresh` across
the current AstroHUD product surfaces. Shared tokens and usage rules live in
`UI_THEME.md`.

- `/` and `/sender.html` use the refreshed photo-transmission interface.
- `/frame.html` keeps photos primary and adds restrained connection/place
  telemetry plus a themed empty state.
- The captive portal is self-contained and uses the same offline-safe palette,
  typography, panel geometry, and explicit status language.
- The native 1280×720 setup card uses the same palette while preserving a pure
  black-on-white QR matrix and full quiet zone.
- The legacy `/wasm_index.html` experiment remains available and its existing
  uncommitted user edits were not changed or included.

The ARM64 themed provisioner was installed while dormant; its SHA-256 is
`92a5836e0d120659f3592be63b9783258ca13ed0f7b013308d4408b28ce08755`.
The frame remained active and connected at `192.168.50.57`. A live setup-mode
visual review is still required before calling the physical provisioning UI
final.

### Native photo chrome and location metadata

The SDL viewer now leaves breathing room around each image and draws restrained
ADM-themed chrome outside it. Rail side, signal positions, segment length, and
accent rotation are derived from the photo ID, so different photos vary while
the same photo remains stable across redraws and reboots. The provisioning QR
screen deliberately bypasses photo chrome.

Photo uploads now offer an unchecked `Display approximate location` control.
When selected, `astrohud-rest` reads EXIF GPS before resizing strips metadata,
stores the coordinates server-side, and publishes only a one-decimal display
label such as `40.0 N / 105.3 W` in the frame manifest. The native viewer
renders that optional value in a compact, asymmetric data tab attached to the
lower image edge. Photos without a label receive no tab. The path was verified
end to end with a synthetic geotagged upload; the test database row, server
media copy, and Pi cache entry were removed after visual confirmation.

Human-readable city/region reverse geocoding is intentionally shelved. It is
recorded as future item `C-202` and must remain server-side, round coordinates
before a provider request, cache results, satisfy attribution and usage rules,
and fall back to the existing coordinate label.

### Product burndown

`PRODUCT_BURNDOWN.md` is the living delivery backlog. It separates the Pi
appliance, cloud software/user engagement, and mobile application into three
tracks with product gates, priorities, relative sizes, dependencies, and
acceptance evidence. The agreed sequence is:

1. finish the unattended appliance, physical reset, regular Pi Zero benchmark,
   reproducible image, and resilience testing;
2. establish hosted identity, secure pairing, authorization, invitations, and
   durable media, then prove outside-the-home sending through the responsive
   web experience; and
3. begin a thin native mobile client only after those API and product flows are
   stable.

## Current project state

- Repository: `/home/sbeskur/repos/adm/astro-hud`
- Release branch: `master`
- Release commit/tag: `dede9f8` / `v0.2.0`
- Preserved development worktree: `feature/ui-refresh` at `6ecae82`
- Product-direction commit: `2a72915`
- Persistent local-frame slice commit: `626a9f3`
- Native frame/provisioning POC commit: `938ac77`
- QR-guided onboarding commit: `951b87c`
- ADM-aligned UI commit: `ccf8617`
- Native photo chrome commit: `d0da0c0`
- Opt-in location tab commit: `18e4a15`
- Shelved reverse-geocoding roadmap commit: `2eadfd2`
- Three-track product burndown commit: `6b7f931`
- Final pre-release handoff commit: `6ecae82`
- Release merge commit: `dede9f8`
- Final developer-worktree check: clean; the earlier local
  `astrohud-rest/static/wasm_index.html` edit was not included in the release.

The workstation proof of concept includes persistent SQLite channels, frames,
subscriptions and photo metadata; filesystem image storage; `/sender.html`;
`/frame.html`; and the seeded frame `Grandma's kitchen`.

## Pi Zero 2 W bring-up: resolved

The Pi Zero 2 W is booted and reachable on the LAN at `192.168.50.57`
(previously stuck at seven ACT LED flashes / rainbow splash — resolved via
re-imaging, see git history above for the prior troubleshooting steps if this
recurs on the second unit).

## Kiosk bring-up: resolved

The Pi Zero 2 W (`192.168.50.57`, Debian 13 trixie, kernel
`6.18.34+rpt-rpi-v8`) runs a labwc + Chromium kiosk that autostarts on boot
and displays the workstation's `/frame.html`. Confirmed working end-to-end
across a full reboot: BMX bike photo + "Grandma's kitchen" label rendering
correctly on the physical HDMI output.

Baseline gathered before install: `throttled=0x0` (no undervoltage — power
adapter is fine), 44°C idle temp, HDMI reports `1920x1080`, wifi signal
strong (-40 dBm, no dropped packets), 415Mi total RAM (Pi Zero 2 W spec, very
tight), 12G free on root.

Getting from "boots" to "displays the frame reliably" took four distinct,
stacked fixes — each masked the next one, so they had to be found in order:

1. **Chromium's low-RAM warning dialog blocks forever in a headless kiosk.**
   The Debian `chromium` wrapper shows a GTK "Launch anyway?" dialog
   (`zenoty`) on devices with <512MB RAM, and nothing can click it in kiosk
   mode. Fix: add `--no-memcheck`.
2. **The Pi's GPU (VideoCore IV / VC4) only supports GLES 2.0, but Chromium's
   Raspberry Pi packaging defaults to `--use-angle=gles --enable-gpu-rasterization`,
   which requests an ES3 context and fails
   (`eglCreateContext ES 3.0 failed with error EGL_BAD_ATTRIBUTE`), crash-looping
   the GPU process.** Fix: add `--use-angle=swiftshader --use-gl=angle` to force
   ANGLE's software backend instead of hitting the driver's hardware ES3 path.
   (Tried `--disable-gpu`/`--use-gl=disabled` first — too aggressive, killed
   Wayland buffer presentation entirely, black screen.)
3. **SwiftShader's software rendering is memory-hungry; on 415MB of RAM with
   `zram` swap (which competes for the same physical RAM), the page renderer
   was OOM-crash-looping** decoding 6 photos including a 2016×1512 iPhone
   JPEG. Fix: added a real disk-backed 1G swapfile (`/swapfile`, persisted in
   `/etc/fstab`) alongside the existing zram, giving genuine headroom instead
   of RAM-compressed-into-RAM.
4. **Root cause of the "stuck on blank white screen" symptom, found last:**
   Chromium's *very first* navigation attempt at launch (from the `--kiosk
   <URL>` command-line argument) reliably fails and silently reverts to
   `about:blank` — with no retry. Any navigation triggered afterward (via
   Chrome DevTools Protocol) succeeds immediately, every time, so it isn't a
   "network not ready yet" race — it's a reproducible quirk of navigating via
   launch argument specifically. Fix: launch Chromium pointed at `about:blank`
   with `--remote-debugging-port=9222 --remote-debugging-address=127.0.0.1`,
   and run a small companion watchdog
   (`~/.local/bin/kiosk_watchdog.py`) that polls the CDP `/json` endpoint
   every 10s and calls `Page.navigate` whenever the current page isn't the
   target URL. This also gives free recovery if the workstation is ever
   unreachable at boot and comes back later — verified by manually resetting
   the page to `about:blank` and watching the watchdog recover it within 15s.

There was a separate, now-resolved red herring along the way: the systemd
`--user` unit (`kiosk.service`, via `loginctl enable-linger`) intermittently
crash-looped labwc itself (exit code 1, sometimes 20+ restarts/minute) because
a lingering user manager has no real seat/logind session, and `seatd` can be
flaky granting DRM device access without one. **Replaced systemd user-service
approach entirely** with console auto-login + shell-launched compositor,
which is the standard reliable pattern for Pi kiosks:

- `sudo raspi-config nonint do_boot_behaviour B2` — auto-login `sbeskur` on
  tty1 (creates `/etc/systemd/system/getty@tty1.service.d/autologin.conf`).
- `~/.bash_profile` sources `.bashrc`, then `exec /usr/bin/labwc` when on
  tty1 with no existing Wayland session.
- The old `~/.config/systemd/user/kiosk.service` was disabled
  (`systemctl --user disable kiosk.service`) and is no longer used — ignore
  it if you see it referenced in shell history.

Current `~/.config/labwc/autostart` on the Pi:

```sh
chromium \
  --no-memcheck \
  --use-angle=swiftshader \
  --use-gl=angle \
  --remote-debugging-port=9222 \
  --remote-debugging-address=127.0.0.1 \
  --kiosk \
  --noerrdialogs \
  --disable-infobars \
  --disable-session-crashed-bubble \
  --disable-features=TranslateUI \
  --check-for-update-interval=31536000 \
  --overscroll-history-navigation=0 \
  --disable-pinch \
  --start-fullscreen \
  about:blank &
python3 ~/.local/bin/kiosk_watchdog.py "http://192.168.50.144:8080/frame.html" &
```

Other setup:

- `chromium`, `labwc`, `seatd`, `grim` installed via apt; `seatd` active.
  `grim` (wlroots screenshot tool) was essential for remote visual
  verification — `XDG_RUNTIME_DIR=/run/user/1000 grim /tmp/shot.png` from an
  SSH session, since there's no keyboard/mouse attached to the Pi.
- SSH key auth configured (`~/.ssh/id_ed25519` → `sbeskur@192.168.50.57`) and
  passwordless sudo granted via `/etc/sudoers.d/010-sbeskur-nopasswd`.

Known quirk: chaining multiple commands in one SSH invocation (e.g.
`systemctl ... ; pkill ...; sleep ...`) occasionally drops the SSH
connection itself (exit 255, no output) — cause not fully diagnosed. Splitting
into separate SSH calls per command avoided it every time.

Keep the current service LAN-only; it has no production authentication and
must not be port-forwarded or exposed publicly.

## Image quality: known hardware-limited tradeoff, not fixed

Photos render visibly softer on the kiosk than the source files — confirmed
by direct comparison (source `3528ff92...png` at 1294×898 vs. displayed at
~1556×1080, a ~20% upscale via `object-fit: contain`, with fine text like
the "Vision Street Wear" signage noticeably blurred).

Root cause: the `--use-angle=swiftshader` fix (needed to avoid the VC4/ES3
crash, see above) forces Chromium's image scaling through its cheaper CPU
resampling path instead of GPU-accelerated scaling.

Tried the obvious alternative — `--use-gl=egl` to get a native (non-ANGLE)
GLES2 hardware path. Finding: **modern Chromium (151.x) has removed the
legacy non-ANGLE EGL backend** — `--use-gl=egl` silently downgrades to
`--use-gl=disabled` internally. It still rendered (images looked sharper —
confirmed by screenshot) but was measurably less stable: visible
flicker/ghosting on every ~12s photo transition, and tighter memory
(~308Mi/415Mi used vs. swiftshader's steadier footprint). Reverted to the
swiftshader config — a picture frame that has to run unattended should
favor reliability over sharpness.

This is a genuine hardware ceiling on the Pi Zero 2 W (VideoCore IV /
VC4 GPU, ES2-only, 512MB RAM), not a config bug to keep chasing. Options if
quality matters more going forward:

- **Pre-scale photos server-side — implemented 2026-08-21.**
  `upload_photo` in `astrohud-rest/src/api.rs` now calls
  `downscale_if_oversized()`: photos with a long edge over 1600px are
  decoded, EXIF orientation is applied (verified correct on a real
  2016×1512 iPhone photo tagged "upper-right" — resized to 1200×1600
  portrait, not sideways), downscaled with Lanczos3, and re-encoded as
  JPEG q87. Photos already ≤1600px are stored byte-for-byte untouched
  (verified). Decode failures fall back to storing the original bytes
  unchanged, so no upload type is rejected because of this. Added the
  `image` crate scoped to `default-features = false, features = ["jpeg",
  "png"]` only — the full default feature set pulls in AVIF/rav1e, a
  heavy, slow-compiling dependency this project doesn't need.
  **This measurably helps oversized originals but does not fix the
  softness itself** — a photo already close to display resolution (e.g.
  1600×1200 shown at ~1440×1080) still visibly softens on the kiosk,
  confirmed by direct screenshot-vs-source comparison. The blur is coming
  from Chromium's software scaler at *display* time, not from source
  resolution, so this fix reduces the worst cases without touching the
  root cause.
- **Different Pi Zero 2 W-class board with a real GPU**: e.g. Orange Pi
  Zero 2W (Allwinner H618, Mali-G31 MP2 — supports OpenGL ES 3.2/Vulkan
  1.1 natively, so no ES3 crash or forced-software-rendering tradeoff at
  all) — evaluated 2026-08-21 as a candidate if picture quality turns out
  to matter enough to justify a hardware swap; not tested on real
  hardware. Rejected for now: long lead time, and the Pi Zero 2 W's 512MB
  RAM ceiling isn't something Orange Pi solves any better at a comparable
  price point.
- **Exact-canvas server-side compositing** (resize + letterbox every
  photo to the display's exact pixel dimensions, so Chromium does zero
  scaling) — proposed 2026-08-22, **rejected**: hardcodes one display
  resolution, which breaks the "plug into any monitor" appliance goal.
  Would need to become resolution-aware (client reports its viewport
  size, server resizes-on-request and caches per-resolution) to be worth
  doing — not started.

### Clearing all photos

No delete endpoint exists yet. Direct DB + filesystem access, done
2026-08-22 to reset the demo frame after quality testing:

```sh
python3 -c "
import sqlite3
db = sqlite3.connect('/home/sbeskur/repos/adm/astro-hud/astrohud-rest/data/astrohud.sqlite3')
db.execute('DELETE FROM photos')
db.commit()
"
rm /home/sbeskur/repos/adm/astro-hud/astrohud-rest/data/media/*
```

Safe to run while the service is up (SQLite handles the concurrent
`DELETE` fine). A proper `DELETE /api/photos/:id` (and maybe a
clear-channel endpoint) would be worth adding instead of repeating this.

## Why frame.html doesn't use the WASM viewer (astroview_wasm)

Investigated 2026-08-22 after a question about reusing `astroview_wasm` /
`wasm_index.html` for image display. Findings, grounded in the actual code
(not speculation):

- `astroview_wasm`/`wasm_index.html` is the **older** piece. `frame.html`
  was written fresh in the "add persistent local frame slice" commit
  (`626a9f3`) as a deliberately different, simpler approach — plain
  `<img>` + polling `/api/frames/:id/manifest` every 3s — not an evolution
  of the WASM viewer.
- **The WASM/WebSocket path is disconnected from the working upload flow
  today.** `sender.html` posts to `/api/photos` over HTTP multipart only
  (grepped — zero WebSocket references). The `/ws/` `ImageWebSocket`
  actor (`astrohud-rest/src/websocket.rs`) only relays binary frames
  between whatever clients happen to have a socket open *at that exact
  moment* — nothing today opens one to send a photo. The line that would
  persist an incoming image server-side is commented out:
  `// images.push(bin.clone());`. Client-side, `start_viewer()` creates
  exactly one `<img>` element, overwritten on every message — no history,
  no local storage, nothing written to disk.
- **Net effect: if a frame's socket is disconnected the instant someone
  sends over that path, the photo is gone for that frame, permanently,
  with no record anywhere.** This is close to the *opposite* of "images
  already sent remain available" — that property (last-successfully-shown
  photo stays on screen through a brief network blip) is just normal
  browser behavior and holds equally for `frame.html`'s plain `<img>`.
- The REST+polling model is more resilient specifically *because* every
  photo is durably stored (filesystem + SQLite row) and exposed via a
  manifest: a frame reconnecting after being offline catches up on
  everything it missed, in order. The WebSocket relay has no equivalent
  catch-up — only live listeners ever receive anything.
- This matches `VISION.md`'s stated principles directly: *"WebSockets or
  server-sent events carry only lightweight change notifications"* (not
  raw image delivery) and *"a local cache keeps the slideshow working
  through a service or network outage"* (requires a manifest/store model,
  which the WS relay doesn't have).
- Not fully dead code: the `/ws/` route is still registered in
  `main.rs`. Nothing was removed — the WASM path is just not wired into
  the current product surface. `wasm_index.html` remains the user's
  uncommitted in-progress work (see top of this doc).
- Published a diagram of the current flow + where images physically live:
  see chat history 2026-08-22, artifact "Where Images Live" (published
  from this session — not saved to a file in-repo).

## Scaling beyond one LAN: sender → cloud → registered kiosk

Design discussion 2026-08-22, not started. Current model (one workstation
= the only durable photo store, LAN-only, no auth) already matches
`VISION.md`'s stated hosted-path plan — *"a filesystem-backed storage
interface locally, with an S3-compatible implementation as the hosted
path"* and SQLite migrating to Postgres for multi-tenant use. Scaling
doesn't introduce a new server; the workstation already plays that role
for one LAN — it just needs to become reachable beyond one WiFi network
and handle multiple tenants.

Proposed shape: sender → cloud (multi-tenant) → registered kiosk polls →
image transfers to local device storage → device displays from local
storage. Gaps identified in that shape, in priority order:

1. **Fixed-interval polling doesn't scale to real device counts** — most
   polls return "nothing changed," which is still a request that has to
   be served. The right pattern (and what `VISION.md` already specifies)
   is notify-then-pull: a lightweight push (WebSocket/SSE/push
   notification) says "something changed," which triggers an on-demand
   fetch — not a payload-bearing socket, and not constant timed polling.
   Notably, this is the one part of the old `astroview_wasm` idea that
   was pointed in a reasonable direction — it just needs to trigger a
   pull instead of carrying the image itself, and needs to pair with
   local persistence.
2. **No pairing/device-credential system exists in the code at all.**
   `VISION.md` names this as PoC step 2 ("pair and name the frame from a
   phone-sized web interface"); `api.rs` currently only has
   channels/photos/frames — no registration, no revocable device
   credential.
3. **"Available storage on device" needs an explicit bound + eviction
   policy.** The Pi Zero 2 W has SD card room to spare (~12G free,
   RAM was always the constrained resource, not disk) but nothing today
   defines a cap or what gets evicted first once one is hit.
4. **Where the sync/cache logic runs matters, given this session's
   experience.** It should not live inside Chromium/JS on the Pi — that's
   already the most fragile component in the whole system (see the four
   stacked kiosk-bringup fixes above). A small separate native process
   (e.g. a lightweight Rust binary that pulls and writes files to disk)
   is a lot more robust on this hardware than adding more responsibility
   to the browser tab.
5. **Structural change from today's code**: `frame.html` currently
   fetches straight from the network on every load. A local-cache model
   means the Pi needs *something* serving local files — a tiny local HTTP
   server, or pointing Chromium at `file://` paths directly — not just a
   different URL for the same fetch-and-render pattern.

## Next task

Kiosk is live, confirmed working across a full reboot. Remaining follow-ups:

- The "Waiting for a photo…" placeholder text is still faintly visible
  overlaid on the first photo after load — cosmetic CSS issue in
  `frame.html` (the `#empty` div's `hidden` state lags one paint behind),
  not a functional bug.
- Screen blanking/DPMS behavior over long idle periods hasn't been checked.
- The workstation service (`astrohud-rest`) currently runs via manual
  `cargo run` in the background, not as a persistent systemd unit — it
  needs to be started manually after every workstation reboot.
- Given the workstation's LAN IP (`192.168.50.144`) is DHCP-assigned, a
  static reservation on the router would prevent the kiosk's hardcoded
  target URL from going stale.

## Running the workstation service

```sh
cd /home/sbeskur/repos/adm/astro-hud
cargo run --package astrohud-rest -- 0.0.0.0:8080
```

Use `http://<workstation-LAN-IP>:8080/sender.html` from the phone and
`http://<workstation-LAN-IP>:8080/frame.html` from the Pi.

The workstation's current LAN IP on the same subnet as the Pi (`192.168.50.57`)
is `192.168.50.144`, so from the Pi's kiosk browser that is
`http://192.168.50.144:8080/frame.html`.
