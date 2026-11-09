# Memory Lane product burndown

This is the living product backlog for turning the current proof of concept
into a household-ready product. It separates work into three tracks so the Pi
appliance, hosted service, and mobile experience can advance without hiding
dependencies between them.

This document does not assign calendar dates. Add an owner and target release
when work enters a delivery cycle. Sizes are relative planning estimates, not
elapsed-time promises.

## Vocabulary

| Field | Meaning |
| --- | --- |
| `P0` | Required for the next meaningful product gate |
| `P1` | Required before a wider family beta |
| `P2` | Valuable after the core journey is proven |
| `S`, `M`, `L` | Small, medium, or large relative effort |
| `Done` | Implemented and covered by automated checks |
| `Validated` | Implemented and exercised on the reference hardware or live flow |
| `Ready` | Understood well enough to begin |
| `Waiting` | Needs hardware, a product choice, or another backlog item |
| `Later` | Intentionally shelved; not part of the active burn |

Row counts are not progress percentages: an `L` identity system carries more
risk and effort than an `S` display refinement.

## Product gates

| Gate | Pi device | Cloud and engagement | Mobile | Exit evidence |
| --- | --- | --- | --- | --- |
| Local appliance POC | Native cached frame and Wi-Fi onboarding | LAN server and responsive sender | Responsive web only | Completed on the reference Pi Zero 2 W |
| Unattended household prototype | Reset control, reproducible appliance image, soak tests | Still LAN-scoped | Responsive web only | A nontechnical household can recover it without SSH |
| Remote family alpha | Authenticated device connection | Hosted service, identity, invitations, authorization, durable media | Web sender first; thin native client may begin | A family member outside the home can securely send to the intended frame |
| Family beta | Updates, diagnostics, production hardware decision | Support tooling, lifecycle controls, observability, privacy operations | Installable iOS and Android flow | Multiple households use it repeatedly without developer intervention |

## Track A — Pi appliance

### Proven foundation

| ID | Status | Capability and evidence |
| --- | --- | --- |
| `D-001` | Validated | Native SDL2 viewer owns the DRM display and renders sharper images without Chromium. |
| `D-002` | Validated | Bounded on-device cache, atomic downloads, offline startup, EXIF orientation, and continuous retry work on the reference Pi. |
| `D-003` | Validated | Protected per-device setup SSID, QR join flow, captive portal, Wi-Fi selection, rollback, and normal reboot have passed attended hardware tests. |
| `D-004` | Validated | Persistent NetworkManager profile, recovery backup, Wi-Fi guard, and bounded persistent logs recover from a missing profile. |
| `D-005` | Done | Reproducible ARM64 Bookworm container build produces a Pi-compatible binary without installing Rust on the appliance. |
| `D-006` | Validated | ADM-aligned onboarding, deterministic per-photo chrome, and optional approximate-location tabs render natively. |

### Active burn

