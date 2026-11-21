# Memory Lane handoff

## Attended reset confirmed empty frame and quicker portal — 2026-11-20

The user confirmed old photos stayed gone after re-enrollment and the portal
felt quicker. On .57 the manifest and on-disk photo cache both had zero
photos. On the latest setup the first portal response was logged 0.33 seconds
after DHCP acknowledgment. That measures the Pi response, rather than the
full time from camera scan to the page appearing on the phone.

The user still needed two scans and confirmed entering the home Wi-Fi password
in the setup window opened from Camera. The prior attempt showed the iPhone
closing that window on the Wi-Fi switch. The second QR remains the recovery
path after the phone closes its setup page. A regular Safari tab has yet to be
tested in an attended run.

## Photo reset fix live; setup DNS corrected on .57 — 2026-11-20

User repeated reset: photos reappeared, two QR scans still needed, initial
portal slow. Latest provisioner hash d9a480... verified running. Identity
rotated at 06:24:32 UTC and current cached photos were written from 06:25:40:
the server downloaded old photos again. Hosted legacy channel subscriptions
caused this. v0.5.8 adds frame-local exclusions on re-enrollment of a device
with completed reset history, for BOTH bootstrap/QR and manual-code claims.
The reset device's TV manifest and owner gallery start empty; household photo
rows and other devices remain intact. Later deliveries still work. Changes
apply at the NEXT claim, not retroactively to the currently claimed device.
Do not delete the household library or automatically reset the Pi.

61 API/admin tests passed in isolated production checkout, including a new
regression covering both claim paths, gallery/manifest parity, old credential
rejection, other-device preservation and new deliveries. Production master
6c831ea pushed and merged back to release/0.2.x. Live health verified v0.5.8,
schema 16/status ok, and onboarding bytes match. Verification changed no
accounts. Report target/reset-library-deployment.json; tests
/tmp/astrohud-reset-library-production-tests.log. Brief rollout 502s recovered.

Portal timing: last setup AP ready 06:24:35 UTC, DHCPACK 06:24:48, first HTTP
06:24:49; response sends <1 ms. This does not measure phone rendering or prove
the whole pre-DHCP interval was network delay rather than user interaction.
Found actual Pi config lacked repository local=/#/. Inherited system DNS also
overrides that catch-all when upstream exists. Added no-resolv to the shared
setup-only config. Installed /etc/NetworkManager/dnsmasq-shared.d/astrohud-captive.conf
SHA256 3b5f5bc9da72d23d43080951f3331562cc15b0db757f53b817e378cd05d7d588.
Effective NEXT setup AP; no current network restart. Isolated tests on .57
reproduced old timeouts and verified installed A/AAAA/SVCB/HTTPS replies in
0.3–0.4 ms with no upstream forwarding. Actual phone speed remains unmeasured.

DNS test now simulates resolv.conf upstream instead of forcing a command-line
server, and requires binding isolated loopback 127.233.47.1:53. Test and
configuration were checked together. Failed wildcard candidates were not
activated; final uses address=/#/, local=/#/, no-resolv. Rollback backup
/opt/memory-lane/local-tests/release-0.2/astrohud-captive.before-trial.conf;
installed local-release-trial.py now restores DNS on rollback too.

One-scan captive-window flow is STILL unreliable: confirmed phone returns to
Camera when the AP closes. These fixes do not stop that. Normal Safari-tab
workaround has not been confirmed successful by the user. Keep the TV QR as
fallback; do not describe one-scan onboarding as completed.

## iPhone captive window closed; handoff policy fix installed — 2026-11-20

User reported the phone returned to Camera after submitting household Wi-Fi,
then the TV showed its second QR. New trial binary was verified running; this
was not an old-build problem. This confirms the captive window closed, so its
JavaScript cannot continue. A seamless one-scan captive flow is NOT validated.

Separately found a defect in our implementation: portal response CSP defaulted
connect-src to self and blocked hosted readiness polling. Earlier intercepted
browser tests omitted the real response headers. Fixed the connecting policy
to allow only self and configured service origin (no token/path/credentials in
header), and kept the connecting page on root/index reloads while Applying or
Connected. Logs showed a discovery/root reload immediately after POST 202.
The browser regression now uses real policy headers; it passed. A real HTTP
test covers response policy plus reloads, and all 19 provisioner tests passed
(4 manual previews ignored). ARM64 provisioner rebuilt successfully.

Installed ONLY corrected trial provisioner on .57, SHA256
d9a480bb11b319602713b532bfc15ed7634321a70b9858809ed64316d6eac27a.
Prior trial binary is retained as
/opt/memory-lane/local-tests/release-0.2/astrohud-provisioner.before-handoff-policy.
Viewer was not restarted; identity, Wi-Fi, updater state and signed provisioner
were verified unchanged. Hosted v0.5.7 needs no change for this correction.

User advised: after first Wi-Fi QR joins the setup network, open Safari at
http://10.42.0.1 BEFORE entering home Wi-Fi, to try the regular-browser handoff.
This is a manual workaround, not a completed seamless onboarding redesign.
Phone behavior from a normal Safari tab remains unvalidated. Do not reset the
Pi automatically; leave attended tests to the user. TV QR remains a fallback.

## Local Pi release trial installed; hosted v0.5.7 live — 2026-11-20

User explicitly authorized installation on .57 for an attended factory-reset
test and then authorized deploying the hosted changes too. Remote branch is
master (no main). Production d0f229f contains ONLY hosted handoff/household
confirmation, API version 0.5.7 and release notes; schema remains 16. Pushed
origin/master, triggering auto-deploy. It is merged back into release/0.2.x.
Live health verified status ok, version 0.5.7, schema 16. Hosted onboarding
bytes match local source; setup-origin POST/OPTIONS pass, foreign origin is
403 with no CORS access and no credentialed CORS support. Verification used
an unknown synthetic token and changed no accounts. Report:
target/hosted-onboarding-deployment.json. Brief rollout 502s recovered before
verification completed. User told both sides are ready for their reset test.

Pi 192.168.50.57 runs the built trial frame and provisioner from
/opt/memory-lane/local-tests/release-0.2, with the tested Wi-Fi guard script
from the same directory. All three units have 99-local-release-trial.conf
overrides; original signed release and older display-idle override remain.
Viewer and provisioner active; guard run returned Result=success/status 0;
wlan0 connected. Installer verified signed artifacts, identity, Wi-Fi and
updater state unchanged. Display-idle remains OBSERVE, automatic pause off.
The user will perform the reset; no reset or Wi-Fi disconnect was initiated
by the agent. Overrides survive factory reset, so new provisioning is tested.

Rollback: sudo python3
/opt/memory-lane/local-tests/release-0.2/local-release-trial.py rollback
restores PRE-TRIAL runtime (older observer), not the signed viewer directly.
Then use the display-idle helper rollback before normal signed updates.
New source helper: astrohud-frame/scripts/local-release-trial.py.
The automatic handoff still needs an attended phone test; a closing captive
window can require the backup QR. Same-household setup can download existing
hosted photos again; the new page explicitly identifies that household and
requires confirmation. Broader account security changes remain outstanding.

## Release branch consolidation and one-scan setup — 2026-11-20

User selected `release/0.2.x` for consolidated development and explicitly
abandoned Orange Pi support. Release branch combines existing Tauri work,
Raspberry Pi display-idle prototype (default Off; local Pi still Observe), and
the changes below. Production master remains ec95829; branch naming does not
change package versions. Orange Pi code must stay excluded. Keep its archive
tag only for historical reference; do not revive it as active device support.

