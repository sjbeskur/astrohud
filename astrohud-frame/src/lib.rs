pub mod cache;
pub mod config;
pub mod model;
pub mod order;
pub mod prepared;
pub mod storage;
pub mod sync;
pub mod wifi;

use std::error::Error;

pub type BoxError = Box<dyn Error + Send + Sync + 'static>;
