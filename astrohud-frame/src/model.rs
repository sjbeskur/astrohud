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
    #[serde(default)]
    pub playback: Playback,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default)]
pub struct Playback {
    pub next_photo_id: Option<String>,
    pub next_revision: u64,
    pub next_expires_at: Option<u64>,
    pub slide_seconds: u64,
    pub blur_background: bool,
    pub selected_photo_id: Option<String>,
    pub selection_revision: u64,
    pub pinned_photo_id: Option<String>,
    pub pinned_until: Option<u64>,
}
impl Default for Playback {
    fn default() -> Self {
        Self {
            next_photo_id: None,
            next_revision: 0,
            next_expires_at: None,
            slide_seconds: 0,
            blur_background: true,
            selected_photo_id: None,
            selection_revision: 0,
            pinned_photo_id: None,
            pinned_until: None,
        }
    }
}
impl Playback {
    pub fn requested_next_at(&self, now: u64, seen_revision: u64) -> Option<&str> {
        if self.next_revision > seen_revision && self.next_expires_at.is_some_and(|end| end > now) {
            self.next_photo_id.as_deref()
        } else {
            None
        }
    }
    pub fn pinned_at(&self, now: u64) -> Option<&str> {
        if self.pinned_until.is_some_and(|end| end <= now) {
            None
        } else {
            self.pinned_photo_id.as_deref()
        }
    }
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
    pub playback: Playback,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn guest_next_is_consumed_once_and_expires() {
        let playback = Playback {
            next_photo_id: Some("guest-photo".into()),
            next_revision: 4,
            next_expires_at: Some(100),
            ..Default::default()
        };
        assert_eq!(playback.requested_next_at(99, 3), Some("guest-photo"));
        assert_eq!(playback.requested_next_at(99, 4), None);
        assert_eq!(playback.requested_next_at(100, 3), None);
        assert_eq!(Playback::default().requested_next_at(99, 0), None);
    }
    #[test]
    fn old_cached_manifests_load_and_finite_pins_expire_offline() {
        let old: FrameManifest =
            serde_json::from_str(r#"{"frame_id":"tv","place_name":"TV","photos":[]}"#).unwrap();
        assert_eq!(old.playback.slide_seconds, 0);
        assert!(old.playback.blur_background);
        let mut playback = Playback {
            pinned_photo_id: Some("photo".into()),
            pinned_until: Some(100),
            ..Default::default()
        };
        assert_eq!(playback.pinned_at(99), Some("photo"));
        assert_eq!(playback.pinned_at(100), None);
        playback.pinned_until = None;
        assert_eq!(playback.pinned_at(1000), Some("photo"));
    }
}