One-scan prototype: local portal polls hosted bootstrap/handoff for at most
five minutes, omits browser credentials, and automatically opens the private
onboarding link only after a positive ready response. CORS applies solely to
that read-only endpoint and exact setup origin http://10.42.0.1. Manual Continue
and second TV QR remain fallbacks. Both hosted API/site and provisioner changes
are needed; actual captive-window survival on iPhone/Android is unvalidated.
Onboarding names the remembered household and requires explicit confirmation
before adding the device. This does not replace the broader account/session
security hardening discussed with the user; that remains future work.

User clarified the Wi-Fi overlay should be ONLY a symbol. New viewer briefly
shows a small top-right Wi-Fi icon at photo changes: green connected, amber
slashed offline, gray unknown; local link only, not internet health/strength.
No probe runs network commands. Decoded photo is retained for three seconds to
remove the symbol without another decode; setup and idle suppress the overlay.
Wi-Fi guard fix below is included. No hosted push, Pi install, R2 publication,
factory reset or Wi-Fi disruption has been performed for these changes.

Validation: 115 active Rust tests passed across frame, provisioner, API/admin;
seven manual rendering tests remain ignored in the normal suite. Separate SDL
Wi-Fi preview/removal check passed, as did four Wi-Fi guard and three passive
CEC Python checks. Synthetic browser test passed readiness gating, old API
manual fallback, failed Wi-Fi and household confirmation at phone/desktop
widths. Pi ARM64 cross-build succeeded. Logs: /tmp/astrohud-release-tests.log,
/tmp/astrohud-release-pi-build.log. Symbol previews:
/tmp/memory-lane-wifi-preview/wifi-{connected,offline,unknown}.png.
Prepared frame SHA256 4908d257d53409f050d26ed5dde7e71a6826c19e2c692e01d919b7c815db30d8;
provisioner d2fa5b44717ae54676934d47c454fc59c2270530ece872bf255ea42a0edf0029.
Orange branch retired to archive/orange-pi-abandoned-2026-11-20. Fully merged
local feature branches were removed; old display-idle worktree is detached at
e6263e3 for historical diagnostics. Remote branches remain unchanged. Next:
review/apply the prepared local Pi trial and arrange matching hosted handoff
deployment before an attended phone setup test. Do not reset the device just
to demonstrate the Wi-Fi icon or remove photos from the household library.

## Local Pi Wi-Fi failure and post-reset photos investigated — 2026-11-20

User power-cycled to recover Wi-Fi, then reported old photos in both TV and app
after factory reset/reinitialization; whether setup rejoined the same household
is unclear. Read-only SSH succeeded at the pinned reference Pi 192.168.50.57.
Previous boot's wifi-guard repeatedly exited 127 (`logger: command not found`)
before `nmcli connection up`. Local script now logs through printf to stderr;
four isolated regression checks in scripts/test-wifi-guard.py pass (missing
logger, failed reconnect, already connected, provisioning mode), plus shell
syntax and diff checks. Patch is NOT installed on the Pi or published.

Pi reset rotated device credentials at 20:23:26 MST November 20; all 13 current
cached images have post-reconnect modification timestamps starting 20:27:38.
Reset journal is gone. Evidence points to photos downloaded again after setup,
not surviving files. Same-household bootstrap claims subscribe the new frame
to the existing Family channel; legacy photos without photo_delivery_plans
remain channel-visible, unlike newer per-frame photos. This can explain the
subset, but household identity and legacy status of these specific photos have
not been confirmed. No hosted data, photo routing or reset code changed.

Live updater agent and check timer are enabled and active. Viewer remains
pinned to the display-idle prototype by 99-local-display-idle.conf, Observe
mode; the override is not equivalent to disabling the updater. No service
restart, device installation, reset, network change or update was performed.

## Session closed: passive TV standby capture — 2026-11-15

Resume display-idle work in `target/display-idle` on `feature/display-idle`.
Diagnostics and results committed locally as `e6263e3`; branch not pushed.
Root checkout remains `feature/tauri-client`; preserve its existing HANDOFF.md
changes and untracked brochure/docs assets. Orange work remains shelved.

Attended passive CEC capture succeeded: TV broadcast standby `0f:36` at
monotonic 654006.208, then replied `03` (On to Standby) until returning On
at 654039.556. HDMI stayed connected. User confirmed photos returned. Capture
finished automatically at 654071.936; no monitor needs stopping. Raw filtered
JSONL: `target/display-idle-cec-passive.jsonl` in the root checkout. Diagnostic
scripts and detailed findings are in the feature worktree's
`astrohud-frame/DISPLAY_IDLE.md`; all three Python decoder tests passed.

This is evidence of a TV-originated standby command, not a definite Standby
power reply. No new idle policy was installed. Local Pi remains in OBSERVE
mode, auto pause disabled, normal photo sync/playback running. Next session:
design a conservative, bounded policy using the standby event plus transition
reports, keep Unknown fail-open, then conduct an attended automatic-pause/wake
test and matched CPU measurements. Neither auto wake nor savings is validated.
No deployment/publishing/merge to production is authorized by this handoff.

Access: local reference Zero 2 W `192.168.50.57`, MAC `88:a2:9e:bb:9d:99`,
SSH user `astrohud-support`, pinned host key `target/display-idle-pi-known-hosts`.
Use BatchMode and StrictHostKeyChecking=yes. Prototype stays pinned by
`/etc/systemd/system/astrohud-frame.service.d/99-local-display-idle.conf`.
Before normal updates, run `astrohud-frame/scripts/local-display-idle.py rollback`
as root on the Pi (or feed script through SSH to `sudo python3 - rollback`).
Signed installed release remains 3-0.5.4; prototype binary hash is
`bd0fbbd5fdf361a564c0580a6cf7b9ccaa74ec6c87dcaf0dbb9031d7a56d0698`.
No Pi shutdown or reboot requested; leave it displaying photos.

## Local Pi running display-idle observer — 2026-11-15

Development Pi Zero 2 W at 192.168.50.57 currently runs the unpublished
feature/display-idle viewer through 99-local-display-idle.conf. Worktree:
target/display-idle, prototype commit 2e3b747. Signed release 3-0.5.4 is intact.
Observation mode only; auto pause disabled. TV off kept HDMI connected and
produced Unknown power state; TV on returned On via CEC and user confirmed
photos returned. Automatic idle/CPU savings not validated. Before using the
normal Pi updater, remove the local override using the rollback helper in
that worktree; it otherwise pins runtime to the prototype. Root target holds
pinned SSH key, local report and off/wake observation logs. Orange remains
shelved. No production API, R2 or field-device changes for this experiment.

## v0.5.6 pushed and live; desktop branch preserved — 2026-11-15

User explicitly approved pushing both branches. origin/master is ec95829;
origin/feature/tauri-client is 4d9bb5a. Auto-deploy completed: live
https://app.astrohud.com/api/health reports status ok, version 0.5.6 and schema
16. Live owner.html verified matching Forget device / Remove from list,
API capability gate and stale-poll protection. No real account was changed
as part of verification. Deployment report:
target/forget-merge/deployment/status.json; merge review:
target/forget-merge/merge-review.json. All 77 active API/admin/provisioner
tests plus production-checkout browser workflow passed before push.

