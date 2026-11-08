# Memory Lane provisioner

`astrohud-provisioner` is the no-mobile-app onboarding path for a Memory Lane
frame. On an unprovisioned device, or after an explicit local reset, it:

1. creates a stable random device code and setup password;
2. runs a fresh nearby Wi-Fi scan before starting the AP (first boot has no
   scan cache); a failed scan is logged and manual network entry remains available;
3. starts the protected `Memory-Lane-XXXXXX` access point (existing appliances
   retain their manufacturing-assigned setup name);
4. shows a large Wi-Fi QR code on the frame for one-tap joining;
5. serves a phone-friendly captive portal at `http://10.42.0.1/`;
6. stops the AP and tests the selected household network;
7. atomically commits both the NetworkManager profile and protected backup; or
8. restores the setup AP when the candidate connection fails;
9. asks the owner to reconnect their phone to the internet and tap **Continue
   setup** once the TV confirms Wi-Fi was saved, carrying a private device-bound
   setup link onto the owner's phone; creates a
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

The service skips Wi-Fi setup when a persistent household profile or its
recovery backup exists. Once claimed, it stays running and checks for owner
reset requests every ten seconds. While an online
device is waiting to be claimed, it refreshes expired setup credentials and
keeps the setup QR on the television. To enter setup mode locally:

```sh
sudo astrohud-enter-setup
```

This intentionally disconnects the current Wi-Fi connection. The existing
Wi-Fi guard ignores the interface while
`/var/lib/astrohud/provisioning-required` exists. Successful provisioning
removes the marker; a failed candidate reopens the setup AP.

The connecting page polls the local status endpoint and displays a returned
failure with a retry link. It never redirects merely because a timer elapsed:
losing the setup AP does not prove that the household connection succeeded.
The setup AP disables Wi-Fi power saving, as the saved household profile does.

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
/etc/astrohud/manufacturing.json
/etc/astrohud/reset.json
/var/lib/astrohud/reset-in-progress
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
present, verifies that the server recognizes it, and stays running to watch for
owner reset requests.

## Owner-requested factory reset

The server stores a request until the authenticated appliance accepts it.
The provisioner first persists `/etc/astrohud/reset.json` (0600), containing
fresh replacement credentials and a reset-only receipt. Acceptance revokes
media access. The provisioner then stops the viewer and Wi-Fi guard, removes
cached state and all NetworkManager Wi-Fi connections (including legacy names),
removes protected Wi-Fi backups and interrupted temporary copies, rotates the
setup password/bootstrap/device credential, and returns to QR provisioning.
Ethernet connections and manufacturing identity are preserved.

`/etc/astrohud/manufacturing.json` is the stable identity record, created
automatically from existing `device.json` on upgrade. The rotatable
`device.json` includes a matching copy of its identifiers for compatibility.
The journal is authoritative during reset and allows replay after interruption,
including between the two credential-file writes. The viewer's unit condition
blocks cached slideshow startup while `reset-in-progress` exists.

Forgetting Wi-Fi normally disconnects the device before it can confirm cleanup.
The reset stays **resetting** until Wi-Fi setup succeeds again and the appliance
submits its restricted receipt. The server then releases the previous household
pairing, invalidates the old bootstrap, installs the new device credential hash,
and allows fresh enrollment. Receipt retries after completion cannot reset a
later owner's device. The journal is removed after confirmation.

Update **both systemd units** as well as the provisioner binary, then reload
systemd and restart the provisioner. Do not deploy only the new server and
assume older appliances will wipe themselves. Test on a spare unit before
reassignment: normal reset, unavailable server, unavailable router, service
failure, and power interruption during every journal stage. Automated tests
use temporary directories and fake network cleanup; physical Wi-Fi/service
behavior and power-loss testing have not yet been validated.

## Planned physical reset

The reference frame does not yet have a GPIO reset button. The planned control
uses a medium hold for recoverable Wi-Fi setup and a much longer hold for a
privacy-preserving factory reset. See [`../RESET_BUTTON.md`](../RESET_BUTTON.md)
for the selected prototype hardware, wiring, state model, and validation plan.

The reference Pi Zero 2 W deployment has been tested end to end from a phone:
AP discovery, DHCP, captive DNS/portal, candidate connection, atomic commit,
image synchronization, and reboot from the newly created profile all passed.
