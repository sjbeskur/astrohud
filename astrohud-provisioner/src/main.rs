mod identity;
mod network;
mod pairing;
mod reset;
mod setup_display;
mod web;

use network::{NetworkManager, ProvisioningPaths};
use std::{path::Path, thread, time::Duration};

const IDENTITY_PATH: &str = "/etc/astrohud/device.json";
const DEVICE_CREDENTIAL_PATH: &str = "/var/lib/astrohud/device-credential";

fn main() {
    if let Err(error) = run() {
        eprintln!("astrohud-provisioner: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let arguments: Vec<_> = std::env::args().skip(1).collect();
    let server_url =
        std::env::var("ASTROHUD_SERVER_URL").unwrap_or_else(|_| "http://127.0.0.1:8080".to_owned());
    if arguments.iter().any(|argument| argument == "--print-label") {
        let identity = load_identity()?;
        println!("Device code: {}", identity.device_code);
        println!("Setup SSID: {}", identity.setup_ssid);
        println!("Wi-Fi QR: {}", identity.wifi_qr_payload());
        return Ok(());
    }
    if arguments.iter().any(|argument| argument == "--enter-setup") {
        ProvisioningPaths::default()
            .request_provisioning()
            .map_err(|e| e.to_string())?;
    }
    loop {
        // Resume before loading credentials: reset can have been interrupted
        // between replacing device.json and the viewer credential file.
        reset::resume(&server_url)?;
        let identity = load_identity()?;
        let paths = ProvisioningPaths::default();
        if paths.provisioning_required() {
            paths.request_provisioning().map_err(|e| e.to_string())?;
            let manager = NetworkManager::new(paths);
            let portal = web::bind()?;
            let networks = manager.start_setup_ap(&identity)?;
            setup_display::write(&identity, Path::new(setup_display::SETUP_SCREEN_PATH))
                .map_err(|e| e.to_string())?;
            eprintln!(
                "provisioning access point {} is ready at http://10.42.0.1/",
                identity.setup_ssid
            );
            web::serve(portal, identity.clone(), manager, networks, &server_url)?;
            setup_display::write_waiting(Path::new(setup_display::SETUP_SCREEN_PATH))?;
        } else if Path::new(setup_display::SETUP_SCREEN_PATH).exists() {
            setup_display::write_waiting(Path::new(setup_display::SETUP_SCREEN_PATH))?;
        }
        // A Wi-Fi wipe disconnects the device. Confirm only after all cleanup
        // has succeeded and a network is available again, before enrolling.
        reset::complete(&server_url)?;
        pairing::ensure_claimed(
            &identity,
            &server_url,
            Path::new(setup_display::SETUP_SCREEN_PATH),
        )?;
        loop {
            match reset::poll(&server_url, &identity) {
                Ok(true) => break,
                Ok(false) => (),
                Err(error) => eprintln!("reset check unavailable: {error}"),
            }
            thread::sleep(Duration::from_secs(10));
        }
    }
}

fn load_identity() -> Result<identity::DeviceIdentity, String> {
    identity::load_or_create(Path::new(IDENTITY_PATH), Path::new(DEVICE_CREDENTIAL_PATH))
        .map_err(|error| format!("could not load device identity: {error}"))
}
