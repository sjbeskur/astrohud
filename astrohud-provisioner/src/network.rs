use crate::identity::DeviceIdentity;
use serde::Serialize;
use std::collections::HashMap;
use std::ffi::OsStr;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

const MAIN_CONNECTION: &str = "astrohud-wifi";
const CANDIDATE_CONNECTION: &str = "astrohud-candidate";
const SETUP_CONNECTION: &str = "astrohud-setup";
const INTERFACE: &str = "wlan0";
const SETUP_PROFILE: &str = "/run/NetworkManager/system-connections/astrohud-setup.nmconnection";

#[derive(Clone, Debug)]
pub struct ProvisioningPaths {
    pub marker: PathBuf,
    pub profile: PathBuf,
    pub backup: PathBuf,
}

impl Default for ProvisioningPaths {
    fn default() -> Self {
        Self {
            marker: PathBuf::from("/var/lib/astrohud/provisioning-required"),
            profile: PathBuf::from(
                "/etc/NetworkManager/system-connections/astrohud-wifi.nmconnection",
            ),
            backup: PathBuf::from("/etc/astrohud/wifi-profile.nmconnection"),
        }
    }
}

impl ProvisioningPaths {
    pub fn provisioning_required(&self) -> bool {
        self.marker.exists() || (!self.profile.exists() && !self.backup.exists())
    }

