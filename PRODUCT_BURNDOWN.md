# AstroHUD product burndown

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
| `D-102` | P0 | M | Ready | Separate immutable manufacturing identity from rotatable setup credentials; factory reset removes owner data, Wi-Fi, and cached media but preserves hardware identity and resumes safely after power loss. | `D-101` |
| `D-103` | P0 | M | Waiting | Benchmark the regular Pi Zero W for decode time, slideshow smoothness, sync latency, memory, thermals, and onboarding responsiveness before choosing it as the cost target. | Pi Zero W hardware |
| `D-104` | P0 | L | Ready | Produce a versioned appliance image or deterministic provisioning process from a blank SD card, including users, permissions, services, NetworkManager, log limits, and first boot. | `D-102`, `D-105` |
| `D-105` | P0 | M | Waiting | Provision a unique device credential and authenticate every manifest/media request. Revocation stops a lost or resold frame without affecting others. | `C-103` |
| `D-106` | P0 | L | Ready | Add signed, atomic software/configuration updates with health checks and automatic rollback to the last bootable version. | `D-104` |
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
| `C-110` | P0 | M | Ready | Add upload quotas, rate limits, content-type verification, abuse controls, and bounded processing so one account cannot exhaust the service. | `C-102`, `C-105` |
| `C-111` | P1 | M | Ready | Track accepted, processed, synchronized, and displayed delivery states without claiming success before the frame acknowledges it. | `C-103`, `C-105`, device protocol |
| `C-112` | P1 | M | Ready | Instrument the core journey with privacy-conscious events: setup started/completed, time to first photo, invitation accepted, first send, successful delivery, repeat sender, recovery event, and support intervention. | `C-102`, `C-109` |
| `C-113` | P1 | M | Waiting | Recruit and operate a small family beta with consent, support expectations, feedback interviews, incident handling, and a documented exit decision. | Remote family alpha gate |
| `C-114` | P1 | M | Ready | Add place/channel management and remote frame status to the owner web experience. Preserve the simple send-first interface for publishers. | `C-103`, `C-104`, `C-111` |
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

### Active burn

| ID | Priority | Size | Status | Outcome / acceptance evidence | Depends on |
| --- | --- | --- | --- | --- | --- |
| `M-101` | P0 | S | Waiting | Choose native iOS/Android or a shared framework using a short technical spike. Record camera/photo-library, background upload, share extension, accessibility, and maintenance tradeoffs. | Draft hosted API contract |
| `M-102` | P0 | M | Waiting | Publish and version the authentication, invitation, channel, upload, delivery, and error contracts; generate or share typed client models where useful. | `C-102` through `C-107` |
| `M-103` | P0 | M | Waiting | Establish iOS and Android builds, CI, signing, environments, secure configuration, crash reporting, and a minimal navigation shell. | `M-101` |
| `M-104` | P0 | M | Waiting | Implement sign-in, account recovery, invite acceptance, and safe credential storage with a complete logout/revocation path. | `C-102`, `C-107`, `M-103` |
| `M-105` | P0 | L | Waiting | Select one or more photos, choose a human-readable destination/channel, upload with visible progress, and receive an honest success or failure state. | `C-105`, `M-102`, `M-104` |
| `M-106` | P0 | S | Waiting | Show a clear per-send metadata control. Location remains off unless selected, and the user previews whether approximate location will appear. | `M-105`, location API contract |
| `M-107` | P0 | M | Waiting | Retry interrupted uploads safely without duplicates; define background execution behavior for both platforms. | `M-105` |
| `M-108` | P0 | M | Waiting | Meet keyboard/screen-reader, dynamic text, contrast, reduced-motion, touch-target, and error-recovery requirements for the core journey. | `M-103` through `M-107` |
| `M-109` | P0 | M | Waiting | Distribute signed builds through TestFlight and an Android testing track with privacy disclosures, tester consent, crash visibility, and rollback. | Core mobile journey |
| `M-201` | P1 | M | Later | Add a system share extension so a photo can be sent to AstroHUD directly from the platform photo viewer. | `M-105`, `M-107` |
| `M-202` | P1 | M | Later | Support invitation and pairing deep links with safe expired-link and wrong-account recovery. | `C-103`, `C-107` |
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
   and media contracts are unlikely to churn. Ship the core send journey before
   share extensions, push notifications, captions, or billing.

## Burndown maintenance

At each product review:

1. Move only demonstrated work to `Validated`.
2. Add an owner and target release to the items entering the active cycle.
3. Split any `L` item before implementation if it cannot produce incremental
   evidence.
4. Record newly discovered blockers in the dependency column.
5. Remove work that does not improve appliance reliability, secure remote
   sending, repeat engagement, or validated product economics.
