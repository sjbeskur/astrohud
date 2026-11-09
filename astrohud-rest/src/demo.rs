use actix_web::web;

/// Legacy public transport examples are opt-in. Claimed frames use beta routes.
pub fn configure_demo_routes(cfg: &mut web::ServiceConfig, enabled: bool) {
    if !enabled {
        return;
    }
    cfg.service(
        web::resource("/api/channels")
            .route(web::get().to(crate::list_channels))
            .route(web::post().to(crate::create_channel)),
    )
    .route("/api/photos", web::post().to(crate::upload_photo))
    .route(
        "/api/frames/{frame_id}/manifest",
        web::get().to(crate::frame_manifest),
    )
    .route("/ws/", web::get().to(crate::ws_handler))
    .route("/media/{storage_key}", web::get().to(crate::demo_media));
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::{App, http::StatusCode, test};
    #[actix_web::test]
    async fn demo_endpoints_are_absent_unless_explicitly_enabled() {
        let app =
            test::init_service(App::new().configure(|cfg| configure_demo_routes(cfg, false))).await;
        for (method, path) in [
            ("GET", "/api/channels"),
            ("POST", "/api/channels"),
            ("POST", "/api/photos"),
            ("GET", "/api/frames/demo-frame/manifest"),
            ("GET", "/media/test.png"),
            ("GET", "/ws/"),
        ] {
            let request = test::TestRequest::default()
                .method(method.parse().unwrap())
                .uri(path)
                .to_request();
            assert_eq!(
                test::call_service(&app, request).await.status(),
                StatusCode::NOT_FOUND,
                "{}",
                path
            );
        }
        let db = rusqlite::Connection::open_in_memory().unwrap();
        crate::initialize_database(&db).unwrap();
        let state = web::Data::new(crate::AppState::new(db, std::env::temp_dir()));
        let app = test::init_service(
            App::new()
                .app_data(state)
                .configure(|cfg| configure_demo_routes(cfg, true)),
        )
        .await;
        assert_eq!(
            test::call_service(
                &app,
                test::TestRequest::get().uri("/api/channels").to_request()
            )
            .await
            .status(),
            StatusCode::OK
        );
    }
}
