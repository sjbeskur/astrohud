//! Privileged, journaled reset executor. No deletion is performed by the viewer.
use crate::{
    identity::{self, DeviceIdentity},
    network::{NetworkManager, ProvisioningPaths},
    setup_display,
};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{self, Read},
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};

pub const JOURNAL: &str = "/etc/astrohud/reset.json";
const BLOCK: &str = "reset-in-progress";

#[derive(Deserialize)]
struct ResetCommand {
    reset_id: String,
}
#[derive(Deserialize)]
struct ResetResponse {
    reset: Option<ResetCommand>,
}

#[derive(Deserialize, Serialize, PartialEq, Eq)]
enum Stage {
    Prepared,
    Accepted,
    Cleaned,
}

#[derive(Deserialize, Serialize)]
pub struct Plan {
    reset_id: String,
    receipt: String,
    next_identity: DeviceIdentity,
    stage: Stage,
}

pub struct Paths {
    pub config: PathBuf,
    pub state: PathBuf,
    pub connections: PathBuf,
    pub runtime_connections: PathBuf,
}

impl Default for Paths {
    fn default() -> Self {
        Self {
            config: "/etc/astrohud".into(),
            state: "/var/lib/astrohud".into(),
            connections: "/etc/NetworkManager/system-connections".into(),
            runtime_connections: "/run/NetworkManager/system-connections".into(),
        }
    }
}

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(8))
        .timeout_read(Duration::from_secs(15))
        .build()
}

fn endpoint(server: &str, suffix: &str) -> Result<String, String> {
    let base = url::Url::parse(server).map_err(|e| e.to_string())?;
    if !matches!(base.scheme(), "http" | "https") {
        return Err("server URL must use HTTP or HTTPS".into());
    }
    base.join(suffix)
        .map(|u| u.to_string())
        .map_err(|e| e.to_string())
}

pub fn poll(server: &str, device: &DeviceIdentity) -> Result<bool, String> {
    let response = agent()
        .get(&endpoint(server, "/api/beta/device/reset")?)
        .set(
            "Authorization",
            &format!("Bearer {}", device.device_credential),
        )
        .call()
        .map_err(|_| "could not check for device reset".to_owned())?;
    let mut body = Vec::new();
    response
        .into_reader()
        .take(64 * 1024 + 1)
        .read_to_end(&mut body)
        .map_err(|e| e.to_string())?;
    if body.len() > 64 * 1024 {
        return Err("reset response too large".into());
    }
    let response: ResetResponse = serde_json::from_slice(&body).map_err(|e| e.to_string())?;
    if let Some(command) = response.reset {
        // IDs come from the server but are validated before constructing URLs.
        uuid::Uuid::parse_str(&command.reset_id).map_err(|_| "invalid reset ID")?;
        let paths = Paths::default();
        let plan = Plan {
            reset_id: command.reset_id,
            receipt: identity::random_human_string(64).map_err(|e| e.to_string())?,
            next_identity: identity::rotated(device).map_err(|e| e.to_string())?,
            stage: Stage::Prepared,
        };
        identity::atomic_write_json(&paths.config.join("reset.json"), &plan)
            .map_err(|e| e.to_string())?;
        identity::atomic_write_json(&paths.state.join(BLOCK), &"reset requested")
            .map_err(|e| e.to_string())?;
        return Ok(true);
    }
    Ok(false)
}

fn accept(server: &str, plan: &Plan, old_credential: &str) -> Result<(), String> {
    let url = endpoint(
        server,
        &format!("/api/beta/device/resets/{}/accept", plan.reset_id),
    )?;
    let body = serde_json::json!({"receipt": plan.receipt, "next_credential": plan.next_identity.device_credential}).to_string();
    // The receipt authorizes retries if the response to the first acceptance
    // was lost. It cannot read media or claim the device.
    for credential in [old_credential, plan.receipt.as_str()] {
        match agent()
            .post(&url)
            .set("Authorization", &format!("Bearer {credential}"))
            .set("Content-Type", "application/json")
            .send_string(&body)
        {
            Ok(_) => return Ok(()),
            Err(ureq::Error::Status(401, _)) => continue,
            Err(_) => return Err("could not acknowledge device reset; will retry".into()),
        }
    }
    Err("device reset was not authorized".into())
}