| ID | Priority | Size | Status | Outcome / acceptance evidence | Depends on |
| --- | --- | --- | --- | --- | --- |
| `D-101` | P0 | M | Waiting | Install and test the documented GPIO17 recovery button. Short presses do nothing, medium holds enter change-Wi-Fi mode, and long holds clearly authorize factory reset. | Physical button and mounting test |
| `D-102` | P0 | M | Implemented; physical validation pending | Immutable manufacturing identity, owner-requested reset, rotatable credentials, and journaled appliance cleanup are implemented. Validate Wi-Fi wipe, service transitions, and power interruption on a spare unit; GPIO authorization remains under `D-101`. | Updated provisioner and systemd units; spare appliance |
| `D-103` | P0 | M | Waiting | Benchmark the regular Pi Zero W for decode time, slideshow smoothness, sync latency, memory, thermals, and onboarding responsiveness before choosing it as the cost target. | Pi Zero W hardware |
| `D-104` | P0 | L | Hardware validation | Clean ARM64 image and containerized just recipes built; verify physical boot, Wi-Fi setup, graphics, root expansion, and memory pressure before distribution. | `D-102`, `D-105` |
| `D-105` | P0 | M | Waiting | Provision a unique device credential and authenticate every manifest/media request. Revocation stops a lost or resold frame without affecting others. | `C-103` |
| `D-106` | P0 | L | Planned; later this week | Add a small application updater for the native viewer/provisioner: signed release metadata, atomic binary installation, health checks, rollback, and visible version/status. Preserve Wi-Fi, identity and cached photos. Keep OS upgrades separate; no browser desktop or full-card reflash for ordinary app updates. User requested this as the next follow-up after the release-review deployment; implementation is deferred. | `D-104` |
| `D-107` | P0 | M | Ready | Pass a repeatable resilience matrix: seven-day soak, router outage, server outage, repeated power loss, full cache, bad image, corrupt download, and clock correction. No blank-screen or SSH-only recovery is allowed. | `D-102`, `D-104`, `D-106` |
| `D-108` | P0 | M | Waiting | Freeze a serviceable BOM for compute module, SD card, power, cable, enclosure, button, mounting, packaging, and manufacturing label; record landed cost and assembly time. | `D-103`, physical prototypes |
| `D-109` | P1 | M | Ready | Run provisioner and viewer with the least privilege practical; audit writable paths, secrets, local ports, and service sandboxing. | `D-104`, `C-103` |
| `D-110` | P1 | S | Ready | Add manifest byte sizes and checksums; detect corrupt cache entries and repair them without discarding healthy offline media. | Cloud manifest change |
| `D-111` | P1 | M | Waiting | Report privacy-safe health telemetry: software version, last sync, storage pressure, temperature/throttling, reboot reason, and recovery events. Never include photos or credentials. | `C-109` |
| `D-201` | P2 | M | Later | Add owner-configurable display schedule, sleep/wake behavior, rotation timing, and HDMI/CEC experiments. | Stable appliance baseline |
| `D-202` | P2 | S | Later | Evaluate restrained transitions only after memory and week-long stability remain within budget. | `D-107` |

### Device definition of done

- A sealed unit can be provisioned, used, moved to new Wi-Fi, factory-reset,
  updated, and recovered without a shell.
- Cached photos remain available through network and cloud outages.
- No owner credential, Wi-Fi secret, or photo survives a completed factory
  reset.
- The chosen low-cost board meets measured display quality and stability goals.

## Track B — Cloud software and user engagement

### Proven foundation

| ID | Status | Capability and evidence |
| --- | --- | --- |
| `C-001` | Done | SQLite channels, frame subscriptions, photo metadata, filesystem media, and ordered manifests provide a persistent local vertical slice. |
| `C-002` | Validated | Responsive sender can create a channel and deliver an image to the reference frame. |
| `C-003` | Done | Oversized uploads are oriented, resized, and encoded into display-ready media before the Pi downloads them. |
| `C-004` | Validated | Sender-controlled EXIF GPS extraction can publish an approximate coordinate label without requiring geocoding on the Pi. |

### Active burn

