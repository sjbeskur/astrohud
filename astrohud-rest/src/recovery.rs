use crate::BetaError;
use crate::beta::{hash_secret, owner_household_id, random_token, validate_secret};
use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;
use uuid::Uuid;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct RecoveryStatus {
    pub created_at: String,
    pub last_used_at: Option<String>,
}

#[derive(Serialize)]
pub struct CreatedRecovery {
    pub token: String,
    pub created_at: String,
}

pub(crate) fn recovery_status(
    db: &Connection,
    household: &str,
) -> rusqlite::Result<Option<RecoveryStatus>> {
    db.query_row(
        "SELECT created_at, last_used_at FROM owner_recovery_links
        WHERE household_id = ?1 AND (expires_at IS NULL OR expires_at > CURRENT_TIMESTAMP)",
        [household],
        |row| {
            Ok(RecoveryStatus {
                created_at: row.get(0)?,
                last_used_at: row.get(1)?,
            })
        },
    )
    .optional()
}

pub fn create_owner_recovery(
    db: &Connection,
    owner_token: &str,
) -> Result<CreatedRecovery, BetaError> {
    let household = owner_household_id(db, owner_token)?;
    let token = random_token();
    let hash = hash_secret(&token);
    let created_at = db.query_row("INSERT INTO owner_recovery_links (household_id, token_hash)
        VALUES (?1, ?2) ON CONFLICT(household_id) DO UPDATE SET
        token_hash = excluded.token_hash, created_at = CURRENT_TIMESTAMP, last_used_at = NULL, expires_at = NULL
        RETURNING created_at", params![household, hash.as_slice()], |row| row.get(0))?;
    Ok(CreatedRecovery { token, created_at })
}

/// A recovery credential is never itself a browser session. Each use creates
/// a separate expiring owner grant; rotating recovery leaves those grants alone.
pub fn recover_owner(db: &mut Connection, recovery_token: &str) -> Result<String, BetaError> {
    validate_secret(recovery_token, "recovery link is invalid")?;
    let hash = hash_secret(recovery_token);
    let tx = db.transaction()?;
    let household: String = tx
        .query_row(
            "SELECT household_id FROM owner_recovery_links
        WHERE token_hash = ?1 AND (expires_at IS NULL OR expires_at > CURRENT_TIMESTAMP)",
            [hash.as_slice()],
            |row| row.get(0),
        )
        .optional()?
        .ok_or(BetaError::Unauthorized)?;
    let session = random_token();
    let session_hash = hash_secret(&session);
    tx.execute(
        "INSERT INTO owner_access_grants (id, household_id, label, token_hash, expires_at)
        VALUES (?1, ?2, 'Recovered owner', ?3, datetime('now', '+30 days'))",
        params![
            Uuid::new_v4().to_string(),
            household,
            session_hash.as_slice()
        ],
    )?;
    tx.execute(
        "UPDATE owner_recovery_links SET last_used_at = CURRENT_TIMESTAMP WHERE household_id = ?1",
        [household],
    )?;
    tx.commit()?;
    Ok(session)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{create_household_owner, initialize_database, owner_context};

    #[test]
    fn recovery_is_hashed_scoped_rotatable_and_independent_of_sessions() {
        let mut db = Connection::open_in_memory().unwrap();
        initialize_database(&db).unwrap();
        let first = create_household_owner(&mut db, "First family", "Owner").unwrap();
        let second = create_household_owner(&mut db, "Second family", "Owner").unwrap();
        assert!(create_owner_recovery(&db, "unknown-owner").is_err());
        let recovery = create_owner_recovery(&db, &first.owner_token).unwrap();
        let other_recovery = create_owner_recovery(&db, &second.owner_token).unwrap();
        let hash: Vec<u8> = db
            .query_row(
                "SELECT token_hash FROM owner_recovery_links WHERE household_id = ?1",
                [&first.household_id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(hash.len(), 32);
        assert_ne!(hash, recovery.token.as_bytes());
        assert!(owner_context(&db, &recovery.token).is_err());
        assert!(recover_owner(&mut db, "short").is_err());
        assert!(
            recover_owner(&mut db, "unknown-recovery-link-abcdefghijklmnopqrstuvwxyz").is_err()
        );
        initialize_database(&db).unwrap();
        let phone = recover_owner(&mut db, &recovery.token).unwrap();
        let tablet = recover_owner(&mut db, &recovery.token).unwrap();
        assert_ne!(phone, tablet);
        assert_ne!(phone, recovery.token);
        let context = owner_context(&db, &phone).unwrap();
        assert_eq!(context.household_name, "First family");
        assert!(context.recovery.unwrap().last_used_at.is_some());
        assert_eq!(
            owner_context(&db, &tablet).unwrap().household_name,
            "First family"
        );
        let replacement = create_owner_recovery(&db, &phone).unwrap();
        assert!(recover_owner(&mut db, &recovery.token).is_err());
        assert!(owner_context(&db, &phone).is_ok());
        assert!(owner_context(&db, &tablet).is_ok());
        assert!(owner_context(&db, &first.owner_token).is_ok());
        let restored = recover_owner(&mut db, &replacement.token).unwrap();
        assert_eq!(
            owner_context(&db, &restored).unwrap().household_name,
            "First family"
        );
        let other = recover_owner(&mut db, &other_recovery.token).unwrap();
        assert_eq!(
            owner_context(&db, &other).unwrap().household_name,
            "Second family"
        );
        db.execute("UPDATE owner_recovery_links SET expires_at = datetime('now', '-1 second') WHERE household_id = ?1", [&first.household_id]).unwrap();
        assert!(recover_owner(&mut db, &replacement.token).is_err());
        assert!(owner_context(&db, &restored).unwrap().recovery.is_none());
        // Recovery rotation does not alter an existing session's expiry.
        let expiring: bool = db.query_row("SELECT expires_at > CURRENT_TIMESTAMP FROM owner_access_grants WHERE token_hash = ?1",
            [hash_secret(&restored).as_slice()], |r| r.get(0)).unwrap();
        assert!(expiring);
    }
}
