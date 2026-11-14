//! Owner-requested appliance updates; the device verifies releases itself.
use crate::beta_web::{bearer_token, beta_http_error, owner_token};
use crate::{AppState, BetaError, claimed_device_access, owner_household_id};
use actix_web::{HttpRequest, HttpResponse, Result, error, http::header, web};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Report {
    pub updater_version: String,
    pub installed_release: Option<String>,
    pub previous_release: Option<String>,
    pub available_release: Option<String>,
    pub available_version: Option<String>,
    pub checked_unix: Option<u64>,
    pub metadata_expires_unix: Option<u64>,
    pub last_result: Option<String>,
    pub request_id: Option<String>,
    pub request_state: Option<String>,
    pub message: Option<String>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct UpdateCommand {
    pub id: String,
    pub action: String,
    pub release_id: Option<String>,
    pub state: String,
    pub expires_unix: u64,
    pub message: Option<String>,
}
#[derive(Serialize)]
struct OwnerUpdate {
    frame_id: String,
    report: Report,
    reported_unix: Option<u64>,
    command: Option<UpdateCommand>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RequestUpdate {
    action: String,
    release_id: Option<String>,
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

pub(crate) fn expire_command(db: &Connection, frame: &str) -> rusqlite::Result<()> {
    db.execute("UPDATE frame_updates SET command_state='failed', message='Request expired. Reconnect the frame and try again.'
        WHERE frame_id=?1 AND command_state IN ('pending','running') AND expires_unix <= ?2", params![frame, now()])?;
    Ok(())
}
fn owner_frame(db: &Connection, owner: &str, frame: &str) -> std::result::Result<(), BetaError> {
    let household = owner_household_id(db, owner)?;
    let exists: bool = db.query_row(
        "SELECT EXISTS(SELECT 1 FROM frames WHERE id=?1 AND household_id=?2)",
        params![frame, household],
        |r| r.get(0),
    )?;
    if exists {
        Ok(())
    } else {
        Err(BetaError::NotFound)
    }
}
fn status(db: &Connection, frame: &str) -> std::result::Result<OwnerUpdate, BetaError> {
    expire_command(db, frame)?;
    let row = db.query_row("SELECT report, reported_unix, command_id, action, release_id, command_state, expires_unix, message FROM frame_updates WHERE frame_id=?1", [frame], |r| {
        let id: Option<String> = r.get(2)?;
        Ok((r.get::<_, String>(0)?, r.get(1)?, id.map(|id| Ok::<_, rusqlite::Error>(UpdateCommand { id, action: r.get(3)?, release_id: r.get(4)?, state: r.get(5)?, expires_unix: r.get(6)?, message: r.get(7)? })).transpose()?))
    }).optional()?;
    let (report, reported_unix, command) = match row {
        Some((report, time, command)) => (
            serde_json::from_str(&report)
                .map_err(|_| BetaError::InvalidInput("invalid device update report"))?,
            time,
            command,
        ),
        None => (Report::default(), None, None),
    };
    Ok(OwnerUpdate {
        frame_id: frame.into(),
        report,
        reported_unix,
        command,
    })
}
fn request_update(
    db: &mut Connection,
    owner: &str,
    frame: &str,
    body: &RequestUpdate,
) -> std::result::Result<OwnerUpdate, BetaError> {
    let tx = db.transaction()?;
    owner_frame(&tx, owner, frame)?;
    if !["check", "update", "rollback"].contains(&body.action.as_str()) {
        return Err(BetaError::InvalidInput("unknown update action"));
    }
    let mut current = status(&tx, frame)?;
    // A capability report comes from the authenticated appliance agent. Older
    // devices and browser simulators must not appear to accept an update.
    if current.report.updater_version.is_empty() {
        return Err(BetaError::InvalidInput(
            "this frame does not have the application updater installed",
        ));
    }
    let resetting: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM device_resets WHERE frame_id=?1 AND state!='complete')",
        [frame],
        |r| r.get(0),
    )?;
    if resetting {
        return Err(BetaError::InvalidInput(
            "finish device reset before requesting an update",
        ));
    }
    if let Some(command) = &current.command
        && ["pending", "running"].contains(&command.state.as_str())
    {
        if command.action == body.action && command.release_id == body.release_id {
            return Ok(current);
        }
        return Err(BetaError::InvalidInput(
            "another device update request is in progress",
        ));
    }
    match body.action.as_str() {
        "update" => {
            if body.release_id.is_none()
                || body.release_id != current.report.available_release
                || current.report.metadata_expires_unix.unwrap_or(0) <= now()
            {
                return Err(BetaError::InvalidInput(
                    "check for updates again before installing this release",
                ));
            }
            if body.release_id == current.report.installed_release {
                return Err(BetaError::InvalidInput("this release is already installed"));
            }
        }
        "rollback"
            if body.release_id.is_none() || body.release_id != current.report.previous_release =>
        {
            return Err(BetaError::InvalidInput(
                "the previous release is no longer available",
            ));
        }
        "check" if body.release_id.is_some() => {
            return Err(BetaError::InvalidInput(
                "check requests do not select a release",
            ));
        }
        _ => (),
    }
    let command = UpdateCommand {
        id: Uuid::new_v4().to_string(),
        action: body.action.clone(),
        release_id: body.release_id.clone(),
        state: "pending".into(),
        expires_unix: now() + 1800,
        message: None,
    };
    tx.execute("UPDATE frame_updates SET command_id=?1, action=?2, release_id=?3, command_state='pending', requested_unix=?4, expires_unix=?5, message=NULL WHERE frame_id=?6", params![command.id, command.action, command.release_id, now(), command.expires_unix, frame])?;
    current.command = Some(command);
    tx.commit()?;
    Ok(current)
}
fn device_report(
    db: &mut Connection,
    credential: &str,
    report: &Report,
) -> std::result::Result<Option<UpdateCommand>, BetaError> {
    let tx = db.transaction()?;
    let access = claimed_device_access(&tx, credential)?;
    let text =
        serde_json::to_string(report).map_err(|_| BetaError::InvalidInput("invalid report"))?;
    if text.len() > 8192 || report.updater_version.is_empty() || report.updater_version.len() > 40 {
        return Err(BetaError::InvalidInput(
            "invalid update report size or updater version",
        ));
    }
    for id in [
        &report.installed_release,
        &report.previous_release,
        &report.available_release,
    ]
    .into_iter()
    .flatten()
    {
        if id.len() > 100
            || id.is_empty()
            || !id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'.')
        {
            return Err(BetaError::InvalidInput("invalid release identifier"));
        }
    }
    if let Some(state) = &report.request_state
        && (!["running", "succeeded", "failed"].contains(&state.as_str())
            || report
                .request_id
                .as_ref()
                .is_none_or(|id| Uuid::parse_str(id).is_err()))
    {
        return Err(BetaError::InvalidInput("invalid update request result"));
    }
    tx.execute("INSERT INTO frame_updates (frame_id,report,reported_unix) VALUES (?1,?2,?3) ON CONFLICT(frame_id) DO UPDATE SET report=excluded.report,reported_unix=excluded.reported_unix", params![access.frame_id, text, now()])?;
    expire_command(&tx, &access.frame_id)?;
    if let (Some(id), Some(state)) = (&report.request_id, &report.request_state) {
        tx.execute("UPDATE frame_updates SET command_state=?1,message=?2 WHERE frame_id=?3 AND command_id=?4 AND command_state IN ('pending','running')", params![state, report.message.as_deref().map(|s| s.chars().take(500).collect::<String>()), access.frame_id, id])?;
    }
    let command = status(&tx, &access.frame_id)?
        .command
        .filter(|c| ["pending", "running"].contains(&c.state.as_str()));
    tx.commit()?;
    Ok(command)
}

pub fn configure_update_routes(config: &mut web::ServiceConfig) {
    config
        .service(
            web::resource("/api/beta/owner/frames/{frame}/updates")
                .app_data(web::JsonConfig::default().limit(16384))
                .route(web::get().to(get_status))
                .route(web::post().to(post_request)),
        )
        .service(
            web::resource("/api/beta/device/updates")
                .app_data(web::JsonConfig::default().limit(16384))
                .route(web::post().to(post_report)),
        );
}
async fn get_status(
    state: web::Data<AppState>,
    request: HttpRequest,
    frame: web::Path<String>,
) -> Result<HttpResponse> {
    let token = owner_token(&request)?;
    let db = state
        .database
        .lock()
        .map_err(|_| error::ErrorInternalServerError("database lock poisoned"))?;
    owner_frame(&db, &token, &frame).map_err(beta_http_error)?;
    Ok(HttpResponse::Ok()
        .insert_header((header::CACHE_CONTROL, "no-store"))
        .json(status(&db, &frame).map_err(beta_http_error)?))
}
async fn post_request(
    state: web::Data<AppState>,
    request: HttpRequest,
    frame: web::Path<String>,
    body: web::Json<RequestUpdate>,
) -> Result<HttpResponse> {
    let token = owner_token(&request)?;
    let mut db = state
        .database
        .lock()
        .map_err(|_| error::ErrorInternalServerError("database lock poisoned"))?;
    Ok(HttpResponse::Accepted()
        .insert_header((header::CACHE_CONTROL, "no-store"))
        .json(request_update(&mut db, &token, &frame, &body).map_err(beta_http_error)?))
}
async fn post_report(
    state: web::Data<AppState>,
    request: HttpRequest,
    body: web::Json<Report>,
) -> Result<HttpResponse> {
    let token = bearer_token(&request)?;
    let mut db = state
        .database
        .lock()
        .map_err(|_| error::ErrorInternalServerError("database lock poisoned"))?;
    let command = device_report(&mut db, &token, &body).map_err(beta_http_error)?;
    Ok(HttpResponse::Ok()
        .insert_header((header::CACHE_CONTROL, "no-store"))
        .json(serde_json::json!({"command":command})))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::*;
    fn fixture() -> (Connection, String, String, String, String) {
        let mut db = Connection::open_in_memory().unwrap();
        initialize_database(&db).unwrap();
        let credential = "updater-device-credential-abcdefghijklmnopqrstuvwxyz".to_string();
        let bootstrap = "updater-bootstrap-secret-abcdefghijklmnopqrstuvwxyz";
        let frame = Uuid::new_v4().to_string();
        register_bootstrap_device(&db, &frame, "AB23CD", &credential, bootstrap).unwrap();
        let owner = claim_bootstrap_device(&mut db, bootstrap, "Kitchen").unwrap();
        let other = create_household_owner(&mut db, "Other", "Owner").unwrap();
        (db, owner.owner_token, other.owner_token, credential, frame)
    }
    fn report() -> Report {
        Report {
            updater_version: "0.1.0".into(),
            installed_release: Some("baseline".into()),
            previous_release: None,
            available_release: Some("6-0.6.0".into()),
            available_version: Some("0.6.0".into()),
            metadata_expires_unix: Some(now() + 3600),
            ..Default::default()
        }
    }
    fn install() -> RequestUpdate {
        RequestUpdate {
            action: "update".into(),
            release_id: Some("6-0.6.0".into()),
        }
    }
    #[test]
    fn update_is_owner_scoped_durable_idempotent_and_acknowledged_only_by_its_device() {
        let (mut db, owner, other, credential, frame) = fixture();
        assert!(request_update(&mut db, &owner, &frame, &install()).is_err());
        assert!(device_report(&mut db, "wrong", &report()).is_err());
        device_report(&mut db, &credential, &report()).unwrap();
        assert!(matches!(
            request_update(&mut db, &other, &frame, &install()),
            Err(BetaError::NotFound)
        ));
        let command = request_update(&mut db, &owner, &frame, &install())
            .unwrap()
            .command
            .unwrap();
        assert_eq!(command.state, "pending");
        assert_eq!(
            request_update(&mut db, &owner, &frame, &install())
                .unwrap()
                .command
                .unwrap()
                .id,
            command.id
        );
        assert!(request_device_reset(&db, &owner, &frame, "Kitchen").is_err());
        initialize_database(&db).unwrap();
        assert_eq!(
            device_report(&mut db, &credential, &report())
                .unwrap()
                .unwrap()
                .id,
            command.id
        );
        let mut heartbeat = report();
        heartbeat.request_id = Some(command.id.clone());
        heartbeat.request_state = Some("running".into());
        assert_eq!(
            device_report(&mut db, &credential, &heartbeat)
                .unwrap()
                .unwrap()
                .state,
            "running"
        );
        heartbeat.request_state = Some("succeeded".into());
        heartbeat.installed_release = Some("6-0.6.0".into());
        heartbeat.previous_release = Some("baseline".into());
        assert!(
            device_report(&mut db, &credential, &heartbeat)
                .unwrap()
                .is_none()
        );
        // Lost completion response is harmless; terminal results never regress.
        heartbeat.request_state = Some("running".into());
        assert!(
            device_report(&mut db, &credential, &heartbeat)
                .unwrap()
                .is_none()
        );
        assert_eq!(
            status(&db, &frame).unwrap().command.unwrap().state,
            "succeeded"
        );
        assert!(request_update(&mut db, &owner, &frame, &install()).is_err());
        let rollback = RequestUpdate {
            action: "rollback".into(),
            release_id: Some("baseline".into()),
        };
        assert_eq!(
            request_update(&mut db, &owner, &frame, &rollback)
                .unwrap()
                .command
                .unwrap()
                .action,
            "rollback"
        );
    }
    #[test]
    fn expiry_wrong_versions_and_reset_prevent_installation() {
        let (mut db, owner, _, credential, frame) = fixture();
        let mut heartbeat = report();
        heartbeat.metadata_expires_unix = Some(now() - 1);
        device_report(&mut db, &credential, &heartbeat).unwrap();
        assert!(request_update(&mut db, &owner, &frame, &install()).is_err());
        device_report(&mut db, &credential, &report()).unwrap();
        let wrong = RequestUpdate {
            action: "update".into(),
            release_id: Some("7-0.7.0".into()),
        };
        assert!(request_update(&mut db, &owner, &frame, &wrong).is_err());
        let command = request_update(&mut db, &owner, &frame, &install())
            .unwrap()
            .command
            .unwrap();
        db.execute(
            "UPDATE frame_updates SET expires_unix=0 WHERE frame_id=?1",
            [&frame],
        )
        .unwrap();
        assert!(
            device_report(&mut db, &credential, &report())
                .unwrap()
                .is_none()
        );
        assert_eq!(
            status(&db, &frame).unwrap().command.unwrap().state,
            "failed"
        );
        heartbeat.request_id = Some(command.id);
        heartbeat.request_state = Some("running".into());
        assert!(
            device_report(&mut db, &credential, &heartbeat)
                .unwrap()
                .is_none()
        );
        request_device_reset(&db, &owner, &frame, "Kitchen").unwrap();
        assert!(request_update(&mut db, &owner, &frame, &install()).is_err());
    }
    #[actix_web::test]
    async fn http_owner_and_device_authentication_are_separate() {
        use actix_web::{App, cookie::Cookie, http::StatusCode, test};
        let (db, owner, other, credential, frame) = fixture();
        let state = web::Data::new(AppState::new(db, "unused".into()));
        let app =
            test::init_service(App::new().app_data(state).configure(configure_beta_routes)).await;
        let url = format!("/api/beta/owner/frames/{frame}/updates");
        for request in [
            test::TestRequest::get().uri(&url).to_request(),
            test::TestRequest::post()
                .uri(&url)
                .set_json(serde_json::json!({"action":"check"}))
                .to_request(),
        ] {
            assert_eq!(
                test::call_service(&app, request).await.status(),
                StatusCode::UNAUTHORIZED
            );
        }
        let request = test::TestRequest::get()
            .uri(&url)
            .cookie(Cookie::new("astrohud_owner", other))
            .to_request();
        assert_eq!(
            test::call_service(&app, request).await.status(),
            StatusCode::NOT_FOUND
        );
        let request = test::TestRequest::post()
            .uri("/api/beta/device/updates")
            .cookie(Cookie::new("astrohud_owner", owner.clone()))
            .set_json(report())
            .to_request();
        assert_eq!(
            test::call_service(&app, request).await.status(),
            StatusCode::UNAUTHORIZED
        );
        let request = test::TestRequest::post()
            .uri("/api/beta/device/updates")
            .insert_header((header::AUTHORIZATION, format!("Bearer {credential}")))
            .set_json(report())
            .to_request();
        assert_eq!(
            test::call_service(&app, request).await.status(),
            StatusCode::OK
        );
        let request = test::TestRequest::post()
            .uri(&url)
            .cookie(Cookie::new("astrohud_owner", owner))
            .set_json(serde_json::json!({"action":"update","release_id":"6-0.6.0"}))
            .to_request();
        assert_eq!(
            test::call_service(&app, request).await.status(),
            StatusCode::ACCEPTED
        );
    }
    #[test]
    fn schema_twelve_migration_keeps_a_pre_upgrade_snapshot() {
        let dir =
            std::env::temp_dir().join(format!("memory-lane-update-migration-{}", Uuid::new_v4()));
        std::fs::create_dir(&dir).unwrap();
        let path = dir.join("db.sqlite3");
        let db = Connection::open(&path).unwrap();
        initialize_database(&db).unwrap();
        db.execute_batch("DROP TABLE frame_updates; PRAGMA user_version=12;")
            .unwrap();
        let snapshot = initialize_database_with_backup(&db, &path)
            .unwrap()
            .unwrap();
        let old = Connection::open(snapshot).unwrap();
        assert_eq!(
            old.pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))
                .unwrap(),
            12
        );
        assert!(old.prepare("SELECT * FROM frame_updates").is_err());
        assert_eq!(
            db.pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))
                .unwrap(),
            14
        );
        assert!(db.prepare("SELECT * FROM frame_updates").is_ok());
        assert!(
            initialize_database_with_backup(&db, &path)
                .unwrap()
                .is_none()
        );
        drop(old);
        drop(db);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
