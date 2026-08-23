use crate::AppState;
use actix_multipart::Multipart;
use actix_web::{HttpResponse, Result, error, web};
use exif::{In, Tag, Value};
use futures_util::StreamExt;
use image::codecs::jpeg::JpegEncoder;
use image::{DynamicImage, ImageDecoder, ImageReader, imageops::FilterType};
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Cursor;
use uuid::Uuid;

const MAX_IMAGE_BYTES: usize = 20 * 1024 * 1024;
// Frames display at up to 1080p; anything wider than this just costs decode
// time and blur on the Pi's software renderer for no visible benefit.
const MAX_PHOTO_LONG_EDGE: u32 = 1600;
const PHOTO_JPEG_QUALITY: u8 = 87;
pub const DEMO_FRAME_ID: &str = "demo-frame";

/// Downscales an oversized photo to `MAX_PHOTO_LONG_EDGE` and re-encodes it as
/// JPEG, applying any EXIF orientation first so the baked-in pixels match
/// what a browser would have shown for the original. Returns `None` (keep
/// the original bytes as-is) when the image is already small enough, or if
/// it can't be decoded — resizing is an optimization, not a requirement.
fn downscale_if_oversized(bytes: &[u8]) -> Option<(Vec<u8>, &'static str, &'static str)> {
    let mut reader = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .ok()?;
    reader.no_limits();
    let mut decoder = reader.into_decoder().ok()?;
    let orientation = decoder
        .orientation()
        .unwrap_or(image::metadata::Orientation::NoTransforms);
    let mut img = DynamicImage::from_decoder(decoder).ok()?;
    img.apply_orientation(orientation);

    let long_edge = img.width().max(img.height());
    if long_edge <= MAX_PHOTO_LONG_EDGE {
        return None;
    }

    let scale = MAX_PHOTO_LONG_EDGE as f32 / long_edge as f32;
    let new_width = ((img.width() as f32 * scale).round() as u32).max(1);
    let new_height = ((img.height() as f32 * scale).round() as u32).max(1);
    let resized = img.resize(new_width, new_height, FilterType::Lanczos3);

    let mut out = Vec::new();
    let encoder = JpegEncoder::new_with_quality(&mut out, PHOTO_JPEG_QUALITY);
    resized.into_rgb8().write_with_encoder(encoder).ok()?;

    Some((out, "jpg", "image/jpeg"))
}

