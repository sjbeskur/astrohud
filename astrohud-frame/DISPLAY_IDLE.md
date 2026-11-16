# Display-aware idle prototype

This experiment is on `feature/display-idle`, based on production master.
It targets the reference Raspberry Pi Zero 2 W. It does not enable Orange Pi
support or change the API, web app, or published update channel.

## Modes

- `--display-idle off` (default): existing playback; no display probe worker.
- `--display-idle observe`: log detected state and whether idle would apply,
  while keeping normal playback. Start hardware tests here.
- `--display-idle auto`: pause after five seconds of positive standby or HDMI
  disconnection. Unknown, transitional and stale observations resume playback.
- `--display-status`: print one HDMI/CEC observation as JSON and exit without
  starting SDL, syncing photos, or reading device credentials.

`ASTROHUD_DISPLAY_IDLE=off|observe|auto` supplies the same mode through the
service environment. Windowed desktop previews always disable the hardware
probe; the host's HDMI state must not pause a preview window.

## Detection and behavior

The probe worker checks HDMI connector status and queries HDMI-CEC power
status. It does not send TV power, standby or input-selection commands. It
uses a shared CEC initiator, reuses existing logical addresses, and claims a
playback logical address only if the adapter is unconfigured. CEC registration
can announce Memory Lane on the CEC bus. No exclusive adapter ownership is
requested. See the [Linux CEC UAPI](https://docs.kernel.org/userspace-api/media/cec/cec-api.html).

The first version queries CEC only with a single HDMI connector and a single
CEC adapter, avoiding attribution to the wrong screen. Missing devices,
permission errors, unsupported queries, transitions and no definite reply
produce Unknown. Connected HDMI alone does not establish that a screen is on.
A disconnected signal across every enumerated HDMI connector is positive off
evidence. No usable connector means Unknown rather than disconnected.

Queries run away from rendering; samples are refreshed approximately every
2–3 seconds. Samples older than ten seconds are ignored. Automatic idle uses
one-second control-loop waits, stops new image decode/prefetch, freezes the
remaining slide time and avoids acknowledging unseen owner/guest selections.
Background photo sync, storage reporting and other appliance services keep
running. On wake, the viewer redraws its current photo and resumes the saved
slide delay. A duration change or removed current photo received while idle
updates that delay appropriately.

This does not yet handle initializing SDL with no usable display at boot.
CEC power reports, HDMI disconnect/reconnect, picture recovery and CPU savings
must be tested on the actual display before treating automatic mode as ready.

## Local deployment and rollback

Build with `bash astrohud-frame/scripts/build-pi-container.sh`. The ARM64
viewer is under `target/pi-bookworm/aarch64-unknown-linux-gnu/release/`.
Transfer only that viewer to `/tmp/memory-lane-display-idle` on the confirmed
local reference Pi. Record its SHA-256, then run
`scripts/local-display-idle.py observe SHA256` as root on the Pi (or feed the
script to `sudo python3 - observe SHA256` through SSH).

The helper checks the board model, ELF architecture, artifact hash and update
state. It installs the test viewer under
`/opt/memory-lane/local-tests/display-idle/`, leaves the signed release intact,
and overrides only the frame service through
`99-local-display-idle.conf`. Identity, Wi-Fi profiles and updater state are
verified unchanged. The viewer briefly restarts. Failed installation rolls
back its service override.

To return to the installed release, run the same script with `rollback` as
root, or feed it through `sudo python3 - rollback`. This removes the test's
service override and restarts the viewer using the normal installed binary.
No published update or device reset is involved. Remove this test override
before using the normal application updater: it deliberately pins the running
viewer to the prototype, independently of the signed release pointer.

## Hardware test

1. Keep the Pi independently powered; record `--display-status` and service
   logs with the screen on.
2. In observe mode, turn only the screen off for at least ten seconds, then
   back on. Check whether standby/disconnected was detected and pictures return.
3. If signals are reliable, enable auto locally and repeat. Check no hidden
   photos are counted as displayed, downloads still arrive, and the picture
   resumes with no skipped slide countdown.
4. Compare CPU over matching 30–60 second intervals covering several slides.
   Do not claim savings from observation mode or from unmatched short samples.
5. Test an unsupported monitor and CEC disabled: Unknown must keep playback
   working. Test HDMI unplug/replug separately from the screen's power button.
