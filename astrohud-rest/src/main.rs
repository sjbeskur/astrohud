use actix::prelude::*;
use actix_web::{web, App, HttpServer, HttpResponse, Responder};
use actix_web_actors::ws;
use actix_files::Files;
use bytes::Bytes;
use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use std::collections::HashSet;

// Existing Todo struct (unchanged)
#[derive(Serialize, Deserialize, Clone)]
struct Todo {
    id: u32,
    title: String,
    completed: bool,
}


// Handler to get all todos
async fn get_todos(data: web::Data<AppState>) -> impl Responder {
    let todos = data.todos.lock().unwrap();
    HttpResponse::Ok().json(&*todos)
}

// Handler to get a specific todo
async fn get_todo(
    data: web::Data<AppState>,
    path: web::Path<u32>,
) -> impl Responder {
    let todos = data.todos.lock().unwrap();
    match todos.iter().find(|todo| todo.id == *path) {
        Some(todo) => HttpResponse::Ok().json(todo),
        None => HttpResponse::NotFound().body("Todo not found"),
    }
}

// Handler to create a new todo
async fn create_todo(
    data: web::Data<AppState>,
    todo: web::Json<Todo>,
) -> impl Responder {
    let mut todos = data.todos.lock().unwrap();
    let new_todo = Todo {
        id: (todos.len() as u32) + 1,
        title: todo.title.clone(),
        completed: todo.completed,
    };
    todos.push(new_todo.clone());
    HttpResponse::Ok().json(new_todo)
}

// Handler to update a todo
async fn update_todo(
    data: web::Data<AppState>,
    path: web::Path<u32>,
    todo: web::Json<Todo>,
) -> impl Responder {
    let mut todos = data.todos.lock().unwrap();
    match todos.iter_mut().find(|t| t.id == *path) {
        Some(existing_todo) => {
            existing_todo.title = todo.title.clone();
            existing_todo.completed = todo.completed;
            HttpResponse::Ok().json(existing_todo)
        }
        None => HttpResponse::NotFound().body("Todo not found"),
    }
}
// Handler to delete a todo
async fn delete_todo(
    data: web::Data<AppState>,
    path: web::Path<u32>,
) -> impl Responder {
    let mut todos = data.todos.lock().unwrap();
    let initial_len = todos.len();
    todos.retain(|todo| todo.id != *path);
    
    if todos.len() < initial_len {
        HttpResponse::Ok().body("Todo deleted")
    } else {
        HttpResponse::NotFound().body("Todo not found")
    }
}

// App state
struct AppState {
    todos: Mutex<Vec<Todo>>,
    images: Mutex<Vec<Bytes>>,  // Store received images
    clients: Mutex<HashSet<Addr<ImageWebSocket>>>, //    
}

// // WebSocket actor
// struct ImageWebSocket {
//     images: web::Data<AppState>,
// }
struct ImageWebSocket {
    app_state: web::Data<AppState>,
}
impl Actor for ImageWebSocket {
    type Context = ws::WebsocketContext<Self>;
    fn started(&mut self, ctx: &mut Self::Context) {
        println!("WebSocket connection started");
        let addr = ctx.address();
        self.app_state.clients.lock().unwrap().insert(addr);
    }

    fn stopped(&mut self, ctx: &mut Self::Context) {
        println!("WebSocket connection stopped");
        self.app_state.clients.lock().unwrap().remove(&ctx.address());
    }
}

impl StreamHandler<Result<ws::Message, ws::ProtocolError>> for ImageWebSocket {
    fn handle(&mut self, msg: Result<ws::Message, ws::ProtocolError>, ctx: &mut Self::Context) {
        match msg {
            Ok(ws::Message::Ping(msg)) => ctx.pong(&msg),
            Ok(ws::Message::Text(text)) => ctx.text(format!("Echo: {}", text)),
            Ok(ws::Message::Binary(bin)) => {
                println!("Received binary data of size: {} bytes", bin.len());
                let mut images = self.app_state.images.lock().unwrap();
                images.push(bin.clone());
                ctx.text(format!("Image received ({} bytes)", bin.len()));
                // Broadcast the image to all clients
                let clients = self.app_state.clients.lock().unwrap();
                for client in clients.iter() {
                    client.do_send(BroadcastImage(bin.clone()));
                }
            }
            Ok(ws::Message::Close(reason)) => {
                println!("WebSocket closing: {:?}", reason);
                ctx.close(reason);
                ctx.stop();
            }
            Err(e) => {
                println!("WebSocket error: {:?}", e);
                ctx.text(format!("Error: {:?}", e));
            }
            _ => (),
        }
    }
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

// Message to broadcast image
#[derive(Message)]
#[rtype(result = "()")]
struct BroadcastImage(Bytes);

impl Handler<BroadcastImage> for ImageWebSocket {
    type Result = ();

    fn handle(&mut self, msg: BroadcastImage, ctx: &mut Self::Context) {
        ctx.binary(msg.0); // Send the image as binary to the client
    }
}


async fn ws_handler(req: actix_web::HttpRequest, stream: web::Payload, data: web::Data<AppState>) -> Result<HttpResponse, actix_web::Error> {
    let ws = ImageWebSocket { app_state: data.clone() };
    ws::WsResponseBuilder::new(ws, &req, stream).frame_size(10 * 1024 * 1024).start()
}

async fn get_image_count(data: web::Data<AppState>) -> impl Responder {
    let images = data.images.lock().unwrap();
    HttpResponse::Ok().body(format!("Number of images: {}", images.len()))
}

async fn get_latest_image(data: web::Data<AppState>) -> impl Responder {
    let images = data.images.lock().unwrap();
    match images.last() {
        Some(image) => HttpResponse::Ok().content_type("image/jpeg").body(image.clone()),
        None => HttpResponse::NotFound().body("No images available"),
    }
}

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