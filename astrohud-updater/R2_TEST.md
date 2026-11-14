# R2 and the reference Pi test

The updater implementation is committed on `feature/app-updater` as `7fd9086`.
On November 14, 2026, the updater's version/help/status smoke checks and all 12
ARM64 unit tests passed directly on the Pi Zero 2 W at `192.168.50.57`.
These tests used temporary directories and simulated service control. They do
not prove an actual application installation, physical power-loss recovery, or
rendered output. Both installed services remained active with zero restarts.

## Test hosting

Bucket: **memorylane-nightly**.
Public development endpoint:
`https://pub-5caa9bbda1e847609c98073d1b5f5ed3.r2.dev`.
Cloudflare's authenticated dashboard address is for administration; it is not a
download endpoint. No R2 API credentials or CLI profile are configured on this
workstation yet. The initial upload therefore uses the bucket dashboard.

The prepared bundle is in ignored
`target/releases/pi-update-test-20261114/upload/updater-test-20261114/`:

| Object key inside the bucket | File |
| --- | --- |
| `updater-test-20261114/astrohud-frame` | Known-good viewer v0.2.2 |
| `updater-test-20261114/astrohud-provisioner` | Known-good provisioner v0.1.1 |
| `updater-test-20261114/stable.json` | Signed test release 0.5.2, sequence 1 |

In the R2 dashboard, create/open `updater-test-20261114`, then upload the three
files. Upload only that folder's contents; no private key or credentials belong
in the bucket. The `0.5.2` label identifies this test bundle; its application
bytes exactly match those currently installed, and no product release/tag was
published. The signing key is a disposable test key in a private `/tmp`
directory outside the repository. The public updater configuration is in
`target/releases/pi-update-test-20261114/updater-config.json`.

Use the public **manifest object URL**, not the bucket root, to verify upload:

```text
https://pub-5caa9bbda1e847609c98073d1b5f5ed3.r2.dev/updater-test-20261114/stable.json
```

The operator must verify the served manifest equals the prepared signed file,
and both served binaries match the signed hashes and lengths before testing.
A denied bucket-root listing does not by itself mean object access is disabled.

## Keep the test isolated

Production remains v0.5.1/schema 12. A local schema-13 API with a synthetic
household is prepared in a private temporary directory. An authenticated device
report is authorized there using the Pi credential's hash; the actual device
credential and identity are not copied or changed. Only the updater agent will
point to the test API. Viewer, provisioner, Wi-Fi, existing photos, and pairing
continue to use their existing production configuration.

A temporary Cloudflare Quick Tunnel supplies an HTTPS address for the Pi and
phone. It is an authenticated application endpoint, with a separate test owner
link. Do not commit that owner link or its token. The tunnel's address changes
when restarted, and stopping the test server/tunnel ends the test session.
Runtime paths and the current public test endpoint are recorded in ignored
`target/releases/pi-update-test-20261114/test-plan.json`; no production database
upgrade is needed for this test. Never put a private owner link in that file.

Before installing, the operator should preserve the existing application/unit
files and record private checksums of identity, device credential, and Wi-Fi
configuration. The bootstrap kit is staged in the Pi's temporary test directory.
Install its agent-only `90-integration-test.conf` override before starting the
bootstrap installer. The first install adopts the originals as `baseline`,
starts the updater agent and check timer, and does not restart the viewer or
provisioner. Verify protected state is unchanged and both services remain active.

## Guided phone test

Once the R2 files are verified and the bootstrap is complete:

1. Open the private **test** owner link on the phone and choose Settings.
   The selected device is named **Local Pi updater test**. Its gallery is
   synthetic/empty; production photos should still be showing on the actual Pi.
2. In **Software update**, choose **Check for updates**. Expect a queued request,
   then the available test version `0.5.2` within roughly 15–30 seconds.
3. Choose **Install update** and confirm. Watch for a brief pause in playback,
   followed by the photos returning and the webpage reporting success. The
   operator verifies both services stayed running for the 15-second health
   window and execute the selected binaries, with no automatic restarts.
4. Choose **Restore previous version** and confirm. Expect another brief pause,
   then successful return to the adopted baseline. The operator repeats service,
   binary, identity, credential, Wi-Fi and cache checks.
5. Reinstall the signed test release if useful. A second update of the same
   installed release should report up to date rather than unnecessarily restart
   the applications.

The human observation matters: report a blank screen, unexpected setup screen,
Wi-Fi loss, misleading progress, or photos that fail to resume. Leave power
connected during this normal-path test. Corrupt-download, failed-startup and
physical interruption tests are separate attended steps, after normal update
and rollback have passed. Do not use factory reset to test the updater.

## R2 layout for continued use

The initial date-specific folder avoids changing any other release channel.
For recurring nightly publication, use immutable versioned binaries under
`releases/<sequence>-<version>/` and a small signed channel manifest at
`channels/nightly/stable.json`. Set `--base-url` to the immutable release
folder when running `scripts/release.py`. Upload and verify binaries first;
replace the channel manifest last. Keep it revalidated/un-cached, and cache
immutable binaries for a long lifetime. Refresh signatures before the default
30-day metadata expiry, even when application bytes have not changed.

For production, attach a custom HTTPS domain to the bucket and configure cache
rules appropriately; Cloudflare's `r2.dev` development URL is for testing.
Clients hold only the public verification key. R2 upload credentials remain on
the publisher workstation or CI. The existing signed HTTPS downloader needs no
provider-specific code.

References: [Cloudflare public buckets](https://developers.cloudflare.com/r2/buckets/public-buckets/),
[dashboard uploads](https://developers.cloudflare.com/r2/objects/upload-objects/),
[Quick Tunnels](https://developers.cloudflare.com/tunnel/get-started/quick-tunnels/).
