use crate::AppState;
use actix_files::NamedFile;
use actix_multipart::Multipart;
use actix_web::{HttpResponse, Result, error, web};
use exif::{In, Tag, Value};
use futures_util::StreamExt;
use image::codecs::jpeg::JpegEncoder;
use image::{DynamicImage, ImageDecoder, ImageReader, imageops::FilterType};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Cursor;
use uuid::Uuid;

const MAX_IMAGE_BYTES: usize = 20 * 1024 * 1024;
// Frames display at up to 1080p; anything wider than this just costs decode
// time and blur on the Pi's software renderer for no visible benefit.
const MAX_PHOTO_LONG_EDGE: u32 = 1600;
const PHOTO_JPEG_QUALITY: u8 = 87;
const SCHEMA_VERSION: i64 = 3;
pub const DEMO_FRAME_ID: &str = "demo-frame";
pub const DEMO_HOUSEHOLD_ID: &str = "demo-household";

const CURRENT_SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS households (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE TABLE IF NOT EXISTS channels (
    id TEXT PRIMARY KEY,
    household_id TEXT NOT NULL REFERENCES households(id) ON DELETE CASCADE,
    name TEXT NOT NULL COLLATE NOCASE,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE (household_id, name)
);
CREATE TABLE IF NOT EXISTS frames (
    id TEXT PRIMARY KEY,
    household_id TEXT NOT NULL REFERENCES households(id) ON DELETE CASCADE,
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
);
CREATE TABLE IF NOT EXISTS owner_access_grants (
    id TEXT PRIMARY KEY,
    household_id TEXT NOT NULL REFERENCES households(id) ON DELETE CASCADE,
    label TEXT NOT NULL,
    token_hash BLOB NOT NULL UNIQUE,
    expires_at TEXT,
    revoked_at TEXT,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE TABLE IF NOT EXISTS device_enrollments (
    id TEXT PRIMARY KEY,
    device_id TEXT NOT NULL UNIQUE,
    device_code TEXT NOT NULL,
    credential_hash BLOB NOT NULL,
    claim_code TEXT NOT NULL UNIQUE,
    status TEXT NOT NULL CHECK (status IN ('pending', 'claimed')),
    household_id TEXT REFERENCES households(id) ON DELETE RESTRICT,
    frame_id TEXT UNIQUE REFERENCES frames(id) ON DELETE RESTRICT,
    expires_at TEXT NOT NULL,
    claimed_at TEXT,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CHECK (
        (status = 'pending' AND household_id IS NULL AND frame_id IS NULL)
        OR
        (status = 'claimed' AND household_id IS NOT NULL AND frame_id IS NOT NULL)
    )
);
CREATE TABLE IF NOT EXISTS sender_invitations (
    id TEXT PRIMARY KEY,
    household_id TEXT NOT NULL REFERENCES households(id) ON DELETE CASCADE,
    channel_id TEXT NOT NULL REFERENCES channels(id) ON DELETE CASCADE,
    label TEXT NOT NULL,
    token_hash BLOB NOT NULL UNIQUE,
    revoked_at TEXT,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE TRIGGER IF NOT EXISTS frame_subscriptions_same_household_insert
BEFORE INSERT ON frame_subscriptions
WHEN NOT EXISTS (
    SELECT 1
    FROM frames f
    JOIN channels c ON c.id = NEW.channel_id
    WHERE f.id = NEW.frame_id
      AND f.household_id = c.household_id
)
BEGIN
    SELECT RAISE(ABORT, 'frame and channel must belong to the same household');
END;
CREATE TRIGGER IF NOT EXISTS frame_subscriptions_same_household_update
BEFORE UPDATE OF frame_id, channel_id ON frame_subscriptions
WHEN NOT EXISTS (
    SELECT 1
    FROM frames f
    JOIN channels c ON c.id = NEW.channel_id
    WHERE f.id = NEW.frame_id
      AND f.household_id = c.household_id
)
BEGIN
    SELECT RAISE(ABORT, 'frame and channel must belong to the same household');
END;
CREATE TRIGGER IF NOT EXISTS frames_preserve_subscription_household
BEFORE UPDATE OF household_id ON frames
WHEN EXISTS (
    SELECT 1
    FROM frame_subscriptions s
    JOIN channels c ON c.id = s.channel_id
    WHERE s.frame_id = OLD.id
      AND c.household_id != NEW.household_id
)
BEGIN
    SELECT RAISE(ABORT, 'frame subscriptions must remain in one household');
END;
CREATE TRIGGER IF NOT EXISTS channels_preserve_subscription_household
BEFORE UPDATE OF household_id ON channels
WHEN EXISTS (
    SELECT 1
    FROM frame_subscriptions s
    JOIN frames f ON f.id = s.frame_id
    WHERE s.channel_id = OLD.id
      AND f.household_id != NEW.household_id
)
BEGIN
    SELECT RAISE(ABORT, 'channel subscriptions must remain in one household');
END;
CREATE TRIGGER IF NOT EXISTS sender_invitations_same_household_insert
BEFORE INSERT ON sender_invitations
WHEN NOT EXISTS (
    SELECT 1
    FROM channels c
    WHERE c.id = NEW.channel_id
      AND c.household_id = NEW.household_id
)
BEGIN
    SELECT RAISE(ABORT, 'invitation channel must belong to its household');
END;
CREATE TRIGGER IF NOT EXISTS sender_invitations_same_household_update
BEFORE UPDATE OF household_id, channel_id ON sender_invitations
WHEN NOT EXISTS (
    SELECT 1
    FROM channels c
    WHERE c.id = NEW.channel_id
      AND c.household_id = NEW.household_id
)
BEGIN
    SELECT RAISE(ABORT, 'invitation channel must belong to its household');
END;
";

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
    pub(crate) id: String,
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
    connection.execute_batch("PRAGMA foreign_keys = OFF;")?;
    let initialization =
        if table_exists(connection, "channels")? && !table_exists(connection, "households")? {
            migrate_legacy_database(connection)
        } else {
            connection.execute_batch(CURRENT_SCHEMA)
        };
    connection.execute_batch("PRAGMA foreign_keys = ON;")?;
    initialization?;

    connection.execute(
        "INSERT OR IGNORE INTO households (id, name) VALUES (?1, ?2)",
        params![DEMO_HOUSEHOLD_ID, "Demo household"],
    )?;
    connection.execute(
        "INSERT OR IGNORE INTO frames (id, household_id, place_name) VALUES (?1, ?2, ?3)",
        params![DEMO_FRAME_ID, DEMO_HOUSEHOLD_ID, "Grandma's kitchen"],
    )?;
    connection.pragma_update(None, "user_version", SCHEMA_VERSION)?;

    ensure_foreign_keys_are_valid(connection)?;

    Ok(())
}

fn table_exists(connection: &Connection, table: &str) -> rusqlite::Result<bool> {
    connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1)",
        params![table],
        |row| row.get(0),
    )
}

