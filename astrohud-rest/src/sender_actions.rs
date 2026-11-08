use crate::{
    AppState, BetaError, SenderAccess, frame_playback, protected_media_storage_key,
    sender_history_media, unix_now,
};
use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;

pub const GUEST_REQUEST_COOLDOWN: u64 = 600;

#[derive(Serialize)]
pub struct SenderPhotoTarget {
    pub frame_id: String,
    pub place_name: String,
    pub enabled: bool,
    pub pinned: bool,
    pub available_at: u64,
}

pub fn sender_photo_targets(
    db: &Connection,
    access: &SenderAccess,
    photo: &str,
) -> Result<Vec<SenderPhotoTarget>, BetaError> {
    if sender_history_media(
        db,
        &access.invitation_id,
        &access.household_id,
        &access.channel_id,
        photo,
    )?
    .is_none()
    {
        return Err(BetaError::NotFound);
    }
    let mut targets = Vec::new();
    for frame in &access.context.frames {
        if protected_media_storage_key(db, &access.household_id, &frame.frame_id, photo)?.is_none()
        {
            continue;
        }
        let playback = frame_playback(db, &frame.frame_id)?;
        let requested: Option<u64> = db
            .query_row(
                "SELECT requested_at FROM frame_guest_requests WHERE frame_id=?1",
                [&frame.frame_id],
                |r| r.get(0),
            )
            .optional()?
            .flatten();
        targets.push(SenderPhotoTarget {
            frame_id: frame.frame_id.clone(),
            place_name: frame.place_name.clone(),
            enabled: playback.guest_display_next,
            pinned: playback.pinned_photo_id.is_some(),
            available_at: requested.map_or(0, |t| t + GUEST_REQUEST_COOLDOWN).max(
                if playback.next_photo_id.is_some() {
                    playback.next_expires_at.unwrap_or(0)
                } else {
                    0
                },
            ),
        });
    }
    Ok(targets)
}

pub enum GuestRequestResult {
    Queued,
    Disabled,
    Pinned,
    Wait(u64),
}

pub fn request_sender_display_next(
    db: &mut Connection,
    access: &SenderAccess,
    photo: &str,
    frame: &str,
) -> Result<GuestRequestResult, BetaError> {
    let tx = db.transaction()?;
    let target = sender_photo_targets(&tx, access, photo)?
        .into_iter()
        .find(|f| f.frame_id == frame)
        .ok_or(BetaError::NotFound)?;
    if !target.enabled {
        return Ok(GuestRequestResult::Disabled);
    }
    if target.pinned {
        return Ok(GuestRequestResult::Pinned);
    }
    let now = unix_now();
    if target.available_at > now {
        return Ok(GuestRequestResult::Wait(target.available_at - now));
    }
    let seconds = frame_playback(&tx, frame)?.slide_seconds;
    tx.execute("UPDATE frame_guest_requests SET photo_id=?2,revision=revision+1,requested_at=?3,expires_at=?4 WHERE frame_id=?1",
        params![frame,photo,now,now + (seconds + 120).max(GUEST_REQUEST_COOLDOWN)])?;
    tx.commit()?;
    Ok(GuestRequestResult::Queued)
}

