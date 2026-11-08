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
}

pub fn co_owner_invitations(
    db: &Connection,
    token: &str,
) -> Result<Vec<CoOwnerInvitation>, BetaError> {
    let household = owner_household_id(db, token)?;
    let hash = hash_secret(token);
    Ok(db.prepare("SELECT g.id,g.label,g.created_at,g.token_hash=?2 FROM co_owner_invitations i
        JOIN owner_access_grants g ON g.id=i.grant_id WHERE g.household_id=?1 AND g.revoked_at IS NULL
        AND (g.expires_at IS NULL OR g.expires_at>CURRENT_TIMESTAMP) ORDER BY g.created_at,g.rowid")?
        .query_map(params![household, hash.as_slice()], |r|Ok(CoOwnerInvitation {
            invitation_id:r.get(0)?,label:r.get(1)?,created_at:r.get(2)?,is_you:r.get(3)?
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
