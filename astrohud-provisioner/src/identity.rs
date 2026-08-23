use serde::{Deserialize, Serialize};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::Path;

const HUMAN_ALPHABET: &[u8] = b"ABCDEFGHJKMNPQRSTUVWXYZ23456789";

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct DeviceIdentity {
    pub device_code: String,
    pub setup_ssid: String,
    pub setup_password: String,
}

impl DeviceIdentity {
    pub fn wifi_qr_payload(&self) -> String {
        format!(
            "WIFI:T:WPA;S:{};P:{};;",
            self.setup_ssid, self.setup_password
        )
    }
}

pub fn load_or_create(path: &Path) -> io::Result<DeviceIdentity> {
    if path.exists() {
        let bytes = fs::read(path)?;
        return serde_json::from_slice(&bytes).map_err(io::Error::other);
    }

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
        fs::set_permissions(parent, fs::Permissions::from_mode(0o700))?;
    }

    let device_code = random_human_string(6)?;
    let identity = DeviceIdentity {
        setup_ssid: format!("AstroHUD-{device_code}"),
        setup_password: random_human_string(16)?,
        device_code,
    };
    atomic_write_json(path, &identity)?;
    Ok(identity)
}

fn random_human_string(length: usize) -> io::Result<String> {
    let mut random = File::open("/dev/urandom")?;
    let acceptance_limit = (u8::MAX as usize + 1) / HUMAN_ALPHABET.len() * HUMAN_ALPHABET.len();
    let mut result = String::with_capacity(length);
    let mut byte = [0_u8; 1];

    while result.len() < length {
        random.read_exact(&mut byte)?;
        if usize::from(byte[0]) < acceptance_limit {
            result.push(HUMAN_ALPHABET[usize::from(byte[0]) % HUMAN_ALPHABET.len()] as char);
        }
    }
    Ok(result)
}

fn atomic_write_json(path: &Path, value: &DeviceIdentity) -> io::Result<()> {
    let temporary = path.with_extension(format!("tmp-{}", std::process::id()));
    let bytes = serde_json::to_vec_pretty(value).map_err(io::Error::other)?;
    let mut file = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .mode(0o600)
        .open(&temporary)?;
    file.set_permissions(fs::Permissions::from_mode(0o600))?;
    file.write_all(&bytes)?;
    file.write_all(b"\n")?;
    file.sync_all()?;
    fs::rename(&temporary, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_identity_is_readable_and_stable() {
        let directory = std::env::temp_dir().join(format!(
            "astrohud-identity-test-{}-{}",
            std::process::id(),
            std::thread::current().name().unwrap_or("unnamed")
        ));
        let path = directory.join("device.json");
        let first = load_or_create(&path).expect("create identity");
        let second = load_or_create(&path).expect("reload identity");

        assert_eq!(first.device_code, second.device_code);
        assert_eq!(first.setup_password, second.setup_password);
        assert_eq!(first.device_code.len(), 6);
        assert_eq!(first.setup_password.len(), 16);
        assert!(
            first
                .device_code
                .bytes()
                .all(|byte| HUMAN_ALPHABET.contains(&byte))
        );
        assert!(!first.device_code.contains(['0', 'O', '1', 'I', 'L']));

        fs::remove_dir_all(directory).expect("remove test directory");
    }
}