pub fn delete_sender_photo(
    db: &mut Connection,
    access: &SenderAccess,
    photo: &str,
) -> Result<(), BetaError> {
    let tx = db.transaction()?;
    let key = sender_history_media(
        &tx,
        &access.invitation_id,
        &access.household_id,
        &access.channel_id,
        photo,
    )?
    .ok_or(BetaError::NotFound)?;
    // Keep a durable cleanup record so a failed unlink can be retried after restart.
    tx.execute(
        "INSERT OR IGNORE INTO deleted_media_files(storage_key) VALUES (?1)",
        [key],
    )?;
    tx.execute("UPDATE frame_playback SET selected_photo_id=CASE WHEN selected_photo_id=?1 THEN NULL ELSE selected_photo_id END,
        pinned_until=CASE WHEN pinned_photo_id=?1 THEN NULL ELSE pinned_until END,
        pinned_photo_id=CASE WHEN pinned_photo_id=?1 THEN NULL ELSE pinned_photo_id END,
        selection_revision=selection_revision+1 WHERE selected_photo_id=?1 OR pinned_photo_id=?1",[photo])?;
    tx.execute("DELETE FROM photos WHERE id=?1", [photo])?;
    tx.commit()?;
    Ok(())
}

pub fn purge_deleted_media(state: &AppState) -> rusqlite::Result<()> {
    let db = state
        .database
        .lock()
        .map_err(|_| rusqlite::Error::InvalidQuery)?;
    let keys = db
        .prepare("SELECT storage_key FROM deleted_media_files")?
        .query_map([], |r| r.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for key in keys {
        match std::fs::remove_file(state.media_dir.join(&key)) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => {
                eprintln!("deleted photo file cleanup will retry: {e}");
                continue;
            }
        }
        db.execute(
            "DELETE FROM deleted_media_files WHERE storage_key=?1",
            [key],
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        PlaybackRequest, create_household_owner, create_sender_invitation, initialize_database,
        protected_manifest_for_household, sender_access, update_playback,
    };
    #[test]
    fn guest_requests_require_opt_in_share_a_cooldown_and_respect_owner_commands() {
        let mut db = Connection::open_in_memory().unwrap();
        initialize_database(&db).unwrap();
        let owner = create_household_owner(&mut db, "Home", "Owner").unwrap();
        let alice = create_sender_invitation_for_test(&mut db, &owner);
        let bob = create_sender_invitation(&db, &owner.owner_token, "Bob").unwrap();
        for (photo, invitation) in [("alice", &alice), ("bob", &bob)] {
            db.execute("INSERT INTO photos(id,channel_id,storage_key,mime_type) VALUES (?1,?2,?1,'image/png')",params![photo,owner.default_channel_id]).unwrap();
            db.execute(
                "INSERT INTO photo_contributors VALUES (?1,?2,?2,?3,'invited')",
                params![photo, invitation.invitation_id, invitation.label],
            )
            .unwrap();
        }
        let a = sender_access(&db, &alice.token).unwrap();
        let b = sender_access(&db, &bob.token).unwrap();
        assert!(matches!(
            request_sender_display_next(&mut db, &a, "alice", "tv").unwrap(),
            GuestRequestResult::Disabled
        ));
        assert!(matches!(
            request_sender_display_next(&mut db, &a, "bob", "tv"),
            Err(BetaError::NotFound)
        ));
        let enable: PlaybackRequest =
            serde_json::from_value(serde_json::json!({"guest_display_next":true})).unwrap();
        update_playback(&mut db, &owner.owner_token, "tv", &enable).unwrap();
        assert!(matches!(
            request_sender_display_next(&mut db, &a, "alice", "tv").unwrap(),
            GuestRequestResult::Queued
        ));
        let queued = frame_playback(&db, "tv").unwrap();
        assert_eq!(queued.next_photo_id.as_deref(), Some("alice"));
        assert!(queued.selected_photo_id.is_none());
        // A request from older history must still reach the bounded device playlist.
        for index in 0..205 {
            db.execute("INSERT INTO photos(id,channel_id,storage_key,mime_type) VALUES (?1,?2,?1,'image/png')",
                params![format!("later-{index}"),owner.default_channel_id]).unwrap();
        }
        let manifest = protected_manifest_for_household(&db, &owner.household_id, "tv")
            .unwrap()
            .unwrap();
        assert_eq!(manifest.photos.len(), 200);
        assert_eq!(manifest.photos[0].id, "alice");
        db.execute("DELETE FROM photos WHERE id LIKE 'later-%'", [])
            .unwrap();
        assert!(matches!(
            request_sender_display_next(&mut db, &b, "bob", "tv").unwrap(),
            GuestRequestResult::Wait(_)
        ));
        let pin: PlaybackRequest = serde_json::from_value(
            serde_json::json!({"action":"pin","photo_id":"bob","pin_seconds":0}),
        )
        .unwrap();
        update_playback(&mut db, &owner.owner_token, "tv", &pin).unwrap();
        assert!(frame_playback(&db, "tv").unwrap().next_photo_id.is_none());
        assert!(matches!(
            request_sender_display_next(&mut db, &a, "alice", "tv").unwrap(),
            GuestRequestResult::Pinned
        ));
        let resume: PlaybackRequest =
            serde_json::from_value(serde_json::json!({"action":"resume"})).unwrap();
        update_playback(&mut db, &owner.owner_token, "tv", &resume).unwrap();
        assert!(matches!(
            request_sender_display_next(&mut db, &a, "alice", "tv").unwrap(),
            GuestRequestResult::Wait(_)
        ));
        db.execute(
            "UPDATE frame_guest_requests SET requested_at=?1,expires_at=?1",
            [unix_now() - 601],
        )
        .unwrap();
        assert!(matches!(
            request_sender_display_next(&mut db, &b, "bob", "tv").unwrap(),
            GuestRequestResult::Queued
        ));
        let off: PlaybackRequest =
            serde_json::from_value(serde_json::json!({"guest_display_next":false})).unwrap();
        update_playback(&mut db, &owner.owner_token, "tv", &off).unwrap();
        assert!(frame_playback(&db, "tv").unwrap().next_photo_id.is_none());
        // An explicit delivery plan prevents a sender from requesting an unselected frame.
        db.execute("INSERT INTO photo_delivery_plans VALUES ('alice')", [])
            .unwrap();
        assert!(matches!(
            request_sender_display_next(&mut db, &a, "alice", "tv"),
            Err(BetaError::NotFound)
        ));
        db.execute(
            "INSERT INTO photo_frame_deliveries VALUES ('alice','tv')",
            [],
        )
        .unwrap();
        let manifest = protected_manifest_for_household(&db, &owner.household_id, "tv")
            .unwrap()
            .unwrap();
        assert_eq!(manifest.photos.len(), 2);
        assert!(matches!(
            delete_sender_photo(&mut db, &a, "bob"),
            Err(BetaError::NotFound)
        ));
        update_playback(&mut db, &owner.owner_token, "tv", &pin).unwrap();
        delete_sender_photo(&mut db, &b, "bob").unwrap();
        assert!(frame_playback(&db, "tv").unwrap().pinned_photo_id.is_none());
        assert!(frame_playback(&db, "tv").unwrap().pinned_until.is_none());
        assert_eq!(
            protected_manifest_for_household(&db, &owner.household_id, "tv")
                .unwrap()
                .unwrap()
                .photos
                .len(),
            1
        );
        db.execute("DELETE FROM frames WHERE id='tv'", []).unwrap();
        delete_sender_photo(&mut db, &a, "alice").unwrap();
        assert_eq!(
            db.query_row("SELECT COUNT(*) FROM deleted_media_files", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            2
        );
        let dir = std::env::temp_dir().join(format!("memory-lane-delete-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("alice"), b"test").unwrap();
        let state = AppState::new(db, dir.clone());
        purge_deleted_media(&state).unwrap();
        assert!(!dir.join("alice").exists());
        assert_eq!(
            state
                .database
                .lock()
                .unwrap()
                .query_row("SELECT COUNT(*) FROM deleted_media_files", [], |r| r
                    .get::<_, i64>(0))
                .unwrap(),
            0
        );
        std::fs::remove_dir_all(dir).unwrap();
    }
    fn create_sender_invitation_for_test(
        db: &mut Connection,
        owner: &crate::OwnerBootstrap,
    ) -> crate::CreatedInvitation {
        db.execute(
            "INSERT INTO frames(id,household_id,place_name) VALUES ('tv',?1,'TV')",
            [&owner.household_id],
        )
        .unwrap();
        db.execute(
            "INSERT INTO frame_subscriptions VALUES ('tv',?1)",
            [&owner.default_channel_id],
        )
        .unwrap();
        create_sender_invitation(db, &owner.owner_token, "Alice").unwrap()
    }
}

#[cfg(test)]
mod migration_tests {
    use super::*;
    #[test]
    fn schema_ten_upgrade_is_backed_up_and_preserves_owner_playback() {
        let dir =
            std::env::temp_dir().join(format!("memory-lane-schema11-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("state.sqlite3");
        let db = Connection::open(&path).unwrap();
        crate::initialize_database(&db).unwrap();
        db.execute(
            "INSERT INTO frame_playback(frame_id,slide_seconds) VALUES ('demo-frame',30)",
            [],
        )
        .unwrap();
        db.execute_batch("DROP TABLE frame_guest_requests; DROP TABLE deleted_media_files; DROP TABLE co_owner_invitations; PRAGMA user_version=10;").unwrap();
        let snapshot = crate::initialize_database_with_backup(&db, &path)
            .unwrap()
            .unwrap();
        let old = Connection::open(snapshot).unwrap();
        assert_eq!(
            old.pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))
                .unwrap(),
            10
        );
        assert_eq!(frame_playback(&db, "demo-frame").unwrap().slide_seconds, 30);
        assert!(
            !frame_playback(&db, "demo-frame")
                .unwrap()
                .guest_display_next
        );
        assert_eq!(
            db.pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))
                .unwrap(),
            12
        );
        assert!(
            crate::initialize_database_with_backup(&db, &path)
                .unwrap()
                .is_none()
        );
        drop(old);
        drop(db);
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn interrupted_file_deletion_is_durable_and_retried() {
        let db = Connection::open_in_memory().unwrap();
        crate::initialize_database(&db).unwrap();
        db.execute("INSERT INTO deleted_media_files VALUES ('pending')", [])
            .unwrap();
        let dir = std::env::temp_dir().join(format!("memory-lane-unlink-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(dir.join("pending")).unwrap();
        let state = AppState::new(db, dir.clone());
        purge_deleted_media(&state).unwrap();
        assert_eq!(
            state
                .database
                .lock()
                .unwrap()
                .query_row("SELECT COUNT(*) FROM deleted_media_files", [], |r| r
                    .get::<_, i64>(0))
                .unwrap(),
            1
        );
        std::fs::remove_dir(dir.join("pending")).unwrap();
        std::fs::write(dir.join("pending"), b"photo").unwrap();
        purge_deleted_media(&state).unwrap();
        assert!(!dir.join("pending").exists());
        std::fs::remove_dir_all(dir).unwrap();
    }
}
