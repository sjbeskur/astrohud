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
                FROM device_resets WHERE household_id = ?1 AND (state != 'complete' OR id IN
                (SELECT id FROM device_resets WHERE household_id = ?1 AND state = 'complete'
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
        AND state = 'resetting' AND receipt_hash = ?2 AND next_credential_hash = ?3)",
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
    let receipt_hash = hash_secret(receipt);
    let tx = db.transaction()?;
    let (enrollment, frame, state, next_hash): (String, String, String, Vec<u8>) = tx
        .query_row(
            "SELECT enrollment_id, frame_id, state, next_credential_hash FROM device_resets
         WHERE id = ?1 AND receipt_hash = ?2 AND state IN ('resetting', 'complete')",
            params![id, receipt_hash.as_slice()],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .optional()?
        .ok_or(BetaError::Unauthorized)?;
    if state == "complete" {
        return Ok(());
    }
    tx.execute(
        "DELETE FROM device_bootstraps WHERE enrollment_id = ?1",
        [&enrollment],
    )?;
    let changed = tx.execute("UPDATE device_enrollments SET status = 'pending', household_id = NULL, frame_id = NULL,
        credential_hash = ?1, claim_code = ?2, claimed_at = NULL, expires_at = datetime('now', '+15 minutes')
        WHERE id = ?3 AND status = 'claimed' AND frame_id = ?4",
        params![next_hash, random_claim_code(), enrollment, frame])?;
    if changed != 1 {
        return Err(BetaError::NotFound);
    }
    tx.execute("DELETE FROM frames WHERE id = ?1", [&frame])?;
    tx.execute("UPDATE device_resets SET state = 'complete', completed_at = CURRENT_TIMESTAMP WHERE id = ?1", [id])?;
    tx.commit()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::*;

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
