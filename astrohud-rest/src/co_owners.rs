use crate::{
    BetaError, CreatedInvitation, bounded_text, hash_secret, owner_household_id, random_token,
};
use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;
use uuid::Uuid;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct CoOwnerInvitation {
    pub invitation_id: String,
    pub label: String,
    pub created_at: String,
    pub is_you: bool,
    pub sender_invitation_id: Option<String>,
}

pub fn co_owner_invitations(
    db: &Connection,
    token: &str,
) -> Result<Vec<CoOwnerInvitation>, BetaError> {
    let household = owner_household_id(db, token)?;
    let hash = hash_secret(token);
    Ok(db.prepare("SELECT g.id,g.label,g.created_at,g.token_hash=?2,p.sender_invitation_id FROM co_owner_invitations i
        JOIN owner_access_grants g ON g.id=i.grant_id LEFT JOIN sender_owner_promotions p ON p.grant_id=g.id
        WHERE g.household_id=?1 AND g.revoked_at IS NULL
        AND (g.expires_at IS NULL OR g.expires_at>CURRENT_TIMESTAMP) ORDER BY g.created_at,g.rowid")?
        .query_map(params![household, hash.as_slice()], |r|Ok(CoOwnerInvitation {
            invitation_id:r.get(0)?,label:r.get(1)?,created_at:r.get(2)?,is_you:r.get(3)?,sender_invitation_id:r.get(4)?
        }))?.collect::<rusqlite::Result<Vec<_>>>()?)
}

pub fn create_co_owner_invitation(
    db: &mut Connection,
    owner_token: &str,
    label: &str,
) -> Result<CreatedInvitation, BetaError> {
    let label = bounded_text(label, "co-owner name", 80)?;
    let tx = db.transaction()?;
    let household = owner_household_id(&tx, owner_token)?;
    let id = Uuid::new_v4().to_string();
    let token = random_token();
    let hash = hash_secret(&token);
    tx.execute(
        "INSERT INTO owner_access_grants(id,household_id,label,token_hash) VALUES (?1,?2,?3,?4)",
        params![id, household, label, hash.as_slice()],
    )?;
    tx.execute(
        "INSERT INTO co_owner_invitations(grant_id) VALUES (?1)",
        [&id],
    )?;
    tx.commit()?;
    Ok(CreatedInvitation {
        invitation_id: id,
        label: label.to_owned(),
        token,
    })
}

/// Issue a separate owner credential without elevating a possibly forwarded
/// sender link. The stable invitation identity and its photo history survive.
pub fn promote_sender_to_co_owner(
    db: &mut Connection,
    owner_token: &str,
    invitation_id: &str,
) -> Result<CreatedInvitation, BetaError> {
    let tx = db.transaction()?;
    let household = owner_household_id(&tx, owner_token)?;
    let label: String = tx.query_row("SELECT label FROM sender_invitations WHERE id=?1 AND household_id=?2 AND revoked_at IS NULL",
        params![invitation_id, household], |r|r.get(0)).optional()?.ok_or(BetaError::NotFound)?;
    let existing: Option<(String, bool)> = tx.query_row("SELECT g.id,g.revoked_at IS NULL AND (g.expires_at IS NULL OR g.expires_at>CURRENT_TIMESTAMP)
        FROM sender_owner_promotions p JOIN owner_access_grants g ON g.id=p.grant_id
        WHERE p.sender_invitation_id=?1 AND g.household_id=?2",params![invitation_id,household],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
    if existing.as_ref().is_some_and(|(_, active)| *active) {
        return Err(BetaError::InvalidInput(
            "this person already has co-owner access; replace their co-owner link instead",
        ));
    }
    let token = random_token();
    let hash = hash_secret(&token);
    let id = if let Some((id, _)) = existing {
        tx.execute("UPDATE owner_access_grants SET token_hash=?2,label=?3,revoked_at=NULL,expires_at=NULL WHERE id=?1",params![id,hash.as_slice(),label])?;
        id
    } else {
        let id = Uuid::new_v4().to_string();
        tx.execute("INSERT INTO owner_access_grants(id,household_id,label,token_hash) VALUES (?1,?2,?3,?4)",params![id,household,label,hash.as_slice()])?;
        tx.execute(
            "INSERT INTO co_owner_invitations(grant_id) VALUES (?1)",
            [&id],
        )?;
        tx.execute(
            "INSERT INTO sender_owner_promotions(sender_invitation_id,grant_id) VALUES (?1,?2)",
            params![invitation_id, id],
        )?;
        id
    };
    tx.commit()?;
    Ok(CreatedInvitation {
        invitation_id: id,
        label,
        token,
    })
}

fn authorize_co_owner_change(db: &Connection, token: &str, id: &str) -> Result<String, BetaError> {
    let household = owner_household_id(db, token)?;
    let hash = hash_secret(token);
    let found = db.query_row("SELECT g.label,g.token_hash=?3 FROM co_owner_invitations i JOIN owner_access_grants g ON g.id=i.grant_id
        WHERE g.id=?1 AND g.household_id=?2 AND g.revoked_at IS NULL AND (g.expires_at IS NULL OR g.expires_at>CURRENT_TIMESTAMP)",
        params![id,household,hash.as_slice()], |r|Ok((r.get::<_,String>(0)?,r.get::<_,bool>(1)?))).optional()?.ok_or(BetaError::NotFound)?;
    if found.1 {
        return Err(BetaError::InvalidInput(
            "ask another owner to change your own access",
        ));
    }
    Ok(found.0)
}

pub fn replace_co_owner_invitation(
    db: &mut Connection,
    token: &str,
    id: &str,
) -> Result<CreatedInvitation, BetaError> {
    let tx = db.transaction()?;
    let label = authorize_co_owner_change(&tx, token, id)?;
    let new_token = random_token();
    let hash = hash_secret(&new_token);
    tx.execute(
        "UPDATE owner_access_grants SET token_hash=?2 WHERE id=?1",
        params![id, hash.as_slice()],
    )?;
    tx.commit()?;
    Ok(CreatedInvitation {
        invitation_id: id.to_owned(),
        label,
        token: new_token,
    })
}

pub fn revoke_co_owner_invitation(
    db: &mut Connection,
    token: &str,
    id: &str,
) -> Result<(), BetaError> {
    let tx = db.transaction()?;
    authorize_co_owner_change(&tx, token, id)?;
    tx.execute(
        "UPDATE owner_access_grants SET revoked_at=CURRENT_TIMESTAMP WHERE id=?1",
        [id],
    )?;
    tx.commit()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        PlaybackRequest, create_household_owner, initialize_database, owner_context,
        update_playback,
    };
    #[test]
    fn co_owners_have_full_household_access_and_revocable_private_links() {
        let mut db = Connection::open_in_memory().unwrap();
        initialize_database(&db).unwrap();
        let root = create_household_owner(&mut db, "Home", "Original owner").unwrap();
        let other = create_household_owner(&mut db, "Other", "Other").unwrap();
        db.execute(
            "INSERT INTO frames(id,household_id,place_name) VALUES ('tv',?1,'TV')",
            [&root.household_id],
        )
        .unwrap();
        db.execute(
            "INSERT INTO frame_subscriptions VALUES ('tv',?1)",
            [&root.default_channel_id],
        )
        .unwrap();
        let co = create_co_owner_invitation(&mut db, &root.owner_token, " Alice ").unwrap();
        assert_eq!(co.label, "Alice");
        let context = owner_context(&db, &co.token).unwrap();
        assert_eq!(context.frames[0].frame_id, "tv");
        assert!(context.co_owners[0].is_you);
        assert_eq!(
            owner_household_id(&db, &co.token).unwrap(),
            root.household_id
        );
        let request: PlaybackRequest = serde_json::from_value(
            serde_json::json!({"slide_seconds":20,"guest_display_next":true}),
        )
        .unwrap();
        assert_eq!(
            update_playback(&mut db, &co.token, "tv", &request)
                .unwrap()
                .slide_seconds,
            20
        );
        let second = create_co_owner_invitation(&mut db, &co.token, "Bob").unwrap();
        assert_eq!(
            co_owner_invitations(&db, &root.owner_token).unwrap().len(),
            2
        );
        assert!(matches!(
            revoke_co_owner_invitation(&mut db, &other.owner_token, &co.invitation_id),
            Err(BetaError::NotFound)
        ));
        assert!(matches!(
            replace_co_owner_invitation(&mut db, &co.token, &co.invitation_id),
            Err(BetaError::InvalidInput(_))
        ));
        assert!(matches!(
            revoke_co_owner_invitation(&mut db, &co.token, &co.invitation_id),
            Err(BetaError::InvalidInput(_))
        ));
        let original_id: String = db
            .query_row(
                "SELECT id FROM owner_access_grants WHERE token_hash=?1",
                [hash_secret(&root.owner_token).as_slice()],
                |r| r.get(0),
            )
            .unwrap();
        assert!(matches!(
            revoke_co_owner_invitation(&mut db, &co.token, &original_id),
            Err(BetaError::NotFound)
        ));
        let replaced =
            replace_co_owner_invitation(&mut db, &root.owner_token, &co.invitation_id).unwrap();
        assert!(owner_household_id(&db, &co.token).is_err());
        assert!(owner_household_id(&db, &replaced.token).is_ok());
        let hash: Vec<u8> = db
            .query_row(
                "SELECT token_hash FROM owner_access_grants WHERE id=?1",
                [&co.invitation_id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(hash, hash_secret(&replaced.token));
        revoke_co_owner_invitation(&mut db, &root.owner_token, &co.invitation_id).unwrap();
        assert!(owner_household_id(&db, &replaced.token).is_err());
        assert!(owner_household_id(&db, &root.owner_token).is_ok());
        assert!(owner_household_id(&db, &second.token).is_ok());
        db.pragma_update(None, "user_version", 10).unwrap();
        initialize_database(&db).unwrap();
        assert!(owner_household_id(&db, &second.token).is_ok());
    }
}

#[cfg(test)]
mod promotion_tests {
    use super::*;
    use crate::{
        create_household_owner, create_sender_invitation, initialize_database, owner_context,
        sender_access, sender_history_page,
    };
    #[test]
    fn promoting_an_invitation_preserves_history_without_elevating_the_sender_secret() {
        let mut db = Connection::open_in_memory().unwrap();
        initialize_database(&db).unwrap();
        let root = create_household_owner(&mut db, "Home", "Owner").unwrap();
        let other = create_household_owner(&mut db, "Other", "Owner").unwrap();
        db.execute(
            "INSERT INTO frames(id,household_id,place_name) VALUES ('tv',?1,'TV')",
            [&root.household_id],
        )
        .unwrap();
        let sender = create_sender_invitation(&db, &root.owner_token, "Alice").unwrap();
        db.execute("INSERT INTO photos(id,channel_id,storage_key,mime_type) VALUES ('photo',?1,'photo.png','image/png')",[&root.default_channel_id]).unwrap();
        db.execute(
            "INSERT INTO photo_contributors VALUES ('photo',?1,?1,'Alice','invited')",
            [&sender.invitation_id],
        )
        .unwrap();
        assert!(matches!(
            promote_sender_to_co_owner(&mut db, &sender.token, &sender.invitation_id),
            Err(BetaError::Unauthorized)
        ));
        assert!(matches!(
            promote_sender_to_co_owner(&mut db, &other.owner_token, &sender.invitation_id),
            Err(BetaError::NotFound)
        ));
        let promoted =
            promote_sender_to_co_owner(&mut db, &root.owner_token, &sender.invitation_id).unwrap();
        assert_eq!(promoted.label, "Alice");
        assert_ne!(promoted.token, sender.token);
        assert_eq!(
            owner_household_id(&db, &promoted.token).unwrap(),
            root.household_id
        );
        assert!(owner_household_id(&db, &sender.token).is_err());
        let access = sender_access(&db, &sender.token).unwrap();
        assert_eq!(
            sender_history_page(
                &db,
                &access.invitation_id,
                &access.household_id,
                &access.channel_id,
                None
            )
            .unwrap()
            .photos
            .len(),
            1
        );
        let context = owner_context(&db, &promoted.token).unwrap();
        assert_eq!(context.invitations.len(), 1);
        assert_eq!(context.co_owners.len(), 1);
        assert_eq!(
            context.co_owners[0].sender_invitation_id.as_deref(),
            Some(sender.invitation_id.as_str())
        );
        assert!(context.co_owners[0].is_you);
        assert!(matches!(
            promote_sender_to_co_owner(&mut db, &root.owner_token, &sender.invitation_id),
            Err(BetaError::InvalidInput(_))
        ));
        assert_eq!(
            co_owner_invitations(&db, &root.owner_token).unwrap().len(),
            1
        );
        let replacement =
            replace_co_owner_invitation(&mut db, &root.owner_token, &promoted.invitation_id)
                .unwrap();
        assert!(owner_household_id(&db, &promoted.token).is_err());
        assert!(sender_access(&db, &sender.token).is_ok());
        revoke_co_owner_invitation(&mut db, &root.owner_token, &promoted.invitation_id).unwrap();
        assert!(owner_household_id(&db, &replacement.token).is_err());
        assert!(sender_access(&db, &sender.token).is_ok());
        let restored =
            promote_sender_to_co_owner(&mut db, &root.owner_token, &sender.invitation_id).unwrap();
        assert_eq!(restored.invitation_id, promoted.invitation_id);
        assert!(owner_household_id(&db, &restored.token).is_ok());
        assert_eq!(
            co_owner_invitations(&db, &root.owner_token).unwrap().len(),
            1
        );
        crate::revoke_sender_invitation(&db, &root.owner_token, &sender.invitation_id).unwrap();
        assert!(matches!(
            promote_sender_to_co_owner(&mut db, &root.owner_token, &sender.invitation_id),
            Err(BetaError::NotFound)
        ));
    }
    #[test]
    fn schema_eleven_upgrade_preserves_existing_sender_and_co_owner_access() {
        let dir = std::env::temp_dir().join(format!("memory-owner-promotion-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("state.sqlite3");
        let mut db = Connection::open(&path).unwrap();
        initialize_database(&db).unwrap();
        let root = create_household_owner(&mut db, "Home", "Owner").unwrap();
        let co = create_co_owner_invitation(&mut db, &root.owner_token, "Bob").unwrap();
        db.execute_batch("DROP TABLE sender_owner_promotions; PRAGMA user_version=11;")
            .unwrap();
        let backup = crate::initialize_database_with_backup(&db, &path)
            .unwrap()
            .unwrap();
        let old = Connection::open(backup).unwrap();
        assert_eq!(
            old.pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))
                .unwrap(),
            11
        );
        assert_eq!(
            db.pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))
                .unwrap(),
            12
        );
        let context = owner_context(&db, &co.token).unwrap();
        assert_eq!(context.co_owners.len(), 1);
        assert!(context.co_owners[0].sender_invitation_id.is_none());
        assert!(
            crate::initialize_database_with_backup(&db, &path)
                .unwrap()
                .is_none()
        );
        drop(old);
        drop(db);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
