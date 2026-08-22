mod identity;
mod network;
mod web;

use network::{NetworkManager, ProvisioningPaths};
use std::path::Path;

const IDENTITY_PATH: &str = "/etc/astrohud/device.json";

fn main() {
    if let Err(error) = run() {
        eprintln!("astrohud-provisioner: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let arguments: Vec<_> = std::env::args().skip(1).collect();
    let identity = identity::load_or_create(Path::new(IDENTITY_PATH))
        .map_err(|error| format!("could not load device identity: {error}"))?;
    let paths = ProvisioningPaths::default();

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
        return Ok(());
    }

    paths
        .request_provisioning()
        .map_err(|error| format!("could not persist setup state: {error}"))?;
    let manager = NetworkManager::new(paths);
    let networks = manager.start_setup_ap(&identity)?;
    eprintln!(
        "provisioning access point {} is ready at http://10.42.0.1/",
        identity.setup_ssid
    );
    web::serve(identity, manager, networks)
}