| ID | Priority | Size | Status | Outcome / acceptance evidence | Depends on |
| --- | --- | --- | --- | --- | --- |
| `C-101` | P0 | L | Ready | Deploy a reproducible hosted environment with PostgreSQL-compatible metadata, S3-compatible media, HTTPS, separate environments, migrations, and continuous delivery. | Hosting/provider decision |
| `C-102` | P0 | L | Ready | Add users, secure sessions, account recovery, and verified contact ownership. Authentication works consistently in web and future native clients. | `C-101` |
| `C-103` | P0 | L | Ready | Implement short-lived, single-use frame pairing and revocable device credentials. Pairing binds the physical code to the authenticated owner and named place. | `C-101`, `C-102` |
| `C-104` | P0 | L | Ready | Enforce owner, publisher, and viewer permissions for places, channels, invitations, publications, and device operations on every API path. | `C-102`, `C-103` |
| `C-105` | P0 | L | Ready | Move media to durable object storage with validated uploads, display variants, checksums, size limits, and authenticated delivery URLs. | `C-101`, `C-104` |
| `C-106` | P0 | M | Ready | Add photo removal, channel removal, account export/deletion, retention rules, and auditable cleanup of database, object storage, and device manifests. | `C-104`, `C-105` |
| `C-107` | P0 | M | Ready | Invite a family member through an expiring link, grant a bounded channel role, and revoke access. The recipient can send without learning a device ID. | `C-102`, `C-104` |
| `C-108` | P0 | M | Ready | Document and test secrets management, encrypted transport, database backups, media durability, and restore procedures. | `C-101`, `C-105` |
| `C-109` | P0 | M | Ready | Add structured logs, metrics, alerting, error correlation, and a minimal support view for frame connectivity and delivery failures. | `C-101`, `D-111` |
| `C-110` | P0 | M | In progress | Release-review changes validate and normalize JPEG/PNG uploads, remove embedded metadata, bound multipart reads and decoder allocations, and serialize image processing. Legacy public demo routes are opt-in. Household/disk quotas, enrollment limits, and sustained rate/abuse controls remain required; see `RELEASE_REVIEW.md`. | `C-102`, `C-105` |
| `C-111` | P1 | M | Ready | Track accepted, processed, synchronized, and displayed delivery states without claiming success before the frame acknowledges it. | `C-103`, `C-105`, device protocol |
| `C-112` | P1 | M | Ready | Instrument the core journey with privacy-conscious events: setup started/completed, time to first photo, invitation accepted, first send, successful delivery, repeat sender, recovery event, and support intervention. | `C-102`, `C-109` |
| `C-113` | P1 | M | Waiting | Recruit and operate a small family beta with consent, support expectations, feedback interviews, incident handling, and a documented exit decision. | Remote family alpha gate |
| `C-114` | P1 | M | Ready | Add place/channel management and remote frame status to the owner web experience. Preserve the simple send-first interface for publishers. | `C-103`, `C-104`, `C-111` |
| `C-115` | P1 | S | Implemented | Owner recovery links restore the existing household after cookies are cleared and work across mobile/desktop browsers. Setup reminds owners to create and save a link; Device settings can replace it without revoking active sessions. Secrets are hashed, and each recovery creates a separate expiring session. Passkeys/verified email remain deferred. | `C-102`, `C-104` |
| `C-116` | P1 | M | Done | Owner upload supports multiple JPEG/PNG selections, per-file previews and validation, sequential progress, and partial-failure retry without resending known successes. Verified with a 12-photo browser batch and a simulated rejected upload; physical-phone picker validation remains. | Existing owner upload API |
| `C-117` | P1 | M | Done | Photos opens as a three-column square-thumbnail gallery. Selecting a photo opens a full preview with Previous/Next, Show now, contextual Pin/Unpin, and confirmed frame-specific removal. Verified at 320/390/1280px with keyboard and automated accessibility checks. | Existing gallery/playback API |
| `C-118` | P1 | S | Done | Slideshow timing and background controls move to a settings dialog above the gallery; pin duration lives with the selected photo. Gallery remains visible first. Verified settings persist through the existing API. | `C-117` |
| `C-119` | P2 | M | Later; TBD | Explore owner customization of photo backgrounds. Decide the useful controls, defaults, preview behavior, and how they apply per device before implementation. Keep the existing soft-background on/off control for now; additional controls are not specified yet. | Usability feedback; viewer/API design |
| `C-120` | P1 | M | Done | A shared frame selector follows Home, Photos, Send photos, and Settings, survives reloads, falls back when a saved selection is unavailable, and scopes device settings. Single-frame owners see the name without a selector. A local three-frame simulator provides distinct sample galleries and intervals. Browser checks cover selection, upload target, settings isolation, and simulated playback; deployment includes the website and API together. | `C-117`, existing owner/device APIs |
| `C-121` | P1 | M | Done | Owner and invited-sender uploads explicitly target one, selected, or all currently permitted frames. Atomic delivery plans govern manifests, media access, and pin authorization. Older photos retain their previous shared-feed visibility; future frames do not inherit new targeted uploads. HTTP and browser checks cover real delivery, unauthorized destinations, cleanup, missing destinations, and new-frame behavior. A routing-aware API checkpoint and automatic pre-migration database snapshot support release recovery. | `C-120`, additive schema v9 |
| `C-122` | P1 | M | Done | Invited senders can browse their own retained uploads with paginated My photos and read-only previews, including after frame-local removal or reset. Owners see sender groups, filters, and paginated galleries. New uploads record stable invitation provenance; older uploads remain Earlier uploads. Tests cover privacy, forged author fields, link replacement/revocation, pagination, and no-frame history. Release includes the API and website together. | `C-121`, additive schema v10 |
| `C-201` | P2 | M | Later | Add carefully controlled delivery or new-photo notifications after measuring whether they help families rather than creating notification pressure. | `C-112`, mobile push support |
| `C-202` | P2 | M | Later | Reverse-geocode opted-in GPS into a city-and-region tab. Round coordinates before the provider request, cache responses, attribute the provider, and retain rounded coordinates as fallback. | Configurable geocoder/provider review |
| `C-203` | P2 | L | Later | Add plans, billing, and entitlement enforcement only after household retention and hardware economics justify them. | Product validation |