Current workspace remains on feature/tauri-client, synced with master and
ready for desktop work. Orange experiment remains shelved at cc0c2de.
No tag, Pi update, R2 publication or device installation performed. Brochure
notes, marketing files and original images remain local and untouched.
Temporary clean production worktree and isolated test servers are cleaned up.
The production branch is named master; no separate main branch was created.

## Selective production merge prepared; Tauri branch retained — 2026-11-15

User requested moving the shared changes to the production branch while
retaining desktop work. The production branch is master, not main. Local
master ec95829 contains only shared portal commit 7184556 and account/API
commit ec95829, cherry-picked without the Tauri app/packaging commits or
Orange hardware. API/site is v0.5.6/schema 16; Pi binaries are not published.
Shared commits merged back into feature/tauri-client to keep future work
aligned. The master checkout is target/main-merge. Deployment push pending;
pushing origin/master triggers the previously confirmed hosted auto-deploy.

Production checkout passed 56 API library + 3 admin + 18 provisioner tests
(4 manual previews ignored). Logs target/forget-merge/production-tests.log.
Runtime preview and browser checks are being completed before requesting the
final production push approval. Brochure/case files remain separate local work.
Orange experiment remains preserved at cc0c2de; do not resume that hardware work.

## Forget reset devices implemented locally — 2026-11-15

User reports no way to forget/delete a device after factory reset. Reset
remained listed until reconnect/receipt completion; completed entries also
had no removal action. Added owner-authenticated DELETE
/api/beta/owner/resets/{reset_id} with confirm_name. Pending resets are refused;
after acceptance, Forget device releases old account pairing and hides the
entry without needing device reconnect. Completed history uses Remove from
list. Other devices and household photos remain. No physical-wipe claim is
made by account removal. Receipts remain stored privately: a lost acceptance
response can retry using receipt, and later completion is idempotent even
after reclaim by a new owner. Tests verify old-owner retries cannot delete
new-owner pairing. Normal completion still behaves as before.

Schema 16 adds device_resets.forgotten_at, with additive migration and normal
pre-upgrade snapshot; dismissed rows excluded from owner history. Existing
reset records preserved. API owner context adds reset_forget_version=1 so
new UI hides actions against older hosted API (important for desktop app).
Polling generations prevent stale requests resurrecting forgotten entries;
another owner's removal also triggers full owner context refresh.

All 56 library + 3 admin tests passed. Browser test in
astrohud-rest/scripts/test-forget-device.mjs passed pending/accepted/completed
states, cancel, delayed stale poll, old API capability, last-device removal
and 320/390/1280px layouts. Screenshot target/forget-device-preview.png uses
sample accounts only. Isolated local API at 127.0.0.1:18119, shell session
13027, uses target/forget-device-preview; production data untouched. Browser
run shell 91564 exited 0. Library test log target/forget-device-tests.log.
API and site must deploy together; older server cannot open schema 16.

API/site changes are prepared on feature/tauri-client for a selective
production merge; previously preserved brochure notes remain separate local work. Original image/brochure files untouched;
Orange hardware remains shelved on feature/orange-pi-experimental. No live
device, hosted API, master push or R2 deployment performed for this feature.

## Retained portal improvements; Orange Pi experiment paused — 2026-11-15

The tester's TV failed completely with the Orange Pi, despite repeated success
on the development monitor. The failure mode is not diagnosed. User requested
shelving hardware support; do not describe Orange Pi as ready for field use.
The board-specific work is preserved separately on feature/orange-pi-experimental.

Retained on feature/tauri-client: offline portal embeds the shared app theme,
compact layout and full-width 56px/16px SSID control with name-only labels;
manual entry and no-JS fallback remain available. DNS answers AAAA/HTTPS/SVCB
locally during setup; a real dnsmasq regression reproduced old upstream
timeouts and verified the fix. Household joining waits for a station scan
once AP is stopped; isolated command tests cover discovery and connect failures
and keep the password off argv. Fixed-label portal request/response timing
logs avoid credentials, tokens and query strings. Preview output directory is
configurable with ASTROHUD_PORTAL_PREVIEW_DIR.

Prior browser checks passed 320/375/390/430/768/1280px, long names, navigation,
manual entry, no-JS fallback and no external assets. An iPhone took 10–15s to
show its sign-in window; this delay is not resolved. Latest recorded DHCPACK
~49.11s and first HTTP probe ~50.73s were only 1.62s apart; response sends <1ms.
Do not attribute the entire perceived delay to an iOS probe timer.

No board image/driver/reset-reboot changes are retained on this branch. No
production, R2 or device deployment is part of shelving. User brochure work
and original case images remain separate local work.

## Care-community brochure drafted — 2026-11-15

Created docs/marketing/memory-lane-brochure/Memory-Lane-Brochure.pdf, a two-page
US Letter overview with the headline "Turn any TV into a family picture frame."
It uses copies of the user's aluminium enclosure images, actual Home/Send web
app screenshots with example frame names, and an explicitly labelled generated
family photograph inside a CSS TV illustration. Contact supplied by user:
sam@advanceddatamachines.com. Includes setup steps, HDMI/power/Wi-Fi/input
requirements, offline cached playback and an invitation to discuss a small
care-community pilot. No pricing, clinical claims or future app features.

Editable HTML, local assets, page previews, render.mjs and image-source/prompt
notes are in the same directory. Chrome rendering checks passed for both pages:
all images loaded and no out-of-page content. PDF verified as two Letter pages.
Original device-images files remain unchanged. Brochure work is not committed
or published; user is reviewing the first draft.

User also requested versions featuring a Black family and an Indian family.
The renderer now accepts `black indian` to export separate PDFs and page
previews using additional generated grandmother/grandchild photos; original
brochure assets stay intact. Exact variant image prompts are recorded in
docs/marketing/memory-lane-brochure/variant-image-prompts.json.
Both family versions passed the two-page Letter PDF/image/bounds checks.
User then requested a playful Big Lebowski edition: render.mjs `dude` uses
an original generated character illustration and a themed opening sentence,
with an unofficial-concept image caption. Prompt is recorded separately in
the-dude-image-prompt.json. All brochures retain the actual app/hardware
images and the user's supplied demonstration email.
User refined the Dude concept to include his child with a resemblance to
Julianne Moore. A separate `dude-family` render variant preserves the first
Dude edition and uses a reference-image edit depicting him beside his young
red-haired daughter. Its prompt and asset are saved beside the other variants.

## Ubuntu desktop preview packaged — 2026-11-15

On feature/tauri-client, packaged desktop preview 0.1.0 specifically for the
tester's Ubuntu 22.04 amd64 system. `just client-package` uses the new isolated
Dockerfile.ubuntu22 and writes a 4.3 MB shareable archive at
target/releases/Memory-Lane-Desktop-0.1.0-ubuntu22-amd64.tar.gz containing the
.deb, README.txt and SHA256SUMS. Debian package name is memory-lane; executable
is memory-lane-app. Installer includes desktop entry/icon and GTK/WebKit runtime
dependencies. Binary needs at most GLIBC_2.34; package declares libc6 >= 2.35.

Release builds default to https://app.astrohud.com; debug builds keep the
loopback API. Sample-frame controls appear only for loopback servers. No
private access links, credentials or photos are bundled. Access still clears
on exit; testers reconnect with their private link. A consumed activation link
requires a fresh recovery link. TESTING.txt explains install/use/uninstall.

