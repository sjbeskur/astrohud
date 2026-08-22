# AstroHUD provisioner

`astrohud-provisioner` is the no-mobile-app onboarding path for an AstroHUD
frame. On an unprovisioned device, or after an explicit local reset, it:

1. creates a stable random device code and setup password;
2. scans nearby Wi-Fi networks;
3. starts the protected `AstroHUD-XXXXXX` access point;
4. serves a phone-friendly captive portal at `http://10.42.0.1/`;
5. stops the AP and tests the selected household network;
6. atomically commits both the NetworkManager profile and protected backup; or
7. restores the setup AP when the candidate connection fails.

The setup suffix uses uppercase characters without `0/O` or `1/I/L`. The
suffix is an identifier, not a credential. A separate 16-character random
password protects the setup AP and is included in the standard Wi-Fi QR
payload.

The service stays dormant when either a persistent household profile or its
recovery backup exists. To enter setup mode locally:

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
`/etc/astrohud/device.json` and
`/etc/astrohud/wifi-profile.nmconnection` must never be committed or copied
into build artifacts.

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
ensures device identity exists, sees the household profile/backup, and exits.

The reference Pi Zero 2 W deployment has been tested end to end from a phone:
AP discovery, DHCP, captive DNS/portal, candidate connection, atomic commit,
image synchronization, and reboot from the newly created profile all passed.
