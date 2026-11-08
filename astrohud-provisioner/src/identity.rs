use serde::{Deserialize, Serialize};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::Path;
use uuid::Uuid;

const HUMAN_ALPHABET: &[u8] = b"ABCDEFGHJKMNPQRSTUVWXYZ23456789";

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct DeviceIdentity {
    pub device_code: String,
    pub setup_ssid: String,
    pub setup_password: String,
    #[serde(default)]
    pub device_id: String,
    #[serde(default)]
    pub device_credential: String,
    #[serde(default)]
    pub bootstrap_token: String,
}

#[derive(Deserialize, Serialize, PartialEq, Eq)]
struct ManufacturingIdentity {
    device_id: String,
    device_code: String,
    setup_ssid: String,
}

pub fn rotated(identity: &DeviceIdentity) -> io::Result<DeviceIdentity> {
    Ok(DeviceIdentity {
        device_id: identity.device_id.clone(),
        device_code: identity.device_code.clone(),
        setup_ssid: identity.setup_ssid.clone(),
        setup_password: random_human_string(16)?,
        device_credential: random_human_string(64)?,
        bootstrap_token: random_human_string(64)?,
    })
}

pub fn install_rotated(
    path: &Path,
    credential_path: &Path,
    identity: &DeviceIdentity,
) -> io::Result<()> {
    // The reset journal is authoritative until both writes have completed.
    atomic_write_json(path, identity)?;
    match fs::remove_file(credential_path) {
        Ok(()) => (),
        Err(error) if error.kind() == io::ErrorKind::NotFound => (),
        Err(error) => return Err(error),
    }
    write_credential_if_missing(credential_path, &identity.device_credential)
}

impl DeviceIdentity {
    pub fn wifi_qr_payload(&self) -> String {
        format!(
            "WIFI:T:WPA;S:{};P:{};;",
            self.setup_ssid, self.setup_password
        )
    }
}

pub fn load_or_create(path: &Path, credential_path: &Path) -> io::Result<DeviceIdentity> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
        fs::set_permissions(parent, fs::Permissions::from_mode(0o700))?;
    }

    let manufacturing_path = path.with_file_name("manufacturing.json");
    let saved_manufacturing: Option<ManufacturingIdentity> = match fs::read(&manufacturing_path) {
        Ok(bytes) => Some(serde_json::from_slice(&bytes).map_err(io::Error::other)?),
        Err(error) if error.kind() == io::ErrorKind::NotFound => None,
        Err(error) => return Err(error),
    };
    let existing_credential = read_credential(credential_path)?;
    let (mut identity, mut changed) = if path.exists() {
        let bytes = fs::read(path)?;
        (
            serde_json::from_slice::<DeviceIdentity>(&bytes).map_err(io::Error::other)?,
            false,
        )
    } else {
        let device_code = match &saved_manufacturing {
            Some(saved) => saved.device_code.clone(),
            None => random_human_string(6)?,
        };
        (
            DeviceIdentity {
                setup_ssid: saved_manufacturing
                    .as_ref()
                    .map(|s| s.setup_ssid.clone())
                    .unwrap_or_else(|| format!("Memory-Lane-{device_code}")),
                setup_password: random_human_string(16)?,
                device_code,
                device_id: saved_manufacturing
                    .as_ref()
                    .map(|s| s.device_id.clone())
                    .unwrap_or_else(|| Uuid::new_v4().to_string()),
                device_credential: existing_credential
                    .clone()
                    .unwrap_or(random_human_string(64)?),
                bootstrap_token: random_human_string(64)?,
            },
            true,
        )
    };

    if identity.device_id.is_empty() {
        identity.device_id = Uuid::new_v4().to_string();
        changed = true;
    }
    if identity.device_credential.is_empty() {
        identity.device_credential = existing_credential
            .clone()
            .unwrap_or(random_human_string(64)?);
        changed = true;
    }
    if identity.bootstrap_token.is_empty() {
        identity.bootstrap_token = random_human_string(64)?;
        changed = true;
    }
    validate_credential(&identity.device_credential)?;
    validate_credential(&identity.bootstrap_token)?;
    if existing_credential
        .as_deref()
        .is_some_and(|existing| existing != identity.device_credential)
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "device identity and frame credential do not match",
        ));
    }

    if changed {
        atomic_write_json(path, &identity)?;
    }
    let manufacturing = ManufacturingIdentity {
        device_id: identity.device_id.clone(),
        device_code: identity.device_code.clone(),
        setup_ssid: identity.setup_ssid.clone(),
    };
    if manufacturing_path.exists() {
        let saved: ManufacturingIdentity =
            serde_json::from_slice(&fs::read(&manufacturing_path)?).map_err(io::Error::other)?;
        if saved != manufacturing {
            return Err(io::Error::other(
                "manufacturing identity does not match device identity",
            ));
        }
    } else {
        atomic_write_json(&manufacturing_path, &manufacturing)?;
    }
    write_credential_if_missing(credential_path, &identity.device_credential)?;
    Ok(identity)
}

