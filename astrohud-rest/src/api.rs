use crate::AppState;
use actix_multipart::Multipart;
use actix_web::{HttpResponse, Result, error, web};
use futures_util::StreamExt;
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use std::fs;
use uuid::Uuid;

const MAX_IMAGE_BYTES: usize = 20 * 1024 * 1024;
pub const DEMO_FRAME_ID: &str = "demo-frame";

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
             created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
         );",
    )?;

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

    let photo_id = Uuid::new_v4().to_string();
    let storage_key = format!("{photo_id}.{}", kind.extension());
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
        "INSERT INTO photos (id, channel_id, storage_key, mime_type)
         VALUES (?1, ?2, ?3, ?4)",
        params![photo_id, channel_id, storage_key, kind.mime_type()],
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
        mime_type: kind.mime_type().to_owned(),
        created_at,
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
            "SELECT p.id, p.channel_id, p.storage_key, p.mime_type, p.created_at
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

#[cfg(test)]
mod tests {
    use super::*;

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
    }
}
