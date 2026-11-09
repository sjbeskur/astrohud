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
    let database_path = data_dir.join("astrohud.sqlite3");
    let database = Connection::open(&database_path).map_err(std::io::Error::other)?;
    if let Some(backup) = initialize_database_with_backup(&database, &database_path)? {
        println!("Saved pre-migration database backup: {}", backup.display());
    }
    let app_state = web::Data::new(AppState::new(database, media_dir.clone()));
    purge_deleted_media(&app_state).map_err(std::io::Error::other)?;

    let enable_demo = env::var("ASTROHUD_ENABLE_DEMO")
        .is_ok_and(|value| matches!(value.as_str(), "1" | "true" | "yes"));
    println!("Server starting on http://{}", args.endpoint);
    HttpServer::new(move || {
        App::new()
            .app_data(app_state.clone())
            .configure(configure_beta_routes)
            .route("/api/health", web::get().to(health))
            .configure(|cfg| configure_demo_routes(cfg, enable_demo))
            .service(
                Files::new("/", env!("CARGO_MANIFEST_DIR").to_string() + "/static")
                    .index_file("owner.html"),
            )
    })
    .bind(args.endpoint)?
    .run()
    .await
}