fn table_columns(connection: &Connection, table: &str) -> rusqlite::Result<Vec<String>> {
    let mut statement = connection.prepare(&format!("PRAGMA table_info({table})"))?;
    statement
        .query_map([], |row| row.get(1))?
        .collect::<rusqlite::Result<Vec<_>>>()
}

fn migrate_legacy_database(connection: &Connection) -> rusqlite::Result<()> {
    let columns = table_columns(connection, "photos")?;
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

    connection.execute_batch(
        "BEGIN IMMEDIATE;
         ALTER TABLE frame_subscriptions RENAME TO legacy_frame_subscriptions;
         ALTER TABLE photos RENAME TO legacy_photos;
         ALTER TABLE frames RENAME TO legacy_frames;
         ALTER TABLE channels RENAME TO legacy_channels;",
    )?;

    let migration = (|| {
        connection.execute_batch(CURRENT_SCHEMA)?;
        connection.execute(
            "INSERT INTO households (id, name) VALUES (?1, ?2)",
            params![DEMO_HOUSEHOLD_ID, "Demo household"],
        )?;
        connection.execute(
            "INSERT INTO frames (id, household_id, place_name, created_at)
             SELECT id, ?1, place_name, created_at FROM legacy_frames",
            params![DEMO_HOUSEHOLD_ID],
        )?;
        connection.execute(
            "INSERT INTO channels (id, household_id, name, created_at)
             SELECT id, ?1, name, created_at FROM legacy_channels",
            params![DEMO_HOUSEHOLD_ID],
        )?;
        connection.execute_batch(
            "INSERT INTO frame_subscriptions (frame_id, channel_id)
             SELECT frame_id, channel_id FROM legacy_frame_subscriptions;
             INSERT INTO photos
                 (id, channel_id, storage_key, mime_type, latitude, longitude,
                  location_label, created_at)
             SELECT id, channel_id, storage_key, mime_type, latitude, longitude,
                    location_label, created_at
             FROM legacy_photos;
             DROP TABLE legacy_frame_subscriptions;
             DROP TABLE legacy_photos;
             DROP TABLE legacy_frames;
             DROP TABLE legacy_channels;
             COMMIT;",
        )?;
        Ok(())
    })();

    if migration.is_err() {
        let _ = connection.execute_batch("ROLLBACK;");
    }
    migration
}

