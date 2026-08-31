use astrohud_rest::{
    DeviceEnrollmentState, create_household_owner, initialize_database, register_device,
};
use image::{DynamicImage, GrayImage, ImageFormat, Luma};
use qrcode::{QrCode, types::Color};
use rusqlite::Connection;
use std::env;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use uuid::Uuid;

fn main() {
    if let Err(error) = run() {
        eprintln!("astrohud-admin: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let arguments: Vec<String> = env::args().skip(1).collect();
    let data_dir = env::var_os("ASTROHUD_DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("data"));
    fs::create_dir_all(&data_dir)?;
    let mut database = Connection::open(data_dir.join("astrohud.sqlite3"))?;
    initialize_database(&database)?;

    match arguments.as_slice() {
        [command, household_name] if command == "create-household" => {
            create_household(&mut database, household_name, "Owner", None)
        }
        [command, household_name, owner_label] if command == "create-household" => {
            create_household(&mut database, household_name, owner_label, None)
        }
        [command, household_name, owner_label, activation_card]
            if command == "create-household" =>
        {
            create_household(
                &mut database,
                household_name,
                owner_label,
                Some(Path::new(activation_card)),
            )
        }
        [command, credential_path, device_code] if command == "create-device" => {
            create_device(&database, PathBuf::from(credential_path), device_code)
        }
        _ => Err(usage().into()),
    }
}

fn create_household(
    database: &mut Connection,
    household_name: &str,
    owner_label: &str,
    activation_card: Option<&Path>,
) -> Result<(), Box<dyn std::error::Error>> {
    let owner = create_household_owner(database, household_name, owner_label)?;
    let public_url = env::var("ASTROHUD_PUBLIC_URL")
        .unwrap_or_else(|_| "http://127.0.0.1:8080".to_owned())
        .trim_end_matches('/')
        .to_owned();

    let activation_url = format!("{public_url}/activate.html#{}", owner.owner_token);
    if let Some(path) = activation_card {
        write_activation_card(&activation_url, path)?;
    }

    println!("Household created: {household_name}");
    println!("Private owner activation link:");
    println!("{activation_url}");
    if let Some(path) = activation_card {
        println!("Private activation QR card: {}", path.display());
    }
    println!("Handle this link like a password. It is shown only once.");
    Ok(())
}

fn write_activation_card(url: &str, path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    const QUIET_ZONE: usize = 4;
    const MODULE_SCALE: usize = 12;

    let code = QrCode::new(url.as_bytes())?;
    let modules = code.width();
    let side = (modules + QUIET_ZONE * 2) * MODULE_SCALE;
    let mut image = GrayImage::from_pixel(side as u32, side as u32, Luma([255]));
    for row in 0..modules {
        for column in 0..modules {
            if code[(column, row)] != Color::Dark {
                continue;
            }
            for y in 0..MODULE_SCALE {
                for x in 0..MODULE_SCALE {
                    image.put_pixel(
                        ((column + QUIET_ZONE) * MODULE_SCALE + x) as u32,
                        ((row + QUIET_ZONE) * MODULE_SCALE + y) as u32,
                        Luma([0]),
                    );
                }
            }
        }
    }

    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)?;
    }
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)?;
    DynamicImage::ImageLuma8(image).write_to(&mut output, ImageFormat::Png)?;
    output.sync_all()?;
    Ok(())
}

fn create_device(
    database: &Connection,
    credential_path: PathBuf,
    device_code: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let device_id = Uuid::new_v4().to_string();
    let credential = format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
    let mut credential_file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&credential_path)?;
    writeln!(credential_file, "{credential}")?;
    credential_file.sync_all()?;

    let registration = register_device(database, &device_id, device_code, &credential)?;
    let DeviceEnrollmentState::Pending {
        claim_code,
        expires_at,
    } = registration.state
    else {
        return Err("new device unexpectedly registered as already claimed".into());
    };

    println!("Device created: {device_id}");
    println!("Device code: {device_code}");
    println!("Claim code: {claim_code}");
    println!("Claim before: {expires_at} UTC");
    println!("Credential written to: {}", credential_path.display());
    Ok(())
}

fn usage() -> &'static str {
    "usage:\n\
     astrohud-admin create-household <household-name> [owner-label] [activation-card.png]\n\
     astrohud-admin create-device <credential-output-path> <six-character-device-code>"
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn activation_card_is_a_private_png() {
        let path = std::env::temp_dir().join(format!(
            "astrohud-owner-activation-{}.png",
            std::process::id()
        ));
        write_activation_card(
            "http://192.168.50.144:8080/activate.html#test-private-token",
            &path,
        )
        .expect("write activation card");

        let bytes = fs::read(&path).expect("read activation card");
        let metadata = fs::metadata(&path).expect("activation card metadata");
        assert!(bytes.starts_with(b"\x89PNG\r\n\x1a\n"));
        assert_eq!(metadata.permissions().mode() & 0o777, 0o600);

        fs::remove_file(path).expect("remove activation card");
    }
}