fn read_credential(path: &Path) -> io::Result<Option<String>> {
    match fs::read_to_string(path) {
        Ok(value) => {
            let value = value.trim().to_owned();
            validate_credential(&value)?;
            Ok(Some(value))
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

fn validate_credential(value: &str) -> io::Result<()> {
    if !(32..=512).contains(&value.len()) || value.chars().any(char::is_whitespace) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid device credential",
        ));
    }
    Ok(())
}

fn write_credential_if_missing(path: &Path, credential: &str) -> io::Result<()> {
    if path.exists() {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let temporary = path.with_extension(format!("tmp-{}", std::process::id()));
    let mut file = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .mode(0o640)
        .open(&temporary)?;
    file.set_permissions(fs::Permissions::from_mode(0o640))?;
    file.write_all(credential.as_bytes())?;
    file.write_all(b"\n")?;
    file.sync_all()?;
    fs::rename(&temporary, path)?;
    File::open(path.parent().unwrap_or(Path::new("/")))?.sync_all()?;
    Ok(())
}

pub(crate) fn random_human_string(length: usize) -> io::Result<String> {
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

pub(crate) fn atomic_write_json(path: &Path, value: &impl Serialize) -> io::Result<()> {
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
    File::open(path.parent().unwrap_or(Path::new("/")))?.sync_all()?;
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
        let credential_path = directory.join("device-credential");
        let first = load_or_create(&path, &credential_path).expect("create identity");
        let second = load_or_create(&path, &credential_path).expect("reload identity");

        assert_eq!(first.device_code, second.device_code);
        assert_eq!(first.setup_password, second.setup_password);
        assert_eq!(first.device_id, second.device_id);
        assert_eq!(first.device_credential, second.device_credential);
        assert_eq!(first.bootstrap_token, second.bootstrap_token);
        assert_eq!(first.device_code.len(), 6);
        assert_eq!(first.setup_password.len(), 16);
        assert_eq!(first.device_credential.len(), 64);
        assert_eq!(first.bootstrap_token.len(), 64);
        assert_eq!(
            fs::read_to_string(&credential_path)
                .expect("read credential")
                .trim(),
            first.device_credential
        );
        assert!(
            first
                .device_code
                .bytes()
                .all(|byte| HUMAN_ALPHABET.contains(&byte))
        );
        assert!(!first.device_code.contains(['0', 'O', '1', 'I', 'L']));

        fs::remove_dir_all(directory).expect("remove test directory");
    }

    #[test]
    fn manufacturing_identity_survives_recreating_rotatable_credentials() {
        let directory =
            std::env::temp_dir().join(format!("astrohud-manufacturing-test-{}", Uuid::new_v4()));
        let path = directory.join("device.json");
        let credential = directory.join("device-credential");
        let before = load_or_create(&path, &credential).unwrap();
        fs::remove_file(&path).unwrap();
        fs::remove_file(&credential).unwrap();
        let after = load_or_create(&path, &credential).unwrap();
        assert_eq!(before.device_id, after.device_id);
        assert_eq!(before.device_code, after.device_code);
        assert_eq!(before.setup_ssid, after.setup_ssid);
        assert_ne!(before.device_credential, after.device_credential);
        assert_ne!(before.setup_password, after.setup_password);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn legacy_identity_adopts_an_existing_frame_credential() {
        let directory = std::env::temp_dir().join(format!(
            "astrohud-legacy-identity-test-{}",
            std::process::id()
        ));
        fs::create_dir_all(&directory).expect("create directory");
        let identity_path = directory.join("device.json");
        let credential_path = directory.join("device-credential");
        let credential = "existing-device-credential-abcdefghijklmnopqrstuvwxyz0123456789";
        fs::write(
            &identity_path,
            br#"{
              "device_code": "AB23CD",
              "setup_ssid": "AstroHUD-AB23CD",
              "setup_password": "23456789ABCDEFGH"
            }"#,
        )
        .expect("write legacy identity");
        fs::write(&credential_path, credential).expect("write credential");

        let identity = load_or_create(&identity_path, &credential_path).expect("migrate identity");
        assert_eq!(identity.device_credential, credential);
        assert!(Uuid::parse_str(&identity.device_id).is_ok());

        fs::remove_dir_all(directory).expect("remove test directory");
    }
}