    pub fn request_provisioning(&self) -> io::Result<()> {
        if let Some(parent) = self.marker.parent() {
            fs::create_dir_all(parent)?;
        }
        OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .mode(0o600)
            .open(&self.marker)?;
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct WifiNetwork {
    pub ssid: String,
    pub signal: u8,
    pub security: String,
}

#[derive(Clone, Debug)]
pub struct NetworkManager {
    paths: ProvisioningPaths,
}

impl NetworkManager {
    pub fn new(paths: ProvisioningPaths) -> Self {
        Self { paths }
    }

    pub fn scan(&self) -> Result<Vec<WifiNetwork>, String> {
        let output = nmcli_output([
            "--terse",
            "--escape",
            "yes",
            "--fields",
            "SSID,SIGNAL,SECURITY",
            "device",
            "wifi",
            "list",
            "ifname",
            INTERFACE,
            "--rescan",
            "yes",
        ])?;
        if !output.status.success() {
            return Err(command_error(&output));
        }
        parse_scan(&String::from_utf8_lossy(&output.stdout))
    }

    pub fn start_setup_ap(&self, identity: &DeviceIdentity) -> Result<Vec<WifiNetwork>, String> {
        let networks = self.scan().unwrap_or_default();
        let _ = nmcli(["connection", "delete", SETUP_CONNECTION]);
        write_setup_profile(identity).map_err(display_error)?;
        nmcli(["connection", "load", SETUP_PROFILE])?;
        nmcli(["--wait", "20", "connection", "up", SETUP_CONNECTION])?;
        Ok(networks)
    }

    pub fn apply_candidate(
        &self,
        identity: &DeviceIdentity,
        ssid: &str,
        password: &str,
    ) -> Result<(), String> {
        let _ = nmcli(["connection", "delete", CANDIDATE_CONNECTION]);
        let _ = nmcli(["connection", "down", SETUP_CONNECTION]);

        if let Err(error) = connect_candidate(ssid, password) {
            let _ = nmcli(["connection", "delete", CANDIDATE_CONNECTION]);
            let _ = self.start_setup_ap(identity);
            return Err(format!("could not join that network: {error}"));
        }

        if let Err(error) = self.commit_candidate() {
            let _ = self.restore_previous_profile();
            let _ = nmcli(["connection", "delete", CANDIDATE_CONNECTION]);
            let _ = self.start_setup_ap(identity);
            return Err(format!("joined Wi-Fi but could not save it: {error}"));
        }
        Ok(())
    }

    fn commit_candidate(&self) -> Result<(), String> {
        let _ = nmcli(["connection", "delete", MAIN_CONNECTION]);
        nmcli([
            "connection",
            "modify",
            CANDIDATE_CONNECTION,
            "connection.id",
            MAIN_CONNECTION,
            "connection.autoconnect",
            "yes",
            "connection.autoconnect-retries",
            "0",
            "802-11-wireless.powersave",
            "2",
        ])?;

        let candidate = self
            .paths
            .profile
            .with_file_name("astrohud-candidate.nmconnection");
        if !candidate.exists() {
            return Err(format!(
                "candidate keyfile was not created at {}",
                candidate.display()
            ));
        }
        if self.paths.profile.exists() {
            fs::remove_file(&self.paths.profile).map_err(display_error)?;
        }
        fs::rename(&candidate, &self.paths.profile).map_err(display_error)?;
        fs::set_permissions(&self.paths.profile, fs::Permissions::from_mode(0o600))
            .map_err(display_error)?;
        nmcli(["connection", "reload"])?;
        atomic_copy(&self.paths.profile, &self.paths.backup).map_err(display_error)?;
        if self.paths.marker.exists() {
            fs::remove_file(&self.paths.marker).map_err(display_error)?;
        }
        let _ = nmcli(["connection", "delete", SETUP_CONNECTION]);
        Ok(())
    }

    fn restore_previous_profile(&self) -> Result<(), String> {
        if !self.paths.backup.exists() {
            return Ok(());
        }
        atomic_copy(&self.paths.backup, &self.paths.profile).map_err(display_error)?;
        nmcli(["connection", "reload"])
    }
}

fn connect_candidate(ssid: &str, password: &str) -> Result<(), String> {
    let mut command = Command::new("nmcli");
    if !password.is_empty() {
        command.arg("--ask");
    }
    command.args([
        "--wait",
        "35",
        "device",
        "wifi",
        "connect",
        ssid,
        "ifname",
        INTERFACE,
        "name",
        CANDIDATE_CONNECTION,
    ]);
    command
        .stdin(if password.is_empty() {
            Stdio::null()
        } else {
            Stdio::piped()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = command.spawn().map_err(display_error)?;
    if !password.is_empty() {
        let mut input = child
            .stdin
            .take()
            .ok_or_else(|| "could not open NetworkManager secret input".to_owned())?;
        input
            .write_all(format!("{password}\n").as_bytes())
            .map_err(display_error)?;
    }
    let output = child.wait_with_output().map_err(display_error)?;
    if output.status.success() {
        Ok(())
    } else {
        Err(command_error(&output))
    }
}

fn write_setup_profile(identity: &DeviceIdentity) -> io::Result<()> {
    let uuid = fs::read_to_string("/proc/sys/kernel/random/uuid")?;
    let profile = format!(
        "[connection]\nid={SETUP_CONNECTION}\nuuid={}\ntype=wifi\ninterface-name={INTERFACE}\nautoconnect=false\n\n[wifi]\nmode=ap\nband=bg\nssid={}\n\n[wifi-security]\nkey-mgmt=wpa-psk\npsk={}\n\n[ipv4]\naddress1=10.42.0.1/24\nmethod=shared\n\n[ipv6]\nmethod=disabled\n",
        uuid.trim(),
        identity.setup_ssid,
        identity.setup_password
    );
    if let Some(parent) = Path::new(SETUP_PROFILE).parent() {
        fs::create_dir_all(parent)?;
    }
    let mut file = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .mode(0o600)
        .open(SETUP_PROFILE)?;
    file.set_permissions(fs::Permissions::from_mode(0o600))?;
    file.write_all(profile.as_bytes())?;
    file.sync_all()?;
    Ok(())
}

fn nmcli<I, S>(arguments: I) -> Result<(), String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let output = nmcli_output(arguments)?;
    if output.status.success() {
        Ok(())
    } else {
        Err(command_error(&output))
    }
}

fn nmcli_output<I, S>(arguments: I) -> Result<Output, String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    Command::new("nmcli")
        .args(arguments)
        .output()
        .map_err(display_error)
}

fn command_error(output: &Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
    if stderr.is_empty() {
        format!("NetworkManager exited with {}", output.status)
    } else {
        stderr
    }
}

fn display_error(error: impl std::fmt::Display) -> String {
    error.to_string()
}

fn parse_scan(output: &str) -> Result<Vec<WifiNetwork>, String> {
    let mut networks: HashMap<String, WifiNetwork> = HashMap::new();
    for line in output.lines() {
        let fields = split_nmcli_fields(line);
        if fields.len() != 3 || fields[0].is_empty() {
            continue;
        }
        let signal = fields[1]
            .parse::<u8>()
            .map_err(|error| format!("invalid signal strength: {error}"))?;
        let candidate = WifiNetwork {
            ssid: fields[0].clone(),
            signal,
            security: fields[2].clone(),
        };
        networks
            .entry(candidate.ssid.clone())
            .and_modify(|existing| {
                if candidate.signal > existing.signal {
                    *existing = candidate.clone();
                }
            })
            .or_insert(candidate);
    }

    let mut networks: Vec<_> = networks.into_values().collect();
    networks.sort_by(|left, right| {
        right
            .signal
            .cmp(&left.signal)
            .then_with(|| left.ssid.cmp(&right.ssid))
    });
    Ok(networks)
}

fn split_nmcli_fields(line: &str) -> Vec<String> {
    let mut fields = vec![String::new()];
    let mut escaped = false;
    for character in line.chars() {
        if escaped {
            fields.last_mut().expect("one field").push(character);
            escaped = false;
        } else if character == '\\' {
            escaped = true;
        } else if character == ':' && fields.len() < 3 {
            fields.push(String::new());
        } else {
            fields.last_mut().expect("one field").push(character);
        }
    }
    if escaped {
        fields.last_mut().expect("one field").push('\\');
    }
    fields
}

fn atomic_copy(source: &Path, destination: &Path) -> io::Result<()> {
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)?;
        fs::set_permissions(parent, fs::Permissions::from_mode(0o700))?;
    }
    let temporary = destination.with_extension(format!("tmp-{}", std::process::id()));
    let bytes = fs::read(source)?;
    let mut output = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .mode(0o600)
        .open(&temporary)?;
    output.write_all(&bytes)?;
    output.sync_all()?;
    fs::rename(&temporary, destination)?;
    File::open(destination.parent().unwrap_or(Path::new("/")))?.sync_all()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scan_parser_handles_escaped_ssids_and_duplicates() {
        let networks = parse_scan(
            "Grandma\\:Kitchen:72:WPA2\nGrandma\\:Kitchen:80:WPA2\nBack\\\\Hall:55:WPA3\n",
        )
        .expect("parse scan");

        assert_eq!(networks.len(), 2);
        assert_eq!(networks[0].ssid, "Grandma:Kitchen");
        assert_eq!(networks[0].signal, 80);
        assert_eq!(networks[1].ssid, "Back\\Hall");
    }

    #[test]
    fn existing_backup_prevents_automatic_setup_mode() {
        let directory =
            std::env::temp_dir().join(format!("astrohud-path-test-{}", std::process::id()));
        fs::create_dir_all(&directory).expect("create directory");
        let paths = ProvisioningPaths {
            marker: directory.join("marker"),
            profile: directory.join("profile"),
            backup: directory.join("backup"),
        };

        assert!(paths.provisioning_required());
        fs::write(&paths.backup, "backup").expect("write backup");
        assert!(!paths.provisioning_required());
        paths.request_provisioning().expect("write marker");
        assert!(paths.provisioning_required());

        fs::remove_dir_all(directory).expect("remove directory");
    }
}
