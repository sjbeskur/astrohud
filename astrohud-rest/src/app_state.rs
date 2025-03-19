use actix::prelude::*;
use std::sync::Mutex;
use std::collections::HashSet;
use crate::websocket::ImageWebSocket;


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