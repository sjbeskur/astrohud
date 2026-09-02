use crate::{
    AppState, BetaError, DeviceEnrollmentState, DeviceRegistration, claim_bootstrap_device,
    claim_device, claimed_device_access, create_sender_invitation, device_bootstrap_status,
    enrollment_status, owner_context, protected_manifest_for_household,
    protected_media_storage_key, register_bootstrap_device, register_device,
    revoke_sender_invitation, sender_access, upload_invited_photo,
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
        .route(
            "/api/beta/owner/invitations",
            web::post().to(create_invitation),
        )
        .route(
            "/api/beta/owner/invitations/{invitation_id}",
            web::delete().to(revoke_invitation),
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
    Ok(HttpResponse::Created().json(invitation))
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
    Ok(HttpResponse::Ok().json(context))
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
