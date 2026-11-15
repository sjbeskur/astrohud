//! Latest device storage snapshot; contains counts and bytes, never file names.
use crate::beta_web::{bearer_token, beta_http_error, owner_token};
use crate::{AppState, BetaError, claimed_device_access};
use actix_web::{HttpRequest, HttpResponse, Result, error, http::header, web};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

const MIB: u64 = 1024 * 1024;
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Report {
    viewer_version: String,
    disk_total_bytes: u64,
    disk_available_bytes: u64,
    disk_used_bytes: u64,
    photo_bytes: u64,
    photo_count: u64,
    photo_limit_bytes: u64,
    reserve_bytes: u64,
}

impl Report {
    fn validate(&self) -> std::result::Result<(), BetaError> {
        if self.viewer_version.is_empty()
            || self.viewer_version.len() > 40
            || self.disk_total_bytes == 0
            || self.disk_total_bytes > (1 << 50)
            || self.disk_used_bytes > self.disk_total_bytes
            || self.disk_available_bytes > self.disk_total_bytes - self.disk_used_bytes
            || self.photo_bytes > (1 << 50)
            || self.photo_limit_bytes == 0
            || self.photo_limit_bytes > (1 << 50)
            || self.photo_count > 1_000_000
            || (self.photo_count == 0) != (self.photo_bytes == 0)
            || self.reserve_bytes < 512 * MIB
            || self.reserve_bytes > (1 << 50)
        {
            return Err(BetaError::InvalidInput("invalid storage report"));
        }
        Ok(())
    }
    fn estimate(&self) -> Option<u64> {
        let remaining = self
            .photo_limit_bytes
            .saturating_sub(self.photo_bytes)
            .min(self.disk_available_bytes.saturating_sub(self.reserve_bytes));
        if remaining == 0 {
            return Some(0);
        }
        if self.photo_count == 0 {
            return None;
        }
        // Use actual stored (server-optimized) image sizes, with 25% headroom.
        // A minimum avoids implausible promises from a few tiny test images.
        let average = self.photo_bytes.div_ceil(self.photo_count).max(64 * 1024);
        Some(remaining / average.saturating_add(average.div_ceil(4)))
    }
    fn space_status(&self) -> &'static str {
        if self.disk_available_bytes < 100 * MIB {
            "very_low"
        } else if self.disk_available_bytes <= self.reserve_bytes {
            "low"
        } else {
            "healthy"
        }
    }
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn store_report(
    db: &Connection,
    credential: &str,
    report: &Report,
) -> std::result::Result<(), BetaError> {
    let access = claimed_device_access(db, credential)?;
    report.validate()?;
    let report = serde_json::to_string(report)
        .map_err(|_| BetaError::InvalidInput("invalid storage report"))?;
    db.execute("INSERT INTO frame_storage (frame_id,report,reported_unix) VALUES (?1,?2,?3)
        ON CONFLICT(frame_id) DO UPDATE SET report=excluded.report,reported_unix=excluded.reported_unix",
        params![access.frame_id, report, now()])?;
    Ok(())
}

