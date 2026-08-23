# AstroHUD Vision

## Product thesis

AstroHUD turns an ordinary HDMI monitor into a quiet, shared communication
object. The defining experience is not managing a photo library; it is being
able to send a moment to a place and to the people who gather there.

The reference story is simple:

> Send this photo to Grandma's kitchen.

A small Raspberry Pi module attaches to a new or secondhand monitor. A
nontechnical owner pairs it from a phone, subscribes it to one or more named
channels, and then leaves it alone. Friends and family publish photos to those
channels from a mobile-friendly sender. New photos arrive quickly, while the
frame remains useful when its network connection does not.

## Product principles

- **Places, not devices.** People send to recognizable destinations such as
  `Grandma's kitchen`, not hardware IDs.
- **Channels, not library administration.** Channels express an audience or
  shared context such as `Family`, `New baby`, or `Vacation`.
- **Appliance-simple.** Pairing a fresh frame should take less than two minutes
  and require no shell access.
- **Calm by default.** The frame displays photographs rather than becoming
  another notification screen.
- **Offline-capable.** A local cache keeps the slideshow working through a
  service or network outage.
- **Respectful ownership.** Owners control who can publish, what appears, and
  how their media can be exported or deleted.
- **Useful hardware reuse.** The experience should feel intentional on an
  inexpensive Raspberry Pi and a secondhand monitor.

## Proof-of-concept promise

The first proof of concept is successful when a user can:

1. Start an unpaired frame and see a QR code or short pairing code.
2. Pair and name the frame from a phone-sized web interface.
3. Create two channels and subscribe the frame to either or both.
4. Upload a photo to a selected channel.
5. See it appear on the frame within five seconds on a normal connection.
6. Disconnect the frame and continue viewing its cached slideshow.

The proof of concept is intentionally a responsive web application, not a
native mobile app. Native iOS and Android clients remain essential before a
consumer launch, once the interaction model has earned that investment.

## Initial architecture

- A single Rust web service exposes the sender, pairing, channel, media, and
  frame synchronization APIs.
- SQLite stores proof-of-concept metadata. The data model should remain easy to
  migrate to PostgreSQL when hosted multi-user testing requires it.
- Media uses a filesystem-backed storage interface locally, with an
  S3-compatible implementation as the hosted path.
- HTTPS handles uploads and media downloads. WebSockets or server-sent events
  carry only lightweight change notifications.
- The frame runs a fullscreen web viewer on Raspberry Pi OS and maintains a
  bounded local media cache and manifest.
- A frame authenticates with a revocable device credential issued during
  pairing; pairing codes are short-lived and single-use.

## Core domain

- **User:** a person who owns frames or publishes photos.
- **Place:** the human-readable destination represented by a frame, such as
  `Grandma's kitchen`.
- **Frame:** a provisioned display device associated with a place.
- **Channel:** a named stream to which users publish and frames subscribe.
- **Membership:** a user's role and permissions within a channel.
- **Subscription:** a frame's selection of channels.
- **Photo:** an uploaded original and its display-ready variants.
- **Publication:** the act of placing a photo into a channel.

## Deliberate exclusions from the proof of concept

- Native mobile applications
- Billing and subscriptions
- Video and audio
- Comments, reactions, and a general-purpose social feed
- Face recognition or automatic photo-library organization
- Complex fleet management and unattended operating-system updates
- Microservices or Kubernetes

## Future feature candidates

- **Human-readable photo locations.** Replace the current opt-in, rounded
  latitude/longitude tab with a city-and-region label such as
  `Boulder, Colorado`. Reverse geocoding belongs on the server, never the Pi.
  Before implementation, choose a configurable provider, round coordinates
  before transmission, cache results, provide required attribution, and retain
  the current coordinate label as the offline/failure fallback.

## Competitive boundary

Immich with ImmichFrame or Immich Kiosk is the reference implementation for a
self-hosted photo library displayed as a slideshow. AstroHUD should not rebuild
that product category feature-for-feature.

AstroHUD is worth continuing only if appliance-style pairing, place-oriented
destinations, and channel-based sending produce a meaningfully simpler and more
human experience than managing shared albums. Existing projects should remain
benchmarks and potential integration points, not reasons to ignore the
question.

## Kill and pivot criteria

Pause or change direction if early testing shows that:

- Shared albums communicate the channel model just as naturally.
- Existing software can provide the complete intended journey with light
  configuration.
- Pairing and maintaining commodity Pi hardware cannot be made reliable enough
  for a nontechnical household.
- The combined module, power, storage, mounting, and support costs undermine
  the secondhand-monitor value proposition.
- Test users enjoy receiving photos but do not repeatedly send them.

## Milestones

### 0. Rehabilitate the prototype

Preserve the existing WebSocket image demo, remove machine-specific settings,
make the workspace reproducibly buildable, and document the baseline.

### 1. Local vertical slice

Implement persistent photos and channels, a phone-sized sender, one frame
identity, and a display manifest using local storage.

### 2. Pairing and offline frame

Add QR pairing, device credentials, channel subscriptions, a bounded local
cache, reconnect behavior, and a Raspberry Pi kiosk launch path.

### 3. Family beta

Add user accounts, invitations, publisher roles, moderation, image variants,
quotas, hosted object storage, observability, and basic remote diagnostics.

### 4. Product validation

Run household trials, compare the complete journey with ImmichFrame, validate
hardware and support costs, and decide whether native mobile clients and a
consumer launch are justified.