All three native transport unit tests passed inside Ubuntu 22.04. The binary
extracted from the installer passed actual Tauri/WebKit automation inside
Ubuntu 22.04/Xvfb against disposable API data on port 18116: sample creation,
multipart uploads, protected gallery images, file selection/preview, owner
recovery, invited sender access/upload, role isolation and restart clearing.
Formatting-only rebuild produced an identical extracted binary. The test
container required writable XDG directories and forwarding
TAURI_WEBVIEW_AUTOMATION=true; its sandbox override was test-only. No hosted
deployment, Pi/R2 channel change, branch merge or tag was requested. User
requested committing the packaging changes on feature/tauri-client at session
close; the built tester archive remains under ignored target/releases/.
User screenshots in docs/device-images remain untouched.

## Web v0.5.5 deployed; desktop branch retained — 2026-11-15

Completed the approved split and deployment. master/origin/master and tag
v0.5.5 point to web-only commit 892f84f. Production health is v0.5.5/schema 15;
live owner.html, send.html, photo-picker.js and astrohud-theme.css match the
tested source byte-for-byte. The service recovered from a temporary 502 during
rollout. No schema migration, Pi update or R2 channel change was required.
Operational verification is saved under ignored
target/releases/web-v0.5.5/deployment-status.json.

feature/tauri-client is pushed and retained. Desktop implementation commit
310a905 follows the shared web commit; Tauri files are absent from master.
Workspace is back on feature/tauri-client for further app work. Browser tests
passed picker, slideshow and navigation scenarios; REST tests passed (52
library plus three admin tests). User screenshots under docs/device-images
remain untracked and untouched. Refresh the live webpage for the new picker;
desktop users continue with the already-built draft or just client-build/run.

## Web picker split from desktop draft — 2026-11-15

User approved publishing the picker to the website while keeping
feature/tauri-client for later desktop/mobile work. Web-only commit 892f84f
contains REST/site v0.5.5, shared photo-picker.js, owner/sender markup and
styles, asset cache hashes and a self-contained browser picker test at
astrohud-rest/scripts/test-photo-picker.mjs. No schema or Pi/R2 change is
required. Picker/slideshow/navigation browser checks and REST tests passed.
The Tauri files, just recipes and app planning/docs are saved separately on
feature/tauri-client; user screenshots under docs/device-images stay untouched.
The production master branch receives only the web commit. Website deployment
verification is the remaining rollout step at this checkpoint.

## Large photo-picker card prototyped — 2026-11-15

User observed a tester struggle to find the native file control and approved
a large tappable photo card for review in the desktop client. Shared owner/
sender forms now show Choose photos / Pick from your photo library, with a
photo icon and lavender surface. The whole card opens the existing native
picker. After selection it becomes Add more photos; previews have Remove
controls and the final button names the count/destination. Adding preserves
the queue, duplicate file picks are ignored, cancel preserves selected photos,
and successful sends restore the initial card. File validation is unchanged.

Added shared photo-picker.js and bundled it into the Tauri UI preparation.
These are local source changes on feature/tauri-client, not deployed website
changes. Native desktop was rebuilt. Browser tests passed actual chooser
activation, add/remove/cancel/deduplication/destination labels/uploads for both
roles at 320/390/1280px; native WebKit end-to-end checks also passed including
hidden file-input selection and uploads. Preview screenshots are at
/tmp/memory-lane-photo-picker.png and /tmp/memory-lane-native-photo-picker.png.
A separate local native review window is open on Send photos with
sample frames via visible driver 4449/native 4450. Keep the user's existing
live-connected desktop app open: draft sessions clear on close. User-created
screenshots under docs/device-images are unrelated and should be preserved.

## Tauri desktop draft working — 2026-11-15

User approved `feature/tauri-client` and explicitly requested desktop iteration
before any phone deployment. Created that branch from production master.
New astrohud-app is a standalone Tauri 2 workspace (v0.1.0), leaving Pi/server
build dependencies and the deployed web assets unchanged. UI preparation copies
the actual owner/sender HTML/CSS/JS, injects a native fetch bridge, converts
protected image loads to native requests/blob URLs, and preserves hosted origins
for invitation/recovery links. Current navigation, playback cards and grouped
settings are reused rather than reimplemented.

Rust transport uses a private in-memory reqwest cookie jar. It restricts API
paths/origins/methods/content types/body sizes and disables redirects; cookies
never go to JS or plaintext files. HTTPS is required except for loopback dev.
Private owner activation/recovery and sender invitation links are accepted at
the connection screen. Disconnect/server change/restart clear session access.
The visible draft banner explains access clears on close. OS secure persistence,
app-specific revocable sessions, deep-link registration, HEIC conversion,
background uploads and share extensions remain future work; no phones/SDKs,
production API changes or deployments were made.

Linux build uses a Docker bookworm image with GTK/WebKit development packages;
host runtime libraries already existed. `just client-build` prepares shared UI
and builds target/tauri-desktop/debug/memory-lane-app. `just client-run` launches
it against http://127.0.0.1:18116 by default. Existing isolated API there is still
running with disposable sample data. Desktop windows were launched for review.
See astrohud-app/README.md for connection/demo/run instructions.

Validation: native transport tests passed for origin/path guards, cookie session
isolation and refused redirects. Actual Tauri/WebKit tests passed sample-frame
creation, protected gallery images, file picker/preview/owner upload, private
owner recovery, sender invitation/upload, owner-role isolation and loss of
access after restart. Screenshot: /tmp/memory-lane-tauri-draft.png. Tests use an
isolated Xvfb display :99 and tauri-driver on 4447/native 4448, with tools kept
under target/tauri-tools and target/tauri-driver; no test dev packages installed
on the workstation. A separate visible test-driver window on 4445/native 4446
was also opened earlier; leave user-facing windows alone while testing.

Final desktop rebuild, Clippy, formatting and repeated native end-to-end checks
passed. Work is not committed, tagged, pushed or released. Production v0.5.4 remains healthy on schema 15 and
R2 channel sequence 3 (approved release completed before this branch began).

## v0.5.4 release build verified; R2 CLI setup in progress — 2026-11-14

User requested build, commit, tag and R2 deployment, then asked to configure
AWS CLI for uploads. Final ARM64 Docker build passed. All 116 workspace Rust
tests passed (six manual previews ignored), along with Clippy/format/diff checks
and final playback/navigation/storage browser checks plus native-viewer storage
integration. Private signing key was verified against the existing device trust;
the published channel was verified as v0.5.3/sequence 2 before packaging.

Signed and reverified v0.5.4/sequence 3 bundle is ready under ignored
target/releases/v0.5.4/r2-upload/storage-0.5.4/. It contains frame v0.4.0
(6,010,488 bytes), provisioner v0.1.1 (3,898,432 bytes), and stable.json.
Channel promotion copy is r2-upload/updater-test-20261114/stable.json.
Standalone binaries and SHA256SUMS are alongside r2-upload. Signing private
key remains outside the repository; upload only release artifacts.

Installed AWS CLI v2.40.3 for the user at ~/.local/share/aws-cli with binary
~/.local/bin/aws, using the official AWS installer and verified archive
signature. No R2 credentials existed in environment/home/repo profiles.
User was instructed to create an Object Read & Write R2 token restricted to
memorylane-nightly, then run `aws configure --profile memorylane-r2` privately
with R2 Access Key ID/Secret Access Key, region auto, output json. Wait for
the user to confirm setup; never request or print keys.

