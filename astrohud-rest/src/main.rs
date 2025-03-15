use actix_web::{web, App, HttpServer};
use actix_files::Files;
use std::sync::Mutex;
use std::collections::HashSet;

use astrohud_rest::*;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let app_state = web::Data::new(AppState {
        todos: Mutex::new(vec![Todo { id: 1, title: "Learn Rust".to_string(), completed: false }]),
        images: Mutex::new(Vec::new()),
        clients: Mutex::new(HashSet::new()),
    });
    
    let static_path = std::env::current_dir().unwrap().join("static");
    println!("Looking for static files at: {}", static_path.display());

    println!("Server starting on http://127.0.0.1:8080");
    HttpServer::new(move || {
        App::new()
            .app_data(app_state.clone())
            // Existing todo routes (omitted for brevity)
            .route("/ws/", web::get().to(ws_handler))  // WebSocket endpoint
            .route("/images/count", web::get().to(get_image_count))  // Check image count

            .route("/todos", web::get().to(get_todos))
            .route("/todos", web::post().to(create_todo))
            .route("/todos/{id}", web::get().to(get_todo))
            .route("/todos/{id}", web::put().to(update_todo))
            .route("/todos/{id}", web::delete().to(delete_todo))            
            .route("/image", web::get().to(get_latest_image))  // New endpoint for image
            //.route("/", web::get().to(index))  // Simple HTML page  
            // Serve static files from the "static" directory
            //.service(Files::new("/", "./static").index_file("canvas.html"))          

            .service(Files::new("/", env!("CARGO_MANIFEST_DIR").to_string() + "/static").index_file("wasm_index.html"))
            //.service(Files::new("/", env!("CARGO_MANIFEST_DIR").to_string() + "/static").index_file("canvas.html"))
            
            // Note:
            // env!("CARGO_MANIFEST_DIR") gives the directory containing the crate’s Cargo.toml (e.g., my_workspace/server/).

    })
    .bind(("127.0.0.1", 8080))?
    .run()
    .await
}


// Handle incoming WebSocket messages
// impl StreamHandler<Result<ws::Message, ws::ProtocolError>> for ImageWebSocket {
//     fn handle(&mut self, msg: Result<ws::Message, ws::ProtocolError>, ctx: &mut Self::Context) {
        
//         println!("Received message");
//         match msg {
//             Ok(ws::Message::Ping(msg)) => ctx.pong(&msg),
//             Ok(ws::Message::Text(text)) => ctx.text(format!("Echo: {}", text)),
//             Ok(ws::Message::Binary(bin)) => {
//                 // Handle image data
//                 let mut images = self.images.images.lock().unwrap();
//                 images.push(bin);
//                 ctx.text("Image received");
//             }
//             Ok(ws::Message::Close(_)) => {
//                 ctx.close(None);
//                 ctx.stop();
//             }
//             _ => (),
//         }
//     }
// }


// // WebSocket route handler
// async fn ws_handler(
//     req: actix_web::HttpRequest,
//     stream: web::Payload,
//     data: web::Data<AppState>,
// ) -> Result<HttpResponse, actix_web::Error> {
//     let ws = ImageWebSocket {
//         images: data.clone(),
//     };

//     // Configure WebSocket with larger max frame size (e.g., 10MB)
//     ws::WsResponseBuilder::new(ws, &req, stream)
//         .frame_size(10 * 1024 * 1024)  // Set max frame size to 10MB
//         .start()    
//     //ws::start(ws, &req, stream)
// }

// Handler to get number of stored images (for testing)
// async fn get_image_count(data: web::Data<AppState>) -> impl Responder {
//     let images = data.images.lock().unwrap();
//     HttpResponse::Ok().body(format!("Number of images: {}", images.len()))
// }


// async fn get_latest_image(data: web::Data<AppState>) -> impl Responder {
//     let images = data.images.lock().unwrap();
//     match images.last() {
//         Some(image) => {
//             let kind = infer::get(&image);
//             let content_type = kind.map_or("application/octet-stream", |k| k.mime_type());
//             HttpResponse::Ok()
//                 .content_type(content_type)
//                 .body(image.clone())
//         }
//         None => HttpResponse::NotFound().body("No images available"),
//     }
// }

// // Simple HTML page to display the image
// async fn index() -> impl Responder {
//     HttpResponse::Ok()
//         .content_type("text/html")
//         .body(r#"
//             <!DOCTYPE html>
//             <html>
//             <head>
//                 <title>Image Display</title>
//             </head>
//             <body>
//                 <h1>Latest Uploaded Image</h1>
//                 <img src="/image" alt="Latest Image" style="max-width: 100%;">
//             </body>
//             </html>
//         "#)
// }

