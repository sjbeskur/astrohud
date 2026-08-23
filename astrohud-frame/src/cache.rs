use crate::BoxError;
use crate::model::{CachedPhoto, FrameManifest, Photo, Playlist};
use image::{ImageFormat, ImageReader};
use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{self, BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};

const MANIFEST_FILE: &str = "manifest.json";
const MAX_DOWNLOAD_BYTES: u64 = 25 * 1024 * 1024;
// A malformed or unexpectedly huge source must not exhaust a 512 MiB Pi while
// the image crate allocates its decoded pixel buffer.
const MAX_DECODED_PIXELS: u64 = 20_000_000;

pub struct CacheStore {
    root: PathBuf,
    media_dir: PathBuf,
    max_bytes: u64,
}

impl CacheStore {
    pub fn open(root: impl Into<PathBuf>, max_bytes: u64) -> Result<Self, BoxError> {
        let root = root.into();
        let media_dir = root.join("media");
        fs::create_dir_all(&media_dir)?;

        for entry in fs::read_dir(&media_dir)? {
            let path = entry?.path();
            if path
                .extension()
                .is_some_and(|extension| extension == "part")
            {
                let _ = fs::remove_file(path);
            }
        }

        Ok(Self {
            root,
            media_dir,
            max_bytes,
        })
    }