### Engagement questions to burn down with evidence

- Can a new owner reach the first displayed photo in under two minutes without
  developer help?
- Does an invited publisher understand the place/channel model without an
  explanation?
- Do senders return after the novelty week, and which moments cause them to
  send again?
- Is a delivered-to-frame acknowledgement more valuable than generic push
  notifications?
- How often does a household need support, a Wi-Fi recovery, or a physical
  reset?
- Does the inexpensive secondhand-monitor proposition remain credible after
  hardware, hosting, fulfillment, and support costs?

## Track C — Mobile application

The responsive web sender remains the product-learning surface until identity,
authorization, invitations, and the upload contract are stable. Native work
should begin as a thin client for the proven journey, not as a second product
specification.

### Planned direction — November 8, 2026

Mobile implementation is deferred while we work through further usability
enhancements to the current website and device experience. Tauri 2 is the
proposed framework, subject to a practical iPhone and Android feasibility test.
No mobile application has been scaffolded or built yet.

- Build one Memory Lane app for owners and invited senders. Preserve the
  simple **Invite**, **Send photos**, and **Add a device** home, with Photos
  and Settings as secondary actions. Senders only see actions they can use.
- Reuse the hosted service and its authorization rules. Bundle the app UI
  locally and share frontend components where practical; native integrations
  may require Swift on iOS and Kotlin on Android.
- Keep web invitations and the browser Wi-Fi wizard available, including for
  people who do not install the app.
- The developer currently has no Mac. Plan Android development on the Linux
  workstation, hosted macOS/Xcode builds for iOS, and real-iPhone testing
  through TestFlight. Apple developer membership, signing credentials, and
  physical test phones are prerequisites for the private beta. Access to a
  Mac may still be needed for interactive iOS debugging.

| Phase | Scope | Exit evidence |
| --- | --- | --- |
| 1. Validate Tauri on phones | Prototype photo selection, iPhone HEIC/HEIF conversion, invitation links, protected credential storage, and receiving photos from the system Share menu. Test iOS share-extension and Android share-intent behavior early. | A real iPhone and Android phone can activate an invitation and send a photo to a test frame; record native integration gaps before committing to the framework. |
| 2. Establish app access | Add revocable app sessions with credentials protected through iOS Keychain and Android Keystore. Define transfer from an existing owner browser, recovery, logout, expiry, and invitation replacement. Browser cookies alone are not the app authentication design. | Access survives app restart, revoked access fails, and reinstalling or changing phones has an explicit recovery path. |
| 3. Build everyday flows | Implement invitations, sending with preview/destination/progress, gallery, Show now, pin/resume, slideshow controls, recovery, and reset progress. Normalize photos to server-supported formats; retain opt-in location controls. Define safe retries and background-upload limits. | Owners and senders complete their authorized tasks without assistance; retries cannot duplicate publications or silently lose media. |
| 4. Improve setup handoff | Integrate the Pi's existing Wi-Fi setup with app navigation, QR/link handling, permission prompts, temporary-network changes, and confirmed cloud pairing. Retain browser fallback. | Wrong passwords, denied permissions, interrupted setup, and returning to the home network all have clear recovery steps. |
| 5. Run a private beta | Establish signed builds, separate test/production configuration, TestFlight and Google Play testing, accessibility checks, privacy disclosures, diagnostics, and release procedures. | Testers on both platforms exercise large photos, limited photo access, weak connections, multiple devices, upgrades, and recovery successfully. |

The first milestone is invitation activation and one photo sent on both
platforms, including entry through the system Share menu. Full owner controls
follow that proof. Notifications, billing, and a general photo-library manager
remain outside the initial scope. A schedule should follow the feasibility
results rather than assume all required mobile integrations are ready-made.

### Active burn

