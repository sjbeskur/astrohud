use crate::BoxError;
use image::{DynamicImage, ImageDecoder, ImageReader, RgbaImage};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, SyncSender};

pub struct PreparedImage {
    pub foreground: RgbaImage,
    pub backdrop: RgbaImage,
}

impl PreparedImage {
    pub fn load(path: &Path) -> Result<Self, BoxError> {
        let mut decoder = ImageReader::open(path)?
            .with_guessed_format()?
            .into_decoder()?;
        let (w, h) = decoder.dimensions();
        if u64::from(w) * u64::from(h) > 20_000_000 {
            return Err("image exceeds frame decode limit".into());
        }
        let orientation = decoder
            .orientation()
            .unwrap_or(image::metadata::Orientation::NoTransforms);
        let mut image = DynamicImage::from_decoder(decoder)?;
        image.apply_orientation(orientation);
        // Prepare a small, softened backdrop off the render thread. Rendering
        // crops it from the center to fill the screen without distortion.
        let mut backdrop = image.thumbnail(640, 640).blur(8.0).into_rgba8();
        for pixel in backdrop.pixels_mut() {
            for channel in &mut pixel.0[..3] {
                *channel = (u16::from(*channel) * 78 / 100) as u8;
            }
        }
        let foreground = if image.width() > 2048 || image.height() > 2048 {
            image
                .resize(2048, 2048, image::imageops::FilterType::Triangle)
                .into_rgba8()
        } else {
            image.into_rgba8()
        };
        Ok(Self {
            foreground,
            backdrop,
        })
    }
}

pub struct Prefetcher {
    requests: SyncSender<PathBuf>,
    results: Receiver<(PathBuf, Result<PreparedImage, BoxError>)>,
    requested: Option<PathBuf>,
    ready: Option<(PathBuf, PreparedImage)>,
}
impl Default for Prefetcher {
    fn default() -> Self {
        Self::new()
    }
}
impl Prefetcher {
    pub fn new() -> Self {
        let (requests, input) = mpsc::sync_channel::<PathBuf>(1);
        let (output, results) = mpsc::sync_channel(1);
        std::thread::spawn(move || {
            while let Ok(path) = input.recv() {
                let result = PreparedImage::load(&path);
                if output.send((path, result)).is_err() {
                    break;
                }
            }
        });
        Self {
            requests,
            results,
            requested: None,
            ready: None,
        }
    }
    pub fn poll(&mut self) {
        while let Ok((path, result)) = self.results.try_recv() {
            if self.requested.as_ref() == Some(&path) {
                match result {
                    Ok(image) => self.ready = Some((path, image)),
                    Err(error) => {
                        eprintln!("photo prefetch failed: {error}");
                        self.requested = None;
                    }
                }
            }
        }
    }
    pub fn request(&mut self, path: &Path) {
        if self.requested.as_deref() == Some(path) {
            return;
        }
        if self.requests.try_send(path.to_owned()).is_ok() {
            self.requested = Some(path.to_owned());
            self.ready = None;
        }
    }
    pub fn take(&mut self, path: &Path) -> Option<PreparedImage> {
        self.poll();
        if self.ready.as_ref().is_some_and(|(p, _)| p == path) {
            self.requested = None;
            self.ready.take().map(|(_, image)| image)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn background_is_bounded_gently_dimmed_and_foreground_retains_its_pixels() {
        let path = std::env::temp_dir().join(format!("memory-backdrop-{}.png", std::process::id()));
        let source = RgbaImage::from_pixel(1280, 960, image::Rgba([200, 160, 120, 255]));
        source.save(&path).unwrap();
        let prepared = PreparedImage::load(&path).unwrap();
        assert_eq!(prepared.foreground, source);
        assert!(prepared.backdrop.width() <= 640 && prepared.backdrop.height() <= 640);
        assert_eq!(prepared.backdrop.get_pixel(0, 0).0, [156, 124, 93, 255]);
        std::fs::remove_file(path).unwrap();
    }
}