    pub fn load_manifest(&self) -> Result<Option<FrameManifest>, BoxError> {
        let path = self.root.join(MANIFEST_FILE);
        match File::open(path) {
            Ok(file) => Ok(Some(serde_json::from_reader(BufReader::new(file))?)),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    pub fn save_manifest(&self, manifest: &FrameManifest) -> Result<(), BoxError> {
        let temporary = self.root.join("manifest.json.part");
        let result = (|| -> Result<(), BoxError> {
            let file = File::create(&temporary)?;
            let mut writer = BufWriter::new(file);
            serde_json::to_writer(&mut writer, manifest)?;
            writer.flush()?;
            writer.get_ref().sync_all()?;
            fs::rename(&temporary, self.root.join(MANIFEST_FILE))?;
            Ok(())
        })();
        if result.is_err() {
            let _ = fs::remove_file(temporary);
        }
        result
    }

    pub fn contains(&self, photo: &Photo) -> bool {
        self.photo_path(photo).is_ok_and(|path| path.is_file())
    }

    pub fn install(&self, photo: &Photo, reader: impl Read) -> Result<PathBuf, BoxError> {
        if !photo.is_supported() {
            return Err(format!("unsupported image type: {}", photo.mime_type).into());
        }

        let destination = self.photo_path(photo)?;
        let temporary = destination.with_extension("part");
        let result = (|| -> Result<PathBuf, BoxError> {
            let file = File::create(&temporary)?;
            let mut writer = BufWriter::new(file);
            let mut limited = reader.take(MAX_DOWNLOAD_BYTES + 1);
            let bytes_written = io::copy(&mut limited, &mut writer)?;
            if bytes_written > MAX_DOWNLOAD_BYTES {
                return Err("downloaded image exceeds 25 MiB".into());
            }
            writer.flush()?;
            writer.get_ref().sync_all()?;
            drop(writer);

            validate_image(&temporary)?;
            fs::rename(&temporary, &destination)?;
            Ok(destination.clone())
        })();
        if result.is_err() {
            let _ = fs::remove_file(temporary);
        }
        result
    }

    pub fn total_bytes(&self) -> Result<u64, BoxError> {
        let mut total = 0_u64;
        for entry in fs::read_dir(&self.media_dir)? {
            let entry = entry?;
            if is_cached_image(&entry.path()) {
                total = total.saturating_add(entry.metadata()?.len());
            }
        }
        Ok(total)
    }

    /// Keeps the newest photos that fit. The first photo is retained even when
    /// it alone exceeds the configured cap, ensuring the frame can show at
    /// least one image.
    pub fn reconcile(&self, manifest: &FrameManifest) -> Result<Playlist, BoxError> {
        let mut retained = HashSet::new();
        let mut photos = Vec::new();
        let mut used = 0_u64;

        for photo in manifest.photos.iter().filter(|photo| photo.is_supported()) {
            let path = self.photo_path(photo)?;
            let Ok(metadata) = fs::metadata(&path) else {
                continue;
            };
            let size = metadata.len();
            let fits = used.saturating_add(size) <= self.max_bytes;
            if !fits && !photos.is_empty() {
                continue;
            }
            used = used.saturating_add(size);
            retained.insert(path.clone());
            photos.push(CachedPhoto {
                id: photo.id.clone(),
                path,
                location_label: photo.location_label.clone(),
            });
        }

        for entry in fs::read_dir(&self.media_dir)? {
            let path = entry?.path();
            if is_cached_image(&path) && !retained.contains(&path) {
                fs::remove_file(path)?;
            }
        }

        Ok(Playlist {
            place_name: manifest.place_name.clone(),
            photos,
        })
    }

    pub fn cached_playlist(&self) -> Result<Playlist, BoxError> {
        match self.load_manifest()? {
            Some(manifest) => self.reconcile(&manifest),
            None => Ok(Playlist {
                place_name: "AstroHUD".to_owned(),
                photos: Vec::new(),
            }),
        }
    }

    fn photo_path(&self, photo: &Photo) -> Result<PathBuf, BoxError> {
        validate_photo_id(&photo.id)?;
        Ok(self.media_dir.join(format!("{}.image", photo.id)))
    }
}

fn validate_photo_id(id: &str) -> Result<(), BoxError> {
    let valid = !id.is_empty()
        && id.len() <= 128
        && id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'));
    if valid {
        Ok(())
    } else {
        Err("manifest contains an invalid photo ID".into())
    }
}

fn validate_image(path: &Path) -> Result<(), BoxError> {
    let reader = ImageReader::open(path)?.with_guessed_format()?;
    let format = reader
        .format()
        .ok_or("image format could not be detected")?;
    if !matches!(format, ImageFormat::Jpeg | ImageFormat::Png) {
        return Err("frame supports JPEG and PNG images only".into());
    }
    let (width, height) = reader.into_dimensions()?;
    if width == 0
        || height == 0
        || u64::from(width).saturating_mul(u64::from(height)) > MAX_DECODED_PIXELS
    {
        return Err("image dimensions exceed the frame safety limit".into());
    }
    Ok(())
}

fn is_cached_image(path: &Path) -> bool {
    path.extension()
        .is_some_and(|extension| extension == "image")
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{DynamicImage, ImageFormat, RgbaImage};
    use std::io::Cursor;
    use std::time::{SystemTime, UNIX_EPOCH};

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new() -> Self {
            let unique = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system clock")
                .as_nanos();
            let path = std::env::temp_dir().join(format!(
                "astrohud-frame-test-{}-{unique}",
                std::process::id()
            ));
            fs::create_dir_all(&path).expect("create test directory");
            Self(path)
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn png_bytes(width: u32, height: u32) -> Vec<u8> {
        let image = DynamicImage::ImageRgba8(RgbaImage::new(width, height));
        let mut bytes = Cursor::new(Vec::new());
        image
            .write_to(&mut bytes, ImageFormat::Png)
            .expect("encode png");
        bytes.into_inner()
    }

    fn photo(id: &str) -> Photo {
        Photo {
            id: id.to_owned(),
            channel_id: "family".to_owned(),
            url: format!("/media/{id}.png"),
            mime_type: "image/png".to_owned(),
            created_at: "2026-08-22 00:00:00".to_owned(),
            location_label: None,
        }
    }

    #[test]
    fn manifest_and_images_survive_reopening_the_cache() {
        let directory = TestDirectory::new();
        let store = CacheStore::open(&directory.0, 1024 * 1024).expect("open cache");
        let photo = photo("photo-1");
        store
            .install(&photo, Cursor::new(png_bytes(2, 2)))
            .expect("install image");
        let manifest = FrameManifest {
            frame_id: "demo-frame".to_owned(),
            place_name: "Kitchen".to_owned(),
            photos: vec![photo],
        };
        store.save_manifest(&manifest).expect("save manifest");

        let reopened = CacheStore::open(&directory.0, 1024 * 1024).expect("reopen cache");
        let playlist = reopened.cached_playlist().expect("read cached playlist");
        assert_eq!(playlist.place_name, "Kitchen");
        assert_eq!(playlist.photos.len(), 1);
        assert!(playlist.photos[0].path.is_file());
    }

    #[test]
    fn unsafe_manifest_ids_cannot_escape_the_cache() {
        let directory = TestDirectory::new();
        let store = CacheStore::open(&directory.0, 1024).expect("open cache");
        let result = store.install(&photo("../outside"), Cursor::new(png_bytes(1, 1)));
        assert!(result.is_err());
    }

    #[test]
    fn byte_cap_prefers_the_newest_photo() {
        let directory = TestDirectory::new();
        let store = CacheStore::open(&directory.0, 1).expect("open cache");
        let newest = photo("newest");
        let older = photo("older");
        store
            .install(&newest, Cursor::new(png_bytes(2, 2)))
            .expect("install newest");
        store
            .install(&older, Cursor::new(png_bytes(2, 2)))
            .expect("install older");
        let manifest = FrameManifest {
            frame_id: "demo-frame".to_owned(),
            place_name: "Kitchen".to_owned(),
            photos: vec![newest, older],
        };

        let playlist = store.reconcile(&manifest).expect("reconcile cache");
        assert_eq!(playlist.photos.len(), 1);
        assert_eq!(playlist.photos[0].id, "newest");
        assert_eq!(
            store.total_bytes().expect("cache size"),
            fs::metadata(&playlist.photos[0].path)
                .expect("photo metadata")
                .len()
        );
    }
}
