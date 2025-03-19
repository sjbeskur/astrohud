use actix_web::{web, App, HttpServer};
use actix_files::Files;
use std::sync::Mutex;
use std::collections::HashSet;

use astrohud_rest::*;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let app_state = web::Data::new(AppState {
        clients: Mutex::new(HashSet::new()),
    });
    
    let static_path = std::env::current_dir().unwrap().join("static");
    println!("Looking for static files at: {}", static_path.display());

    let args = Cli::parse_args();


    println!("Server starting on http://{}:{}", args.ip_address, args.port);
    HttpServer::new(move || {
        App::new()
            .app_data(app_state.clone())
            // Existing todo routes (omitted for brevity)
            .route("/ws/", web::get().to(ws_handler))  // WebSocket endpoint
            // .route("/images/count", web::get().to(get_image_count))  // Check image count
            // .route("/image", web::get().to(get_latest_image))  // New endpoint for image

            .service(Files::new("/", env!("CARGO_MANIFEST_DIR").to_string() + "/static").index_file("wasm_index.html"))

    })
    .bind((args.ip_address, args.port))?
    .run()
    .await
}