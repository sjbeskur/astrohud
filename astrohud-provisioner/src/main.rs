mod identity;
mod network;
mod pairing;
mod setup_display;
mod web;

use network::{NetworkManager, ProvisioningPaths};
use std::path::Path;

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
    let identity =
        identity::load_or_create(Path::new(IDENTITY_PATH), Path::new(DEVICE_CREDENTIAL_PATH))
            .map_err(|error| format!("could not load device identity: {error}"))?;
    let paths = ProvisioningPaths::default();
    let server_url =
        std::env::var("ASTROHUD_SERVER_URL").unwrap_or_else(|_| "http://127.0.0.1:8080".to_owned());

    if arguments.iter().any(|argument| argument == "--print-label") {
        println!("Device code: {}", identity.device_code);
        println!("Setup SSID: {}", identity.setup_ssid);
        println!("Wi-Fi QR: {}", identity.wifi_qr_payload());
        return Ok(());
    }
    if arguments.iter().any(|argument| argument == "--enter-setup") {
        paths
            .request_provisioning()
            .map_err(|error| format!("could not request setup mode: {error}"))?;
    }
    if !paths.provisioning_required() {
        setup_display::remove(Path::new(setup_display::SETUP_SCREEN_PATH))
            .map_err(|error| format!("could not remove stale setup screen: {error}"))?;
    } else {
        paths
            .request_provisioning()
            .map_err(|error| format!("could not persist setup state: {error}"))?;
        let manager = NetworkManager::new(paths);
        let networks = manager.start_setup_ap(&identity)?;
        setup_display::write(&identity, Path::new(setup_display::SETUP_SCREEN_PATH))
            .map_err(|error| format!("could not create setup screen: {error}"))?;
        eprintln!(
            "provisioning access point {} is ready at http://10.42.0.1/",
            identity.setup_ssid
        );
        web::serve(identity.clone(), manager, networks, &server_url)?;
        setup_display::remove(Path::new(setup_display::SETUP_SCREEN_PATH))
            .map_err(|error| format!("could not remove setup screen: {error}"))?;
    }

    pairing::ensure_claimed(
        &identity,
        &server_url,
        Path::new(setup_display::SETUP_SCREEN_PATH),
    )
}