Remaining release steps: commit/tag requested source, upload immutable
storage-0.5.4 objects through AWS CLI, verify public bytes/signature, then
promote the channel after matching API/site v0.5.4/schema 15 is healthy.
The explicit deployment request named R2; origin/master auto-deploys the
production API/site and schema migration, so coordinate that separately.
No production or device changes have occurred for v0.5.4 yet.

## Settings group separation refined — 2026-11-14

User requested clearer distinction between groups. Slideshow, Storage,
Software update, Your devices and Owner access now have separate raised
cards with complete borders, stronger heading dividers, 24px gaps and subtle
accent colors from the existing palette. Device/reset controls now share a
labeled group container. Navigation/slideshow/storage browser checks passed
at phone and desktop widths; the interactive simulator was refreshed.
Also removed obsolete Settings-button ID spacing rules that displaced the
new navigation item; icons, labels and button heights were verified aligned.

## Slideshow consolidated into main Settings — 2026-11-14

User approved making the main gear the single entry for device settings.
Moved playback cards, timing, background and guest Display next controls into
an inline Slideshow section at the top of Settings, followed by Storage and
Software updates; device/reset and owner-access controls follow below. The
selected-frame context stays above them. Removed the Photos slideshow gear
and modal so persistent Home navigation remains usable. Photos keeps Show now,
Pin and Resume slideshow.

Added owner-scoped GET /api/beta/owner/frames/{frame_id}/playback with no-store
responses so Settings loads independently of the gallery. Separate per-frame
slideshow state and request generations discard stale load/save responses
after switching frames. Failed loads disable unknown controls and offer retry;
failed saves restore persisted choices. Settings refreshes playback capability
in the background every 15 seconds without replacing an active form edit.

Validation: all 52 REST library tests and Clippy passed, including GET auth/
household scoping. Browser playback/settings checks passed for all modes,
keyboard, failed saves, older-viewer gating, timing/background/guest saves,
per-frame defaults, a held save completing after switching frames, failed
load/retry, sticky navigation and storage at phone/desktop widths. This remains
part of local prepared v0.5.4 on feature/device-storage, not deployed.

## Persistent owner navigation included in v0.5.4 — 2026-11-14

User observed a test owner struggling to return Home and requested a persistent
top menu with buttons for other features. Added a sticky top-level navigation
outside the scrolling main content with labeled Home, Photos, Send, Invite,
and Settings destinations. Native anchors preserve Back/Forward behavior;
the active section uses aria-current and amber styling. Selected frame stays
unchanged. Settings retains the existing red update badge, now visible from
every owner section. Removed the movable/duplicated heading Home links. The
navigation is hidden before owner authorization; Add a device remains on Home.

Browser test test-owner-navigation.mjs passed at 320/390/1280px: all routes,
44px targets, no horizontal overflow, sticky/clickable Home after scrolling,
selected-frame retention, update badge and browser Back. Playback-card and
storage browser checks passed after the change. Preview:
/tmp/memory-lane-owner-navigation.png. Changes remain local on
feature/device-storage as part of the same prepared v0.5.4 release.

## Playback cards included in storage release — 2026-11-14

User confirmed the storage design and requested improving slideshow dropdowns
in this same version. Storage remains on the main Settings page, scoped to
the selected frame. Photos → Slideshow settings now uses three stacked native
radio cards for The Scenic Route, Fresh Prints and Serendipity, each with its
description visible. The selection has an amber border/tint and Selected label.
Selections save immediately; unsupported viewers keep controls disabled and
failed saves restore the persisted choice. Keyboard and accessible naming/
descriptions use native radio/fieldset semantics. Updated browser checks cover
keyboard selection, failed-save recovery, normal playback and capability
gating/rollback at 320/390/1280px; storage browser checks also passed with the
shared CSS change. Preview: /tmp/memory-lane-playback-modes.png. These UI
changes are part of prepared v0.5.4 on feature/device-storage, still local.

## Storage visibility and photo reserve implemented locally — 2026-11-14

User approved Settings storage visibility and requested protection against
filling the device plus an estimated photo count for nontechnical owners.
One-off paid storage is a future idea; do not add billing or change allowances
as part of this work. Branch feature/device-storage starts at 8a9486d.

Prepared API/site v0.5.4/schema 15 and viewer v0.4.0. Owner Storage card shows
disk used/total/available, photo bytes/count/limit and approximate additional
photos. It uses remaining cache allowance vs disk available minus 512 MiB,
whichever is smaller, divided by sampled mean photo bytes (+25%, minimum
64 KiB base). Empty samples show a hint; fewer than five samples are labeled
early estimates. Full cache allowance is normal, not a low-disk alarm. A quiet
home warning appears only for fresh low/very-low readings; older than ten
minutes or failed refreshes retain the last measurement with stale labels.

Viewer reports authenticated numeric snapshots on startup/every five minutes;
server retains only the latest report per frame. Owner/co-owner scope uses
existing authorization, no paths/photo IDs/secrets are reported. Existing
updater agent stays unchanged, so signed viewer updates can deliver this.
Schema 15 adds frame_storage with the existing pre-migration snapshot; deploy
API/site before viewer. Older viewers remain compatible; newer viewer on old
server keeps playback while reporting fails with backoff.

Photo downloads inspect available space before HTTP/download and each chunk
write, fail closed on measurement errors, and remove partial files on failure.
Existing photos remain. Fixed unchanged-manifest optimization so missing older
photos retry after a space pause when allowance has room. The reserve bounds
the viewer's writes, not unrelated OS/updater activity; no physical disk was
filled to test it.

Validation: all 115 workspace Rust tests passed (six previews ignored), plus
Clippy/format checks. Tests cover reserve exhaustion during a write, partial
cleanup/preserved photos, retrying missing history, estimates, auth/scoping,
latest-only snapshots and schema-14 backup migration. Browser checks passed
for estimates/empty and early samples/cache full/low and critical/stale/error/
frame changes and 320/390/1280px layouts. Real native viewer passed against
isolated API after download/restart, reporting actual filesystem/photo stats.
Final ARM64 Docker build passed. Preview: /tmp/memory-lane-storage-preview.png.
Local QA API runs at 127.0.0.1:18116 with disposable data only.

Remaining: review local changes, commit, coordinate API/site deployment and
signed R2 release, then validate the local Pi. No commit/push/tag/deployment,
R2 publication or physical-device changes were performed for this feature.

## Field rollout confirmed; v0.5.3 release — 2026-11-14

User reports the field unit "updated and deployed with little drama" after
the fresh v0.5.2 card was supplied for the v0.5.3 update test. Record this as
user-confirmed field setup/update success, not an independent hardware audit.
Production API/site, signed R2 publication and field deployment are complete.
Individual playback-mode behavior on physical hardware remains to be checked;
power-interruption and broader resilience testing remain outstanding.
Release notes and rollout guide now reflect the deployed release. Historical
pending/uncommitted notes below describe earlier stages and are superseded.

## Fresh field-replacement update-test card — 2026-11-14

User requested a new SD card, then explicitly chose fresh setup/pairing and
factory applications v0.5.2 so the v0.5.3 update can be exercised again.
Prepared target/astrohud-images/memory-lane-v0.5.2-update-test-20261114.img
(1,291,845,632 bytes), using the clean cached Debian OS, original R2 v0.5.2
viewer/provisioner (known hashes verified), updater v0.1.1 and the same nightly
channel/public verification key as the local Pi. It contains no device/Wi-Fi/
photo state or private signing key. Factory updater status initially identifies
the original application release as baseline.