pub fn resume(server: &str) -> Result<bool, String> {
    let paths = Paths::default();
    if !Path::new(JOURNAL).exists() {
        return Ok(false);
    }
    let mut plan: Plan = serde_json::from_slice(&fs::read(JOURNAL).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    if plan.stage == Stage::Prepared {
        let old: DeviceIdentity = serde_json::from_slice(
            &fs::read(paths.config.join("device.json")).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        accept(server, &plan, &old.device_credential)?;
        plan.stage = Stage::Accepted;
        identity::atomic_write_json(Path::new(JOURNAL), &plan).map_err(|e| e.to_string())?;
    }
    if plan.stage == Stage::Accepted {
        systemctl(&[
            "stop",
            "astrohud-frame.service",
            "astrohud-wifi-guard.timer",
            "astrohud-wifi-guard.service",
        ])?;
        cleanup(&paths, &plan.next_identity, || {
            NetworkManager::new(ProvisioningPaths::default()).forget_customer_connections()
        })?;
        plan.stage = Stage::Cleaned;
        identity::atomic_write_json(Path::new(JOURNAL), &plan).map_err(|e| e.to_string())?;
    }
    // The renderer restarts with fresh credentials and only the setup card.
    setup_display::write(
        &plan.next_identity,
        Path::new(setup_display::SETUP_SCREEN_PATH),
    )
    .map_err(|e| e.to_string())?;
    remove_if_exists(&paths.state.join(BLOCK)).map_err(|e| e.to_string())?;
    systemctl(&[
        "start",
        "astrohud-frame.service",
        "astrohud-wifi-guard.timer",
    ])?;
    Ok(true)
}

pub fn complete(server: &str) -> Result<(), String> {
    if !Path::new(JOURNAL).exists() {
        return Ok(());
    }
    let plan: Plan = serde_json::from_slice(&fs::read(JOURNAL).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    if plan.stage != Stage::Cleaned {
        return Err("reset cleanup has not completed".into());
    }
    agent()
        .post(&endpoint(
            server,
            &format!("/api/beta/device/resets/{}/complete", plan.reset_id),
        )?)
        .set("Authorization", &format!("Bearer {}", plan.receipt))
        .call()
        .map_err(|_| "could not confirm device reset; will retry after reconnecting".to_owned())?;
    remove_if_exists(Path::new(JOURNAL)).map_err(|e| e.to_string())?;
    fs::File::open("/etc/astrohud")
        .and_then(|f| f.sync_all())
        .map_err(|e| e.to_string())?;
    Ok(())
}

fn systemctl(args: &[&str]) -> Result<(), String> {
    let status = Command::new("systemctl")
        .args(args)
        .status()
        .map_err(|e| e.to_string())?;
    if status.success() {
        Ok(())
    } else {
        Err("could not transition appliance services for reset".into())
    }
}

fn remove_if_exists(path: &Path) -> io::Result<()> {
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.is_dir() => fs::remove_dir_all(path),
        Ok(_) => fs::remove_file(path),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e),
    }
}

// Paths and network actions are injected so crash/retry behavior can be tested
// without touching a developer's Wi-Fi, system services, or appliance files.
fn cleanup(
    paths: &Paths,
    next: &DeviceIdentity,
    forget_network: impl FnOnce() -> Result<(), String>,
) -> Result<(), String> {
    fs::create_dir_all(&paths.state).map_err(|e| e.to_string())?;
    identity::atomic_write_json(&paths.state.join(BLOCK), &"reset in progress")
        .map_err(|e| e.to_string())?;
    for entry in fs::read_dir(&paths.state).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        if entry.file_name() != BLOCK {
            remove_if_exists(&entry.path()).map_err(|e| e.to_string())?;
        }
    }
    fs::write(paths.state.join("provisioning-required"), b"").map_err(|e| e.to_string())?;
    identity::install_rotated(
        &paths.config.join("device.json"),
        &paths.state.join("device-credential"),
        next,
    )
    .map_err(|e| e.to_string())?;
    for directory in [
        &paths.config,
        &paths.connections,
        &paths.runtime_connections,
    ] {
        if !directory.exists() {
            continue;
        }
        for entry in fs::read_dir(directory).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            let name = entry.file_name();
            let name = name.to_string_lossy();
            let is_wifi_profile = if directory != &paths.config && entry.path().is_file() {
                fs::read_to_string(entry.path())
                    .map_err(|e| e.to_string())?
                    .lines()
                    .any(|line| matches!(line.trim(), "type=wifi" | "type=802-11-wireless"))
            } else {
                false
            };
            if is_wifi_profile
                || name.starts_with("wifi-profile.")
                || name.starts_with("device.tmp-")
                || ["astrohud-wifi", "astrohud-candidate", "astrohud-setup"]
                    .iter()
                    .any(|prefix| name.starts_with(prefix))
            {
                remove_if_exists(&entry.path()).map_err(|e| e.to_string())?;
            }
        }
    }
    forget_network()?;
    for directory in [&paths.state, &paths.config, &paths.connections] {
        fs::File::open(directory)
            .and_then(|f| f.sync_all())
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::{PermissionsExt, symlink};

    #[test]
    fn interrupted_cleanup_can_resume_without_old_data_or_identity_loss() {
        let root =
            std::env::temp_dir().join(format!("astrohud-reset-test-{}", uuid::Uuid::new_v4()));
        let paths = Paths {
            config: root.join("config"),
            state: root.join("state"),
            connections: root.join("connections"),
            runtime_connections: root.join("runtime"),
        };
        for path in [
            &paths.config,
            &paths.state,
            &paths.connections,
            &paths.runtime_connections,
        ] {
            fs::create_dir_all(path).unwrap();
        }
        let original = identity::load_or_create(
            &paths.config.join("device.json"),
            &paths.state.join("device-credential"),
        )
        .unwrap();
        let manufacture = fs::read(paths.config.join("manufacturing.json")).unwrap();
        let next = identity::rotated(&original).unwrap();
        fs::write(paths.state.join("manifest.json"), "private manifest").unwrap();
        fs::create_dir(paths.state.join("media")).unwrap();
        fs::write(paths.state.join("media/photo.image"), "private photo").unwrap();
        fs::write(
            paths.config.join("wifi-profile.nmconnection"),
            "private Wi-Fi",
        )
        .unwrap();
        fs::write(
            paths.config.join("wifi-profile.tmp-1"),
            "interrupted Wi-Fi backup",
        )
        .unwrap();
        fs::write(paths.config.join("device.tmp-1"), "old credential").unwrap();
        fs::write(paths.config.join("image-release"), "keep firmware version").unwrap();
        for dir in [&paths.connections, &paths.runtime_connections] {
            for name in [
                "astrohud-wifi.nmconnection",
                "astrohud-candidate.nmconnection",
                "astrohud-setup.nmconnection",
            ] {
                fs::write(dir.join(name), "Wi-Fi secret").unwrap();
            }
        }
        fs::write(
            paths.connections.join("legacy-home.nmconnection"),
            "[connection]\nid=old-home\ntype=802-11-wireless\n[wifi-security]\npsk=private",
        )
        .unwrap();
        fs::write(
            paths.connections.join("ethernet.nmconnection"),
            "[connection]\ntype=ethernet",
        )
        .unwrap();
        let outside = root.join("outside");
        fs::write(&outside, "do not follow symlinks").unwrap();
        symlink(&outside, paths.state.join("linked-photo")).unwrap();
        assert!(
            cleanup(&paths, &next, || Err(
                "simulated interruption removing network".into()
            ))
            .is_err()
        );
        // Replaying an accepted journal uses the same next credentials.
        cleanup(&paths, &next, || Ok(())).unwrap();
        let loaded = identity::load_or_create(
            &paths.config.join("device.json"),
            &paths.state.join("device-credential"),
        )
        .unwrap();
        assert_eq!(loaded.device_id, original.device_id);
        assert_eq!(loaded.device_code, original.device_code);
        assert_eq!(loaded.setup_ssid, original.setup_ssid);
        assert_ne!(loaded.setup_password, original.setup_password);
        assert_ne!(loaded.bootstrap_token, original.bootstrap_token);
        assert_ne!(loaded.device_credential, original.device_credential);
        assert_eq!(loaded.device_credential, next.device_credential);
        assert_eq!(
            fs::read(paths.config.join("manufacturing.json")).unwrap(),
            manufacture
        );
        assert!(!paths.state.join("media").exists());
        assert!(!paths.state.join("manifest.json").exists());
        assert!(!paths.config.join("wifi-profile.nmconnection").exists());
        assert!(!paths.config.join("wifi-profile.tmp-1").exists());
        assert!(!paths.config.join("device.tmp-1").exists());
        assert!(paths.state.join("provisioning-required").exists());
        assert!(paths.state.join(BLOCK).exists());
        assert!(paths.config.join("image-release").exists());
        assert_eq!(
            fs::read_to_string(&outside).unwrap(),
            "do not follow symlinks"
        );
        assert_eq!(fs::read_dir(&paths.connections).unwrap().count(), 1);
        assert!(paths.connections.join("ethernet.nmconnection").is_file());
        assert_eq!(fs::read_dir(&paths.runtime_connections).unwrap().count(), 0);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn reset_journal_is_private_and_contains_only_fresh_secrets() {
        let root = std::env::temp_dir().join(format!(
            "astrohud-reset-journal-test-{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&root).unwrap();
        let original =
            identity::load_or_create(&root.join("device.json"), &root.join("device-credential"))
                .unwrap();
        let plan = Plan {
            reset_id: uuid::Uuid::new_v4().to_string(),
            receipt: identity::random_human_string(64).unwrap(),
            next_identity: identity::rotated(&original).unwrap(),
            stage: Stage::Prepared,
        };
        let path = root.join("reset.json");
        identity::atomic_write_json(&path, &plan).unwrap();
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        let text = fs::read_to_string(&path).unwrap();
        assert!(!text.contains(&original.device_credential));
        assert!(!text.contains(&original.bootstrap_token));
        assert!(!text.contains(&original.setup_password));
        fs::remove_dir_all(root).unwrap();
    }
}
