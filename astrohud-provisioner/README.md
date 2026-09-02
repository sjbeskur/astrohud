# AstroHUD provisioner

`astrohud-provisioner` is the no-mobile-app onboarding path for an AstroHUD
frame. On an unprovisioned device, or after an explicit local reset, it:

1. creates a stable random device code and setup password;
2. scans nearby Wi-Fi networks;
3. starts the protected `AstroHUD-XXXXXX` access point;
4. shows a large Wi-Fi QR code on the frame for one-tap joining;
5. serves a phone-friendly captive portal at `http://10.42.0.1/`;
6. stops the AP and tests the selected household network;
7. atomically commits both the NetworkManager profile and protected backup; or
8. restores the setup AP when the candidate connection fails;
9. carries a private device-bound setup link onto the owner's phone, creates a
   unique device ID and credential locally, and enrolls with the server;
10. asks the owner to name the place as soon as the server sees the frame; and
11. removes the setup screen automatically after the owner claims the frame.

The setup suffix uses uppercase characters without `0/O` or `1/I/L`. The
suffix is an identifier, not a credential. A separate 16-character random
password protects the setup AP and is included in the standard Wi-Fi QR
payload. The SDL viewer watches `/var/lib/astrohud/setup-screen.ppm`, gives the
setup card priority over photos while it exists, and resumes the slideshow
after successful provisioning. The SSID and password appear on the card only
as a fallback for phones that cannot scan Wi-Fi QR codes.

The private bootstrap token is separate from the appliance credential. It is
sent in the URL fragment, stored only as a hash by the server, expires with the
pending enrollment, and can claim exactly one frame. The television retains a
short claim code as an attended support fallback; normal onboarding does not
ask the owner to type it or obtain a separate activation link.

The service stays dormant when either a persistent household profile or its
recovery backup exists and the device is already claimed. While an online
device is waiting to be claimed, it refreshes expired setup credentials and
keeps the setup QR on the television. To enter setup mode locally:

```sh
sudo astrohud-enter-setup
```

This intentionally disconnects the current Wi-Fi connection. The existing
Wi-Fi guard ignores the interface while
`/var/lib/astrohud/provisioning-required` exists. Successful provisioning
removes the marker; a failed candidate reopens the setup AP.

To cancel setup locally and restore the protected household profile:

```sh
sudo astrohud-exit-setup
```

To print manufacturing label data locally on the device:

```sh
sudo astrohud-provisioner --print-label
```

The output contains the setup password and must be handled as a credential.
`/etc/astrohud/device.json` and `/var/lib/astrohud/device-credential` contain
the unique device credential. They are created on the appliance, are never
displayed, and must not be copied into another appliance image.

Before capturing a reusable image, verify that it contains none of these
runtime files:

```text
/etc/astrohud/device.json
/var/lib/astrohud/device-credential
/var/lib/astrohud/manifest.json
/var/lib/astrohud/media/
/etc/astrohud/wifi-profile.nmconnection
/etc/NetworkManager/system-connections/astrohud-wifi.nmconnection
```

Do not remove those files from an assigned appliance; that is a manufacturing
image requirement, not a reset procedure.

## Pi dependencies

NetworkManager performs AP/client switching. Its shared IPv4 mode uses
`dnsmasq` for DHCP and DNS, so install `dnsmasq-base`. Install the files as:

```sh
sudo install -m 0755 astrohud-provisioner /usr/local/sbin/astrohud-provisioner
sudo install -m 0755 deploy/astrohud-enter-setup.sh \
  /usr/local/sbin/astrohud-enter-setup
sudo install -m 0755 deploy/astrohud-exit-setup.sh \
  /usr/local/sbin/astrohud-exit-setup
sudo install -m 0644 deploy/astrohud-provisioner.service \
  /etc/systemd/system/astrohud-provisioner.service
sudo install -d -m 0755 /etc/NetworkManager/dnsmasq-shared.d
sudo install -m 0644 deploy/astrohud-captive-dns.conf \
  /etc/NetworkManager/dnsmasq-shared.d/astrohud-captive.conf
sudo systemctl daemon-reload
sudo systemctl enable --now astrohud-provisioner.service
```

Enabling the service on an already provisioned device is non-disruptive: it
ensures device identity exists, adopts a matching legacy frame credential when
present, verifies that the server recognizes it, and exits.

## Planned physical reset

The reference frame does not yet have a GPIO reset button. The planned control
uses a medium hold for recoverable Wi-Fi setup and a much longer hold for a
privacy-preserving factory reset. See [`../RESET_BUTTON.md`](../RESET_BUTTON.md)
for the selected prototype hardware, wiring, state model, and validation plan.

The reference Pi Zero 2 W deployment has been tested end to end from a phone:
AP discovery, DHCP, captive DNS/portal, candidate connection, atomic commit,
image synchronization, and reboot from the newly created profile all passed.
