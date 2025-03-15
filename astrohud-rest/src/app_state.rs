use actix::prelude::*;
use bytes::Bytes;
use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use std::collections::HashSet;
use crate::websocket::ImageWebSocket;

// Existing Todo struct (unchanged)
#[derive(Serialize, Deserialize, Clone)]
pub struct Todo {
    pub id: u32,
    pub title: String,
    pub completed: bool,
}

pub struct AppState {
    pub todos: Mutex<Vec<Todo>>,
    pub images: Mutex<Vec<Bytes>>,  // Store received images
    pub clients: Mutex<HashSet<Addr<ImageWebSocket>>>, //    
}

impl AppState {
    pub fn new() -> Self {
        AppState {            
            todos: Mutex::new(vec![Todo { id: 1, title: "Finish AstroHud".to_string(), completed: false }]),
            images: Mutex::new(Vec::new()),
            clients: Mutex::new(HashSet::new()),
        }
    }
}   