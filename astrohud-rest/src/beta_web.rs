use crate::{
    AppState, BetaError, DeviceEnrollmentState, DeviceRegistration, claim_bootstrap_device,
    claim_device, claimed_device_access, create_sender_invitation, device_bootstrap_status,
    enrollment_status, owner_context, owner_household_id, protected_manifest_for_household,
    protected_media_storage_key, register_bootstrap_device, register_device,
    replace_sender_invitation, revoke_sender_invitation, sender_access, upload_invited_photo,
};
use actix_files::NamedFile;
use actix_multipart::Multipart;
use actix_web::cookie::{Cookie, SameSite, time::Duration};
use actix_web::{HttpRequest, HttpResponse, Result, error, http::header, web};
use serde::{Deserialize, Serialize};
use std::env;

const OWNER_COOKIE: &str = "astrohud_owner";
const SENDER_COOKIE: &str = "astrohud_sender";

#[derive(Deserialize)]
struct OwnerSessionRequest {
    token: String,
}

#[derive(Deserialize)]
struct ClaimDeviceRequest {
    claim_code: String,
    place_name: String,
}

#[derive(Deserialize)]
struct BootstrapTokenRequest {
    token: String,
}

#[derive(Deserialize)]
struct BootstrapClaimRequest {
    token: String,
    place_name: String,
}

#[derive(Deserialize)]
struct InvitationRequest {
    label: String,
}

#[derive(Deserialize)]
struct ResetRequest {
    confirm_name: String,
}

#[derive(Deserialize)]
struct AcceptResetRequest {
    receipt: String,
    next_credential: String,
}

#[derive(Deserialize)]
struct RegisterDeviceRequest {
    device_id: String,
    device_code: String,
    credential: String,
    bootstrap_token: Option<String>,
}

#[derive(Serialize)]
struct DeviceRegistrationResponse {
    enrollment_id: String,
    status: &'static str,
    claim_code: Option<String>,
    expires_at: Option<String>,
    frame_id: Option<String>,
}

impl From<DeviceRegistration> for DeviceRegistrationResponse {
    fn from(registration: DeviceRegistration) -> Self {
        match registration.state {
            DeviceEnrollmentState::Pending {
                claim_code,
                expires_at,
            } => Self {
                enrollment_id: registration.enrollment_id,
                status: "pending",
                claim_code: Some(claim_code),
                expires_at: Some(expires_at),
                frame_id: None,
            },
            DeviceEnrollmentState::Claimed { frame_id, .. } => Self {
                enrollment_id: registration.enrollment_id,
                status: "claimed",
                claim_code: None,
                expires_at: None,
                frame_id: Some(frame_id),
            },
        }
    }
}

pub fn configure_beta_routes(config: &mut web::ServiceConfig) {
    config
        .route(
            "/api/beta/owner/session",
            web::post().to(create_owner_session),
        )
        .route("/api/beta/owner", web::get().to(get_owner_context))
        .route("/api/beta/owner/recovery", web::post().to(create_recovery))
        .route(
            "/api/beta/owner/recovery/session",
            web::post().to(recover_session),
        )
        .route(
            "/api/beta/owner/frames/{frame_id}/reset",
            web::post().to(request_reset),
        )
        .route("/api/beta/device/reset", web::get().to(get_reset))
        .route(
            "/api/beta/device/resets/{reset_id}/accept",
            web::post().to(accept_reset),
        )
        .route(
            "/api/beta/device/resets/{reset_id}/complete",
            web::post().to(complete_reset),
        )
        .service(
            web::resource("/api/beta/owner/frames/{frame_id}/photos")
                .route(web::get().to(get_owner_photos))
                .route(web::post().to(upload_owner_photo)),
        )
        .route(
            "/api/beta/owner/frames/{frame_id}/playback",
            web::post().to(set_owner_playback),
        )
        .service(
            web::resource("/api/beta/owner/frames/{frame_id}/photos/{photo_id}")
                .route(web::get().to(get_owner_photo))
                .route(web::delete().to(remove_owner_photo)),
        )
        .route(
            "/api/beta/owner/invitations",
            web::post().to(create_invitation),
        )
        .route(
            "/api/beta/owner/invitations/{invitation_id}",
            web::delete().to(revoke_invitation),
        )
        .route(
            "/api/beta/owner/invitations/{invitation_id}/replace",
            web::post().to(replace_invitation),
        )
        .route(
            "/api/beta/owner/claim",
            web::post().to(claim_pending_device),
        )
        .route(
            "/api/beta/bootstrap/status",
            web::post().to(get_bootstrap_status),
        )
        .route("/api/beta/bootstrap/claim", web::post().to(claim_bootstrap))
        .route(
            "/api/beta/devices/enrollments",
            web::post().to(create_device_enrollment),
        )
        .route(
            "/api/beta/devices/enrollments/{enrollment_id}",
            web::get().to(get_device_enrollment),
        )
        .route(
            "/api/beta/device/manifest",
            web::get().to(get_authenticated_device_manifest),
        )
        .route(
            "/api/beta/device/media/{photo_id}",
            web::get().to(get_authenticated_device_media),
        )
        .route(
            "/api/beta/sender/session",
            web::post().to(create_sender_session),
        )
        .route("/api/beta/sender", web::get().to(get_sender_context))
        .route(
            "/api/beta/sender/photos",
            web::post().to(upload_sender_photo),
        );
}

async fn create_owner_session(
    state: web::Data<AppState>,
    request: web::Json<OwnerSessionRequest>,
) -> Result<HttpResponse> {
    let token = request.token.trim();
    if token.is_empty() || token.len() > 512 {
        return Err(error::ErrorBadRequest("activation link is invalid"));
    }
    {
        let database = state
            .database
            .lock()
            .map_err(|_| error::ErrorInternalServerError("database lock poisoned"))?;
        owner_context(&database, token).map_err(beta_http_error)?;
    }

    let cookie = access_cookie(OWNER_COOKIE, token);
    Ok(HttpResponse::NoContent().cookie(cookie).finish())
}

async fn create_invitation(
    state: web::Data<AppState>,
    http_request: HttpRequest,
    request: web::Json<InvitationRequest>,
) -> Result<HttpResponse> {
    let token = owner_token(&http_request)?;
    let database = state
        .database
        .lock()
        .map_err(|_| error::ErrorInternalServerError("database lock poisoned"))?;
    let invitation =
        create_sender_invitation(&database, &token, &request.label).map_err(beta_http_error)?;
    Ok(HttpResponse::Created()
        .insert_header((header::CACHE_CONTROL, "no-store"))
        .json(invitation))
}

async fn replace_invitation(
    state: web::Data<AppState>,
    request: HttpRequest,
    invitation_id: web::Path<String>,
) -> Result<HttpResponse> {
    let token = owner_token(&request)?;
    let database = state
        .database
        .lock()
        .map_err(|_| error::ErrorInternalServerError("database lock poisoned"))?;
    let invitation =
        replace_sender_invitation(&database, &token, &invitation_id).map_err(beta_http_error)?;
    Ok(HttpResponse::Ok()
        .insert_header((header::CACHE_CONTROL, "no-store"))
        .json(invitation))
}

async fn revoke_invitation(
    state: web::Data<AppState>,
    http_request: HttpRequest,
    invitation_id: web::Path<String>,
) -> Result<HttpResponse> {
    let token = owner_token(&http_request)?;
    let database = state
        .database
        .lock()
        .map_err(|_| error::ErrorInternalServerError("database lock poisoned"))?;
    revoke_sender_invitation(&database, &token, &invitation_id).map_err(beta_http_error)?;
    Ok(HttpResponse::NoContent().finish())
}

