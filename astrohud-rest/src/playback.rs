use crate::{BetaError, owner_household_id, protected_media_storage_key};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct Playback {
    pub playback_mode: String,
    pub viewer_supports_playback_modes: bool,
    pub guest_display_next: bool,
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
            playback_mode: "scenic_route".into(),
            viewer_supports_playback_modes: false,
            guest_display_next: false,
            next_photo_id: None,
            next_revision: 0,
            next_expires_at: None,
            slide_seconds: 12,
            blur_background: true,
            selected_photo_id: None,
            selection_revision: 0,
            pinned_photo_id: None,
            pinned_until: None,
        }
    }
}

#[derive(Deserialize)]
pub struct PlaybackRequest {
    pub playback_mode: Option<String>,
    pub guest_display_next: Option<bool>,
    pub action: Option<String>,
    pub photo_id: Option<String>,
    pub pin_seconds: Option<u64>,
    pub slide_seconds: Option<u64>,
    pub blur_background: Option<bool>,
}

pub(crate) fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

pub(crate) fn frame_playback(db: &Connection, frame: &str) -> rusqlite::Result<Playback> {
    let mut playback = db
        .query_row(
            "SELECT slide_seconds, blur_background, selected_photo_id,
        selection_revision, pinned_photo_id, pinned_until FROM frame_playback WHERE frame_id = ?1",
            [frame],
            |row| {
                Ok(Playback {
                    slide_seconds: row.get(0)?,
                    blur_background: row.get(1)?,
                    selected_photo_id: row.get(2)?,
                    selection_revision: row.get(3)?,
                    pinned_photo_id: row.get(4)?,
                    pinned_until: row.get(5)?,
                    ..Playback::default()
                })
            },
        )
        .optional()?
        .unwrap_or_default();
    if let Some((mode, supported)) = db
        .query_row(
            "SELECT mode,viewer_supports_modes FROM frame_playback_modes WHERE frame_id=?1",
            [frame],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, bool>(1)?)),
        )
        .optional()?
    {
        playback.playback_mode = mode;
        playback.viewer_supports_playback_modes = supported;
    }
    if let Some((enabled, photo, revision, expires)) = db.query_row(
        "SELECT enabled,photo_id,revision,expires_at FROM frame_guest_requests WHERE frame_id=?1",
        [frame], |r| Ok((r.get::<_, bool>(0)?, r.get::<_, Option<String>>(1)?, r.get::<_, u64>(2)?, r.get::<_, Option<u64>>(3)?))
    ).optional()? {
        playback.guest_display_next = enabled;
        playback.next_revision = revision;
        playback.next_expires_at = expires;
        if enabled && expires.is_some_and(|end| end > unix_now()) && let Some(photo) = photo {
            let household: Option<String> = db.query_row("SELECT household_id FROM frames WHERE id=?1",[frame],|r|r.get(0)).optional()?;
            if let Some(household) = household && protected_media_storage_key(db, &household, frame, &photo)?.is_some() {
                playback.next_photo_id = Some(photo);
            }
        }
    }
    if playback.pinned_until.is_some_and(|end| end <= unix_now()) {
        playback.pinned_photo_id = None;
        playback.pinned_until = None;
    }
    // Excluded or unsubscribed photos must never remain active commands.
    for photo in [
        &mut playback.selected_photo_id,
        &mut playback.pinned_photo_id,
    ] {
        if let Some(id) = photo.as_ref() {
            let available:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM photos p JOIN frame_subscriptions s ON s.channel_id=p.channel_id
                WHERE s.frame_id=?1 AND p.id=?2 AND NOT EXISTS(SELECT 1 FROM frame_photo_removals r WHERE r.frame_id=?1 AND r.photo_id=p.id))",
                params![frame,id],|r|r.get(0))?;
            if !available {
                *photo = None;
            }
        }
    }
    if playback.pinned_photo_id.is_none() {
        playback.pinned_until = None;
    }
    Ok(playback)
}

