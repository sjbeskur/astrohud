# Friendly beta MVP

## Outcome

AstroHUD's next product gate is a three-household friendly beta. Each household
receives one pre-enrolled Pi Zero appliance. An owner connects it to Wi-Fi,
claims it with a short code shown on the television, and creates private links
that trusted friends and family use to send photos to that place.

The beta is successful when all three households can use the complete journey
without sharing data, credentials, or destinations with one another.

## Product vocabulary

- **Household** is the internal security and ownership boundary. It does not
  need to appear in ordinary sender language.
- **Place** is the human-readable destination shown to people, such as
  `Mom's living room`.
- **Frame** is a physical AstroHUD appliance assigned to a place.
- **Owner** names the place, claims its frame, manages invitations, and removes
  photos.
- **Sender** follows an invitation that already determines where they may send.
- **Operator** is the beta administrator who creates households and supports
  the three loaned appliances.

## Core journeys

### Prepare a tester

1. The operator creates a household and an owner activation link.
2. The same reusable appliance image is written to an SD card.
3. On first boot, the appliance creates its own identity and credential. No
   per-device secret is included in the image.
4. The tester completes the existing QR-guided Wi-Fi setup.
5. The appliance registers as pending and displays a short-lived claim code.
6. The owner opens their activation link, names the place, and enters the code.
7. The server binds the appliance to that household and authorizes sync.

### Invite a sender

1. The owner creates a labeled invitation, for example `Alice`.
2. The owner shares the private link out of band.
3. The sender opens it and sees the fixed destination. Device IDs, household
   selection, and channel administration are hidden.
4. The owner can revoke that invitation without affecting other senders.

### Send a photo

1. The sender sees the place name and whether its frame was online recently.
2. They choose and preview a JPEG or PNG.
3. Approximate location remains off unless they explicitly select it.
4. The server accepts and prepares the image.
5. The UI distinguishes `received by the server`, `waiting for the frame`, and
   `delivered to the frame cache`.

### Operate a household

The owner can rename the place, inspect recent photos, delete a photo, create
and revoke sender invitations, and see the frame's last contact time.

## Security boundaries

- Every frame, channel, invitation, and photo is owned by one household.
- Authorization derives the household from the authenticated owner, sender,
  or device credential. A client-supplied household ID is never trusted.
- Device credentials and invitation tokens are random, independently
  revocable, and stored hashed on the server.
- A short claim code identifies a pending enrollment; it is not a credential.
- Manifests and media require device authentication.
- Cross-household subscriptions are rejected by the database and by API query
  scoping.
- Upload byte size, decoded dimensions, format, and household storage are
  bounded before the service is exposed publicly.

## Initial data model

The first implementation establishes the household boundary around the
existing local vertical slice:

```text
households
  |-- frames
  |-- channels
        |-- photos
        |-- frame_subscriptions -- frames
```

Enrollment, owner grants, sender invitations, and delivery acknowledgements
will be added in vertical slices as their user journeys are implemented. This
keeps the schema adjustable while requiring tenant scoping from the start.

SQLite stores metadata on the server; image bytes remain in server filesystem
storage. Each appliance continues to use a saved JSON manifest and ordinary
cached image files rather than SQLite.

## MVP constraints

- Three operator-created households
- One owner and one physical frame per household in the initial UI
- One default channel per household, hidden from invited senders
- Immediate publication by trusted senders; owner deletion instead of a
  moderation queue
- A reusable image with first-boot identity generation
- Local development and simulated frames before hosted deployment

The schema may allow more than one frame or channel per household, but the beta
does not need management UI for those cases.

## Deliberately deferred

- Factory reset and resale automation
- Public signup, email delivery, passwords, and account recovery
- Self-service ownership transfer
- Native mobile applications
- Postgres and object storage
- Billing, push notifications, comments, reactions, and captions
- General fleet management and automatic operating-system updates

Loaned appliances must be manually wiped before reassignment while factory
reset remains deferred.

## Delivery order

- [x] Establish the household schema, migrate the current demo data, and prove
  isolation with automated tests.
- [x] Add pending device enrollment and the owner claim journey.
- [x] Add owner access and invitation-scoped sender access.
- [x] Authenticate manifest and media delivery.
- [x] Generate per-appliance identity on first boot and display the claim code.
- [ ] Add frame last-seen state, cache-delivery acknowledgements, deletion, and
  quotas.
- [ ] Exercise three local households with simulated devices.
- [ ] Validate the same flow on the three physical appliances.
- [ ] Deploy the already-proven service to a public HTTPS host.

The server, browser simulator, and native Pi now exercise authenticated
manifest and media delivery. The provisioner generates the device credential
on the appliance, enrolls after Wi-Fi setup, and owns the claim-code display.