#[derive(Debug, Serialize)]
pub struct Channel {
    id: String,
    name: String,
    created_at: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateChannel {
    name: String,
}

#[derive(Debug, Serialize)]
pub struct Photo {
    id: String,
    channel_id: String,
    url: String,
    mime_type: String,
    created_at: String,
    location_label: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct FrameManifest {
    frame_id: String,
    place_name: String,
    photos: Vec<Photo>,
}

#[derive(Debug, Serialize)]
pub struct Health {
    status: &'static str,
}

pub fn initialize_database(connection: &Connection) -> rusqlite::Result<()> {
    connection.execute_batch(
        "PRAGMA foreign_keys = ON;
         CREATE TABLE IF NOT EXISTS channels (
             id TEXT PRIMARY KEY,
             name TEXT NOT NULL UNIQUE COLLATE NOCASE,
             created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
         );
         CREATE TABLE IF NOT EXISTS frames (
             id TEXT PRIMARY KEY,
             place_name TEXT NOT NULL,
             created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
         );
         CREATE TABLE IF NOT EXISTS frame_subscriptions (
             frame_id TEXT NOT NULL REFERENCES frames(id) ON DELETE CASCADE,
             channel_id TEXT NOT NULL REFERENCES channels(id) ON DELETE CASCADE,
             PRIMARY KEY (frame_id, channel_id)
         );
         CREATE TABLE IF NOT EXISTS photos (
             id TEXT PRIMARY KEY,
             channel_id TEXT NOT NULL REFERENCES channels(id) ON DELETE CASCADE,
             storage_key TEXT NOT NULL UNIQUE,
             mime_type TEXT NOT NULL,
             latitude REAL,
             longitude REAL,
             location_label TEXT,
             created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
         );",
    )?;

    // `CREATE TABLE IF NOT EXISTS` does not add columns to an existing POC
    // database, so keep these small migrations explicit and idempotent.
    let columns = {
        let mut statement = connection.prepare("PRAGMA table_info(photos)")?;
        statement
            .query_map([], |row| row.get::<_, String>(1))?
            .collect::<rusqlite::Result<Vec<_>>>()?
    };
    for (name, sql) in [
        ("latitude", "ALTER TABLE photos ADD COLUMN latitude REAL"),
        ("longitude", "ALTER TABLE photos ADD COLUMN longitude REAL"),
        (
            "location_label",
            "ALTER TABLE photos ADD COLUMN location_label TEXT",
        ),
    ] {
        if !columns.iter().any(|column| column == name) {
            connection.execute(sql, [])?;
        }
    }

    connection.execute(
        "INSERT OR IGNORE INTO frames (id, place_name) VALUES (?1, ?2)",
        params![DEMO_FRAME_ID, "Grandma's kitchen"],
    )?;

    Ok(())
}

pub async fn health() -> web::Json<Health> {
    web::Json(Health { status: "ok" })
}

pub async fn list_channels(state: web::Data<AppState>) -> Result<web::Json<Vec<Channel>>> {
    let database = state
        .database
        .lock()
        .map_err(|_| error::ErrorInternalServerError("database lock poisoned"))?;
    let mut statement = database
        .prepare("SELECT id, name, created_at FROM channels ORDER BY name")
        .map_err(error::ErrorInternalServerError)?;
    let channels = statement
        .query_map([], |row| {
            Ok(Channel {
                id: row.get(0)?,
                name: row.get(1)?,
                created_at: row.get(2)?,
            })
        })
        .map_err(error::ErrorInternalServerError)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(error::ErrorInternalServerError)?;

    Ok(web::Json(channels))
}

pub async fn create_channel(
    state: web::Data<AppState>,
    request: web::Json<CreateChannel>,
) -> Result<HttpResponse> {
    let name = request.name.trim();
    if name.is_empty() || name.chars().count() > 80 {
        return Err(error::ErrorBadRequest(
            "channel name must contain 1 to 80 characters",
        ));
    }

    let id = Uuid::new_v4().to_string();
    let database = state
        .database
        .lock()
        .map_err(|_| error::ErrorInternalServerError("database lock poisoned"))?;
    database
        .execute(
            "INSERT INTO channels (id, name) VALUES (?1, ?2)",
            params![id, name],
        )
        .map_err(|err| match err {
            rusqlite::Error::SqliteFailure(_, _) => {
                error::ErrorConflict("a channel with that name already exists")
            }
            other => error::ErrorInternalServerError(other),
        })?;
    database
        .execute(
            "INSERT INTO frame_subscriptions (frame_id, channel_id) VALUES (?1, ?2)",
            params![DEMO_FRAME_ID, id],
        )
        .map_err(error::ErrorInternalServerError)?;
    let created_at = database
        .query_row(
            "SELECT created_at FROM channels WHERE id = ?1",
            params![id],
            |row| row.get(0),
        )
        .map_err(error::ErrorInternalServerError)?;

    Ok(HttpResponse::Created().json(Channel {
        id,
        name: name.to_owned(),
        created_at,
    }))
}

pub async fn upload_photo(
    state: web::Data<AppState>,
    mut multipart: Multipart,
) -> Result<HttpResponse> {
    let mut channel_id = None;
    let mut image = Vec::new();
    let mut include_location = false;

    while let Some(field) = multipart.next().await {
        let mut field = field.map_err(error::ErrorBadRequest)?;
        let field_name = field.name().unwrap_or_default().to_owned();
        let mut bytes = Vec::new();
        while let Some(chunk) = field.next().await {
            let chunk = chunk.map_err(error::ErrorBadRequest)?;
            if bytes.len() + chunk.len() > MAX_IMAGE_BYTES {
                return Err(error::ErrorPayloadTooLarge("image exceeds 20 MiB"));
            }
            bytes.extend_from_slice(&chunk);
        }

        match field_name.as_str() {
            "channel_id" => {
                channel_id = Some(
                    String::from_utf8(bytes)
                        .map_err(|_| error::ErrorBadRequest("invalid channel id"))?,
                );
            }
            "photo" => image = bytes,
            "include_location" => {
                include_location = matches!(bytes.as_slice(), b"true" | b"on" | b"1")
            }
            _ => {}
        }
    }

    let channel_id = channel_id.ok_or_else(|| error::ErrorBadRequest("channel_id is required"))?;
    if image.is_empty() {
        return Err(error::ErrorBadRequest("photo is required"));
    }
    let kind = infer::get(&image)
        .filter(|kind| kind.mime_type().starts_with("image/"))
        .ok_or_else(|| error::ErrorUnsupportedMediaType("file is not a supported image"))?;
    let coordinates = include_location.then(|| extract_gps(&image)).flatten();
    let location_label = coordinates.map(approximate_location_label);

    let (image, extension, mime_type) = match downscale_if_oversized(&image) {
        Some((resized, extension, mime_type)) => (resized, extension, mime_type),
        None => (image, kind.extension(), kind.mime_type()),
    };

    let photo_id = Uuid::new_v4().to_string();
    let storage_key = format!("{photo_id}.{extension}");
    let path = state.media_dir.join(&storage_key);
    fs::write(&path, &image).map_err(error::ErrorInternalServerError)?;

    let database = state
        .database
        .lock()
        .map_err(|_| error::ErrorInternalServerError("database lock poisoned"))?;
    let channel_exists = database
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM channels WHERE id = ?1)",
            params![channel_id],
            |row| row.get::<_, bool>(0),
        )
        .map_err(error::ErrorInternalServerError)?;
    if !channel_exists {
        let _ = fs::remove_file(path);
        return Err(error::ErrorNotFound("channel not found"));
    }