async fn get_owner_context(
    state: web::Data<AppState>,
    request: HttpRequest,
) -> Result<HttpResponse> {
    let token = owner_token(&request)?;
    let database = state
        .database
        .lock()
        .map_err(|_| error::ErrorInternalServerError("database lock poisoned"))?;
    let context = owner_context(&database, &token).map_err(beta_http_error)?;
    Ok(HttpResponse::Ok()
        .insert_header((header::CACHE_CONTROL, "no-store"))
        .json(context))
}

async fn create_recovery(state: web::Data<AppState>, request: HttpRequest) -> Result<HttpResponse> {
    let token = owner_token(&request)?;
    let db = state
        .database
        .lock()
        .map_err(|_| error::ErrorInternalServerError("database lock poisoned"))?;
    let recovery = crate::create_owner_recovery(&db, &token).map_err(beta_http_error)?;
    Ok(HttpResponse::Created()
        .insert_header((header::CACHE_CONTROL, "no-store"))
        .json(recovery))
}

async fn recover_session(
    state: web::Data<AppState>,
    body: web::Json<OwnerSessionRequest>,
) -> Result<HttpResponse> {
    let mut db = state
        .database
        .lock()
        .map_err(|_| error::ErrorInternalServerError("database lock poisoned"))?;
    let token = crate::recover_owner(&mut db, &body.token).map_err(beta_http_error)?;
    Ok(HttpResponse::NoContent()
        .insert_header((header::CACHE_CONTROL, "no-store"))
        .cookie(access_cookie(OWNER_COOKIE, &token))
        .finish())
}

async fn request_reset(
    state: web::Data<AppState>,
    request: HttpRequest,
    frame: web::Path<String>,
    body: web::Json<ResetRequest>,
) -> Result<HttpResponse> {
    let token = owner_token(&request)?;
    let db = state
        .database
        .lock()
        .map_err(|_| error::ErrorInternalServerError("database lock poisoned"))?;
    let reset = crate::request_device_reset(&db, &token, &frame, &body.confirm_name)
        .map_err(beta_http_error)?;
    Ok(HttpResponse::Accepted()
        .insert_header((header::CACHE_CONTROL, "no-store"))
        .json(reset))
}

async fn get_reset(state: web::Data<AppState>, request: HttpRequest) -> Result<HttpResponse> {
    let credential = bearer_token(&request)?;
    let db = state
        .database
        .lock()
        .map_err(|_| error::ErrorInternalServerError("database lock poisoned"))?;
    let reset = crate::pending_device_reset(&db, &credential).map_err(beta_http_error)?;
    Ok(HttpResponse::Ok()
        .insert_header((header::CACHE_CONTROL, "no-store"))
        .json(serde_json::json!({"reset": reset})))
}

async fn accept_reset(
    state: web::Data<AppState>,
    request: HttpRequest,
    id: web::Path<String>,
    body: web::Json<AcceptResetRequest>,
) -> Result<HttpResponse> {
    let credential = bearer_token(&request)?;
    let db = state
        .database
        .lock()
        .map_err(|_| error::ErrorInternalServerError("database lock poisoned"))?;
    crate::accept_device_reset(&db, &credential, &id, &body.receipt, &body.next_credential)
        .map_err(beta_http_error)?;
    Ok(HttpResponse::NoContent().finish())
}

async fn complete_reset(
    state: web::Data<AppState>,
    request: HttpRequest,
    id: web::Path<String>,
) -> Result<HttpResponse> {
    let receipt = bearer_token(&request)?;
    let mut db = state
        .database
        .lock()
        .map_err(|_| error::ErrorInternalServerError("database lock poisoned"))?;
    crate::complete_device_reset(&mut db, &id, &receipt).map_err(beta_http_error)?;
    Ok(HttpResponse::NoContent().finish())
}

async fn upload_owner_photo(
    state: web::Data<AppState>,
    request: HttpRequest,
    frame: web::Path<String>,
    multipart: Multipart,
) -> Result<HttpResponse> {
    let token = owner_token(&request)?;
    let (household, channel) = {
        let db = state
            .database
            .lock()
            .map_err(|_| error::ErrorInternalServerError("database lock poisoned"))?;
        crate::owner_frame_channel(&db, &token, &frame).map_err(beta_http_error)?
    };
    let photo = upload_invited_photo(state, multipart, household, channel).await?;
    Ok(HttpResponse::Created().json(serde_json::json!({"status":"received","photo_id":photo.id})))
}

async fn set_owner_playback(
    state: web::Data<AppState>,
    request: HttpRequest,
    frame: web::Path<String>,
    body: web::Json<crate::PlaybackRequest>,
) -> Result<HttpResponse> {
    let token = owner_token(&request)?;
    let mut db = state
        .database
        .lock()
        .map_err(|_| error::ErrorInternalServerError("database lock poisoned"))?;
    let playback =
        crate::update_playback(&mut db, &token, &frame, &body).map_err(beta_http_error)?;
    Ok(HttpResponse::Ok()
        .insert_header((header::CACHE_CONTROL, "no-store"))
        .json(playback))
}

async fn get_owner_photos(
    state: web::Data<AppState>,
    request: HttpRequest,
    frame_id: web::Path<String>,
) -> Result<HttpResponse> {
    let token = owner_token(&request)?;
    let database = state
        .database
        .lock()
        .map_err(|_| error::ErrorInternalServerError("database lock poisoned"))?;
    let household_id = owner_household_id(&database, &token).map_err(beta_http_error)?;
    let mut manifest = protected_manifest_for_household(&database, &household_id, &frame_id)
        .map_err(error::ErrorInternalServerError)?
        .ok_or_else(|| error::ErrorNotFound("frame not found"))?;
    for photo in &mut manifest.photos {
        photo.url = format!("/api/beta/owner/frames/{frame_id}/photos/{}", photo.id);
    }
    Ok(HttpResponse::Ok()
        .insert_header((header::CACHE_CONTROL, "no-store"))
        .json(manifest))
}

async fn get_owner_photo(
    state: web::Data<AppState>,
    request: HttpRequest,
    path: web::Path<(String, String)>,
) -> Result<HttpResponse> {
    let token = owner_token(&request)?;
    let (frame_id, photo_id) = path.into_inner();
    let storage_key = {
        let database = state
            .database
            .lock()
            .map_err(|_| error::ErrorInternalServerError("database lock poisoned"))?;
        let household_id = owner_household_id(&database, &token).map_err(beta_http_error)?;
        protected_media_storage_key(&database, &household_id, &frame_id, &photo_id)
            .map_err(error::ErrorInternalServerError)?
            .ok_or_else(|| error::ErrorNotFound("photo not found"))?
    };
    let file = NamedFile::open_async(state.media_dir.join(storage_key))
        .await
        .map_err(|err| match err.kind() {
            std::io::ErrorKind::NotFound => error::ErrorNotFound("photo not found"),
            _ => error::ErrorInternalServerError(err),
        })?;
    let mut response = file.into_response(&request);
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        header::HeaderValue::from_static("no-store"),
    );
    Ok(response)
}

