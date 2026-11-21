use crate::BetaError;
use crate::beta::{hash_secret, owner_household_id, random_claim_code, validate_secret};
use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;
use uuid::Uuid;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct DeviceReset {
    pub reset_id: String,
    pub frame_id: String,
    pub place_name: String,
    pub state: String,
    pub requested_at: String,
    pub completed_at: Option<String>,
}

pub(crate) fn owner_resets(db: &Connection, household: &str) -> rusqlite::Result<Vec<DeviceReset>> {
    db.prepare(
        "SELECT id, frame_id, place_name, state, requested_at, completed_at
                FROM device_resets WHERE household_id = ?1 AND forgotten_at IS NULL AND (state != 'complete' OR id IN
                (SELECT id FROM device_resets WHERE household_id = ?1 AND state = 'complete' AND forgotten_at IS NULL
                 ORDER BY requested_at DESC, rowid DESC LIMIT 50))
                ORDER BY requested_at DESC, rowid DESC",
    )?
    .query_map([household], |row| {
        Ok(DeviceReset {
            reset_id: row.get(0)?,
            frame_id: row.get(1)?,
            place_name: row.get(2)?,
            state: row.get(3)?,
            requested_at: row.get(4)?,
            completed_at: row.get(5)?,
        })
    })?
    .collect()
}

pub fn request_device_reset(
    db: &Connection,
    owner: &str,
    frame: &str,
    confirm_name: &str,
) -> Result<DeviceReset, BetaError> {
    let household = owner_household_id(db, owner)?;
    let (enrollment, place): (String, String) = db
        .query_row(
            "SELECT e.id, f.place_name FROM frames f JOIN device_enrollments e ON e.frame_id = f.id
         WHERE f.id = ?1 AND f.household_id = ?2 AND e.status = 'claimed'",
            params![frame, household],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?
        .ok_or(BetaError::NotFound)?;
    if confirm_name != place {
        return Err(BetaError::InvalidInput(
            "enter the device's place name to confirm reset",
        ));
    }
    crate::updates::expire_command(db, frame)?;
    let updating: bool = db.query_row(
        "SELECT EXISTS(SELECT 1 FROM frame_updates WHERE frame_id=?1 AND command_state IN ('pending','running'))",
        [frame], |r| r.get(0))?;
    if updating {
        return Err(BetaError::InvalidInput(
            "finish the application update request before resetting the device",
        ));
    }
    db.execute("INSERT OR IGNORE INTO device_resets (id, enrollment_id, household_id, frame_id, place_name, state)
                VALUES (?1, ?2, ?3, ?4, ?5, 'pending')",
        params![Uuid::new_v4().to_string(), enrollment, household, frame, place])?;
    owner_resets(db, &household)?
        .into_iter()
        .find(|r| r.frame_id == frame && r.state != "complete")
        .ok_or(BetaError::NotFound)
}

pub fn pending_device_reset(
    db: &Connection,
    credential: &str,
) -> Result<Option<DeviceReset>, BetaError> {
    let hash = hash_secret(credential);
    let household: String = db
        .query_row(
            "SELECT household_id FROM device_enrollments
        WHERE credential_hash = ?1 AND status = 'claimed'",
            [hash.as_slice()],
            |row| row.get(0),
        )
        .optional()?
        .ok_or(BetaError::Unauthorized)?;
    let reset_id: Option<String> = db
        .query_row(
            "SELECT r.id FROM device_resets r
        JOIN device_enrollments e ON e.id = r.enrollment_id
        WHERE e.credential_hash = ?1 AND r.state != 'complete'",
            [hash.as_slice()],
            |row| row.get(0),
        )
        .optional()?;
    Ok(owner_resets(db, &household)?
        .into_iter()
        .find(|r| Some(&r.reset_id) == reset_id.as_ref()))
}

pub fn accept_device_reset(
    db: &Connection,
    credential: &str,
    id: &str,
    receipt: &str,
    next_credential: &str,
) -> Result<(), BetaError> {
    validate_secret(receipt, "reset receipt")?;
    validate_secret(next_credential, "next device credential")?;
    if credential == next_credential || receipt == next_credential {
        return Err(BetaError::InvalidInput(
            "reset must rotate the device credential",
        ));
    }
    let hash = hash_secret(credential);
    let receipt_hash = hash_secret(receipt);
    let next_hash = hash_secret(next_credential);
    let changed = db.execute(
        "UPDATE device_resets SET state = 'resetting', receipt_hash = ?1, next_credential_hash = ?2
        WHERE id = ?3 AND state = 'pending' AND enrollment_id IN
        (SELECT id FROM device_enrollments WHERE credential_hash = ?4 AND status = 'claimed')",
        params![
            receipt_hash.as_slice(),
            next_hash.as_slice(),
            id,
            hash.as_slice()
        ],
    )?;
    if changed == 1 {
        return Ok(());
    }
    let retry: bool = db.query_row(
        "SELECT EXISTS(SELECT 1 FROM device_resets WHERE id = ?1
        AND (state = 'resetting' OR (state = 'complete' AND forgotten_at IS NOT NULL))
        AND receipt_hash = ?2 AND next_credential_hash = ?3)",
        params![id, receipt_hash.as_slice(), next_hash.as_slice()],
        |row| row.get(0),
    )?;
    if retry && credential == receipt {
        Ok(())
    } else {
        Err(BetaError::Unauthorized)
    }
}

