use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct Photo {
    pub id: String,
    pub channel_id: String,
    pub url: String,
    pub mime_type: String,
    pub created_at: String,
    #[serde(default)]
    pub location_label: Option<String>,
}

impl Photo {
    pub fn is_supported(&self) -> bool {
        matches!(self.mime_type.as_str(), "image/jpeg" | "image/png")
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct FrameManifest {
    pub frame_id: String,
    pub place_name: String,
    pub photos: Vec<Photo>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CachedPhoto {
    pub id: String,
    pub path: PathBuf,
    pub location_label: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Playlist {
    pub place_name: String,
    pub photos: Vec<CachedPhoto>,
}
