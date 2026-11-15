# Release notes

## v0.5.5 — 2026-11-15

- Owner and invited-sender uploads start with a large Choose photos card instead of the native file-input control.
- Selected photos stay visible in previews with Add more photos and Remove controls. Cancelling keeps the selection; picking the same file again does not duplicate it.
- Send buttons show the valid photo count and destination. Empty selections cannot be submitted.
- Web-only release; schema 15 and Pi software remain unchanged. Tauri desktop work continues separately on feature/tauri-client.

## v0.5.4 — 2026-11-14

- Slideshow controls now live directly in the main Settings page, followed by Storage and Software updates. Photos retains Show now, Pin and Resume slideshow; the separate slideshow gear/dialog is removed.
- Owner navigation stays visible while scrolling, with labeled Home, Photos, Send, Invite and Settings links, the current section highlighted, and software-update badges available from every section.
- Slideshow playback styles use three visible selection cards with descriptions, an orange selected state, keyboard support, and immediate saving instead of a dropdown.
- Owner Settings gains a per-device Storage card with available disk space, downloaded photo count, photo storage usage/allowance, and an approximate number of additional photos that fit.
- Estimates use actual stored image sizes with a 25% margin and respect both the configured photo allowance and a 512 MiB device reserve. Empty collections wait for a sample; old readings retain their timestamp and do not produce current home-page warnings.
- Viewer v0.4.0 reports storage on startup and every five minutes. Photo downloads preserve the reserve, remove incomplete files on failure, and retry missing photos when space returns while existing photos keep playing.
- Additive schema 15 stores only the latest authenticated storage snapshot per frame. API/site deployment and physical Pi validation are pending; existing updater agents need no upgrade.

## v0.5.3 — 2026-11-14

- Photos → Slideshow settings gains The Scenic Route (default fair shuffle), Fresh Prints (newest first), and Serendipity (independent random picks).
- Pi viewer v0.3.0 and the browser simulator retain Scenic progress across restarts, preserve progress across syncs, and keep pins/guest requests in priority. Playback catalogues now include up to 2,000 eligible photos.
- Additive schema 14 stores per-frame playback style and viewer capability. Older viewers keep working; new controls enable only when the viewer advertises support. API and website deploy together with a pre-migration snapshot.

- Updater v0.1.1 explicitly sets public application directory permissions so the unprivileged viewer can start releases staged by the agent with `UMask=0077`; retries also repair verified release directories.

- Headless appliance update controls in owner Settings, backed by scoped, durable requests and a device agent; checks are automatic and installs require owner confirmation.
- Signed ARM64 application releases, atomic viewer/provisioner switching, process health checks, rollback and boot recovery; operator CLI and release packaging/bootstrap tooling.
- Additive schema 13 stores per-frame update reports and requests, using the existing pre-upgrade snapshot. API/site v0.5.3 are deployed with schema 14; install/rollback and failed-startup rollback passed on the reference Pi.
- Fresh SD images can include an explicitly configured update channel and verification key, initialize the updater on first boot, and select older factory binaries for upgrade tests. The user confirmed the fresh v0.5.2 field unit updated and deployed successfully.

## v0.5.0 — 2026-11-08

- Promote an existing photo sender to co-owner using a new owner link, preserving sender access and upload history and showing one invitation row.
- Schema 12 records the promotion relationship; upgrading creates a snapshot and rollback requires schema-12 support.
- Pi viewer v0.2.1 hides the mouse cursor in fullscreen mode while retaining it in windowed desktop mode.

## v0.4.0 — 2026-11-08

- Center-cropped, more blurred and slightly darker photo backgrounds.
- Invited senders can delete their own uploads from all receiving frames.
- Owner-enabled guest Display next, limited to own photos and a shared per-frame 10-minute cooldown, with owner pin priority.
- Full-access co-owner invitations with private, replaceable and revocable links.
- Compact invitation rows and create/copy/share/replace/disable icons, preserving the existing colors and mobile touch targets.
- Pi viewer v0.2.0 supports queued Display next requests and centered photo backgrounds.
- Schema 11 adds durable media cleanup, guest request state, and co-owner invitation records. Upgrading creates a database snapshot; rollback requires a schema-11-compatible server.

## v0.3.0 — 2026-11-08

- Compact mobile layouts with clear frame context, invitations, sending, and photo controls.
- Multi-photo uploads for owners and invited senders, with previews, progress, and retries for failed photos.
- Delivery to one frame, selected frames, or all currently eligible frames.
- Private invitation photo history and owner galleries grouped and filtered by sender.
- Linked copyright attribution to Advanced Data Machines, LLC d/b/a Santamatronic Labs.
- Server version in `/api/health` for deployment verification.

This release uses database schema 10, including the existing pre-upgrade
snapshot safeguard. Existing Pi manifest and media APIs remain compatible;
no Pi update is required for these website changes. See the README for
schema-compatible rollback instructions.
