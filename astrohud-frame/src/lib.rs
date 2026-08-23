pub mod cache;
pub mod config;
pub mod model;
pub mod sync;

use std::error::Error;

pub type BoxError = Box<dyn Error + Send + Sync + 'static>;
