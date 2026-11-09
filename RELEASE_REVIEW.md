# Pi Zero 2 W release review

Review date: 2026-11-08. Published baseline: v0.5.0 (`110d4bd`), schema 12.
The fixes below are included in release v0.5.1, with viewer v0.2.2 and
provisioner v0.1.1. This release does not change the database schema.

## Hardware and rollback

The unpublished original Pi Zero/Zero W experiment has been removed, including
its separate recipes, build helpers, configuration, and isolated image/cache
artifacts. The supported target remains the **Pi Zero 2 W**, using the existing
ARM64 build and image recipes. Existing Zero 2 W image artifacts were retained.
No card was flashed during this review. The follow-up device deployment uses
binary updates on the existing Pi Zero 2 W, preserving its setup and photos.

## Outstanding findings

| Priority | Finding and consequence | Recommended next step |
| --- | --- | --- |
| P0 before broader rollout | No household storage quota or disk reserve. New uploads accumulate on the server's persistent disk; photos removed from one frame can remain in sender history. Bounding individual uploads does not bound total storage. | Add household byte/photo limits, disk-pressure rejection, and an operator usage report (`C-110`). Set limits from measured hosted capacity. |
| P0 before broader rollout | Device enrollment and initial household creation are public self-service operations. Disabling the old demo API does not stop someone creating identities and consuming storage through the normal onboarding API. | Add enrollment and sustained upload rate/abuse controls alongside quotas (`C-110`). |
| P0 operational | Migration snapshots cover the database, live on the same disk, and have no retention policy. They are not an independent backup of photos. A database rollback cannot restore already-deleted media. | Back up the database and media to independent storage; test a coordinated restore and document retention (`C-108`). |
| P1 | A process crash between writing a photo and committing its database row can leave an orphan file. Queued failed deletions retry at startup or another deletion, rather than on a periodic schedule. | Add a periodic deletion retry and conservative, age-delayed orphan reconciliation. Define owner-wide deletion and history retention before automatically removing retained photos (`C-106`). |
| P1 privacy | Older small uploads may still contain original EXIF/GPS metadata, and older opt-in coordinates may retain more precision. The new normalization only affects future uploads. | Audit historical media and decide on a backed-up, explicit sanitization migration before wider sharing. |
| P1 release process | Browser regression scripts used for this review live outside the repository; there is no checked-in CI workflow to run the release checks. | Move the useful browser cases into the repo and automate Rust, image-helper, and browser checks. |
| P1 before wider distribution | The image includes the supplied operator SSH public key, with a support account and passwordless sudo. This is documented beta support access and survives as an OS maintenance policy. | Decide how production owners enable/revoke support access; make the image's support policy explicit. |
| P1 maintenance | There is no application updater yet. Deploying a web change does not update native playback, setup, or recovery behavior on existing frames. | Keep `D-106` on the release roadmap; record installed software versions and a supported update/rollback procedure. |

Actual hosted disk usage and capacity were not measured in this review.

## Release fixes

- **Legacy public demo API is opt-in.** The unauthenticated channel, photo,
  manifest, media, and WebSocket routes were enabled on every server. They now
  require `ASTROHUD_ENABLE_DEMO=true`. Normal owner/sender/device routes and the
  authenticated local simulator continue to work without it. Legacy demo
  clients need that flag on an isolated development server.
- **Uploads are validated and bounded.** Reject corrupt/non-JPEG/PNG data,
  excessive image dimensions and decoded sizes, duplicate photo fields, and
  excessive multipart metadata. Cap multipart reads at 60 seconds and process
  one upload at a time per server process, off the HTTP worker. A blocking
  decoder retains its processing permit if the client disconnects. Concurrent
  attempts receive 429; the web interface shows the reason and retains failed
  selections for retry. Decoder limits reduce exposure but do not guarantee a
  fixed total process memory footprint or replace storage/rate quotas.
- **Newly shared files lose embedded metadata.** Always apply orientation and
  encode from decoded pixels, including small photos. Limit the long edge to
  1,600 pixels; use JPEG quality 87 for opaque images and preserve alpha in
  PNG. Only explicitly requested location coordinates are saved, rounded to
  one decimal place. Historical files are not rewritten.
- **Admin commands respect migration backups.** The CLI now uses the same
  pre-upgrade snapshot guard as server startup. Help and invalid commands no
  longer create or migrate a database.
- **Native media requests stay on the configured server.** The Pi rejects
  cross-origin or credential-bearing media URLs before attaching its device
  credential. Current relative API media URLs remain compatible.
- **Setup failures get visible feedback.** First-boot setup shows a preparing
  screen before starting the AP. A provisioning failure shows setup unavailable
  and automatic retry information instead of leaving a misleading empty-photo
  screen. Existing systemd restart behavior remains in effect.
- Removed the unused `infer` dependency and updated stale README descriptions
  of the application, image handling, and blurred backgrounds.

## Verification

- Rust server, admin, viewer, and provisioner tests, including new regressions
  for upload rejection without persistence, metadata removal, migration
  snapshots, demo-route gating, and media URL origin checks.
- Formatting and Clippy; the existing test-only
  `chunks_exact_to_as_chunks` suggestion is allowed for this host toolchain.
- Existing boot configuration, image artifact, and root expansion Python tests.
- ARM64 container build of both native Pi applications; original Zero 2 W
  `just` build/repack recipes remain intact.
- Chrome end-to-end checks for compact invitations, co-owner permissions and
  promotion, multi-file sending and partial retries, private photo history,
  guest display/delete controls, pin priority, responsive layouts, and
  automated accessibility checks at mobile and desktop sizes.

Follow up on hosted disk capacity and independent backup coverage. Before
distributing a rebuilt image: smoke-test first boot, iPhone
and Android onboarding, normal/offline playback, and factory reset on a physical
Pi Zero 2 W. That hardware cycle has not been repeated for these review fixes.
A full dependency advisory audit was not performed.

The application updater (`D-106`) is planned for later this week. It is not
part of this release; this deployment updates the native binaries directly.
