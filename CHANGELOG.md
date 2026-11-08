# Release notes

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
