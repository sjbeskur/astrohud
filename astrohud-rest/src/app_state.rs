use crate::websocket::ImageWebSocket;
use actix::prelude::*;
use rusqlite::Connection;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Mutex;

pub struct AppState {
    pub clients: Mutex<HashSet<Addr<ImageWebSocket>>>,
    pub database: Mutex<Connection>,
    pub media_dir: PathBuf,
}

impl AppState {
    pub fn new(database: Connection, media_dir: PathBuf) -> Self {
        Self {
            clients: Mutex::new(HashSet::new()),
            database: Mutex::new(database),
            media_dir,
        }
    }
}
