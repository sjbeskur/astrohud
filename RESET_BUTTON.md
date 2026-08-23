# Planned reset button

Status: hardware and software design only. No GPIO reset handler is installed
on the reference frame yet.

AstroHUD should have one local recovery control that still works when Wi-Fi,
the server, and the image viewer are unavailable. The control must distinguish
between changing Wi-Fi and removing all customer data; those are deliberately
different operations.

## Prototype shopping list

- [SparkFun COM-11996 black panel-mount momentary
  button](https://www.sparkfun.com/momentary-button-panel-mount-black.html), or
  an equivalent SPST normally-open momentary switch
- two lengths of 24–28 AWG stranded wire
- one 1 kΩ resistor
- heat-shrink tubing and strain relief
- female 2.54 mm jumper sockets if the Pi already has header pins; otherwise,
  a soldered 2×20 male header or two wires soldered directly to the required
  through-holes

The recommended button is small, unlit, non-latching, and needs a hole of
approximately 6.75–7 mm. Buy a spare for destructive mounting experiments.

The FLIRC Raspberry Pi Zero case includes an alternate lid that exposes the
GPIO area. Route the button cable through that opening, but mount the button on
the picture frame's rear panel or a replaceable plastic part. Do not drill the
aluminum case until the placement and ergonomics have been validated.

## Proposed wiring

Use GPIO17 as the active-low input unless later hardware introduces a conflict.

```text
GPIO17, physical pin 11 ── 1 kΩ ── normally-open button ── GND, physical pin 9
```

The software will enable the GPIO's internal pull-up. Pressing the button then
pulls the input low. The series resistor protects the GPIO if a software or
configuration error ever drives that pin high.

Power the Pi down before connecting or changing the wiring. Never connect this
button to 3.3 V, 5 V, or the `RUN` pad. Pulling `RUN` low immediately resets the
processor and bypasses AstroHUD's safe, power-loss-aware reset path.

## User-visible behavior

Perform the action when the button is released, based on the continuous hold
duration:

| Hold time | Action |
| --- | --- |
| Less than 5 seconds | Cancel; make no persistent change |
| 5 to less than 15 seconds | Enter change-Wi-Fi mode |
| 15 seconds or longer | Factory reset |

While held, the display should show progress and clearly label the action that
will occur on release. Ignore a button already held during boot until it has
first been released. Debounce both edges and accept only one action per press.

### Change Wi-Fi

This is a recoverable networking operation, not a factory reset. It should:

- preserve device identity, ownership, settings, and cached photos;
- preserve the existing household Wi-Fi profile as a rollback;
- show the existing QR-guided setup experience;
- commit the replacement profile only after it connects successfully; and
- allow a local cancellation that restores the previous profile.

The existing `astrohud-enter-setup` and `astrohud-exit-setup` commands already
provide most of this behavior.

### Factory reset

This is the resale/privacy operation. It should:

- remove active, candidate, setup, and backup Wi-Fi profiles;
- remove the future owner/pairing token and customer preferences;
- remove cached photos and manifests;
- preserve an immutable manufacturing identity/device code;
- rotate the temporary setup password; and
- immediately enter QR-guided provisioning.

The current `/etc/astrohud/device.json` combines the device code and setup
password. Split immutable identity from rotatable setup credentials before
implementing factory reset. Never delete a future hardware identity or device
certificate as part of a customer reset.

## Software boundary

Implement GPIO monitoring as a small, privileged system service independent of
the SDL viewer and network services. The monitor owns reset authorization and
filesystem changes; the viewer only renders progress/status files. This keeps
privileged deletion out of the display process and leaves recovery available
when the viewer crashes.

Reset intent and progress should be persisted atomically before destructive
work begins so an interrupted reset can safely resume on the next boot. Do not
log Wi-Fi passwords, pairing secrets, or the QR payload.

## Hardware validation checklist

1. Confirm whether the target Pi has a populated GPIO header and confirm the
   button's mounting depth behind the frame.
2. With destructive actions disabled, verify idle/high and pressed/low states,
   debounce, boot-held-button handling, and all three timing bands.
3. Enable only change-Wi-Fi mode; enter it, cancel it, and verify the original
   profile and cached slideshow return.
4. Re-enter change-Wi-Fi mode and complete QR provisioning to a replacement
   network.
5. Back up a test unit, enable factory reset, hold for at least 15 seconds, and
   verify credentials, ownership data, and cached photos are gone while the
   manufacturing identity remains stable.
6. Interrupt power at each factory-reset stage and verify the next boot reaches
   a deterministic, recoverable provisioning state.