| ID | Priority | Size | Status | Outcome / acceptance evidence | Depends on |
| --- | --- | --- | --- | --- | --- |
| `M-101` | P0 | S | Later; Tauri 2 proposed | Run the phone feasibility milestone above before committing to Tauri. Record photo-library/HEIC, secure storage, deep-link, background-upload, share-extension, accessibility, and maintenance tradeoffs. | Draft hosted API contract; iPhone/Android test devices; hosted macOS build access |
| `M-102` | P0 | M | Waiting | Publish and version the authentication, invitation, channel, upload, delivery, and error contracts; generate or share typed client models where useful. | `C-102` through `C-107` |
| `M-103` | P0 | M | Waiting | Establish iOS and Android builds, CI, signing, environments, secure configuration, crash reporting, and a minimal navigation shell. Use hosted macOS/Xcode builds for iOS while no development Mac is available. | `M-101`; signing accounts and credentials |
| `M-104` | P0 | M | Waiting | Implement sign-in, account recovery, invite acceptance, and safe credential storage with a complete logout/revocation path. | `C-102`, `C-107`, `M-103` |
| `M-105` | P0 | L | Waiting | Select one or more photos, choose a human-readable destination/channel, upload with visible progress, and receive an honest success or failure state. | `C-105`, `M-102`, `M-104` |
| `M-106` | P0 | S | Waiting | Show a clear per-send metadata control. Location remains off unless selected, and the user previews whether approximate location will appear. | `M-105`, location API contract |
| `M-107` | P0 | M | Waiting | Retry interrupted uploads safely without duplicates; define background execution behavior for both platforms. | `M-105` |
| `M-108` | P0 | M | Waiting | Meet keyboard/screen-reader, dynamic text, contrast, reduced-motion, touch-target, and error-recovery requirements for the core journey. | `M-103` through `M-107` |
| `M-109` | P0 | M | Waiting | Distribute signed builds through TestFlight and an Android testing track with privacy disclosures, tester consent, crash visibility, and rollback. | Core mobile journey |
| `M-201` | P1 | M | Later | Complete system sharing so a photo can be sent to Memory Lane directly from the platform photo viewer. Validate the platform integration during `M-101` before completing the production flow. | `M-101`, `M-105`, `M-107` |
| `M-202` | P0 | M | Later | Support invitation and pairing deep links with safe expired-link and wrong-account recovery. Invitation-link handling is part of the first mobile milestone. | `M-101`, `C-103`, `C-107` |
| `M-203` | P1 | M | Later | Show recent sends, delivery state, and authorized removal without becoming a general photo-library manager. | `C-106`, `C-111` |
| `M-204` | P1 | M | Later | Add an offline send queue with explicit storage limits and user-controlled cancellation. | `M-107` |
| `M-205` | P1 | M | Later | Add opt-in push notifications only for validated engagement or operational needs. | `C-201` |
| `M-206` | P2 | S | Later | Add lightweight captions if household research shows that context materially improves the receiving experience. | Family beta evidence |

### Mobile definition of done

- A newly invited, nontechnical sender can install, authenticate, choose a
  destination, and deliver a photo without setup help.
- Upload retries cannot create duplicate publications or silently lose media.
- The app explains metadata and notification choices at the moment they matter.
- Both platform builds are signed, reproducible, observable, accessible, and
  remotely revocable.

## Recommended burn order

1. **Finish the unattended appliance prototype:** `D-101`, `D-102`, `D-103`,
   `D-104`, then the resilience matrix in `D-107`.
2. **Open the secure remote path:** `C-101` through `C-108`, paired with
   `D-105`. Prove one outside-the-home family send before expanding features.
3. **Measure the web journey:** `C-109` through `C-113`. Use actual household
   behavior to refine the channel, invitation, and delivery model.
4. **Build the thin mobile client:** start `M-101` only when the hosted identity
   and media contracts are unlikely to churn and the current usability pass is
   complete. Validate system sharing and invitation links in the feasibility
   spike, then ship the core send journey before push notifications, captions,
   or billing.

## Burndown maintenance

At each product review:

1. Move only demonstrated work to `Validated`.
2. Add an owner and target release to the items entering the active cycle.
3. Split any `L` item before implementation if it cannot produce incremental
   evidence.
4. Record newly discovered blockers in the dependency column.
5. Remove work that does not improve appliance reliability, secure remote
   sending, repeat engagement, or validated product economics.