pub fn complete_device_reset(
    db: &mut Connection,
    id: &str,
    receipt: &str,
) -> Result<(), BetaError> {
    let tx = db.transaction()?;
    let authorized: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM device_resets WHERE id=?1 AND receipt_hash=?2
         AND state IN ('resetting','complete'))",
        params![id, hash_secret(receipt)],
        |row| row.get(0),
    )?;
    if !authorized {
        return Err(BetaError::Unauthorized);
    }
    finish_reset(&tx, id)?;
    tx.commit()?;
    Ok(())
}

/// Remove an accepted reset device from its old household, even while offline.
/// Keep its receipt so interrupted acceptance/cleanup can resume afterwards.
/// A completed historical reset must never touch a later owner's pairing.
pub fn forget_reset_device(
    db: &mut Connection,
    owner: &str,
    id: &str,
    confirm_name: &str,
) -> Result<(), BetaError> {
    let tx = db.transaction()?;
    let household = owner_household_id(&tx, owner)?;
    let (state, place): (String, String) = tx
        .query_row(
            "SELECT state, place_name FROM device_resets WHERE id=?1 AND household_id=?2",
            params![id, household],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?
        .ok_or(BetaError::NotFound)?;
    if place != confirm_name {
        return Err(BetaError::InvalidInput(
            "confirm the device's place name to forget it",
        ));
    }
    if state == "pending" {
        return Err(BetaError::InvalidInput(
            "the device must accept its factory reset before it can be forgotten",
        ));
    }
    finish_reset(&tx, id)?;
    tx.execute("UPDATE device_resets SET forgotten_at=COALESCE(forgotten_at,CURRENT_TIMESTAMP) WHERE id=?1", [id])?;
    tx.commit()?;
    Ok(())
}

fn finish_reset(db: &Connection, id: &str) -> Result<(), BetaError> {
    let (enrollment, frame, state, next_hash): (String, String, String, Vec<u8>) = db
        .query_row(
            "SELECT enrollment_id, frame_id, state, next_credential_hash FROM device_resets
         WHERE id=?1 AND state IN ('resetting','complete')",
            [id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .optional()?
        .ok_or(BetaError::NotFound)?;
    if state == "complete" {
        return Ok(());
    }
    db.execute(
        "DELETE FROM device_bootstraps WHERE enrollment_id = ?1",
        [&enrollment],
    )?;
    let changed = db.execute("UPDATE device_enrollments SET status = 'pending', household_id = NULL, frame_id = NULL,
        credential_hash = ?1, claim_code = ?2, claimed_at = NULL, expires_at = datetime('now', '+15 minutes')
        WHERE id = ?3 AND status = 'claimed' AND frame_id = ?4",
        params![next_hash, random_claim_code(), enrollment, frame])?;
    if changed != 1 {
        return Err(BetaError::NotFound);
    }
    db.execute("DELETE FROM frames WHERE id = ?1", [&frame])?;
    db.execute("UPDATE device_resets SET state = 'complete', completed_at = CURRENT_TIMESTAMP WHERE id = ?1", [id])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::*;

    #[test]
    fn reclaiming_a_reset_frame_starts_empty_without_deleting_household_photos() {
        for manual_code in [false, true] {
            let mut db = Connection::open_in_memory().unwrap();
            initialize_database(&db).unwrap();
            let id = Uuid::new_v4().to_string();
            let old = "reset-library-old-credential-abcdefghijklmnopqrstuvwxyz";
            let next = "reset-library-next-credential-abcdefghijklmnopqrstuvwxyz";
            let bootstrap = "reset-library-bootstrap-abcdefghijklmnopqrstuvwxyz";
            register_bootstrap_device(&db, &id, "AB23CD", old, bootstrap).unwrap();
            let claim = claim_bootstrap_device(&mut db, bootstrap, "Kitchen").unwrap();
            let household = &claim.frame.household_id;
            let channel: String = db
                .query_row(
                    "SELECT id FROM channels WHERE household_id=?1",
                    [household],
                    |r| r.get(0),
                )
                .unwrap();
            for photo in ["legacy-photo", "targeted-photo"] {
                db.execute("INSERT INTO photos(id,channel_id,storage_key,mime_type) VALUES(?1,?2,?3,'image/jpeg')",
                    rusqlite::params![photo, channel, format!("{photo}.jpg")]).unwrap();
            }
            db.execute(
                "INSERT INTO photo_delivery_plans(photo_id) VALUES('targeted-photo')",
                [],
            )
            .unwrap();
            db.execute(
                "INSERT INTO photo_frame_deliveries(photo_id,frame_id) VALUES('targeted-photo',?1)",
                [&id],
            )
            .unwrap();
            assert_eq!(
                protected_manifest_for_household(&db, household, &id)
                    .unwrap()
                    .unwrap()
                    .photos
                    .len(),
                2
            );
            // Never-reset devices retain existing behavior, and keep their photos.
            let other = Uuid::new_v4().to_string();
            let other_bootstrap = "reset-library-other-bootstrap-abcdefghijklmnopqrstuvwxyz";
            register_bootstrap_device(
                &db,
                &other,
                "BC23DE",
                "reset-library-other-credential-abcdefghijklmnopqrstuvwxyz",
                other_bootstrap,
            )
            .unwrap();
            claim_bootstrap_device_for_owner(&mut db, other_bootstrap, "Den", &claim.owner_token)
                .unwrap();
            assert_eq!(
                protected_manifest_for_household(&db, household, &other)
                    .unwrap()
                    .unwrap()
                    .photos
                    .len(),
                1
            );
            let reset = request_device_reset(&db, &claim.owner_token, &id, "Kitchen").unwrap();
            let receipt = "reset-library-receipt-abcdefghijklmnopqrstuvwxyz";
            accept_device_reset(&db, old, &reset.reset_id, receipt, next).unwrap();
            complete_device_reset(&mut db, &reset.reset_id, receipt).unwrap();
            let fresh_bootstrap = "reset-library-fresh-bootstrap-abcdefghijklmnopqrstuvwxyz";
            let registration =
                register_bootstrap_device(&db, &id, "AB23CD", next, fresh_bootstrap).unwrap();
            if manual_code {
                let DeviceEnrollmentState::Pending { claim_code, .. } = registration.state else {
                    panic!("expected pending")
                };
                claim_device(&mut db, &claim.owner_token, &claim_code, "Kitchen again").unwrap();
            } else {
                claim_bootstrap_device_for_owner(
                    &mut db,
                    fresh_bootstrap,
                    "Kitchen again",
                    &claim.owner_token,
                )
                .unwrap();
            }
            assert!(
                protected_manifest_for_household(&db, household, &id)
                    .unwrap()
                    .unwrap()
                    .photos
                    .is_empty()
            );
            assert!(
                owner_gallery_page(&db, household, &id, None, None)
                    .unwrap()
                    .unwrap()
                    .photos
                    .is_empty()
            );
            assert_eq!(
                protected_manifest_for_household(&db, household, &other)
                    .unwrap()
                    .unwrap()
                    .photos
                    .len(),
                1
            );
            let count: i64 = db
                .query_row("SELECT COUNT(*) FROM photos", [], |r| r.get(0))
                .unwrap();
            assert_eq!(count, 2, "reset must not delete the household library");
            assert!(claimed_device_access(&db, old).is_err());
            // A subsequent delivery still reaches the newly claimed frame.
            db.execute("INSERT INTO photos(id,channel_id,storage_key,mime_type) VALUES('fresh-photo',?1,'fresh.jpg','image/jpeg')", [&channel]).unwrap();
            let photos = protected_manifest_for_household(&db, household, &id)
                .unwrap()
                .unwrap()
                .photos;
            assert_eq!(photos.len(), 1);
            assert_eq!(photos[0].id, "fresh-photo");
            let gallery = owner_gallery_page(&db, household, &id, None, None)
                .unwrap()
                .unwrap();
            assert_eq!(gallery.photos.len(), 1);
            assert_eq!(gallery.photos[0].id, "fresh-photo");
        }
    }

    #[test]
    fn forgetting_an_accepted_reset_preserves_retry_and_other_household_data() {
        let mut db = Connection::open_in_memory().unwrap();
        initialize_database(&db).unwrap();
        let original = "forget-original-credential-abcdefghijklmnopqrstuvwxyz";
        let next = "forget-next-credential-abcdefghijklmnopqrstuvwxyz";
        let bootstrap = "forget-bootstrap-abcdefghijklmnopqrstuvwxyz";
        let receipt = "forget-receipt-abcdefghijklmnopqrstuvwxyz";
        let id = Uuid::new_v4().to_string();
        register_bootstrap_device(&db, &id, "AB23CD", original, bootstrap).unwrap();
        let claim = claim_bootstrap_device(&mut db, bootstrap, "Kitchen").unwrap();
        let other = create_household_owner(&mut db, "Other", "Owner").unwrap();
        let channel: String = db
            .query_row(
                "SELECT id FROM channels WHERE household_id=?1",
                [&claim.frame.household_id],
                |r| r.get(0),
            )
            .unwrap();
        db.execute("INSERT INTO photos(id,channel_id,storage_key,mime_type) VALUES('saved-photo',?1,'photo.jpg','image/jpeg')", [&channel]).unwrap();
        db.execute(
            "INSERT INTO frames(id,household_id,place_name) VALUES('other-frame',?1,'Den')",
            [&claim.frame.household_id],
        )
        .unwrap();
        let reset = request_device_reset(&db, &claim.owner_token, &id, "Kitchen").unwrap();
        assert!(matches!(
            forget_reset_device(&mut db, &other.owner_token, &reset.reset_id, "Kitchen"),
            Err(BetaError::NotFound)
        ));
        assert!(matches!(
            forget_reset_device(&mut db, &claim.owner_token, &reset.reset_id, "Kitchen"),
            Err(BetaError::InvalidInput(_))
        ));
        accept_device_reset(&db, original, &reset.reset_id, receipt, next).unwrap();
        assert!(
            forget_reset_device(&mut db, &claim.owner_token, &reset.reset_id, "wrong").is_err()
        );
        forget_reset_device(&mut db, &claim.owner_token, &reset.reset_id, "Kitchen").unwrap();
        let context = owner_context(&db, &claim.owner_token).unwrap();
        assert!(context.resets.is_empty());
        assert_eq!(context.frames.len(), 1);
        assert_eq!(context.frames[0].frame_id, "other-frame");
        assert_eq!(
            db.query_row(
                "SELECT COUNT(*) FROM photos WHERE id='saved-photo'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            1
        );
        assert!(claimed_device_access(&db, original).is_err());
        // Acceptance response may have been lost before owner forgot the device.
        accept_device_reset(&db, receipt, &reset.reset_id, receipt, next).unwrap();
        assert!(accept_device_reset(&db, receipt, &reset.reset_id, receipt, original).is_err());
        complete_device_reset(&mut db, &reset.reset_id, receipt).unwrap();
        let fresh_bootstrap = "forget-fresh-bootstrap-abcdefghijklmnopqrstuvwxyz";
        register_bootstrap_device(&db, &id, "AB23CD", next, fresh_bootstrap).unwrap();
        let new_owner = claim_bootstrap_device(&mut db, fresh_bootstrap, "New home").unwrap();
        // Old owner retries cannot delete a later owner's pairing.
        forget_reset_device(&mut db, &claim.owner_token, &reset.reset_id, "Kitchen").unwrap();
        complete_device_reset(&mut db, &reset.reset_id, receipt).unwrap();
        assert_eq!(
            claimed_device_access(&db, next).unwrap().household_id,
            new_owner.frame.household_id
        );
        let new_reset = request_device_reset(&db, &new_owner.owner_token, &id, "New home").unwrap();
        assert_eq!(
            pending_device_reset(&db, next).unwrap().unwrap().reset_id,
            new_reset.reset_id
        );
    }

    #[test]
    fn forgetting_completed_history_does_not_touch_a_reclaimed_device() {
        let mut db = Connection::open_in_memory().unwrap();
        initialize_database(&db).unwrap();
        let old = "history-old-credential-abcdefghijklmnopqrstuvwxyz";
        let next = "history-next-credential-abcdefghijklmnopqrstuvwxyz";
        let bootstrap = "history-bootstrap-abcdefghijklmnopqrstuvwxyz";
        let receipt = "history-receipt-abcdefghijklmnopqrstuvwxyz";
        let id = Uuid::new_v4().to_string();
        register_bootstrap_device(&db, &id, "AB23CD", old, bootstrap).unwrap();
        let claim = claim_bootstrap_device(&mut db, bootstrap, "Kitchen").unwrap();
        let reset = request_device_reset(&db, &claim.owner_token, &id, "Kitchen").unwrap();
        accept_device_reset(&db, old, &reset.reset_id, receipt, next).unwrap();
        complete_device_reset(&mut db, &reset.reset_id, receipt).unwrap();
        let fresh = "history-fresh-bootstrap-abcdefghijklmnopqrstuvwxyz";
        register_bootstrap_device(&db, &id, "AB23CD", next, fresh).unwrap();
        let new_owner = claim_bootstrap_device(&mut db, fresh, "Bedroom").unwrap();
        forget_reset_device(&mut db, &claim.owner_token, &reset.reset_id, "Kitchen").unwrap();
        assert!(
            owner_context(&db, &claim.owner_token)
                .unwrap()
                .resets
                .is_empty()
        );
        assert_eq!(
            claimed_device_access(&db, next).unwrap().household_id,
            new_owner.frame.household_id
        );
    }

    #[test]
    fn schema_15_upgrade_preserves_reset_and_creates_a_backup() {
        let directory = std::env::temp_dir().join(format!("forget-migration-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join("state.sqlite3");
        let mut db = Connection::open(&path).unwrap();
        initialize_database(&db).unwrap();
        let bootstrap = "migration-reset-bootstrap-abcdefghijklmnopqrstuvwxyz";
        let credential = "migration-reset-credential-abcdefghijklmnopqrstuvwxyz";
        let id = Uuid::new_v4().to_string();
        register_bootstrap_device(&db, &id, "AB23CD", credential, bootstrap).unwrap();
        let claim = claim_bootstrap_device(&mut db, bootstrap, "Kitchen").unwrap();
        let reset = request_device_reset(&db, &claim.owner_token, &id, "Kitchen").unwrap();
        db.execute_batch(
            "ALTER TABLE device_resets DROP COLUMN forgotten_at; PRAGMA user_version=15;",
        )
        .unwrap();
        let backup = initialize_database_with_backup(&db, &path)
            .unwrap()
            .unwrap();
        let old = Connection::open(backup).unwrap();
        assert_eq!(
            old.pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))
                .unwrap(),
            15
        );
        assert_eq!(
            old.query_row(
                "SELECT count(*) FROM pragma_table_info('device_resets') WHERE name='forgotten_at'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            0
        );
        assert_eq!(
            owner_context(&db, &claim.owner_token).unwrap().resets[0],
            reset
        );
        assert!(
            initialize_database_with_backup(&db, &path)
                .unwrap()
                .is_none()
        );
        drop(old);
        drop(db);
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn reset_is_scoped_durable_and_cannot_be_replayed_against_a_new_owner() {
        let mut db = Connection::open_in_memory().unwrap();
        initialize_database(&db).unwrap();
        let original = "original-device-credential-abcdefghijklmnopqrstuvwxyz";
        let next = "next-device-credential-abcdefghijklmnopqrstuvwxyz012345";
        let bootstrap = "original-bootstrap-secret-abcdefghijklmnopqrstuvwxyz";
        let fresh_bootstrap = "fresh-bootstrap-secret-abcdefghijklmnopqrstuvwxyz0123";
        let receipt = "reset-receipt-secret-abcdefghijklmnopqrstuvwxyz012345";
        let device_id = Uuid::new_v4().to_string();
        register_bootstrap_device(&db, &device_id, "AB23CD", original, bootstrap).unwrap();
        let claim = claim_bootstrap_device(&mut db, bootstrap, "Kitchen").unwrap();
        let other_owner = create_household_owner(&mut db, "Other", "Owner").unwrap();
        let household = &claim.frame.household_id;
        let channel: String = db
            .query_row(
                "SELECT id FROM channels WHERE household_id = ?1",
                [household],
                |r| r.get(0),
            )
            .unwrap();
        db.execute(
            "INSERT INTO frames (id, household_id, place_name) VALUES ('second-frame', ?1, 'Den')",
            [household],
        )
        .unwrap();
        db.execute(
            "INSERT INTO frame_subscriptions (frame_id, channel_id) VALUES ('second-frame', ?1)",
            [&channel],
        )
        .unwrap();
        db.execute("INSERT INTO photos (id, channel_id, storage_key, mime_type) VALUES ('family-photo', ?1, 'private.jpg', 'image/jpeg')", [&channel]).unwrap();
        assert!(matches!(
            request_device_reset(&db, &other_owner.owner_token, &device_id, "Kitchen"),
            Err(BetaError::NotFound)
        ));
        assert!(matches!(
            request_device_reset(&db, &claim.owner_token, &device_id, "wrong"),
            Err(BetaError::InvalidInput(_))
        ));
        assert!(pending_device_reset(&db, original).unwrap().is_none());
        let reset = request_device_reset(&db, &claim.owner_token, &device_id, "Kitchen").unwrap();
        assert_eq!(reset.state, "pending");
        assert_eq!(
            request_device_reset(&db, &claim.owner_token, &device_id, "Kitchen")
                .unwrap()
                .reset_id,
            reset.reset_id
        );
        initialize_database(&db).unwrap();
        assert_eq!(pending_device_reset(&db, original).unwrap().unwrap(), reset);
        assert!(complete_device_reset(&mut db, &reset.reset_id, receipt).is_err());
        assert!(accept_device_reset(&db, "wrong-device", &reset.reset_id, receipt, next).is_err());
        assert!(accept_device_reset(&db, original, &reset.reset_id, receipt, original).is_err());
        accept_device_reset(&db, original, &reset.reset_id, receipt, next).unwrap();
        // A lost acceptance response is retried with the restricted receipt.
        accept_device_reset(&db, receipt, &reset.reset_id, receipt, next).unwrap();
        assert!(accept_device_reset(&db, "wrong-device", &reset.reset_id, receipt, next).is_err());
        assert!(claimed_device_access(&db, original).is_err());
        assert!(claimed_device_access(&db, receipt).is_err());
        assert!(
            register_bootstrap_device(&db, &device_id, "AB23CD", next, fresh_bootstrap).is_err()
        );
        assert_eq!(
            owner_context(&db, &claim.owner_token).unwrap().resets[0].state,
            "resetting"
        );
        complete_device_reset(&mut db, &reset.reset_id, receipt).unwrap();
        assert!(claimed_device_access(&db, original).is_err());
        assert!(register_device(&db, &device_id, "AB23CD", original).is_err());
        assert!(claim_bootstrap_device(&mut db, bootstrap, "Old bootstrap").is_err());
        let context = owner_context(&db, &claim.owner_token).unwrap();
        assert_eq!(context.frames.len(), 1);
        assert_eq!(context.frames[0].frame_id, "second-frame");
        assert_eq!(context.resets[0].state, "complete");
        assert!(context.resets[0].completed_at.is_some());
        assert!(
            protected_media_storage_key(&db, household, "second-frame", "family-photo")
                .unwrap()
                .is_some()
        );
        register_bootstrap_device(&db, &device_id, "AB23CD", next, fresh_bootstrap).unwrap();
        let new_claim = claim_bootstrap_device(&mut db, fresh_bootstrap, "New home").unwrap();
        assert_ne!(new_claim.frame.household_id, *household);
        assert!(
            protected_media_storage_key(
                &db,
                &new_claim.frame.household_id,
                &device_id,
                "family-photo"
            )
            .unwrap()
            .is_none()
        );
        assert!(matches!(
            request_device_reset(&db, &claim.owner_token, &device_id, "New home"),
            Err(BetaError::NotFound)
        ));
        // Completion retries are harmless even after resale and a new claim.
        complete_device_reset(&mut db, &reset.reset_id, receipt).unwrap();
        assert_eq!(
            claimed_device_access(&db, next).unwrap().household_id,
            new_claim.frame.household_id
        );
        assert!(accept_device_reset(&db, receipt, &reset.reset_id, receipt, next).is_err());
    }
}