Image tooling now supports explicit ASTROHUD_UPDATER_CONFIG and
ASTROHUD_APP_BIN_DIR. A first-boot unit initializes the managed baseline after
update recovery and before apps/control services; without a config it skips.
Image sealing, checksum, unit ordering and all 13 image-tooling tests passed.
The initial v0.5.3 image was superseded by the user's v0.5.2 choice; do not use
it for this test. Selected fresh artifact hash inputs are recorded in its
/etc/astrohud/image-release file. Source changes are local and uncommitted.

Confirmed removable target /dev/sde, 15,931,539,456 bytes (14.8 GiB), USB reader
serial 201601060924. Unmounted BOOT through udisksctl. Flash completed in exec
session 37133: 1,291,845,632 bytes written, full `cmp` readback passed, exit 0.
Host sudo requires interactive authentication; approved Docker device access
was used for the write. `udisksctl power-off -b /dev/sde` then succeeded; the
card is safe to remove. No field device was accessed or migrated. Next:
fresh phone setup/pairing and owner-page v0.5.3 update test.

## Production API/site deployed — 2026-11-14

User explicitly approved merge and deployment through feature commit f07423a.
Merged as 8551718 on master and pushed origin/master. Production health now
reports v0.5.3, schema 14 and playback_modes_version 1. Live owner.html,
display.js, device-updates.js and astrohud-theme.css match repository bytes.

Only the local Pi updater agent was moved to https://app.astrohud.com. Its
temporary API override is backed up under
/var/backups/memory-lane/updater-20261114.cm7l2b/90-integration-test.conf.
Viewer/provisioner/agent are active with zero automatic restarts, protected
identity/pairing/Wi-Fi fingerprints match, and recent agent logs show no
connection errors. The temporary updater-test Cloudflare tunnel was stopped.
Actual current application release is 1-0.5.2, previous baseline; the user
apparently installed again after the earlier recorded rollback. No install
was initiated during this deployment and the field Pi was not accessed.

User uploaded the v0.5.3 R2 bundle and promoted the channel. All four public
objects (two binaries and both manifest copies) match the prepared artifacts
byte-for-byte. The local Pi's `memory-lane check` verified signed metadata and
reports v0.5.3 available; all three services remain active. No installation
was initiated: current remains 1-0.5.2, previous baseline.

Remaining: user installs v0.5.3 through the normal owner Settings page, then
verify installation health and physical playback-mode switching on the local
Pi. The other field Pi remains under the user's manual update control.
Earlier entries below are historical; their production-pending statements
are superseded by this entry. Release/burndown documentation can be finalized
alongside the remaining R2 and physical rollout results.

## Playback styles implemented locally — 2026-11-14

User finalized **The Scenic Route** (fair shuffled rounds; default), **Fresh
Prints** (newest first), and **Serendipity** (independent random picks including
repeats), and explicitly requested implementation as the first real feature
update. User will manually update the one other field-test Pi after this dev
cycle. Hosted service auto-deploys from `origin/master` (user confirmed).

Local API/site target v0.5.3/schema 14; viewer is v0.3.0, updater v0.1.1 and
provisioner v0.1.1. New owner selector is in Photos → Slideshow settings. Mode
updates are household/frame scoped, validated, preserve pins/timing, and require
an authenticated viewer capability header before the control enables. Older
viewers keep their existing behavior. Rolling back a viewer clears capability
on its next manifest request while retaining the preference. Manifest catalogue
limit is 2,000 instead of 200; local cache limits still apply. Native/browser
selection state survives restart, sync and membership changes; prefetch doesn't
consume draws, and consumed owner commands aren't replayed after restart.

All 106 Rust tests passed (six preview tests ignored), plus browser order
properties and real browser selector/playback tests covering legacy gating,
random repeats, pins, restart progress and mobile layouts. Clippy/format checks
passed. Final ARM64 build and passive --version/--help loader checks on the
local Pi passed; its running viewer remains unchanged. Standalone release
files, SHA256SUMS, signed v0.5.3/sequence-2 R2 bundle and channel promotion copy
are prepared under ignored `target/releases/v0.5.3/`. See
`astrohud-frame/PLAYBACK_RELEASE.md` for the concrete rollout and field-Pi guide.
Production is still v0.5.1/schema 12; no merge/push/deployment has occurred.
The field Pi will be manually updated by the user; do not access it directly.
A parallel-test fork/exec lock inheritance race was fixed by serializing the
updater umask helper against the immediate lock-release assertion (test-only).
Production API/site must deploy together before enabling real-device mode
selection. Once ready, move only the local updater agent's temporary test API
override to the live API; preserve its viewer/provisioner production config.

## Normal updater cycle validated — 2026-11-14

The user successfully exercised both Install update and Restore previous version
from the isolated owner webpage. Explicit rollback is confirmed `succeeded`: Pi
current release is `baseline`, previous is `1-0.5.2`, and updater is v0.1.1. Both
application services are active/running with zero automatic restarts. Baseline
binary hashes and protected identity/pairing/Wi-Fi fingerprints are unchanged.
The earlier failed-startup automatic rollback was also visually confirmed.
Normal install/explicit rollback and a real failed-startup rollback have passed;
physical power-interruption and broader resilience tests remain unperformed.
Production API/site remain v0.5.1/schema 12; test agent still uses the isolated
API/tunnel for the planned first feature update. Do not treat this as a
production rollout or rotate signing trust implicitly.

Next task requested by user: examine slideshow recent-photo bias and offer
playback options as the first real feature update through this updater. Initial
source review: device manifests order eligible photos newest-first and cap at
200 (`astrohud-rest/src/api.rs`); native cache favors newer entries when full
(`cache.rs`); native and browser players cycle sequentially. Native sync
preserves the current photo by ID, so it does not reset to newest on every sync.
Startup begins at the first eligible photo. Review participation/retention as
well as ordering before promising fair coverage of the entire library. Playback
mode choices and their defaults have not yet been finalized or implemented.

## Successful owner-requested install — 2026-11-14

The user retried Install update with updater v0.1.1 and reported it worked.
The isolated API records `succeeded`, installed release `1-0.5.2`, previous
release `baseline`. Both application services are active/running with zero
automatic restarts. Release directory mode is 0755; the `astrohud` account
can execute the viewer. Installed binary hashes match the signed known-good
R2 payload; identity, pairing and Wi-Fi fingerprints are unchanged. The
updater reports Up to date. Explicit owner-requested Restore previous version
still needs to be tested before moving to slideshow changes. The earlier
automatic failed-startup rollback was already confirmed visually by the user.

## First hardware install attempt and fix — 2026-11-14

The user initiated `0.5.2` installation from the isolated test webpage. The
viewer failed to execute because the agent's `UMask=0077` made the staged
release directory mode 0700. Automatic rollback restored `baseline`; both
services are active/running with zero automatic restarts, protected-state
checksums match, and the user confirmed photos returned. No factory reset or
credential rotation was used. This exercised a real failed-startup rollback.

Updater v0.1.1 now explicitly uses 0755 for public release/store/baseline
directories, including previously staged verified releases. A new regression
test runs init/staging/reuse in a dedicated child with umask 077; all 13 updater
tests and Clippy passed, followed by the ARM64 build. The patched updater was
installed manually, restarting only its agent; viewer/provisioner PIDs and
protected state remained unchanged. v0.1.0 is backed up alongside the existing
app backups. A fresh owner-initiated install and explicit rollback still need
to pass before this is considered validated. No R2 re-upload is necessary.