pub(crate) fn owner_frame_channel(
    db: &Connection,
    token: &str,
    frame: &str,
) -> Result<(String, String), BetaError> {
    let household = owner_household_id(db, token)?;
    let channel=db.query_row("SELECT c.id FROM frames f JOIN frame_subscriptions s ON s.frame_id=f.id
        JOIN channels c ON c.id=s.channel_id WHERE f.id=?1 AND f.household_id=?2 AND c.household_id=?2
        ORDER BY c.created_at,c.rowid LIMIT 1",params![frame,household],|r|r.get(0)).optional()?.ok_or(BetaError::NotFound)?;
    Ok((household, channel))
}

pub fn update_playback(
    db: &mut Connection,
    token: &str,
    frame: &str,
    request: &PlaybackRequest,
) -> Result<Playback, BetaError> {
    let (household, _) = owner_frame_channel(db, token, frame)?;
    if request
        .slide_seconds
        .is_some_and(|n| !(5..=3600).contains(&n))
    {
        return Err(BetaError::InvalidInput(
            "photo interval must be between 5 and 3600 seconds",
        ));
    }
    let tx = db.transaction()?;
    let mut playback = frame_playback(&tx, frame)?;
    if let Some(mode) = &request.playback_mode {
        if !["scenic_route", "fresh_prints", "serendipity"].contains(&mode.as_str()) {
            return Err(BetaError::InvalidInput("unknown playback style"));
        }
        if !playback.viewer_supports_playback_modes {
            return Err(BetaError::InvalidInput(
                "update this frame's software before choosing a playback style",
            ));
        }
        tx.execute("INSERT INTO frame_playback_modes(frame_id,mode) VALUES (?1,?2) ON CONFLICT(frame_id) DO UPDATE SET mode=excluded.mode", params![frame,mode])?;
    }
    if let Some(seconds) = request.slide_seconds {
        playback.slide_seconds = seconds;
    }
    if let Some(blur) = request.blur_background {
        playback.blur_background = blur;
    }
    if let Some(enabled) = request.guest_display_next {
        tx.execute(
            "INSERT INTO frame_guest_requests(frame_id,enabled) VALUES (?1,?2)
            ON CONFLICT(frame_id) DO UPDATE SET enabled=excluded.enabled,
            photo_id=CASE WHEN excluded.enabled=0 THEN NULL ELSE frame_guest_requests.photo_id END",
            params![frame, enabled],
        )?;
    }
    if let Some(action) = request.action.as_deref() {
        match action {
            "show" | "pin" => {
                let photo = request
                    .photo_id
                    .as_deref()
                    .ok_or(BetaError::InvalidInput("choose a photo"))?;
                if protected_media_storage_key(&tx, &household, frame, photo)?.is_none() {
                    return Err(BetaError::NotFound);
                }
                playback.selected_photo_id = Some(photo.to_owned());
                playback.pinned_photo_id = None;
                playback.pinned_until = None;
                if action == "pin" {
                    let duration = request
                        .pin_seconds
                        .ok_or(BetaError::InvalidInput("choose how long to pin"))?;
                    if duration != 0 && !(60..=86400).contains(&duration) {
                        return Err(BetaError::InvalidInput(
                            "pin duration must be 1 minute to 24 hours, or until resumed",
                        ));
                    }
                    playback.pinned_photo_id = Some(photo.to_owned());
                    playback.pinned_until = (duration != 0).then(|| unix_now() + duration);
                }
            }
            "resume" => {
                playback.pinned_photo_id = None;
                playback.pinned_until = None;
                playback.selected_photo_id = None;
            }
            _ => return Err(BetaError::InvalidInput("unknown slideshow action")),
        }
        playback.selection_revision += 1;
        // Any explicit owner playback command cancels an outstanding guest request.
        tx.execute(
            "UPDATE frame_guest_requests SET photo_id=NULL WHERE frame_id=?1",
            [frame],
        )?;
    }
    tx.execute("INSERT INTO frame_playback (frame_id,slide_seconds,blur_background,selected_photo_id,selection_revision,pinned_photo_id,pinned_until)
        VALUES (?1,?2,?3,?4,?5,?6,?7) ON CONFLICT(frame_id) DO UPDATE SET slide_seconds=excluded.slide_seconds,
        blur_background=excluded.blur_background,selected_photo_id=excluded.selected_photo_id,selection_revision=excluded.selection_revision,
        pinned_photo_id=excluded.pinned_photo_id,pinned_until=excluded.pinned_until",
        params![frame,playback.slide_seconds,playback.blur_background,playback.selected_photo_id,playback.selection_revision,playback.pinned_photo_id,playback.pinned_until])?;
    tx.commit()?;
    Ok(frame_playback(db, frame)?)
}

/// An authenticated manifest request advertises capability; an older viewer
/// clears it again after rollback. Merely upgrading the API never claims that
/// an old frame has applied its new playback setting.
pub(crate) fn record_playback_capability(
    db: &Connection,
    frame: &str,
    supported: bool,
) -> rusqlite::Result<()> {
    db.execute(
        "INSERT INTO frame_playback_modes(frame_id,viewer_supports_modes) VALUES (?1,?2)
        ON CONFLICT(frame_id) DO UPDATE SET viewer_supports_modes=excluded.viewer_supports_modes
        WHERE viewer_supports_modes != excluded.viewer_supports_modes",
        params![frame, supported],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{create_household_owner, initialize_database, protected_manifest_for_household};

    #[test]
    fn playback_commands_are_scoped_and_removed_photos_cannot_stay_pinned() {
        let mut db = Connection::open_in_memory().unwrap();
        initialize_database(&db).unwrap();
        let owner = create_household_owner(&mut db, "Home", "Owner").unwrap();
        let other = create_household_owner(&mut db, "Other", "Owner").unwrap();
        db.execute(
            "INSERT INTO frames (id,household_id,place_name) VALUES ('tv',?1,'TV')",
            [&owner.household_id],
        )
        .unwrap();
        db.execute(
            "INSERT INTO frame_subscriptions VALUES ('tv',?1)",
            [&owner.default_channel_id],
        )
        .unwrap();
        db.execute("INSERT INTO photos (id,channel_id,storage_key,mime_type) VALUES ('old-photo',?1,'old.png','image/png')",[&owner.default_channel_id]).unwrap();
        // A pinned photo must remain in the bounded manifest after new uploads.
        for i in 0..2005 {
            db.execute("INSERT INTO photos (id,channel_id,storage_key,mime_type) VALUES (?1,?2,?1,'image/png')",params![format!("photo-{i}"),owner.default_channel_id]).unwrap();
        }
        let pin:PlaybackRequest=serde_json::from_value(serde_json::json!({"action":"pin","photo_id":"old-photo","pin_seconds":900,"slide_seconds":30,"blur_background":true})).unwrap();
        assert!(matches!(
            update_playback(&mut db, &other.owner_token, "tv", &pin),
            Err(BetaError::NotFound)
        ));
        let first = update_playback(&mut db, &owner.owner_token, "tv", &pin).unwrap();
        assert_eq!(first.selection_revision, 1);
        assert!(first.pinned_until.unwrap() > unix_now());
        let manifest = protected_manifest_for_household(&db, &owner.household_id, "tv")
            .unwrap()
            .unwrap();
        assert_eq!(manifest.photos.len(), 2000);
        assert_eq!(manifest.photos[0].id, "old-photo");
        let invalid: PlaybackRequest =
            serde_json::from_value(serde_json::json!({"slide_seconds":0})).unwrap();
        assert!(update_playback(&mut db, &owner.owner_token, "tv", &invalid).is_err());
        db.execute(
            "UPDATE frame_playback SET pinned_until=1 WHERE frame_id='tv'",
            [],
        )
        .unwrap();
        assert!(frame_playback(&db, "tv").unwrap().pinned_photo_id.is_none());
        let forever: PlaybackRequest = serde_json::from_value(
            serde_json::json!({"action":"pin","photo_id":"old-photo","pin_seconds":0}),
        )
        .unwrap();
        let next = update_playback(&mut db, &owner.owner_token, "tv", &forever).unwrap();
        assert!(next.pinned_until.is_none());
        assert_eq!(next.pinned_photo_id.as_deref(), Some("old-photo"));
        db.execute(
            "INSERT INTO frame_photo_removals (frame_id,photo_id) VALUES ('tv','old-photo')",
            [],
        )
        .unwrap();
        assert!(frame_playback(&db, "tv").unwrap().pinned_photo_id.is_none());
        assert!(matches!(
            update_playback(&mut db, &owner.owner_token, "tv", &pin),
            Err(BetaError::NotFound)
        ));
        let resume: PlaybackRequest =
            serde_json::from_value(serde_json::json!({"action":"resume"})).unwrap();
        assert!(
            update_playback(&mut db, &owner.owner_token, "tv", &resume)
                .unwrap()
                .selected_photo_id
                .is_none()
        );
    }
    #[test]
    fn playback_styles_are_scoped_capability_gated_and_do_not_interrupt_pins() {
        let mut db = Connection::open_in_memory().unwrap();
        initialize_database(&db).unwrap();
        let owner = create_household_owner(&mut db, "Home", "Owner").unwrap();
        let other = create_household_owner(&mut db, "Other", "Owner").unwrap();
        db.execute(
            "INSERT INTO frames(id,household_id,place_name) VALUES ('styles',?1,'Living room')",
            [&owner.household_id],
        )
        .unwrap();
        db.execute(
            "INSERT INTO frame_subscriptions VALUES ('styles',?1)",
            [&owner.default_channel_id],
        )
        .unwrap();
        db.execute("INSERT INTO photos(id,channel_id,storage_key,mime_type) VALUES ('pinned',?1,'pinned.jpg','image/jpeg')",[&owner.default_channel_id]).unwrap();
        let request = |mode: &str| {
            serde_json::from_value::<PlaybackRequest>(serde_json::json!({"playback_mode":mode}))
                .unwrap()
        };
        assert_eq!(
            frame_playback(&db, "styles").unwrap().playback_mode,
            "scenic_route"
        );
        assert!(
            update_playback(
                &mut db,
                &owner.owner_token,
                "styles",
                &request("serendipity")
            )
            .is_err()
        );
        record_playback_capability(&db, "styles", true).unwrap();
        assert!(matches!(
            update_playback(
                &mut db,
                &other.owner_token,
                "styles",
                &request("serendipity")
            ),
            Err(BetaError::NotFound)
        ));
        let pin: PlaybackRequest=serde_json::from_value(serde_json::json!({"action":"pin","photo_id":"pinned","pin_seconds":0,"slide_seconds":20})).unwrap();
        update_playback(&mut db, &owner.owner_token, "styles", &pin).unwrap();
        for mode in ["scenic_route", "fresh_prints", "serendipity"] {
            let playback =
                update_playback(&mut db, &owner.owner_token, "styles", &request(mode)).unwrap();
            assert_eq!(playback.playback_mode, mode);
            assert_eq!(playback.pinned_photo_id.as_deref(), Some("pinned"));
            assert_eq!(playback.slide_seconds, 20);
        }
        assert!(
            update_playback(
                &mut db,
                &owner.owner_token,
                "styles",
                &request("surprise_me")
            )
            .is_err()
        );
        assert_eq!(
            frame_playback(&db, "styles").unwrap().playback_mode,
            "serendipity"
        );
        initialize_database(&db).unwrap();
        assert_eq!(
            frame_playback(&db, "styles").unwrap().playback_mode,
            "serendipity"
        );
        record_playback_capability(&db, "styles", false).unwrap();
        let playback = frame_playback(&db, "styles").unwrap();
        assert!(!playback.viewer_supports_playback_modes);
        assert_eq!(playback.playback_mode, "serendipity");
        assert!(
            update_playback(
                &mut db,
                &owner.owner_token,
                "styles",
                &request("fresh_prints")
            )
            .is_err()
        );
    }

    #[test]
    fn schema_thirteen_upgrade_is_backed_up_before_adding_playback_styles() {
        let root =
            std::env::temp_dir().join(format!("memory-lane-styles-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&root).unwrap();
        let path = root.join("db.sqlite3");
        let db = Connection::open(&path).unwrap();
        initialize_database(&db).unwrap();
        db.execute_batch("DROP TABLE frame_playback_modes; PRAGMA user_version=13;")
            .unwrap();
        let snapshot = crate::initialize_database_with_backup(&db, &path)
            .unwrap()
            .unwrap();
        let old = Connection::open(snapshot).unwrap();
        assert_eq!(
            old.pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))
                .unwrap(),
            13
        );
        assert!(old.prepare("SELECT * FROM frame_playback_modes").is_err());
        assert!(db.prepare("SELECT * FROM frame_playback_modes").is_ok());
        assert_eq!(
            db.pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))
                .unwrap(),
            crate::api::SCHEMA_VERSION
        );
        drop(old);
        drop(db);
        std::fs::remove_dir_all(root).unwrap();
    }
}