async fn remove_owner_photo(
    state: web::Data<AppState>,
    request: HttpRequest,
    path: web::Path<(String, String)>,
) -> Result<HttpResponse> {
    let token = owner_token(&request)?;
    let (frame_id, photo_id) = path.into_inner();
    let database = state
        .database
        .lock()
        .map_err(|_| error::ErrorInternalServerError("database lock poisoned"))?;
    let household_id = owner_household_id(&database, &token).map_err(beta_http_error)?;
    // Validate membership independently of the removal record so retries succeed.
    let changed = database
        .execute(
            "INSERT OR IGNORE INTO frame_photo_removals (frame_id, photo_id)
         SELECT f.id, p.id FROM frames f
         JOIN frame_subscriptions s ON s.frame_id = f.id
         JOIN photos p ON p.channel_id = s.channel_id
         JOIN channels c ON c.id = p.channel_id
         WHERE f.id = ?1 AND p.id = ?2 AND f.household_id = ?3 AND c.household_id = ?3",
            rusqlite::params![frame_id, photo_id, household_id],
        )
        .map_err(error::ErrorInternalServerError)?;
    if changed == 0 {
        let already_removed: bool = database
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM frame_photo_removals r
             JOIN frames f ON f.id = r.frame_id
             WHERE r.frame_id = ?1 AND r.photo_id = ?2 AND f.household_id = ?3)",
                rusqlite::params![frame_id, photo_id, household_id],
                |row| row.get(0),
            )
            .map_err(error::ErrorInternalServerError)?;
        if !already_removed {
            return Err(error::ErrorNotFound("photo not found"));
        }
    }
    Ok(HttpResponse::NoContent().finish())
}

async fn claim_pending_device(
    state: web::Data<AppState>,
    http_request: HttpRequest,
    request: web::Json<ClaimDeviceRequest>,
) -> Result<HttpResponse> {
    let token = owner_token(&http_request)?;
    let mut database = state
        .database
        .lock()
        .map_err(|_| error::ErrorInternalServerError("database lock poisoned"))?;
    claim_device(
        &mut database,
        &token,
        &request.claim_code,
        &request.place_name,
    )
    .map_err(beta_http_error)?;
    let context = owner_context(&database, &token).map_err(beta_http_error)?;
    Ok(HttpResponse::Ok().json(context))
}

async fn get_bootstrap_status(
    state: web::Data<AppState>,
    request: web::Json<BootstrapTokenRequest>,
) -> Result<HttpResponse> {
    let database = state
        .database
        .lock()
        .map_err(|_| error::ErrorInternalServerError("database lock poisoned"))?;
    let status =
        device_bootstrap_status(&database, request.token.trim()).map_err(beta_http_error)?;
    Ok(HttpResponse::Ok().json(status))
}

async fn claim_bootstrap(
    state: web::Data<AppState>,
    request: web::Json<BootstrapClaimRequest>,
) -> Result<HttpResponse> {
    let mut database = state
        .database
        .lock()
        .map_err(|_| error::ErrorInternalServerError("database lock poisoned"))?;
    let claim = claim_bootstrap_device(&mut database, request.token.trim(), &request.place_name)
        .map_err(beta_http_error)?;
    let context = owner_context(&database, &claim.owner_token).map_err(beta_http_error)?;
    Ok(HttpResponse::Ok()
        .cookie(access_cookie(OWNER_COOKIE, &claim.owner_token))
        .json(context))
}

async fn create_device_enrollment(
    state: web::Data<AppState>,
    request: web::Json<RegisterDeviceRequest>,
) -> Result<HttpResponse> {
    let database = state
        .database
        .lock()
        .map_err(|_| error::ErrorInternalServerError("database lock poisoned"))?;
    let registration = match request.bootstrap_token.as_deref() {
        Some(bootstrap_token) => register_bootstrap_device(
            &database,
            &request.device_id,
            &request.device_code,
            &request.credential,
            bootstrap_token,
        ),
        None => register_device(
            &database,
            &request.device_id,
            &request.device_code,
            &request.credential,
        ),
    }
    .map_err(beta_http_error)?;
    Ok(HttpResponse::Ok().json(DeviceRegistrationResponse::from(registration)))
}

async fn get_device_enrollment(
    state: web::Data<AppState>,
    request: HttpRequest,
    enrollment_id: web::Path<String>,
) -> Result<HttpResponse> {
    let credential = bearer_token(&request)?;
    let database = state
        .database
        .lock()
        .map_err(|_| error::ErrorInternalServerError("database lock poisoned"))?;
    let registration =
        enrollment_status(&database, &enrollment_id, &credential).map_err(beta_http_error)?;
    Ok(HttpResponse::Ok().json(DeviceRegistrationResponse::from(registration)))
}

async fn get_authenticated_device_manifest(
    state: web::Data<AppState>,
    request: HttpRequest,
) -> Result<HttpResponse> {
    let credential = bearer_token(&request)?;
    let database = state
        .database
        .lock()
        .map_err(|_| error::ErrorInternalServerError("database lock poisoned"))?;
    let access = claimed_device_access(&database, &credential).map_err(beta_http_error)?;
    let manifest =
        protected_manifest_for_household(&database, &access.household_id, &access.frame_id)
            .map_err(error::ErrorInternalServerError)?
            .ok_or_else(|| error::ErrorNotFound("frame not found"))?;
    Ok(HttpResponse::Ok().json(manifest))
}

async fn get_authenticated_device_media(
    state: web::Data<AppState>,
    request: HttpRequest,
    photo_id: web::Path<String>,
) -> Result<NamedFile> {
    let credential = bearer_token(&request)?;
    let storage_key = {
        let database = state
            .database
            .lock()
            .map_err(|_| error::ErrorInternalServerError("database lock poisoned"))?;
        let access = claimed_device_access(&database, &credential).map_err(beta_http_error)?;
        protected_media_storage_key(&database, &access.household_id, &access.frame_id, &photo_id)
            .map_err(error::ErrorInternalServerError)?
            .ok_or_else(|| error::ErrorNotFound("photo not found"))?
    };
    NamedFile::open_async(state.media_dir.join(storage_key))
        .await
        .map_err(|err| match err.kind() {
            std::io::ErrorKind::NotFound => error::ErrorNotFound("photo not found"),
            _ => error::ErrorInternalServerError(err),
        })
}

async fn create_sender_session(
    state: web::Data<AppState>,
    request: web::Json<OwnerSessionRequest>,
) -> Result<HttpResponse> {
    let token = request.token.trim();
    if token.is_empty() || token.len() > 512 {
        return Err(error::ErrorBadRequest("invitation link is invalid"));
    }
    {
        let database = state
            .database
            .lock()
            .map_err(|_| error::ErrorInternalServerError("database lock poisoned"))?;
        sender_access(&database, token).map_err(beta_http_error)?;
    }

    Ok(HttpResponse::NoContent()
        .cookie(access_cookie(SENDER_COOKIE, token))
        .finish())
}

async fn get_sender_context(
    state: web::Data<AppState>,
    request: HttpRequest,
) -> Result<HttpResponse> {
    let token = sender_token(&request)?;
    let database = state
        .database
        .lock()
        .map_err(|_| error::ErrorInternalServerError("database lock poisoned"))?;
    let access = sender_access(&database, &token).map_err(beta_http_error)?;
    Ok(HttpResponse::Ok().json(access.context))
}