The user requested slideshow algorithm/fairness review **after the updater**.
They observe a recent-photo bias and want playback options; agreed this can be
the first real feature payload through the updater. Do not start that feature
until updater install/rollback testing is complete.

## Pi/R2 test preparation — 2026-11-14

Updater implementation committed as `7fd9086` on `feature/app-updater`; do not
include the unrelated deletion of `futo-lookback.jpg` in updater commits.
The user selected R2 bucket `memorylane-nightly` and provided its public
`r2.dev` URL. All 12 updater tests passed natively on the Pi using isolated
temporary directories and simulated services. Read-only smoke checks passed;
installed application hashes still match the November 8 release and services
have zero restarts. Updater bootstrap is now installed; actual application switching still awaits
the user's phone test. Originals were adopted as `baseline` without restarting
the viewer/provisioner. Protected identity, pairing and Wi-Fi file checksums
were preserved. Backups: `/var/backups/memory-lane/updater-20261114.cm7l2b/`.

A signed known-good test bundle and public config are prepared under ignored
`target/releases/pi-update-test-20261114/`; only the three files inside
`upload/updater-test-20261114` go to R2. A disposable test signing key remains
outside the repository in a private `/tmp` directory. R2 CLI credentials are
not configured. The user uploaded the bundle into `updater-test-20261114/`. Its public manifest
and both binaries were verified; the Pi successfully checked the signed release
and reported test version `0.5.2` available. See `astrohud-updater/R2_TEST.md`.

The isolated schema-13 test API is at loopback port 18114, with data in private
`/tmp/memory-lane-updater-integration-20261114`. A temporary Cloudflare tunnel
is running in container `memory-lane-updater-test-tunnel-20261114`. Its address
is recorded in ignored `test-plan.json` beside the bundle. The synthetic
household authorizes the Pi by its credential hash; actual device secrets and
production pairing are unchanged. The test owner token is stored privately in
that temporary data directory, never in repository documentation. Bootstrap
kit staged at `/tmp/memory-lane-updater-test-20261114/bootstrap` on the Pi.
The agent-only test API override is installed and the agent is reporting every
15 seconds to the isolated API. Viewer/provisioner retain their production API
configuration. Initial test authentication failed because the credential file
has a trailing newline; the synthetic enrollment hash was corrected to hash the
trimmed value without copying/changing the actual secret. Future fixtures must
hash the same trimmed credential used by the HTTP client.
Production remains v0.5.1/schema 12; no production deployment was performed.

## Current development — 2026-11-09

The application updater (`D-106`) is now implemented locally, not published or
installed. This section supersedes the next-feature/deferred notes below.
The device is a **headless appliance**: owners initiate checks/installation/
rollback through **webpage Settings → Software update**, eventually the phone
app. The user chose automatic checks and **manual installation**. The rustup-
style CLI is for operator servicing, never the primary household interface.

The new `astrohud-updater` workspace package builds `/usr/local/bin/memory-lane`.
A separate root agent receives authenticated owner requests, while a timer
checks signed release metadata automatically. Installation verifies exact owner-
confirmed versions, signatures, expiry, sequence, hashes, sizes and ARM64 ELF;
switches one shared release pointer after stopping viewer/provisioner; checks
both processes for 15 seconds; and commits or restores the previous release.
An on-disk journal and required boot recovery unit handle interrupted switches.
The baseline/current/previous binaries are retained. App data and Wi-Fi remain
outside the release store. The updater/agent and OS stay outside app updates.

Backend and website changes are local too. Server **schema 13** adds durable
per-frame update state and uses the existing pre-migration snapshot guard.
Production is still v0.5.1/schema 12. Deploy website and API together; old
schema-12 server code cannot open schema 13. Requests are household/device-
scoped, deduplicated and expire after 30 minutes; reset/update requests exclude
one another. The new agent must be bootstrapped onto an existing appliance
before remote controls become available. No production trust key was generated.

Validated locally: 98 Rust tests passed (six preview tests ignored), 15 Python
checks, formatting/Clippy with the existing test-only lint exception, ARM64
viewer/provisioner/updater build, and browser update-flow checks at
320/390/1280px with 44px touch targets. Tests use synthetic local devices.

See `astrohud-updater/README.md` for the diagram, API, signing/publication,
bootstrap and validation commands. Physical signed-update, offline rollback,
failed update and power-interruption tests remain required before distribution.
Do not imply that local tests prove physical appliance behavior.

## Resume here — 2026-11-08

The user is ending the session after a successful release. Backend and Pi
updates are complete; nothing remains pending for this deployment. The next
planned feature is the application updater, deferred until later this week.
Stay focused on **Pi Zero 2 W**. Do not restart the original Zero/Zero W effort.

Repository: `/home/sbeskur/repos/adm/astro-hud`, branch `master`.
Release commit: `eaaf6a7`, annotated tag `v0.5.1`.
GitHub: https://github.com/sjbeskur/astrohud/releases/tag/v0.5.1
Hosted application: https://app.astrohud.com

This section is authoritative. The August notes below are retained only as
historical research; their release versions, LAN server, device identity,
Chromium setup, permissions, and deletion/reset instructions are obsolete.

## Published and installed state

| Component | Version | Last verification |
| --- | --- | --- |
| Hosted REST API and website | v0.5.1 | Live health, schema 12, all 14 checked website assets match local release; protected endpoints reject anonymous access. |
| Pi native viewer | v0.2.2 | Installed and running from the expected binary; active, zero automatic restarts. |
| Pi Wi-Fi provisioner | v0.1.1 | Installed and running from the expected binary; active, zero automatic restarts. |
| Database | Schema 12 | Unchanged from v0.5.0. Older schema-11 code cannot open it. |

The live legacy unauthenticated channel/upload/manifest/media/WebSocket routes
are now disabled. `ASTROHUD_ENABLE_DEMO=true` opts in only for an isolated local
transport demonstration. Owner, invited-sender, paired-device, and authenticated
local simulator flows do not need that flag.

The release includes versioned ARM64 binaries and `SHA256SUMS` on GitHub.
Local copies are in ignored `target/releases/v0.5.1/`. No new SD image was
built or flashed for this deployment. Existing image files can contain older
applications; rebuild/repack before calling them v0.5.1 images.

## Pi access, verification, and rollback

- Target: Pi Zero 2 W, ARM64, `192.168.50.57`.
- SSH account: `astrohud-support`, key authentication and passwordless sudo.
- Previously verified/trusted ED25519 fingerprint:
  `SHA256:p0Fts2kLqA/jmMlqrWyFUYW/lDrM5w315yzQ7B80Vno`.
- Session host-key file: `/tmp/memory-lane-pi-57-host-key` (temporary; it may
  disappear after workstation shutdown). If reconstructing it, compare the
  scanned fingerprint to the trusted value rather than bypassing host checks.
- Viewer: `/usr/local/bin/astrohud-frame`, `astrohud-frame.service`.
- Provisioner: `/usr/local/sbin/astrohud-provisioner`,
  `astrohud-provisioner.service`.
- Cache and saved manifest: `/var/lib/astrohud`.
- Runtime identity: `/etc/astrohud/device.json`; device credential:
  `/var/lib/astrohud/device-credential`. Never print or commit their contents.
