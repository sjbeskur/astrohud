use crate::websocket::ImageWebSocket;
use actix::prelude::*;
use std::collections::HashSet;
use std::sync::Mutex;

pub struct AppState {
    pub clients: Mutex<HashSet<Addr<ImageWebSocket>>>, //
}

impl AppState {
    pub fn new() -> Self {
        AppState {
            clients: Mutex::new(HashSet::new()),
        }
    }
}