async fn upload_sender_photo(
    state: web::Data<AppState>,
    request: HttpRequest,
    multipart: Multipart,
) -> Result<HttpResponse> {
    let token = sender_token(&request)?;
    let access = {
        let database = state
            .database
            .lock()
            .map_err(|_| error::ErrorInternalServerError("database lock poisoned"))?;
        sender_access(&database, &token).map_err(beta_http_error)?
    };
    let photo =
        upload_invited_photo(state, multipart, access.household_id, access.channel_id).await?;
    Ok(HttpResponse::Created().json(serde_json::json!({
        "status": "received",
        "photo_id": photo.id
    })))
}

fn owner_token(request: &HttpRequest) -> Result<String> {
    request
        .cookie(OWNER_COOKIE)
        .map(|cookie| cookie.value().to_owned())
        .ok_or_else(|| error::ErrorUnauthorized("owner activation required"))
}

fn sender_token(request: &HttpRequest) -> Result<String> {
    request
        .cookie(SENDER_COOKIE)
        .map(|cookie| cookie.value().to_owned())
        .ok_or_else(|| error::ErrorUnauthorized("sender invitation required"))
}

fn access_cookie(name: &'static str, token: &str) -> Cookie<'static> {
    Cookie::build(name, token.to_owned())
        .http_only(true)
        .same_site(SameSite::Strict)
        .secure(secure_cookies())
        .path("/")
        .max_age(Duration::days(30))
        .finish()
}

fn bearer_token(request: &HttpRequest) -> Result<String> {
    request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .ok_or_else(|| error::ErrorUnauthorized("device credential required"))
}

fn beta_http_error(error_value: BetaError) -> actix_web::Error {
    match error_value {
        BetaError::InvalidInput(message) => error::ErrorBadRequest(message),
        BetaError::NotFound => error::ErrorNotFound("record not found"),
        BetaError::Unauthorized => error::ErrorUnauthorized("credential is not authorized"),
        BetaError::ClaimUnavailable => {
            error::ErrorConflict("claim code is invalid, expired, or already used")
        }
        BetaError::Database(database_error) => error::ErrorInternalServerError(database_error),
    }
}

fn secure_cookies() -> bool {
    env::var("ASTROHUD_SECURE_COOKIES")
        .is_ok_and(|value| matches!(value.as_str(), "1" | "true" | "yes"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{create_household_owner, create_sender_invitation, initialize_database};
    use actix_web::{App, http::StatusCode, test};
    use rusqlite::Connection;
    use std::path::PathBuf;
    use uuid::Uuid;

    #[actix_web::test]
    async fn owner_upload_uses_the_frames_destination_and_playback_rejects_foreign_owners() {
        let mut database = Connection::open_in_memory().unwrap();
        initialize_database(&database).unwrap();
        let owner = create_household_owner(&mut database, "Home", "Owner").unwrap();
        let other = create_household_owner(&mut database, "Other", "Owner").unwrap();
        database
            .execute(
                "INSERT INTO frames (id,household_id,place_name) VALUES ('tv',?1,'TV')",
                [&owner.household_id],
            )
            .unwrap();
        database
            .execute(
                "INSERT INTO frame_subscriptions VALUES ('tv',?1)",
                [&owner.default_channel_id],
            )
            .unwrap();
        let directory =
            std::env::temp_dir().join(format!("memory-owner-upload-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&directory).unwrap();
        let state = web::Data::new(AppState::new(database, directory.clone()));
        let app = test::init_service(
            App::new()
                .app_data(state.clone())
                .configure(configure_beta_routes),
        )
        .await;
        let settings = "/api/beta/owner/frames/tv/playback";
        let anonymous = test::TestRequest::post()
            .uri(settings)
            .set_json(serde_json::json!({"slide_seconds":30}))
            .to_request();
        assert_eq!(
            test::call_service(&app, anonymous).await.status(),
            StatusCode::UNAUTHORIZED
        );
        let foreign = test::TestRequest::post()
            .uri(settings)
            .cookie(Cookie::new(OWNER_COOKIE, other.owner_token.clone()))
            .set_json(serde_json::json!({"slide_seconds":30}))
            .to_request();
        assert_eq!(
            test::call_service(&app, foreign).await.status(),
            StatusCode::NOT_FOUND
        );
        let mut png = std::io::Cursor::new(Vec::new());
        image::DynamicImage::new_rgb8(2, 2)
            .write_to(&mut png, image::ImageFormat::Png)
            .unwrap();
        let mut body=format!("--boundary\r\nContent-Disposition: form-data; name=\"channel_id\"\r\n\r\n{}\r\n--boundary\r\nContent-Disposition: form-data; name=\"photo\"; filename=\"photo.png\"\r\nContent-Type: image/png\r\n\r\n",other.default_channel_id).into_bytes();
        body.extend(png.into_inner());
        body.extend(b"\r\n--boundary--\r\n");
        for cookie in [
            None,
            Some(Cookie::new(SENDER_COOKIE, "sender-token")),
            Some(Cookie::new(OWNER_COOKIE, other.owner_token)),
        ] {
            let mut request = test::TestRequest::post()
                .uri("/api/beta/owner/frames/tv/photos")
                .insert_header((
                    header::CONTENT_TYPE,
                    "multipart/form-data; boundary=boundary",
                ))
                .set_payload(body.clone());
            let expected = if cookie.as_ref().is_some_and(|c| c.name() == OWNER_COOKIE) {
                StatusCode::NOT_FOUND
            } else {
                StatusCode::UNAUTHORIZED
            };
            if let Some(cookie) = cookie {
                request = request.cookie(cookie);
            }
            assert_eq!(
                test::call_service(&app, request.to_request())
                    .await
                    .status(),
                expected
            );
        }
        let upload = test::TestRequest::post()
            .uri("/api/beta/owner/frames/tv/photos")
            .cookie(Cookie::new(OWNER_COOKIE, owner.owner_token.clone()))
            .insert_header((
                header::CONTENT_TYPE,
                "multipart/form-data; boundary=boundary",
            ))
            .set_payload(body)
            .to_request();
        let response = test::call_service(&app, upload).await;
        assert_eq!(response.status(), StatusCode::CREATED);
        let result: serde_json::Value = test::read_body_json(response).await;
        let id = result["photo_id"].as_str().unwrap();
        let command=test::TestRequest::post().uri(settings).cookie(Cookie::new(OWNER_COOKIE,owner.owner_token.clone()))
            .set_json(serde_json::json!({"action":"pin","photo_id":id,"pin_seconds":900,"slide_seconds":30})).to_request();
        assert_eq!(
            test::call_service(&app, command).await.status(),
            StatusCode::OK
        );
        {
            let db = state.database.lock().unwrap();
            let channel: String = db
                .query_row("SELECT channel_id FROM photos WHERE id=?1", [id], |r| {
                    r.get(0)
                })
                .unwrap();
            assert_eq!(channel, owner.default_channel_id);
            assert_eq!(
                crate::frame_playback(&db, "tv")
                    .unwrap()
                    .pinned_photo_id
                    .as_deref(),
                Some(id)
            );
        }
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[actix_web::test]
    async fn saved_recovery_link_restores_a_fresh_browser_and_replacement_keeps_sessions() {
        let mut database = Connection::open_in_memory().unwrap();
        initialize_database(&database).unwrap();
        let owner = create_household_owner(&mut database, "Saved household", "Owner").unwrap();
        let state = web::Data::new(AppState::new(database, PathBuf::from("unused")));
        let app =
            test::init_service(App::new().app_data(state).configure(configure_beta_routes)).await;
        let anonymous = test::TestRequest::post()
            .uri("/api/beta/owner/recovery")
            .to_request();
        assert_eq!(
            test::call_service(&app, anonymous).await.status(),
            StatusCode::UNAUTHORIZED
        );
        let sender = test::TestRequest::post()
            .uri("/api/beta/owner/recovery")
            .cookie(Cookie::new(SENDER_COOKIE, owner.owner_token.clone()))
            .to_request();
        assert_eq!(
            test::call_service(&app, sender).await.status(),
            StatusCode::UNAUTHORIZED
        );
        let create = test::TestRequest::post()
            .uri("/api/beta/owner/recovery")
            .cookie(Cookie::new(OWNER_COOKIE, owner.owner_token.clone()))
            .to_request();
        let response = test::call_service(&app, create).await;
        assert_eq!(response.status(), StatusCode::CREATED);
        assert_eq!(
            response.headers().get(header::CACHE_CONTROL).unwrap(),
            "no-store"
        );
        let link: serde_json::Value = test::read_body_json(response).await;
        let token = link["token"].as_str().unwrap();
        let cleared = test::TestRequest::get().uri("/api/beta/owner").to_request();
        assert_eq!(
            test::call_service(&app, cleared).await.status(),
            StatusCode::UNAUTHORIZED
        );
        let restore = test::TestRequest::post()
            .uri("/api/beta/owner/recovery/session")
            .set_json(serde_json::json!({"token": token}))
            .to_request();
        let response = test::call_service(&app, restore).await;
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
        assert_eq!(
            response.headers().get(header::CACHE_CONTROL).unwrap(),
            "no-store"
        );
        let cookie = response
            .response()
            .cookies()
            .find(|c| c.name() == OWNER_COOKIE)
            .unwrap()
            .into_owned();
        assert_ne!(cookie.value(), token);
        assert!(cookie.http_only().unwrap());
        assert_eq!(cookie.same_site(), Some(SameSite::Strict));
        let restored = test::TestRequest::get()
            .uri("/api/beta/owner")
            .cookie(cookie.clone())
            .to_request();
        let response = test::call_service(&app, restored).await;
        assert_eq!(response.status(), StatusCode::OK);
        let context: serde_json::Value = test::read_body_json(response).await;
        assert_eq!(context["household_name"], "Saved household");
        assert!(context["recovery"].get("token").is_none());
        assert!(context["recovery"].get("token_hash").is_none());
        let rotate = test::TestRequest::post()
            .uri("/api/beta/owner/recovery")
            .cookie(cookie.clone())
            .to_request();
        assert_eq!(
            test::call_service(&app, rotate).await.status(),
            StatusCode::CREATED
        );
        let old = test::TestRequest::post()
            .uri("/api/beta/owner/recovery/session")
            .set_json(serde_json::json!({"token": token}))
            .to_request();
        assert_eq!(
            test::call_service(&app, old).await.status(),
            StatusCode::UNAUTHORIZED
        );
        let still_signed_in = test::TestRequest::get()
            .uri("/api/beta/owner")
            .cookie(cookie)
            .to_request();
        assert_eq!(
            test::call_service(&app, still_signed_in).await.status(),
            StatusCode::OK
        );
    }

    #[actix_web::test]
    async fn reset_http_flow_requires_owner_confirmation_and_device_acknowledgement() {
        let mut database = Connection::open_in_memory().unwrap();
        initialize_database(&database).unwrap();
        let credential = "reset-http-device-credential-abcdefghijklmnopqrstuvwxyz";
        let bootstrap = "reset-http-bootstrap-token-abcdefghijklmnopqrstuvwxyz";
        let next = "reset-http-next-credential-abcdefghijklmnopqrstuvwxyz";
        let receipt = "reset-http-receipt-token-abcdefghijklmnopqrstuvwxyz";
        let device_id = Uuid::new_v4().to_string();
        register_bootstrap_device(&database, &device_id, "AB23CD", credential, bootstrap).unwrap();
        let claim = claim_bootstrap_device(&mut database, bootstrap, "Kitchen").unwrap();
        let state = web::Data::new(AppState::new(database, PathBuf::from("unused")));
        let app =
            test::init_service(App::new().app_data(state).configure(configure_beta_routes)).await;
        let uri = format!("/api/beta/owner/frames/{device_id}/reset");
        let anonymous = test::TestRequest::post()
            .uri(&uri)
            .set_json(serde_json::json!({"confirm_name": "Kitchen"}))
            .to_request();
        assert_eq!(
            test::call_service(&app, anonymous).await.status(),
            StatusCode::UNAUTHORIZED
        );
        let wrong_name = test::TestRequest::post()
            .uri(&uri)
            .cookie(Cookie::new(OWNER_COOKIE, claim.owner_token.clone()))
            .set_json(serde_json::json!({"confirm_name": "wrong"}))
            .to_request();
        assert_eq!(
            test::call_service(&app, wrong_name).await.status(),
            StatusCode::BAD_REQUEST
        );
        let request = test::TestRequest::post()
            .uri(&uri)
            .cookie(Cookie::new(OWNER_COOKIE, claim.owner_token.clone()))
            .set_json(serde_json::json!({"confirm_name": "Kitchen"}))
            .to_request();
        let response = test::call_service(&app, request).await;
        assert_eq!(response.status(), StatusCode::ACCEPTED);
        let reset: serde_json::Value = test::read_body_json(response).await;
        assert_eq!(reset["state"], "pending");
        let id = reset["reset_id"].as_str().unwrap();
        let poll = test::TestRequest::get()
            .uri("/api/beta/device/reset")
            .insert_header((header::AUTHORIZATION, format!("Bearer {credential}")))
            .to_request();
        let response = test::call_service(&app, poll).await;
        assert_eq!(response.status(), StatusCode::OK);
        let command: serde_json::Value = test::read_body_json(response).await;
        assert_eq!(command["reset"]["reset_id"], id);
        let premature = test::TestRequest::post()
            .uri(&format!("/api/beta/device/resets/{id}/complete"))
            .insert_header((header::AUTHORIZATION, format!("Bearer {credential}")))
            .to_request();
        assert_eq!(
            test::call_service(&app, premature).await.status(),
            StatusCode::UNAUTHORIZED
        );
        let accept = test::TestRequest::post()
            .uri(&format!("/api/beta/device/resets/{id}/accept"))
            .insert_header((header::AUTHORIZATION, format!("Bearer {credential}")))
            .set_json(serde_json::json!({"receipt": receipt, "next_credential": next}))
            .to_request();
        assert_eq!(
            test::call_service(&app, accept).await.status(),
            StatusCode::NO_CONTENT
        );
        let manifest = test::TestRequest::get()
            .uri("/api/beta/device/manifest")
            .insert_header((header::AUTHORIZATION, format!("Bearer {credential}")))
            .to_request();
        assert_eq!(
            test::call_service(&app, manifest).await.status(),
            StatusCode::UNAUTHORIZED
        );
        let complete = test::TestRequest::post()
            .uri(&format!("/api/beta/device/resets/{id}/complete"))
            .insert_header((header::AUTHORIZATION, format!("Bearer {receipt}")))
            .to_request();
        assert_eq!(
            test::call_service(&app, complete).await.status(),
            StatusCode::NO_CONTENT
        );
        let owner = test::TestRequest::get()
            .uri("/api/beta/owner")
            .cookie(Cookie::new(OWNER_COOKIE, claim.owner_token))
            .to_request();
        let response = test::call_service(&app, owner).await;
        let context: serde_json::Value = test::read_body_json(response).await;
        assert_eq!(context["frames"], serde_json::json!([]));
        assert_eq!(context["resets"][0]["state"], "complete");
        assert!(context["resets"][0].get("receipt_hash").is_none());
    }

    #[actix_web::test]
    async fn replacement_preserves_recipient_and_revokes_old_link_and_session() {
        let mut database = Connection::open_in_memory().unwrap();
        initialize_database(&database).unwrap();
        let owner = create_household_owner(&mut database, "Home", "Owner").unwrap();
        let other = create_household_owner(&mut database, "Other", "Owner").unwrap();
        database.execute("INSERT INTO frames (id, household_id, place_name) VALUES ('kitchen', ?1, 'Kitchen')",
            rusqlite::params![owner.household_id]).unwrap();
        let invitation = create_sender_invitation(&database, &owner.owner_token, "Alice").unwrap();
        let uri = format!(
            "/api/beta/owner/invitations/{}/replace",
            invitation.invitation_id
        );
        let state = web::Data::new(AppState::new(database, PathBuf::from("unused")));
        let app = test::init_service(
            App::new()
                .app_data(state.clone())
                .configure(configure_beta_routes),
        )
        .await;
        for cookie in [
            None,
            Some(Cookie::new(SENDER_COOKIE, invitation.token.clone())),
        ] {
            let mut request = test::TestRequest::post().uri(&uri);
            if let Some(cookie) = cookie {
                request = request.cookie(cookie);
            }
            assert_eq!(
                test::call_service(&app, request.to_request())
                    .await
                    .status(),
                StatusCode::UNAUTHORIZED
            );
        }
        let foreign = test::TestRequest::post()
            .uri(&uri)
            .cookie(Cookie::new(OWNER_COOKIE, other.owner_token))
            .to_request();
        assert_eq!(
            test::call_service(&app, foreign).await.status(),
            StatusCode::NOT_FOUND
        );
        let existing = test::TestRequest::get()
            .uri("/api/beta/sender")
            .cookie(Cookie::new(SENDER_COOKIE, invitation.token.clone()))
            .to_request();
        assert_eq!(
            test::call_service(&app, existing).await.status(),
            StatusCode::OK
        );
        let replace = test::TestRequest::post()
            .uri(&uri)
            .cookie(Cookie::new(OWNER_COOKIE, owner.owner_token.clone()))
            .to_request();
        let response = test::call_service(&app, replace).await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get(header::CACHE_CONTROL).unwrap(),
            "no-store"
        );
        let replacement: serde_json::Value = test::read_body_json(response).await;
        assert_eq!(replacement["invitation_id"], invitation.invitation_id);
        assert_eq!(replacement["label"], "Alice");
        let new_token = replacement["token"].as_str().unwrap();
        assert_ne!(new_token, invitation.token);
        let old_activation = test::TestRequest::post()
            .uri("/api/beta/sender/session")
            .set_json(serde_json::json!({"token": invitation.token}))
            .to_request();
        assert_eq!(
            test::call_service(&app, old_activation).await.status(),
            StatusCode::UNAUTHORIZED
        );
        let old_session = test::TestRequest::get()
            .uri("/api/beta/sender")
            .cookie(Cookie::new(SENDER_COOKIE, invitation.token))
            .to_request();
        assert_eq!(
            test::call_service(&app, old_session).await.status(),
            StatusCode::UNAUTHORIZED
        );
        let activate = test::TestRequest::post()
            .uri("/api/beta/sender/session")
            .set_json(serde_json::json!({"token": new_token}))
            .to_request();
        assert_eq!(
            test::call_service(&app, activate).await.status(),
            StatusCode::NO_CONTENT
        );
        {
            let database = state.database.lock().unwrap();
            let access = sender_access(&database, new_token).unwrap();
            assert_eq!(access.channel_id, owner.default_channel_id);
            assert_eq!(access.household_id, owner.household_id);
            let context = owner_context(&database, &owner.owner_token).unwrap();
            assert_eq!(context.invitations.len(), 1);
            let stored: Vec<u8> = database
                .query_row(
                    "SELECT token_hash FROM sender_invitations WHERE id = ?1",
                    rusqlite::params![invitation.invitation_id],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(stored.len(), 32);
            assert_ne!(stored, new_token.as_bytes());
            revoke_sender_invitation(&database, &owner.owner_token, &invitation.invitation_id)
                .unwrap();
        }
        let disabled = test::TestRequest::post()
            .uri(&uri)
            .cookie(Cookie::new(OWNER_COOKIE, owner.owner_token))
            .to_request();
        assert_eq!(
            test::call_service(&app, disabled).await.status(),
            StatusCode::NOT_FOUND
        );
    }

    #[actix_web::test]
    async fn owner_removal_is_private_frame_scoped_and_retryable() {
        let mut database = Connection::open_in_memory().expect("open database");
        initialize_database(&database).expect("initialize database");
        let owner = create_household_owner(&mut database, "Tester", "Owner").unwrap();
        let stranger = create_household_owner(&mut database, "Other", "Owner").unwrap();
        for frame_id in ["kitchen", "bedroom"] {
            database
                .execute(
                    "INSERT INTO frames (id, household_id, place_name) VALUES (?1, ?2, ?1)",
                    rusqlite::params![frame_id, owner.household_id],
                )
                .unwrap();
            database
                .execute(
                    "INSERT INTO frame_subscriptions (frame_id, channel_id) VALUES (?1, ?2)",
                    rusqlite::params![frame_id, owner.default_channel_id],
                )
                .unwrap();
        }
        database.execute("INSERT INTO photos (id, channel_id, storage_key, mime_type) VALUES ('shared', ?1, 'shared.jpg', 'image/jpeg')",
            rusqlite::params![owner.default_channel_id]).unwrap();
        let media_dir =
            std::env::temp_dir().join(format!("astrohud-removal-test-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&media_dir).unwrap();
        std::fs::write(media_dir.join("shared.jpg"), b"private-photo").unwrap();
        let state = web::Data::new(AppState::new(database, media_dir.clone()));
        let app = test::init_service(
            App::new()
                .app_data(state.clone())
                .configure(configure_beta_routes),
        )
        .await;
        let photo_url = "/api/beta/owner/frames/kitchen/photos/shared";

        for method in [
            actix_web::http::Method::GET,
            actix_web::http::Method::DELETE,
        ] {
            let anonymous = test::TestRequest::default()
                .method(method.clone())
                .uri(photo_url)
                .to_request();
            assert_eq!(
                test::call_service(&app, anonymous).await.status(),
                StatusCode::UNAUTHORIZED
            );
            let sender = test::TestRequest::default()
                .method(method.clone())
                .uri(photo_url)
                .cookie(Cookie::new(SENDER_COOKIE, owner.owner_token.clone()))
                .to_request();
            assert_eq!(
                test::call_service(&app, sender).await.status(),
                StatusCode::UNAUTHORIZED
            );
            let foreign = test::TestRequest::default()
                .method(method)
                .uri(photo_url)
                .cookie(Cookie::new(OWNER_COOKIE, stranger.owner_token.clone()))
                .to_request();
            assert_eq!(
                test::call_service(&app, foreign).await.status(),
                StatusCode::NOT_FOUND
            );
        }
        let foreign_list = test::TestRequest::get()
            .uri("/api/beta/owner/frames/kitchen/photos")
            .cookie(Cookie::new(OWNER_COOKIE, stranger.owner_token.clone()))
            .to_request();
        assert_eq!(
            test::call_service(&app, foreign_list).await.status(),
            StatusCode::NOT_FOUND
        );
        let listing = test::TestRequest::get()
            .uri("/api/beta/owner/frames/kitchen/photos")
            .cookie(Cookie::new(OWNER_COOKIE, owner.owner_token.clone()))
            .to_request();
        let response = test::call_service(&app, listing).await;
        assert_eq!(response.status(), StatusCode::OK);
        let body: serde_json::Value = test::read_body_json(response).await;
        assert_eq!(body["photos"][0]["url"], photo_url);
        let preview = test::TestRequest::get()
            .uri(photo_url)
            .cookie(Cookie::new(OWNER_COOKIE, owner.owner_token.clone()))
            .to_request();
        let response = test::call_service(&app, preview).await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get(header::CACHE_CONTROL).unwrap(),
            "no-store"
        );
        assert_eq!(test::read_body(response).await, b"private-photo"[..]);

        for _ in 0..2 {
            let removal = test::TestRequest::delete()
                .uri(photo_url)
                .cookie(Cookie::new(OWNER_COOKIE, owner.owner_token.clone()))
                .to_request();
            assert_eq!(
                test::call_service(&app, removal).await.status(),
                StatusCode::NO_CONTENT
            );
        }
        let listing = test::TestRequest::get()
            .uri("/api/beta/owner/frames/kitchen/photos")
            .cookie(Cookie::new(OWNER_COOKIE, owner.owner_token.clone()))
            .to_request();
        let response = test::call_service(&app, listing).await;
        let body: serde_json::Value = test::read_body_json(response).await;
        assert_eq!(body["photos"], serde_json::json!([]));
        let removed_preview = test::TestRequest::get()
            .uri(photo_url)
            .cookie(Cookie::new(OWNER_COOKIE, owner.owner_token.clone()))
            .to_request();
        assert_eq!(
            test::call_service(&app, removed_preview).await.status(),
            StatusCode::NOT_FOUND
        );
        {
            let database = state.database.lock().unwrap();
            let kitchen =
                protected_manifest_for_household(&database, &owner.household_id, "kitchen")
                    .unwrap()
                    .unwrap();
            let bedroom =
                protected_manifest_for_household(&database, &owner.household_id, "bedroom")
                    .unwrap()
                    .unwrap();
            assert!(kitchen.photos.is_empty());
            assert_eq!(bedroom.photos.len(), 1);
            assert!(
                protected_media_storage_key(&database, &owner.household_id, "kitchen", "shared")
                    .unwrap()
                    .is_none()
            );
            assert!(
                protected_media_storage_key(&database, &owner.household_id, "bedroom", "shared")
                    .unwrap()
                    .is_some()
            );
            // Removal must survive restarting the server and reinitializing its schema.
            initialize_database(&database).unwrap();
            assert!(
                protected_manifest_for_household(&database, &owner.household_id, "kitchen")
                    .unwrap()
                    .unwrap()
                    .photos
                    .is_empty()
            );
            database.execute("UPDATE owner_access_grants SET revoked_at = CURRENT_TIMESTAMP WHERE household_id = ?1",
                rusqlite::params![owner.household_id]).unwrap();
        }
        let revoked = test::TestRequest::delete()
            .uri(photo_url)
            .cookie(Cookie::new(OWNER_COOKIE, owner.owner_token))
            .to_request();
        assert_eq!(
            test::call_service(&app, revoked).await.status(),
            StatusCode::UNAUTHORIZED
        );
        assert!(media_dir.join("shared.jpg").is_file());
        std::fs::remove_dir_all(media_dir).unwrap();
    }

    #[actix_web::test]
    async fn device_bootstrap_claim_creates_owner_session_and_rejects_replay() {
        let database = Connection::open_in_memory().expect("open database");
        initialize_database(&database).expect("initialize database");
        let state = web::Data::new(AppState::new(database, PathBuf::from("unused")));
        let app =
            test::init_service(App::new().app_data(state).configure(configure_beta_routes)).await;
        let device_id = Uuid::new_v4().to_string();
        let credential = "simulated-device-credential-abcdefghijklmnopqrstuvwxyz";
        let bootstrap_token = "simulated-bootstrap-token-abcdefghijklmnopqrstuvwxyz0123456789";

        let anonymous_owner = test::TestRequest::get().uri("/api/beta/owner").to_request();
        assert_eq!(
            test::call_service(&app, anonymous_owner).await.status(),
            StatusCode::UNAUTHORIZED
        );

        let enrollment_request = test::TestRequest::post()
            .uri("/api/beta/devices/enrollments")
            .set_json(serde_json::json!({
                "device_id": device_id,
                "device_code": "ABC2D3",
                "credential": credential,
                "bootstrap_token": bootstrap_token
            }))
            .to_request();
        let enrollment_response = test::call_service(&app, enrollment_request).await;
        assert_eq!(enrollment_response.status(), StatusCode::OK);

        let status_request = test::TestRequest::post()
            .uri("/api/beta/bootstrap/status")
            .set_json(serde_json::json!({"token": bootstrap_token}))
            .to_request();
        let status_response = test::call_service(&app, status_request).await;
        assert_eq!(status_response.status(), StatusCode::OK);
        let status: serde_json::Value = test::read_body_json(status_response).await;
        assert_eq!(status["status"], "ready");
        assert_eq!(status["device_code"], "ABC2D3");

        let claim_request = test::TestRequest::post()
            .uri("/api/beta/bootstrap/claim")
            .set_json(serde_json::json!({
                "token": bootstrap_token,
                "place_name": "Mom's living room"
            }))
            .to_request();
        let claim_response = test::call_service(&app, claim_request).await;
        assert_eq!(claim_response.status(), StatusCode::OK);
        let owner_cookie = claim_response
            .response()
            .cookies()
            .find(|cookie| cookie.name() == OWNER_COOKIE)
            .expect("owner cookie")
            .into_owned();
        assert!(owner_cookie.http_only().unwrap_or(false));
        assert_eq!(owner_cookie.same_site(), Some(SameSite::Strict));
        let claim_body: serde_json::Value = test::read_body_json(claim_response).await;
        assert_eq!(claim_body["household_name"], "Mom's living room");
        assert_eq!(claim_body["frames"][0]["place_name"], "Mom's living room");
        assert!(claim_body.get("household_id").is_none());

        let owner_request = test::TestRequest::get()
            .uri("/api/beta/owner")
            .cookie(owner_cookie)
            .to_request();
        let owner_response = test::call_service(&app, owner_request).await;
        assert_eq!(owner_response.status(), StatusCode::OK);

        let replay_request = test::TestRequest::post()
            .uri("/api/beta/bootstrap/claim")
            .set_json(serde_json::json!({
                "token": bootstrap_token,
                "place_name": "Replay place"
            }))
            .to_request();
        let replay_response = test::call_service(&app, replay_request).await;
        assert_eq!(replay_response.status(), StatusCode::CONFLICT);
        assert!(replay_response.response().cookies().next().is_none());

        let manifest_request = test::TestRequest::get()
            .uri("/api/beta/device/manifest")
            .insert_header((header::AUTHORIZATION, format!("Bearer {credential}")))
            .to_request();
        assert_eq!(
            test::call_service(&app, manifest_request).await.status(),
            StatusCode::OK
        );
    }

    #[actix_web::test]
    async fn owner_session_claims_a_simulated_device_without_a_household_id() {
        let mut database = Connection::open_in_memory().expect("open database");
        initialize_database(&database).expect("initialize database");
        let owner = create_household_owner(&mut database, "Tester", "Owner")
            .expect("create household owner");
        let device_id = Uuid::new_v4().to_string();
        let credential = "simulated-device-credential-abcdefghijklmnopqrstuvwxyz";
        let registration =
            register_device(&database, &device_id, "ABC2D3", credential).expect("register device");
        let DeviceEnrollmentState::Pending { claim_code, .. } = registration.state else {
            panic!("device should be pending");
        };
        let state = web::Data::new(AppState::new(database, PathBuf::from("unused")));
        let app =
            test::init_service(App::new().app_data(state).configure(configure_beta_routes)).await;

        let session_request = test::TestRequest::post()
            .uri("/api/beta/owner/session")
            .set_json(serde_json::json!({"token": owner.owner_token}))
            .to_request();
        let session_response = test::call_service(&app, session_request).await;
        assert_eq!(session_response.status(), StatusCode::NO_CONTENT);
        let cookie = session_response
            .response()
            .cookies()
            .next()
            .expect("owner cookie")
            .into_owned();

        let claim_request = test::TestRequest::post()
            .uri("/api/beta/owner/claim")
            .cookie(cookie)
            .set_json(serde_json::json!({
                "claim_code": claim_code,
                "place_name": "Tester living room"
            }))
            .to_request();
        let claim_response = test::call_service(&app, claim_request).await;
        assert_eq!(claim_response.status(), StatusCode::OK);
        let body: serde_json::Value = test::read_body_json(claim_response).await;
        assert_eq!(body["household_name"], "Tester");
        assert_eq!(body["frames"][0]["place_name"], "Tester living room");
        assert!(body.get("household_id").is_none());
    }

    #[actix_web::test]
    async fn device_status_requires_its_own_credential() {
        let database = Connection::open_in_memory().expect("open database");
        initialize_database(&database).expect("initialize database");
        let registration = register_device(
            &database,
            &Uuid::new_v4().to_string(),
            "ABC2D3",
            "simulated-device-credential-abcdefghijklmnopqrstuvwxyz",
        )
        .expect("register device");
        let state = web::Data::new(AppState::new(database, PathBuf::from("unused")));
        let app =
            test::init_service(App::new().app_data(state).configure(configure_beta_routes)).await;

        let status_request = test::TestRequest::get()
            .uri(&format!(
                "/api/beta/devices/enrollments/{}",
                registration.enrollment_id
            ))
            .to_request();
        let response = test::call_service(&app, status_request).await;
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[actix_web::test]
    async fn sender_session_exposes_a_fixed_place_without_internal_ids() {
        let mut database = Connection::open_in_memory().expect("open database");
        initialize_database(&database).expect("initialize database");
        let owner = create_household_owner(&mut database, "Tester", "Owner")
            .expect("create household owner");
        let registration = register_device(
            &database,
            &Uuid::new_v4().to_string(),
            "ABC2D3",
            "simulated-device-credential-abcdefghijklmnopqrstuvwxyz",
        )
        .expect("register device");
        let DeviceEnrollmentState::Pending { claim_code, .. } = registration.state else {
            panic!("device should be pending");
        };
        claim_device(
            &mut database,
            &owner.owner_token,
            &claim_code,
            "Mom's living room",
        )
        .expect("claim frame");
        let invitation = create_sender_invitation(&database, &owner.owner_token, "Alice")
            .expect("create sender invitation");
        let state = web::Data::new(AppState::new(database, PathBuf::from("unused")));
        let app =
            test::init_service(App::new().app_data(state).configure(configure_beta_routes)).await;

        let session_request = test::TestRequest::post()
            .uri("/api/beta/sender/session")
            .set_json(serde_json::json!({"token": invitation.token}))
            .to_request();
        let session_response = test::call_service(&app, session_request).await;
        assert_eq!(session_response.status(), StatusCode::NO_CONTENT);
        let cookie = session_response
            .response()
            .cookies()
            .next()
            .expect("sender cookie")
            .into_owned();

        let context_request = test::TestRequest::get()
            .uri("/api/beta/sender")
            .cookie(cookie)
            .to_request();
        let context_response = test::call_service(&app, context_request).await;
        assert_eq!(context_response.status(), StatusCode::OK);
        let body: serde_json::Value = test::read_body_json(context_response).await;
        assert_eq!(body["place_name"], "Mom's living room");
        assert_eq!(body["invitation_label"], "Alice");
        assert!(body.get("household_id").is_none());
        assert!(body.get("channel_id").is_none());
    }

    #[actix_web::test]
    async fn claimed_device_uses_its_credential_for_manifest_and_media() {
        let mut database = Connection::open_in_memory().expect("open database");
        initialize_database(&database).expect("initialize database");
        let owner = create_household_owner(&mut database, "Tester", "Owner")
            .expect("create household owner");
        let credential = "simulated-device-credential-abcdefghijklmnopqrstuvwxyz";
        let registration =
            register_device(&database, &Uuid::new_v4().to_string(), "ABC2D3", credential)
                .expect("register device");
        let DeviceEnrollmentState::Pending { claim_code, .. } = registration.state else {
            panic!("device should be pending");
        };
        claim_device(
            &mut database,
            &owner.owner_token,
            &claim_code,
            "Mom's living room",
        )
        .expect("claim frame");
        database
            .execute(
                "INSERT INTO photos (id, channel_id, storage_key, mime_type)
                 VALUES ('photo-one', ?1, 'photo-one.jpg', 'image/jpeg')",
                rusqlite::params![owner.default_channel_id],
            )
            .expect("insert photo");

        let media_dir =
            std::env::temp_dir().join(format!("astrohud-device-media-test-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&media_dir).expect("create media directory");
        std::fs::write(media_dir.join("photo-one.jpg"), b"private-photo").expect("write media");
        let state = web::Data::new(AppState::new(database, media_dir.clone()));
        let app =
            test::init_service(App::new().app_data(state).configure(configure_beta_routes)).await;

        let anonymous_request = test::TestRequest::get()
            .uri("/api/beta/device/manifest")
            .to_request();
        let anonymous_response = test::call_service(&app, anonymous_request).await;
        assert_eq!(anonymous_response.status(), StatusCode::UNAUTHORIZED);

        let manifest_request = test::TestRequest::get()
            .uri("/api/beta/device/manifest")
            .insert_header((header::AUTHORIZATION, format!("Bearer {credential}")))
            .to_request();
        let manifest_response = test::call_service(&app, manifest_request).await;
        assert_eq!(manifest_response.status(), StatusCode::OK);
        let manifest: serde_json::Value = test::read_body_json(manifest_response).await;
        assert_eq!(manifest["place_name"], "Mom's living room");
        assert_eq!(
            manifest["photos"][0]["url"],
            "/api/beta/device/media/photo-one"
        );
        assert!(
            !manifest["photos"][0]["url"]
                .as_str()
                .expect("media URL")
                .contains("photo-one.jpg")
        );

        let media_request = test::TestRequest::get()
            .uri("/api/beta/device/media/photo-one")
            .insert_header((header::AUTHORIZATION, format!("Bearer {credential}")))
            .to_request();
        let media_response = test::call_service(&app, media_request).await;
        assert_eq!(media_response.status(), StatusCode::OK);
        assert_eq!(test::read_body(media_response).await, b"private-photo"[..]);

        std::fs::remove_dir_all(media_dir).expect("remove media directory");
    }
}