- Deployment preserved identity and pairing credential, verified by hashes.
  Wi-Fi settings and cached photos were not wiped. No reboot was required.
- Both services were still healthy on the final follow-up check; the cached
  manifest was being refreshed. No destructive reset/onboarding test was run.
- Previous application binaries are backed up on the Pi in
  `/var/backups/memory-lane/v0.5.1.7ijb40/` as `astrohud-frame` and
  `astrohud-provisioner`. The installer had automatic rollback on failure;
  deployment succeeded. Restoring those binaries and restarting both services
  provides an application rollback, not a database or OS restore.

Installed binary SHA-256 values (also checked against `/proc/<pid>/exe`):

```text
a3c2026385998176d9bf5cede1dcaf27668349223cd190c44b9aeca189fc8b32  astrohud-frame v0.2.2
111a3bec6021ac6bf8dc712b8a9867ee5e0b6efd9d62aa06978fbff843edd396  astrohud-provisioner v0.1.1
```

On this minimal image, do not assume tools such as `hostname`, `curl`, or
Python are installed. Build software on the workstation, not on the Pi.

## Work completed this session

The unpublished original Zero/Zero W source changes, recipes, build helpers,
configuration, and isolated image/cache artifacts were rolled back. The test
board was actually a non-wireless Pi Zero v1.3; its lack of a Wi-Fi interface
was hardware, not a provisioning regression. Existing Zero 2 W build paths,
images, and caches were retained. Do not reintroduce ARMv6 support unless the
user explicitly reopens that work.

The release review produced and published these fixes:

- Gate the old unauthenticated demo routes behind an explicit environment flag.
- Validate JPEG/PNG decoding rather than storing undecodable originals. Bound
  upload bodies/metadata, source dimensions, decoded size and allocations;
  impose a 60-second multipart-read timeout. Process one image at a time off
  the HTTP worker; the blocking decoder retains its permit if a client leaves.
- Normalize every new upload from pixels, even small photos, applying
  orientation and removing embedded metadata. Maximum long edge is 1,600px;
  opaque images use JPEG quality 87, alpha images retain PNG. Opt-in location
  coordinates are rounded to one decimal before storage. Historical files and
  coordinates were not rewritten.
- Show useful 4xx upload errors while retaining failed selections for retry.
  Simultaneous upload attempts can receive 429; successful items are not resent.
- Use pre-migration database snapshots in the admin CLI as well as server
  startup. Help/invalid commands do not create or migrate a database.
- Keep native private media credentials on the configured server origin.
- Display preparing/unavailable setup feedback when provisioning cannot start.
- Remove the unused `infer` dependency and refresh outdated documentation.

Full findings, limitations, and remaining priorities: [RELEASE_REVIEW.md](RELEASE_REVIEW.md).

## Established product behavior to preserve

- Product name is **Memory Lane**; copyright links to Advanced Data Machines,
  LLC d/b/a Santamatronic Labs and the Advanced Data Machines website.
- Compact mobile layout with clear frame context, Home house button and
  upper-right settings gear; warm colors, no decorative vertical bars.
- Multi-frame household context and explicit upload destinations: this frame,
  chosen frames, or all currently allowed frames.
- Owners and invited senders can upload multiple photos; failed items can be
  retried without resending known successes.
- Sender history is scoped to the stable invitation identity and survives
  link replacement and frame-local removal. Owners see photos grouped by sender.
- Guests may delete their own photos globally. Guest Display next is off by
  default, owner-enabled per frame, own photos only, with a shared ten-minute
  cooldown and no interruption of owner pins.
- Co-owners have full household owner access. Promotion issues a new owner
  link; the original sender link remains restricted, history stays intact,
  and the invitation list shows one row. Full owner access is trusted admin
  access, including recovery and managing grants.
- Owner recovery links restore access after browser cookies are cleared;
  recovery secrets are private. Physical GPIO reset remains unimplemented;
  owner-requested factory reset is implemented.
- Native centered cover-crop background with blur and slight darkening,
  complete foreground image, offline cache, photo pinning and display timing,
  next-image preparation, and hidden fullscreen cursor.
- Phone onboarding is a Wi-Fi / Password / Finish wizard at `10.42.0.1` while
  connected to the setup AP; household claiming then uses the hosted service.

## Next session

1. **Application updater (`D-106`), later this week.** Design signed release
   metadata, atomic installation of viewer/provisioner, health checks and
   rollback, and visible installed version/status. Preserve identity, pairing,
   Wi-Fi and photos; keep OS upgrades separate. It is planned, not implemented.
2. **Backend capacity and recovery.** Measure hosted disk usage/capacity;
   add household quotas, disk-pressure rejection, enrollment/upload rate
   controls, and an operator usage view. Establish independent database AND
   media backups and prove a coordinated restore. Migration snapshots alone
   are not sufficient.
3. **Cleanup/privacy.** Define history retention and owner-wide deletion;
   add periodic failed-deletion retries and cautious orphan reconciliation.
   Audit historical images/coordinates before deciding on a backed-up scrub.
4. **Release automation/hardware validation.** Bring browser regression checks
   into the repo and CI. Before distributing a rebuilt image, repeat physical
   Zero 2 W first boot, iPhone/Android onboarding, offline playback, factory
   reset, root expansion and memory-pressure tests. Review production support
   SSH access policy. No full dependency advisory audit was run this session.

Other deferred items remain in [PRODUCT_BURNDOWN.md](PRODUCT_BURNDOWN.md).
Mobile apps: Tauri is a proposed path; iOS needs a Mac or hosted macOS builds,
which the user does not currently have. Do not begin native-app work implicitly.

## Checks and build workflow

Passed for the release: 82 Rust tests (six manual-preview tests ignored),
12 image-helper Python tests, formatting, Clippy (allowing the existing
host-toolchain test-only `chunks_exact_to_as_chunks` suggestion), Chrome
regressions for upload/retry, permissions, invitations/promotion, guest
controls, mobile layouts and automated accessibility, plus the ARM64 build.

```sh
just                         # List existing Zero 2 W recipes
just image-check             # Rust and image-helper checks
just pi-build                # Viewer and provisioner ARM64 binaries
just image-repack target/astrohud-images/memory-lane-v0.5.1.img
# Repack reuses the cached OS; use image-build after OS/package changes.
```

These commands are reference instructions, not pending or running work.
Do not flash an SD card without identifying and authorizing the actual target.

Useful temporary session scripts (may disappear on shutdown):
`/tmp/compact-invitations-qa.mjs`, `/tmp/sender-batch-qa.mjs`,
`/tmp/owner-promotion-qa.mjs`, `/tmp/verify-memory-lane-v0.5.1-release.py`,
`/tmp/memory-lane-install-v0.5.1.sh`.
Browser QA uses Puppeteer from the sibling `adm-website` dependencies and a
local test server at port 18087 with synthetic data; it does not require
production owner links. Local preview processes are not production services
and need not survive workstation shutdown.

The release was pushed to `origin/master` and `v0.5.1`, and published on GitHub.
The working tree was clean after deployment; the end-of-session handoff/backlog
edits are documentation-only and are not another application deployment.

---

## Historical archive — superseded August notes

Everything below predates the current hosted product and clean ARM64 image.
Preserved for investigation only. In particular, do not execute its direct
SQL/media deletion commands, restore the old Chromium kiosk, or treat its
LAN-only authentication and reset statements as current instructions.

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
