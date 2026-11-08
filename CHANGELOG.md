# Release notes

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