fn ensure_foreign_keys_are_valid(connection: &Connection) -> rusqlite::Result<()> {
    let violation = connection
        .query_row("PRAGMA foreign_key_check", [], |_| Ok(()))
        .optional()?;
    if violation.is_some() {
        return Err(rusqlite::Error::InvalidQuery);
    }
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
    let channels = channels_for_household(&database, DEMO_HOUSEHOLD_ID)
        .map_err(error::ErrorInternalServerError)?;

    Ok(web::Json(channels))
}

fn channels_for_household(
    database: &Connection,
    household_id: &str,
) -> rusqlite::Result<Vec<Channel>> {
    let mut statement = database.prepare(
        "SELECT id, name, created_at
             FROM channels
             WHERE household_id = ?1
             ORDER BY name",
    )?;
    statement
        .query_map(params![household_id], |row| {
            Ok(Channel {
                id: row.get(0)?,
                name: row.get(1)?,
                created_at: row.get(2)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()
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
            "INSERT INTO channels (id, household_id, name) VALUES (?1, ?2, ?3)",
            params![id, DEMO_HOUSEHOLD_ID, name],
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
    multipart: Multipart,
) -> Result<HttpResponse> {
    let photo =
        upload_photo_for_household(state, multipart, DEMO_HOUSEHOLD_ID.to_owned(), None).await?;
    Ok(HttpResponse::Created().json(photo))
}

/// Stores a photo in a server-selected household and channel. Invitation
/// uploads use this path so a sender cannot redirect a photo by changing a
/// multipart field in the browser.
pub async fn upload_invited_photo(
    state: web::Data<AppState>,
    multipart: Multipart,
    household_id: String,
    channel_id: String,
) -> Result<Photo> {
    upload_photo_for_household(state, multipart, household_id, Some(channel_id)).await
}

async fn upload_photo_for_household(
    state: web::Data<AppState>,
    mut multipart: Multipart,
    household_id: String,
    selected_channel_id: Option<String>,
) -> Result<Photo> {
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

    let channel_id = selected_channel_id
        .or(channel_id)
        .ok_or_else(|| error::ErrorBadRequest("channel_id is required"))?;
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
            "SELECT EXISTS(
                 SELECT 1 FROM channels WHERE id = ?1 AND household_id = ?2
             )",
            params![channel_id, household_id],
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

    Ok(Photo {
        id: photo_id,
        channel_id,
        url: format!("/media/{storage_key}"),
        mime_type: mime_type.to_owned(),
        created_at,
        location_label,
    })
}

pub async fn frame_manifest(
    state: web::Data<AppState>,
    frame_id: web::Path<String>,
) -> Result<web::Json<FrameManifest>> {
    let database = state
        .database
        .lock()
        .map_err(|_| error::ErrorInternalServerError("database lock poisoned"))?;
    let frame_id = frame_id.into_inner();
    let manifest = manifest_for_household(&database, DEMO_HOUSEHOLD_ID, &frame_id)
        .map_err(error::ErrorInternalServerError)?
        .ok_or_else(|| error::ErrorNotFound("frame not found"))?;

    Ok(web::Json(manifest))
}

pub async fn demo_media(
    state: web::Data<AppState>,
    storage_key: web::Path<String>,
) -> Result<NamedFile> {
    let storage_key = storage_key.into_inner();
    let is_demo_photo = {
        let database = state
            .database
            .lock()
            .map_err(|_| error::ErrorInternalServerError("database lock poisoned"))?;
        database
            .query_row(
                "SELECT EXISTS(
                     SELECT 1
                     FROM photos p
                     JOIN channels c ON c.id = p.channel_id
                     WHERE p.storage_key = ?1 AND c.household_id = ?2
                 )",
                params![storage_key, DEMO_HOUSEHOLD_ID],
                |row| row.get::<_, bool>(0),
            )
            .map_err(error::ErrorInternalServerError)?
    };
    if !is_demo_photo {
        return Err(error::ErrorNotFound("photo not found"));
    }
    NamedFile::open_async(state.media_dir.join(storage_key))
        .await
        .map_err(|err| match err.kind() {
            std::io::ErrorKind::NotFound => error::ErrorNotFound("photo not found"),
            _ => error::ErrorInternalServerError(err),
        })
}

fn manifest_for_household(
    database: &Connection,
    household_id: &str,
    frame_id: &str,
) -> rusqlite::Result<Option<FrameManifest>> {
    manifest_for_household_with_protected_media(database, household_id, frame_id, false)
}

pub(crate) fn protected_manifest_for_household(
    database: &Connection,
    household_id: &str,
    frame_id: &str,
) -> rusqlite::Result<Option<FrameManifest>> {
    manifest_for_household_with_protected_media(database, household_id, frame_id, true)
}

fn manifest_for_household_with_protected_media(
    database: &Connection,
    household_id: &str,
    frame_id: &str,
    protected_media: bool,
) -> rusqlite::Result<Option<FrameManifest>> {
    let place_name = database
        .query_row(
            "SELECT place_name FROM frames WHERE id = ?1 AND household_id = ?2",
            params![frame_id, household_id],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    let Some(place_name) = place_name else {
        return Ok(None);
    };
    let mut statement = database.prepare(
        "SELECT p.id, p.channel_id, p.storage_key, p.mime_type, p.created_at,
                    p.location_label
             FROM photos p
             JOIN channels c ON c.id = p.channel_id
             JOIN frame_subscriptions s ON s.channel_id = p.channel_id
             JOIN frames f ON f.id = s.frame_id
             WHERE s.frame_id = ?1
               AND f.household_id = ?2
               AND c.household_id = ?2
             ORDER BY p.created_at DESC, p.rowid DESC
             LIMIT 200",
    )?;
    let photos = statement
        .query_map(params![frame_id, household_id], |row| {
            let photo_id: String = row.get(0)?;
            let storage_key: String = row.get(2)?;
            Ok(Photo {
                url: if protected_media {
                    format!("/api/beta/device/media/{photo_id}")
                } else {
                    format!("/media/{storage_key}")
                },
                id: photo_id,
                channel_id: row.get(1)?,
                mime_type: row.get(3)?,
                created_at: row.get(4)?,
                location_label: row.get(5)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    Ok(Some(FrameManifest {
        frame_id: frame_id.to_owned(),
        place_name,
        photos,
    }))
}

pub(crate) fn protected_media_storage_key(
    database: &Connection,
    household_id: &str,
    frame_id: &str,
    photo_id: &str,
) -> rusqlite::Result<Option<String>> {
    database
        .query_row(
            "SELECT p.storage_key
             FROM photos p
             JOIN channels c ON c.id = p.channel_id
             JOIN frame_subscriptions s ON s.channel_id = p.channel_id
             JOIN frames f ON f.id = s.frame_id
             WHERE p.id = ?1
               AND s.frame_id = ?2
               AND c.household_id = ?3
               AND f.household_id = ?3",
            params![photo_id, frame_id, household_id],
            |row| row.get(0),
        )
        .optional()
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

        let (household_id, place_name): (String, String) = database
            .query_row(
                "SELECT household_id, place_name FROM frames WHERE id = ?1",
                params![DEMO_FRAME_ID],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("read demo frame");
        assert_eq!(household_id, DEMO_HOUSEHOLD_ID);
        assert_eq!(place_name, "Grandma's kitchen");
        let schema_version: i64 = database
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .expect("read schema version");
        assert_eq!(schema_version, SCHEMA_VERSION);

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
    fn legacy_database_is_migrated_without_losing_metadata() {
        let database = Connection::open_in_memory().expect("open in-memory database");
        database
            .execute_batch(
                "PRAGMA foreign_keys = ON;
                 CREATE TABLE channels (
                     id TEXT PRIMARY KEY,
                     name TEXT NOT NULL UNIQUE COLLATE NOCASE,
                     created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
                 );
                 CREATE TABLE frames (
                     id TEXT PRIMARY KEY,
                     place_name TEXT NOT NULL,
                     created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
                 );
                 CREATE TABLE frame_subscriptions (
                     frame_id TEXT NOT NULL REFERENCES frames(id) ON DELETE CASCADE,
                     channel_id TEXT NOT NULL REFERENCES channels(id) ON DELETE CASCADE,
                     PRIMARY KEY (frame_id, channel_id)
                 );
                 CREATE TABLE photos (
                     id TEXT PRIMARY KEY,
                     channel_id TEXT NOT NULL REFERENCES channels(id) ON DELETE CASCADE,
                     storage_key TEXT NOT NULL UNIQUE,
                     mime_type TEXT NOT NULL,
                     created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
                 );
                 INSERT INTO frames (id, place_name) VALUES ('legacy-frame', 'Legacy den');
                 INSERT INTO channels (id, name) VALUES ('legacy-channel', 'Family');
                 INSERT INTO frame_subscriptions (frame_id, channel_id)
                 VALUES ('legacy-frame', 'legacy-channel');
                 INSERT INTO photos (id, channel_id, storage_key, mime_type)
                 VALUES ('legacy-photo', 'legacy-channel', 'legacy.jpg', 'image/jpeg');",
            )
            .expect("create legacy database");

        initialize_database(&database).expect("migrate legacy database");

        let migrated_household: String = database
            .query_row(
                "SELECT household_id FROM frames WHERE id = 'legacy-frame'",
                [],
                |row| row.get(0),
            )
            .expect("read migrated frame");
        assert_eq!(migrated_household, DEMO_HOUSEHOLD_ID);

        let manifest = manifest_for_household(&database, DEMO_HOUSEHOLD_ID, "legacy-frame")
            .expect("load migrated manifest")
            .expect("legacy frame exists");
        assert_eq!(manifest.place_name, "Legacy den");
        assert_eq!(manifest.photos.len(), 1);
        assert_eq!(manifest.photos[0].id, "legacy-photo");
        assert_eq!(manifest.photos[0].url, "/media/legacy.jpg");
    }

    #[test]
    fn household_namespaces_allow_the_same_channel_name() {
        let database = Connection::open_in_memory().expect("open in-memory database");
        initialize_database(&database).expect("initialize database");
        database
            .execute(
                "INSERT INTO households (id, name) VALUES ('household-two', 'Second household')",
                [],
            )
            .expect("insert second household");
        database
            .execute(
                "INSERT INTO channels (id, household_id, name)
                 VALUES ('demo-family', ?1, 'Family'),
                        ('second-family', 'household-two', 'Family')",
                params![DEMO_HOUSEHOLD_ID],
            )
            .expect("insert household-scoped channels");

        let demo_channels =
            channels_for_household(&database, DEMO_HOUSEHOLD_ID).expect("list demo channels");
        let second_channels = channels_for_household(&database, "household-two")
            .expect("list second household channels");
        assert_eq!(demo_channels.len(), 1);
        assert_eq!(demo_channels[0].id, "demo-family");
        assert_eq!(second_channels.len(), 1);
        assert_eq!(second_channels[0].id, "second-family");
    }

    #[test]
    fn frame_subscriptions_and_manifests_cannot_cross_households() {
        let database = Connection::open_in_memory().expect("open in-memory database");
        initialize_database(&database).expect("initialize database");
        database
            .execute_batch(
                "INSERT INTO households (id, name) VALUES ('household-two', 'Second household');
                 INSERT INTO frames (id, household_id, place_name)
                 VALUES ('second-frame', 'household-two', 'Second kitchen');
                 INSERT INTO channels (id, household_id, name)
                 VALUES ('demo-family', 'demo-household', 'Family'),
                        ('second-family', 'household-two', 'Family');
                 INSERT INTO frame_subscriptions (frame_id, channel_id)
                 VALUES ('second-frame', 'second-family');
                 INSERT INTO photos (id, channel_id, storage_key, mime_type)
                 VALUES ('second-photo', 'second-family', 'second.jpg', 'image/jpeg');",
            )
            .expect("insert isolated household data");

        let cross_household = database.execute(
            "INSERT INTO frame_subscriptions (frame_id, channel_id) VALUES (?1, ?2)",
            params![DEMO_FRAME_ID, "second-family"],
        );
        assert!(cross_household.is_err());

        let own_manifest = manifest_for_household(&database, "household-two", "second-frame")
            .expect("load second household manifest")
            .expect("second frame exists");
        assert_eq!(own_manifest.photos.len(), 1);
        assert_eq!(own_manifest.photos[0].id, "second-photo");

        let protected_manifest =
            protected_manifest_for_household(&database, "household-two", "second-frame")
                .expect("load protected manifest")
                .expect("second frame exists");
        assert_eq!(
            protected_manifest.photos[0].url,
            "/api/beta/device/media/second-photo"
        );
        assert_eq!(
            protected_media_storage_key(&database, "household-two", "second-frame", "second-photo")
                .expect("load own media"),
            Some("second.jpg".to_owned())
        );
        assert!(
            protected_media_storage_key(
                &database,
                DEMO_HOUSEHOLD_ID,
                DEMO_FRAME_ID,
                "second-photo"
            )
            .expect("scope media")
            .is_none()
        );

        let wrong_household = manifest_for_household(&database, DEMO_HOUSEHOLD_ID, "second-frame")
            .expect("query frame through wrong household");
        assert!(wrong_household.is_none());
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
