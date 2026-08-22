# AstroHUD interface language

AstroHUD uses the same retro-futurist operational language as the Advanced
Data Machines company homepage. It should feel like part of the same product
family without turning a picture frame into a busy control panel.

The reference implementation is the `feature/company-homepage-refresh` branch
of `~/repos/nemesis/adm-website`.

## Principles

1. Photos remain primary. Persistent frame chrome is small, translucent, and
   limited to identity, place, and connection state.
2. Setup and sending may be more instrument-like because the user is actively
   operating the product.
3. Status is explicit. Prefer language such as `SIGNAL READY`, `SETUP LINK`,
   and `TRANSFER ACTIVE` over ambiguous spinners.
4. Color communicates structure, not decoration. Amber identifies the product,
   salmon marks panel rails, lavender marks primary actions, blue indicates
   work in progress, and green confirms a healthy link.
5. The visual reference is a calm, original retro-futurist console—not copied
   franchise logos, insignia, typefaces, or terminology.

## Core palette

| Token | Hex | Use |
| --- | --- | --- |
| Void | `#070910` | Page and display background |
| Surface | `#101520` | Primary instrument panels |
| Raised | `#171e2b` | Nested or emphasized panels |
| Line | `#303a4b` | Borders and one-pixel separators |
| Text | `#edf2f7` | Primary text |
| Muted | `#99a6b8` | Supporting copy and telemetry |
| Amber | `#efb46a` | Brand bars and section signals |
| Salmon | `#e98c77` | Panel rails and attention |
| Lavender | `#ad96d8` | Primary action and expressive text |
| Blue | `#70a9d6` | In-progress activity |
| Green | `#69d59b` | Healthy/connected state |

These values are duplicated intentionally in the server stylesheet, the
self-contained captive portal, and the native PPM renderer. The provisioner
must work without internet access or external font and stylesheet requests.

## Shape and typography

- Use near-square left edges with generously rounded right edges, typically
  `5px 20px 20px 5px`.
- Use a narrow colored rail at the left edge of instrument panels.
- Use one-pixel separators and restrained shadows rather than floating cards.
- Use the platform sans-serif stack for human instructions.
- Use the platform monospace stack, uppercase, small size, and generous letter
  spacing for telemetry and state labels.
- A serif italic may be used sparingly for a single warm, human word in large
  headings. It should not appear in operational labels.
- Preserve visible keyboard focus and reduced-motion behavior.

## Implementations

- `astrohud-rest/static/astrohud-theme.css`: shared sender and browser-frame
  styles
- `astrohud-rest/static/sender.html`: responsive photo transmission UI
- `astrohud-rest/static/frame.html`: restrained browser-frame telemetry and
  empty state
- `astrohud-provisioner/src/web.rs`: offline, self-contained captive portal
- `astrohud-provisioner/src/setup_display.rs`: 1280×720 native QR setup card

When adding a new AstroHUD UI, begin with these tokens and interaction rules.
Do not copy the large company homepage stylesheet into the product.