    if let Err(err) = database.execute(
        "INSERT INTO photos
             (id, channel_id, storage_key, mime_type, latitude, longitude, location_label)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            photo_id,
            channel_id,
            storage_key,
            mime_type,
            coordinates.map(|value| value.0),
            coordinates.map(|value| value.1),
            location_label,
        ],
    ) {
        let _ = fs::remove_file(path);
        return Err(error::ErrorInternalServerError(err));
    }
    let created_at = database
        .query_row(
            "SELECT created_at FROM photos WHERE id = ?1",
            params![photo_id],
            |row| row.get(0),
        )
        .map_err(error::ErrorInternalServerError)?;

    Ok(HttpResponse::Created().json(Photo {
        id: photo_id,
        channel_id,
        url: format!("/media/{storage_key}"),
        mime_type: mime_type.to_owned(),
        created_at,
        location_label,
    }))
}

pub async fn frame_manifest(
    state: web::Data<AppState>,
    frame_id: web::Path<String>,
) -> Result<web::Json<FrameManifest>> {
    let database = state
        .database
        .lock()
        .map_err(|_| error::ErrorInternalServerError("database lock poisoned"))?;
    let place_name = database
        .query_row(
            "SELECT place_name FROM frames WHERE id = ?1",
            params![frame_id.as_str()],
            |row| row.get::<_, String>(0),
        )
        .map_err(|err| match err {
            rusqlite::Error::QueryReturnedNoRows => error::ErrorNotFound("frame not found"),
            other => error::ErrorInternalServerError(other),
        })?;
    let mut statement = database
        .prepare(
            "SELECT p.id, p.channel_id, p.storage_key, p.mime_type, p.created_at,
                    p.location_label
             FROM photos p
             JOIN frame_subscriptions s ON s.channel_id = p.channel_id
             WHERE s.frame_id = ?1
             ORDER BY p.created_at DESC, p.rowid DESC
             LIMIT 200",
        )
        .map_err(error::ErrorInternalServerError)?;
    let photos = statement
        .query_map(params![frame_id.as_str()], |row| {
            let storage_key: String = row.get(2)?;
            Ok(Photo {
                id: row.get(0)?,
                channel_id: row.get(1)?,
                url: format!("/media/{storage_key}"),
                mime_type: row.get(3)?,
                created_at: row.get(4)?,
                location_label: row.get(5)?,
            })
        })
        .map_err(error::ErrorInternalServerError)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(error::ErrorInternalServerError)?;

    Ok(web::Json(FrameManifest {
        frame_id: frame_id.into_inner(),
        place_name,
        photos,
    }))
}

fn extract_gps(bytes: &[u8]) -> Option<(f64, f64)> {
    let mut cursor = Cursor::new(bytes);
    let exif = exif::Reader::new().read_from_container(&mut cursor).ok()?;
    let latitude = exif_coordinate(&exif, Tag::GPSLatitude, Tag::GPSLatitudeRef)?;
    let longitude = exif_coordinate(&exif, Tag::GPSLongitude, Tag::GPSLongitudeRef)?;
    (latitude.abs() <= 90.0 && longitude.abs() <= 180.0).then_some((latitude, longitude))
}

