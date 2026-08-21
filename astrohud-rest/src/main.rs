use actix_files::Files;
use actix_web::{App, HttpServer, web};
use rusqlite::Connection;
use std::{env, fs, path::PathBuf};

use astrohud_rest::*;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let args = Cli::parse_args();
    let data_dir = env::var_os("ASTROHUD_DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("data"));
    let media_dir = data_dir.join("media");
    fs::create_dir_all(&media_dir)?;
    let database =
        Connection::open(data_dir.join("astrohud.sqlite3")).map_err(std::io::Error::other)?;
    initialize_database(&database).map_err(std::io::Error::other)?;
    let app_state = web::Data::new(AppState::new(database, media_dir.clone()));

    println!(
        "Server starting on http://{}:{}",
        args.ip_address, args.port
    );
    HttpServer::new(move || {
        App::new()
            .app_data(app_state.clone())
            .route("/api/health", web::get().to(health))
            .service(
                web::resource("/api/channels")
                    .route(web::get().to(list_channels))
                    .route(web::post().to(create_channel)),
            )
            .route("/api/photos", web::post().to(upload_photo))
            .route(
                "/api/frames/{frame_id}/manifest",
                web::get().to(frame_manifest),
            )
            .route("/ws/", web::get().to(ws_handler))
            .service(Files::new("/media", media_dir.clone()))
            .service(
                Files::new("/", env!("CARGO_MANIFEST_DIR").to_string() + "/static")
                    .index_file("wasm_index.html"),
            )
    })
    .bind((args.ip_address, args.port))?
    .run()
    .await
}
