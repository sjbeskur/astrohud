use actix::prelude::*;
use actix_web::{web, HttpResponse};
use actix_web_actors::ws;
use bytes::Bytes;

use crate::app_state::AppState;


pub struct ImageWebSocket {
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
                // let mut images = self.app_state.images.lock().unwrap();
                // images.push(bin.clone());
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


pub async fn ws_handler(req: actix_web::HttpRequest, stream: web::Payload, data: web::Data<AppState>) -> Result<HttpResponse, actix_web::Error> {
    let ws = ImageWebSocket { app_state: data.clone() };
    ws::WsResponseBuilder::new(ws, &req, stream).frame_size(10 * 1024 * 1024).start()
}