fn exif_coordinate(exif: &exif::Exif, value_tag: Tag, reference_tag: Tag) -> Option<f64> {
    let field = exif.get_field(value_tag, In::PRIMARY)?;
    let Value::Rational(parts) = &field.value else {
        return None;
    };
    if parts.len() < 3 {
        return None;
    }
    let mut degrees = parts[0].to_f64() + parts[1].to_f64() / 60.0 + parts[2].to_f64() / 3600.0;
    let reference = exif.get_field(reference_tag, In::PRIMARY)?;
    let negative = match &reference.value {
        Value::Ascii(values) => values
            .first()
            .and_then(|value| value.first())
            .is_some_and(|value| matches!(value.to_ascii_uppercase(), b'S' | b'W')),
        _ => return None,
    };
    if negative {
        degrees = -degrees;
    }
    degrees.is_finite().then_some(degrees)
}

fn approximate_location_label((latitude, longitude): (f64, f64)) -> String {
    let latitude_direction = if latitude < 0.0 { 'S' } else { 'N' };
    let longitude_direction = if longitude < 0.0 { 'W' } else { 'E' };
    format!(
        "{:.1} {latitude_direction} / {:.1} {longitude_direction}",
        latitude.abs(),
        longitude.abs()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn jpeg_with_gps() -> Vec<u8> {
        fn short(bytes: &mut Vec<u8>, value: u16) {
            bytes.extend_from_slice(&value.to_le_bytes());
        }

        fn long(bytes: &mut Vec<u8>, value: u32) {
            bytes.extend_from_slice(&value.to_le_bytes());
        }

        fn rational(bytes: &mut Vec<u8>, numerator: u32, denominator: u32) {
            long(bytes, numerator);
            long(bytes, denominator);
        }

        let mut tiff = Vec::new();
        tiff.extend_from_slice(b"II");
        short(&mut tiff, 42);
        long(&mut tiff, 8);

        short(&mut tiff, 1);
        short(&mut tiff, 0x8825);
        short(&mut tiff, 4);
        long(&mut tiff, 1);
        long(&mut tiff, 26);
        long(&mut tiff, 0);

        short(&mut tiff, 4);
        short(&mut tiff, 1);
        short(&mut tiff, 2);
        long(&mut tiff, 2);
        tiff.extend_from_slice(b"N\0\0\0");
        short(&mut tiff, 2);
        short(&mut tiff, 5);
        long(&mut tiff, 3);
        long(&mut tiff, 80);
        short(&mut tiff, 3);
        short(&mut tiff, 2);
        long(&mut tiff, 2);
        tiff.extend_from_slice(b"W\0\0\0");
        short(&mut tiff, 4);
        short(&mut tiff, 5);
        long(&mut tiff, 3);
        long(&mut tiff, 104);
        long(&mut tiff, 0);

        rational(&mut tiff, 40, 1);
        rational(&mut tiff, 0, 1);
        rational(&mut tiff, 5394, 100);
        rational(&mut tiff, 105, 1);
        rational(&mut tiff, 16, 1);
        rational(&mut tiff, 1397, 100);

        let mut jpeg = vec![0xff, 0xd8, 0xff, 0xe1];
        let segment_length = u16::try_from(tiff.len() + 8).expect("EXIF segment length");
        jpeg.extend_from_slice(&segment_length.to_be_bytes());
        jpeg.extend_from_slice(b"Exif\0\0");
        jpeg.extend_from_slice(&tiff);
        jpeg.extend_from_slice(&[0xff, 0xd9]);
        jpeg
    }

    #[test]
    fn database_initialization_is_idempotent_and_seeds_demo_frame() {
        let database = Connection::open_in_memory().expect("open in-memory database");

        initialize_database(&database).expect("initialize database");
        initialize_database(&database).expect("initialize database again");

        let place_name: String = database
            .query_row(
                "SELECT place_name FROM frames WHERE id = ?1",
                params![DEMO_FRAME_ID],
                |row| row.get(0),
            )
            .expect("read demo frame");
        assert_eq!(place_name, "Grandma's kitchen");

        let columns = database
            .prepare("PRAGMA table_info(photos)")
            .expect("prepare columns")
            .query_map([], |row| row.get::<_, String>(1))
            .expect("read columns")
            .collect::<rusqlite::Result<Vec<_>>>()
            .expect("collect columns");
        assert!(columns.contains(&"location_label".to_owned()));
    }

    #[test]
    fn approximate_location_is_rounded_and_directional() {
        assert_eq!(
            approximate_location_label((40.014984, -105.270546)),
            "40.0 N / 105.3 W"
        );
    }

    #[test]
    fn gps_is_extracted_before_an_image_is_transformed() {
        let (latitude, longitude) = extract_gps(&jpeg_with_gps()).expect("extract GPS");
        assert!((latitude - 40.014983).abs() < 0.000_01);
        assert!((longitude - -105.270547).abs() < 0.000_01);
    }
}
