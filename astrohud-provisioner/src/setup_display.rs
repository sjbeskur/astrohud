use crate::identity::DeviceIdentity;
use font8x8::{BASIC_FONTS, UnicodeFonts};
use qrcode::QrCode;
use qrcode::types::Color;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::Path;

pub const SETUP_SCREEN_PATH: &str = "/var/lib/astrohud/setup-screen.ppm";

const WIDTH: usize = 1280;
const HEIGHT: usize = 720;
const QUIET_ZONE_MODULES: usize = 4;

const BACKGROUND: [u8; 3] = [243, 238, 229];
const CARD: [u8; 3] = [255, 253, 248];
const INK: [u8; 3] = [24, 32, 27];
const MUTED: [u8; 3] = [91, 108, 97];
const ACCENT: [u8; 3] = [23, 95, 69];

pub fn write(identity: &DeviceIdentity, path: &Path) -> Result<(), String> {
    let pixels = render(identity)?;
    atomic_write_ppm(path, &pixels).map_err(|error| error.to_string())
}

pub fn remove(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

fn render(identity: &DeviceIdentity) -> Result<Vec<u8>, String> {
    let mut canvas = Canvas::new(WIDTH, HEIGHT, BACKGROUND);
    canvas.fill_rect(48, 42, WIDTH - 96, HEIGHT - 84, CARD);
    canvas.fill_rect(48, 42, 12, HEIGHT - 84, ACCENT);

    canvas.text(105, 94, 3, "ASTROHUD SETUP", ACCENT);
    canvas.text(105, 170, 5, "SCAN WITH", INK);
    canvas.text(105, 224, 5, "YOUR PHONE", INK);
    canvas.text(105, 325, 2, "1  OPEN YOUR CAMERA", MUTED);
    canvas.text(105, 360, 2, "2  POINT AT THE QR CODE", MUTED);
    canvas.text(105, 395, 2, "3  TAP JOIN", MUTED);
    canvas.text(105, 430, 2, "4  CHOOSE HOME WI-FI", MUTED);

    canvas.text(105, 500, 2, "CAN'T SCAN? JOIN MANUALLY", ACCENT);
    canvas.text(105, 540, 2, &identity.setup_ssid, INK);
    canvas.text(105, 575, 2, &identity.setup_password, INK);
    canvas.text(
        105,
        625,
        1,
        "THE SETUP PAGE SHOULD OPEN AUTOMATICALLY",
        MUTED,
    );

    canvas.qr_code(690, 105, 500, &identity.wifi_qr_payload())?;
    Ok(canvas.pixels)
}

fn atomic_write_ppm(path: &Path, pixels: &[u8]) -> io::Result<()> {
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
    write!(file, "P6\n{WIDTH} {HEIGHT}\n255\n")?;
    file.write_all(pixels)?;
    file.sync_all()?;
    fs::rename(&temporary, path)?;
    File::open(path.parent().unwrap_or(Path::new("/")))?.sync_all()?;
    Ok(())
}

struct Canvas {
    width: usize,
    height: usize,
    pixels: Vec<u8>,
}

impl Canvas {
    fn new(width: usize, height: usize, color: [u8; 3]) -> Self {
        let mut canvas = Self {
            width,
            height,
            pixels: vec![0; width * height * 3],
        };
        canvas.fill_rect(0, 0, width, height, color);
        canvas
    }

    fn fill_rect(&mut self, x: usize, y: usize, width: usize, height: usize, color: [u8; 3]) {
        let x_end = x.saturating_add(width).min(self.width);
        let y_end = y.saturating_add(height).min(self.height);
        for row in y.min(self.height)..y_end {
            for column in x.min(self.width)..x_end {
                let offset = (row * self.width + column) * 3;
                self.pixels[offset..offset + 3].copy_from_slice(&color);
            }
        }
    }

    fn text(&mut self, x: usize, y: usize, scale: usize, text: &str, color: [u8; 3]) {
        let mut cursor = x;
        for character in text.chars() {
            if let Some(glyph) = BASIC_FONTS.get(character) {
                for (row, bits) in glyph.iter().enumerate() {
                    for column in 0..8 {
                        if bits & (1 << column) != 0 {
                            self.fill_rect(
                                cursor + column * scale,
                                y + row * scale,
                                scale,
                                scale,
                                color,
                            );
                        }
                    }
                }
            }
            cursor += 9 * scale;
        }
    }

    fn qr_code(
        &mut self,
        x: usize,
        y: usize,
        available: usize,
        payload: &str,
    ) -> Result<(), String> {
        let code = QrCode::new(payload.as_bytes()).map_err(|error| error.to_string())?;
        let modules = code.width();
        let total_modules = modules + QUIET_ZONE_MODULES * 2;
        let scale = available / total_modules;
        if scale == 0 {
            return Err("setup QR code does not fit the display".to_owned());
        }
        let size = total_modules * scale;
        self.fill_rect(x, y, size, size, [255, 255, 255]);
        for row in 0..modules {
            for column in 0..modules {
                if code[(column, row)] == Color::Dark {
                    self.fill_rect(
                        x + (column + QUIET_ZONE_MODULES) * scale,
                        y + (row + QUIET_ZONE_MODULES) * scale,
                        scale,
                        scale,
                        INK,
                    );
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identity() -> DeviceIdentity {
        DeviceIdentity {
            device_code: "AB23CD".to_owned(),
            setup_ssid: "AstroHUD-AB23CD".to_owned(),
            setup_password: "23456789ABCDEFGH".to_owned(),
        }
    }

    #[test]
    fn setup_card_has_expected_dimensions_and_contrast() {
        let pixels = render(&identity()).expect("render setup screen");
        assert_eq!(pixels.len(), WIDTH * HEIGHT * 3);
        assert!(pixels.chunks_exact(3).any(|pixel| pixel == INK));
        assert!(pixels.chunks_exact(3).any(|pixel| pixel == [255, 255, 255]));
    }

    #[test]
    fn setup_card_is_written_as_a_readable_ppm() {
        let directory =
            std::env::temp_dir().join(format!("astrohud-setup-screen-test-{}", std::process::id()));
        let path = directory.join("setup.ppm");
        write(&identity(), &path).expect("write setup screen");
        let bytes = fs::read(&path).expect("read setup screen");
        assert!(bytes.starts_with(b"P6\n1280 720\n255\n"));
        assert_eq!(
            bytes.len(),
            b"P6\n1280 720\n255\n".len() + WIDTH * HEIGHT * 3
        );
        remove(&path).expect("remove setup screen");
        fs::remove_dir_all(directory).expect("remove test directory");
    }
}