fn owner_status(
    db: &Connection,
    owner: &str,
    frame: &str,
) -> std::result::Result<serde_json::Value, BetaError> {
    crate::updates::owner_frame(db, owner, frame)?;
    let row: Option<(String, u64)> = db
        .query_row(
            "SELECT report, reported_unix FROM frame_storage WHERE frame_id=?1",
            [frame],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    let Some((text, time)) = row else {
        return Ok(serde_json::json!({"frame_id":frame,"report":null,"reported_unix":null}));
    };
    let report: Report = serde_json::from_str(&text)
        .map_err(|_| BetaError::InvalidInput("invalid saved storage report"))?;
    Ok(serde_json::json!({"frame_id":frame, "reported_unix":time,
        "estimated_photos_remaining":report.estimate(), "space_status":report.space_status(),
        "downloads_paused":report.disk_available_bytes <= report.reserve_bytes, "report":report}))
}

pub(crate) fn configure(config: &mut web::ServiceConfig) {
    config
        .service(
            web::resource("/api/beta/device/storage")
                .app_data(web::JsonConfig::default().limit(2048))
                .route(web::post().to(post_report)),
        )
        .route(
            "/api/beta/owner/frames/{frame}/storage",
            web::get().to(get_status),
        );
}
async fn post_report(
    state: web::Data<AppState>,
    request: HttpRequest,
    body: web::Json<Report>,
) -> Result<HttpResponse> {
    let credential = bearer_token(&request)?;
    let db = state
        .database
        .lock()
        .map_err(|_| error::ErrorInternalServerError("database lock poisoned"))?;
    store_report(&db, &credential, &body).map_err(beta_http_error)?;
    Ok(HttpResponse::NoContent()
        .insert_header((header::CACHE_CONTROL, "no-store"))
        .finish())
}
async fn get_status(
    state: web::Data<AppState>,
    request: HttpRequest,
    frame: web::Path<String>,
) -> Result<HttpResponse> {
    let owner = owner_token(&request)?;
    let db = state
        .database
        .lock()
        .map_err(|_| error::ErrorInternalServerError("database lock poisoned"))?;
    Ok(HttpResponse::Ok()
        .insert_header((header::CACHE_CONTROL, "no-store"))
        .json(owner_status(&db, &owner, &frame).map_err(beta_http_error)?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::*;
    fn report() -> Report {
        Report {
            viewer_version: "0.4.0".into(),
            disk_total_bytes: 16_000 * MIB,
            disk_used_bytes: 2_000 * MIB,
            disk_available_bytes: 14_000 * MIB,
            photo_bytes: 100 * MIB,
            photo_count: 100,
            photo_limit_bytes: 1_000 * MIB,
            reserve_bytes: 512 * MIB,
        }
    }
    #[test]
    fn schema_fourteen_upgrade_snapshots_before_adding_storage() {
        let dir = std::env::temp_dir().join(format!(
            "memory-lane-storage-migration-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir(&dir).unwrap();
        let path = dir.join("db.sqlite3");
        let db = Connection::open(&path).unwrap();
        initialize_database(&db).unwrap();
        db.execute_batch("DROP TABLE frame_storage; PRAGMA user_version=14;")
            .unwrap();
        let snapshot = initialize_database_with_backup(&db, &path)
            .unwrap()
            .unwrap();
        let old = Connection::open(snapshot).unwrap();
        assert_eq!(
            old.pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))
                .unwrap(),
            14
        );
        assert!(old.prepare("SELECT * FROM frame_storage").is_err());
        assert!(db.prepare("SELECT * FROM frame_storage").is_ok());
        assert_eq!(
            db.pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))
                .unwrap(),
            SCHEMA_VERSION
        );
        assert!(
            initialize_database_with_backup(&db, &path)
                .unwrap()
                .is_none()
        );
        drop(old);
        drop(db);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[actix_web::test]
    async fn http_reports_and_readings_require_the_correct_device_or_owner() {
        use actix_web::{App, cookie::Cookie, http::StatusCode, test};
        let mut db = Connection::open_in_memory().unwrap();
        initialize_database(&db).unwrap();
        let device = uuid::Uuid::new_v4().to_string();
        let credential = "storage-http-device-abcdefghijklmnopqrstuvwxyz";
        let bootstrap = "storage-http-bootstrap-abcdefghijklmnopqrstuvwxyz";
        register_bootstrap_device(&db, &device, "AB23CD", credential, bootstrap).unwrap();
        let owner = claim_bootstrap_device(&mut db, bootstrap, "Kitchen").unwrap();
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(AppState::new(db, "unused".into())))
                .configure(configure_beta_routes),
        )
        .await;
        let url = format!("/api/beta/owner/frames/{device}/storage");
        let req = test::TestRequest::get().uri(&url).to_request();
        assert_eq!(
            test::call_service(&app, req).await.status(),
            StatusCode::UNAUTHORIZED
        );
        let req = test::TestRequest::get()
            .uri(&url)
            .insert_header(("Authorization", format!("Bearer {credential}")))
            .to_request();
        assert_eq!(
            test::call_service(&app, req).await.status(),
            StatusCode::UNAUTHORIZED
        );
        let req = test::TestRequest::post()
            .uri("/api/beta/device/storage")
            .cookie(Cookie::new("astrohud_owner", &owner.owner_token))
            .set_json(report())
            .to_request();
        assert_eq!(
            test::call_service(&app, req).await.status(),
            StatusCode::UNAUTHORIZED
        );
        let req = test::TestRequest::post()
            .uri("/api/beta/device/storage")
            .insert_header(("Authorization", format!("Bearer {credential}")))
            .set_json(report())
            .to_request();
        assert_eq!(
            test::call_service(&app, req).await.status(),
            StatusCode::NO_CONTENT
        );
        let req = test::TestRequest::get()
            .uri(&url)
            .cookie(Cookie::new("astrohud_owner", &owner.owner_token))
            .to_request();
        let response = test::call_service(&app, req).await;
        assert_eq!(response.headers().get("cache-control").unwrap(), "no-store");
        let body: serde_json::Value = test::read_body_json(response).await;
        assert_eq!(body["estimated_photos_remaining"], 720);
    }

    #[test]
    fn estimates_respect_allowance_disk_reserve_and_missing_samples() {
        let mut r = report();
        assert_eq!(r.estimate(), Some(720));
        r.disk_available_bytes = r.reserve_bytes + 10 * MIB;
        assert_eq!(r.estimate(), Some(8));
        r.disk_available_bytes = r.reserve_bytes;
        assert_eq!(r.estimate(), Some(0));
        assert_eq!(r.space_status(), "low");
        r.disk_available_bytes = 50 * MIB;
        assert_eq!(r.space_status(), "very_low");
        r.disk_available_bytes = 1_000 * MIB;
        r.photo_bytes = r.photo_limit_bytes + 1;
        assert_eq!(r.estimate(), Some(0));
        assert_eq!(r.space_status(), "healthy");
        r.photo_bytes = 0;
        r.photo_count = 0;
        assert_eq!(r.estimate(), None);
    }
    #[test]
    fn reports_are_authenticated_owner_scoped_and_replace_only_the_latest_snapshot() {
        let mut db = Connection::open_in_memory().unwrap();
        initialize_database(&db).unwrap();
        let device = uuid::Uuid::new_v4().to_string();
        let credential = "storage-device-secret-abcdefghijklmnopqrstuvwxyz";
        let bootstrap = "storage-bootstrap-secret-abcdefghijklmnopqrstuvwxyz";
        register_bootstrap_device(&db, &device, "AB23CD", credential, bootstrap).unwrap();
        let owner = claim_bootstrap_device(&mut db, bootstrap, "Kitchen").unwrap();
        let other = create_household_owner(&mut db, "Other", "Owner").unwrap();
        assert!(owner_status(&db, &owner.owner_token, &device).unwrap()["report"].is_null());
        assert!(store_report(&db, "wrong", &report()).is_err());
        store_report(&db, credential, &report()).unwrap();
        assert!(matches!(
            owner_status(&db, &other.owner_token, &device),
            Err(BetaError::NotFound)
        ));
        let mut latest = report();
        latest.photo_count = 101;
        store_report(&db, credential, &latest).unwrap();
        assert_eq!(
            owner_status(&db, &owner.owner_token, &device).unwrap()["report"]["photo_count"],
            101
        );
        assert_eq!(
            db.query_row("SELECT COUNT(*) FROM frame_storage", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            1
        );
        latest.disk_available_bytes = u64::MAX;
        assert!(store_report(&db, credential, &latest).is_err());
        assert_eq!(
            owner_status(&db, &owner.owner_token, &device).unwrap()["report"]["photo_count"],
            101
        );
    }
}
