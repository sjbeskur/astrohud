# Playback styles — prepared release v0.5.3

This release pairs the website/API v0.5.3 (schema 14) with Pi viewer v0.3.0.
The provisioner remains v0.1.1 and updater remains v0.1.1. It is prepared for
review and has not yet been deployed to production.

Owners open **Photos → Slideshow settings (gear) → Playback style**:

| Setting | Behavior |
| --- | --- |
| The Scenic Route | Default on updated frames. Shuffled rounds of downloaded photos, each once before repeats. |
| Fresh Prints | Cycle from newest uploads to oldest; upload order breaks timestamp ties. |
| Serendipity | An independent uniform random pick each interval; immediate repeats are allowed. |

Native progress is recorded atomically in `playback-order.json` inside the
existing cache. Browser viewers persist progress per frame in localStorage
when available. Network refreshes do not start a new round; arrivals join the
unfinished round and removed photos leave it. Restart resumes unfinished
Scenic progress. Prefetch peeks at the next candidate without consuming it.
Consumed owner commands are remembered so a stale Show now is not replayed
after restart. Pins and guest Display next still override normal selection.

The API catalogue now includes up to 2,000 eligible photos rather than 200.
Local storage capacity still bounds the downloaded collection (1 GiB by
default), favoring newer files when full. Scenic fairness is over currently
downloaded/available photos; this is not unbounded server-library pagination.
The native 2 MiB manifest guard and image/cache limits remain in place.

## Compatibility and deployment order

1. Deploy the matching API and website together. Production is currently schema
   12; startup takes a pre-migration snapshot before creating the additive
   schema-13 update table and schema-14 playback-style table.
2. Existing older viewers continue their existing playback. New controls stay
   disabled until an authenticated manifest request advertises
   `X-Memory-Lane-Playback-Modes: 1`. A rollback to an old viewer clears that
   capability on its next sync without deleting the saved preference.
3. Publish/verify the signed R2 application bundle, then promote its channel
   manifest last. On the local Pi, move only the updater agent's test API
   override back to the production API after it is ready; preserve the viewer's
   existing production connection, identity, credentials and cache.
4. Install through the owner webpage. Viewer v0.3.0 advertises support on its
   next sync (normally within five seconds); the selector then becomes usable.
5. The user will update the other field-test Pi manually with the converged
   viewer binary. Its existing software remains supported until then.

Older server code cannot open schema 14. Prefer a compatible forward fix;
restoring the pre-upgrade database snapshot discards later metadata and does
not restore deleted media. Pi binary rollback is separate from server/database
rollback. The new viewer safely uses legacy Fresh Prints behavior when the old
API/manifest has no playback-mode field. Test signing trust remains separate
from a future production signing-key setup.

## R2 and manual field-test artifacts

Prepared artifacts live in ignored `target/releases/v0.5.3/`:

- `astrohud-frame-v0.3.0-aarch64-linux-gnu`: manual field-test viewer.
- `astrohud-provisioner-v0.1.1-aarch64-linux-gnu`: matching bundle provisioner.
- `SHA256SUMS`: checksums of the standalone binaries.
- `r2-upload/slideshow-0.5.3/`: immutable application files and signed manifest.
- `r2-upload/updater-test-20261114/stable.json`: channel promotion copy.

Upload the `slideshow-0.5.3` folder to `memorylane-nightly`, verify its files,
then replace **only** `updater-test-20261114/stable.json` with the promotion
copy. Do not replace the old folder's binary files: they belong to the prior
release. The channel manifest references the new immutable URLs and advertises
product v0.5.3, sequence 2, using the existing local test verification key.

For an unmanaged field Pi, use the standalone viewer, back up its existing
binary, stop the viewer, atomically install the replacement at its configured
executable path and restart the viewer. Check its version, active state,
automatic restart count, visible playback and preserved settings. **Do not
overwrite `/usr/local/bin/astrohud-frame` if it is an updater-managed symlink**;
use the signed application updater on managed appliances instead. No SD reflash,
factory reset, Wi-Fi wipe, credential rotation or provisioner upgrade is needed
for the playback feature itself.

## Checks

```sh
cargo test --workspace
cargo clippy -p astrohud-frame -p astrohud-rest --all-targets -- -D warnings -A clippy::chunks_exact_to_as_chunks
node astrohud-frame/scripts/test-playback-order.mjs
# Requires separately installed Puppeteer and an isolated local test API:
PUPPETEER_MODULE=/path/to/puppeteer/lib/puppeteer/puppeteer.js \
  node astrohud-frame/scripts/test-playback-web.mjs
just pi-build
```

Tests cover rounds, additions/removals, restart progress, mode changes, random
repeats, empty/single collections, prefetch stability, API permissions, migration
snapshots, legacy gating/rollback, real browser image loading, owner pin priority
and 320/390/1280px layouts. Physical mode switching on the Pi still requires the
matching API/site deployment and installing the new signed viewer release.
