use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::error::Error;
use std::fmt::{self, Display, Formatter};
use uuid::Uuid;

const HUMAN_ALPHABET: &[u8] = b"ABCDEFGHJKMNPQRSTUVWXYZ23456789";

#[derive(Debug)]
pub enum BetaError {
    InvalidInput(&'static str),
    NotFound,
    Unauthorized,
    ClaimUnavailable,
    Database(rusqlite::Error),
}

impl Display for BetaError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidInput(message) => formatter.write_str(message),
            Self::NotFound => formatter.write_str("record not found"),
            Self::Unauthorized => formatter.write_str("credential is not authorized"),
            Self::ClaimUnavailable => {
                formatter.write_str("claim code is invalid, expired, or already used")
            }
            Self::Database(error) => write!(formatter, "database error: {error}"),
        }
    }
}

impl Error for BetaError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Database(error) => Some(error),
            _ => None,
        }
    }
}

impl From<rusqlite::Error> for BetaError {
    fn from(error: rusqlite::Error) -> Self {
        Self::Database(error)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OwnerBootstrap {
    pub household_id: String,
    pub default_channel_id: String,
    pub owner_token: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct DeviceRegistration {
    pub enrollment_id: String,
    pub state: DeviceEnrollmentState,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum DeviceBootstrapState {
    Waiting,
    Ready {
        device_code: String,
        expires_at: String,
    },
    Claimed,
    Expired,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BootstrapClaim {
    pub owner_token: String,
    pub frame: ClaimedFrame,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum DeviceEnrollmentState {
    Pending {
        claim_code: String,
        expires_at: String,
    },
    Claimed {
        household_id: String,
        frame_id: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ClaimedFrame {
    pub household_id: String,
    pub frame_id: String,
    pub place_name: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct OwnerContext {
    pub household_name: String,
    pub frames: Vec<OwnerFrame>,
    pub invitations: Vec<OwnerInvitation>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct OwnerFrame {
    pub frame_id: String,
    pub place_name: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct OwnerInvitation {
    pub invitation_id: String,
    pub label: String,
    pub created_at: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct CreatedInvitation {
    pub invitation_id: String,
    pub label: String,
    pub token: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SenderContext {
    pub place_name: String,
    pub invitation_label: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SenderAccess {
    pub household_id: String,
    pub channel_id: String,
    pub context: SenderContext,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeviceAccess {
    pub household_id: String,
    pub frame_id: String,
}

/// Creates the operator-managed household used by the friendly beta and
/// returns the owner secret once. HTTP exposure belongs to a later slice and
/// must protect this operation with an operator credential.
pub fn create_household_owner(
    connection: &mut Connection,
    household_name: &str,
    owner_label: &str,
) -> Result<OwnerBootstrap, BetaError> {
    let household_name = bounded_text(household_name, "household name", 80)?;
    let owner_label = bounded_text(owner_label, "owner label", 80)?;
    let household_id = Uuid::new_v4().to_string();
    let default_channel_id = Uuid::new_v4().to_string();
    let grant_id = Uuid::new_v4().to_string();
    let owner_token = random_token();
    let owner_token_hash = hash_secret(&owner_token);

    let transaction = connection.transaction()?;
    transaction.execute(
        "INSERT INTO households (id, name) VALUES (?1, ?2)",
        params![household_id, household_name],
    )?;
    transaction.execute(
        "INSERT INTO channels (id, household_id, name) VALUES (?1, ?2, 'Family')",
        params![default_channel_id, household_id],
    )?;
    transaction.execute(
        "INSERT INTO owner_access_grants (id, household_id, label, token_hash)
         VALUES (?1, ?2, ?3, ?4)",
        params![
            grant_id,
            household_id,
            owner_label,
            owner_token_hash.as_slice()
        ],
    )?;
    transaction.commit()?;

    Ok(OwnerBootstrap {
        household_id,
        default_channel_id,
        owner_token,
    })
}

/// Registers a first-boot appliance as pending. Repeating the request with the
/// same device credential is idempotent; an expired pending code is rotated.
pub fn register_device(
    connection: &Connection,
    device_id: &str,
    device_code: &str,
    device_credential: &str,
) -> Result<DeviceRegistration, BetaError> {
    register_device_internal(connection, device_id, device_code, device_credential, None)
}

pub fn register_bootstrap_device(
    connection: &Connection,
    device_id: &str,
    device_code: &str,
    device_credential: &str,
    bootstrap_token: &str,
) -> Result<DeviceRegistration, BetaError> {
    validate_secret(bootstrap_token, "bootstrap token")?;
    register_device_internal(
        connection,
        device_id,
        device_code,
        device_credential,
        Some(bootstrap_token),
    )
}

fn register_device_internal(
    connection: &Connection,
    device_id: &str,
    device_code: &str,
    device_credential: &str,
    bootstrap_token: Option<&str>,
) -> Result<DeviceRegistration, BetaError> {
    let device_id = validate_device_id(device_id)?;
    let device_code = validate_device_code(device_code)?;
    validate_device_credential(device_credential)?;
    let credential_hash = hash_secret(device_credential);

    let existing = connection
        .query_row(
            "SELECT id, credential_hash, claim_code, status, household_id,
                    frame_id, expires_at, expires_at > CURRENT_TIMESTAMP
             FROM device_enrollments
             WHERE device_id = ?1",
            params![device_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Vec<u8>>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, Option<String>>(4)?,
                    row.get::<_, Option<String>>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, bool>(7)?,
                ))
            },
        )
        .optional()?;

    if let Some((
        enrollment_id,
        stored_hash,
        claim_code,
        status,
        household_id,
        frame_id,
        expires_at,
        unexpired,
    )) = existing
    {
        if stored_hash != credential_hash {
            return Err(BetaError::Unauthorized);
        }
        if status == "claimed" {
            return Ok(DeviceRegistration {
                enrollment_id,
                state: DeviceEnrollmentState::Claimed {
                    household_id: household_id.ok_or(BetaError::NotFound)?,
                    frame_id: frame_id.ok_or(BetaError::NotFound)?,
                },
            });
        }
        if unexpired {
            if let Some(token) = bootstrap_token {
                upsert_device_bootstrap(connection, &enrollment_id, token)?;
            }
            return Ok(DeviceRegistration {
                enrollment_id,
                state: DeviceEnrollmentState::Pending {
                    claim_code,
                    expires_at,
                },
            });
        }

        let claim_code = random_claim_code();
        connection.execute(
            "UPDATE device_enrollments
             SET device_code = ?1,
                 claim_code = ?2,
                 expires_at = datetime('now', '+15 minutes')
             WHERE id = ?3 AND status = 'pending'",
            params![device_code, claim_code, enrollment_id],
        )?;
        if let Some(token) = bootstrap_token {
            upsert_device_bootstrap(connection, &enrollment_id, token)?;
        }
        return enrollment_status(connection, &enrollment_id, device_credential);
    }

    let enrollment_id = Uuid::new_v4().to_string();
    let claim_code = random_claim_code();
    connection.execute(
        "INSERT INTO device_enrollments
             (id, device_id, device_code, credential_hash, claim_code, status, expires_at)
         VALUES (?1, ?2, ?3, ?4, ?5, 'pending', datetime('now', '+15 minutes'))",
        params![
            enrollment_id,
            device_id,
            device_code,
            credential_hash.as_slice(),
            claim_code
        ],
    )?;
    if let Some(token) = bootstrap_token {
        upsert_device_bootstrap(connection, &enrollment_id, token)?;
    }
    enrollment_status(connection, &enrollment_id, device_credential)
}

fn upsert_device_bootstrap(
    connection: &Connection,
    enrollment_id: &str,
    bootstrap_token: &str,
) -> Result<(), BetaError> {
    let token_hash = hash_secret(bootstrap_token);
    connection.execute(
        "INSERT INTO device_bootstraps (enrollment_id, token_hash, expires_at)
         VALUES (?1, ?2, datetime('now', '+15 minutes'))
         ON CONFLICT(enrollment_id) DO UPDATE SET
             token_hash = excluded.token_hash,
             expires_at = excluded.expires_at
         WHERE device_bootstraps.consumed_at IS NULL",
        params![enrollment_id, token_hash.as_slice()],
    )?;
    Ok(())
}

pub fn device_bootstrap_status(
    connection: &Connection,
    bootstrap_token: &str,
) -> Result<DeviceBootstrapState, BetaError> {
    validate_secret(bootstrap_token, "bootstrap token")?;
    let token_hash = hash_secret(bootstrap_token);
    let state = connection
        .query_row(
            "SELECT e.device_code, e.status, b.expires_at,
                    b.expires_at > CURRENT_TIMESTAMP,
                    e.expires_at > CURRENT_TIMESTAMP,
                    b.consumed_at IS NOT NULL
             FROM device_bootstraps b
             JOIN device_enrollments e ON e.id = b.enrollment_id
             WHERE b.token_hash = ?1",
            params![token_hash.as_slice()],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, bool>(3)?,
                    row.get::<_, bool>(4)?,
                    row.get::<_, bool>(5)?,
                ))
            },
        )
        .optional()?;

    let Some((device_code, enrollment_status, expires_at, bootstrap_valid, claim_valid, consumed)) =
        state
    else {
        return Ok(DeviceBootstrapState::Waiting);
    };
    if consumed || enrollment_status == "claimed" {
        return Ok(DeviceBootstrapState::Claimed);
    }
    if enrollment_status == "pending" && bootstrap_valid && claim_valid {
        return Ok(DeviceBootstrapState::Ready {
            device_code,
            expires_at,
        });
    }
    Ok(DeviceBootstrapState::Expired)
}

pub fn claim_bootstrap_device(
    connection: &mut Connection,
    bootstrap_token: &str,
    place_name: &str,
) -> Result<BootstrapClaim, BetaError> {
    validate_secret(bootstrap_token, "bootstrap token")?;
    let place_name = bounded_text(place_name, "place name", 80)?;
    let token_hash = hash_secret(bootstrap_token);
    let transaction = connection.transaction()?;
    let (enrollment_id, frame_id) = transaction
        .query_row(
            "SELECT e.id, e.device_id
             FROM device_bootstraps b
             JOIN device_enrollments e ON e.id = b.enrollment_id
             WHERE b.token_hash = ?1
               AND b.consumed_at IS NULL
               AND b.expires_at > CURRENT_TIMESTAMP
               AND e.status = 'pending'
               AND e.expires_at > CURRENT_TIMESTAMP",
            params![token_hash.as_slice()],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()?
        .ok_or(BetaError::ClaimUnavailable)?;

    let household_id = Uuid::new_v4().to_string();
    let channel_id = Uuid::new_v4().to_string();
    let grant_id = Uuid::new_v4().to_string();
    let owner_token = random_token();
    let owner_token_hash = hash_secret(&owner_token);
    transaction.execute(
        "INSERT INTO households (id, name) VALUES (?1, ?2)",
        params![household_id, place_name],
    )?;
    transaction.execute(
        "INSERT INTO channels (id, household_id, name) VALUES (?1, ?2, 'Family')",
        params![channel_id, household_id],
    )?;
    transaction.execute(
        "INSERT INTO owner_access_grants (id, household_id, label, token_hash)
         VALUES (?1, ?2, 'Owner', ?3)",
        params![grant_id, household_id, owner_token_hash.as_slice()],
    )?;
    transaction.execute(
        "INSERT INTO frames (id, household_id, place_name) VALUES (?1, ?2, ?3)",
        params![frame_id, household_id, place_name],
    )?;
    transaction.execute(
        "INSERT INTO frame_subscriptions (frame_id, channel_id) VALUES (?1, ?2)",
        params![frame_id, channel_id],
    )?;
    let claimed = transaction.execute(
        "UPDATE device_enrollments
         SET status = 'claimed', household_id = ?1, frame_id = ?2,
             claimed_at = CURRENT_TIMESTAMP
         WHERE id = ?3 AND status = 'pending'",
        params![household_id, frame_id, enrollment_id],
    )?;
    let consumed = transaction.execute(
        "UPDATE device_bootstraps
         SET consumed_at = CURRENT_TIMESTAMP
         WHERE enrollment_id = ?1 AND consumed_at IS NULL",
        params![enrollment_id],
    )?;
    if claimed != 1 || consumed != 1 {
        return Err(BetaError::ClaimUnavailable);
    }
    transaction.commit()?;

    Ok(BootstrapClaim {
        owner_token,
        frame: ClaimedFrame {
            household_id,
            frame_id,
            place_name: place_name.to_owned(),
        },
    })
}

pub fn enrollment_status(
    connection: &Connection,
    enrollment_id: &str,
    device_credential: &str,
) -> Result<DeviceRegistration, BetaError> {
    validate_device_credential(device_credential)?;
    let credential_hash = hash_secret(device_credential);
    let enrollment = connection
        .query_row(
            "SELECT credential_hash, claim_code, status, household_id, frame_id,
                    expires_at, expires_at > CURRENT_TIMESTAMP
             FROM device_enrollments
             WHERE id = ?1",
            params![enrollment_id],
            |row| {
                Ok((
                    row.get::<_, Vec<u8>>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, Option<String>>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, bool>(6)?,
                ))
            },
        )
        .optional()?
        .ok_or(BetaError::NotFound)?;
    if enrollment.0 != credential_hash {
        return Err(BetaError::Unauthorized);
    }

    let state = match enrollment.2.as_str() {
        "claimed" => DeviceEnrollmentState::Claimed {
            household_id: enrollment.3.ok_or(BetaError::NotFound)?,
            frame_id: enrollment.4.ok_or(BetaError::NotFound)?,
        },
        "pending" if enrollment.6 => DeviceEnrollmentState::Pending {
            claim_code: enrollment.1,
            expires_at: enrollment.5,
        },
        _ => return Err(BetaError::ClaimUnavailable),
    };

    Ok(DeviceRegistration {
        enrollment_id: enrollment_id.to_owned(),
        state,
    })
}

pub fn claimed_device_access(
    connection: &Connection,
    device_credential: &str,
) -> Result<DeviceAccess, BetaError> {
    validate_device_credential(device_credential)?;
    let credential_hash = hash_secret(device_credential);
    connection
        .query_row(
            "SELECT household_id, frame_id
             FROM device_enrollments
             WHERE credential_hash = ?1 AND status = 'claimed'
             LIMIT 1",
            params![credential_hash.as_slice()],
            |row| {
                Ok(DeviceAccess {
                    household_id: row.get(0)?,
                    frame_id: row.get(1)?,
                })
            },
        )
        .optional()?
        .ok_or(BetaError::Unauthorized)
}

pub fn claim_device(
    connection: &mut Connection,
    owner_token: &str,
    claim_code: &str,
    place_name: &str,
) -> Result<ClaimedFrame, BetaError> {
    let place_name = bounded_text(place_name, "place name", 80)?;
    let claim_code = claim_code.trim().to_ascii_uppercase();
    if claim_code.len() != 8
        || !claim_code
            .bytes()
            .all(|byte| HUMAN_ALPHABET.contains(&byte))
    {
        return Err(BetaError::ClaimUnavailable);
    }
    let owner_token_hash = hash_secret(owner_token);
    let transaction = connection.transaction()?;
    let household_id = transaction
        .query_row(
            "SELECT household_id
             FROM owner_access_grants
             WHERE token_hash = ?1
               AND revoked_at IS NULL
               AND (expires_at IS NULL OR expires_at > CURRENT_TIMESTAMP)",
            params![owner_token_hash.as_slice()],
            |row| row.get::<_, String>(0),
        )
        .optional()?
        .ok_or(BetaError::Unauthorized)?;
    let (enrollment_id, frame_id) = transaction
        .query_row(
            "SELECT id, device_id
             FROM device_enrollments
             WHERE claim_code = ?1
               AND status = 'pending'
               AND expires_at > CURRENT_TIMESTAMP",
            params![claim_code],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()?
        .ok_or(BetaError::ClaimUnavailable)?;

    transaction.execute(
        "INSERT INTO frames (id, household_id, place_name) VALUES (?1, ?2, ?3)",
        params![frame_id, household_id, place_name],
    )?;
    let default_channel_id = transaction.query_row(
        "SELECT id FROM channels WHERE household_id = ?1 ORDER BY created_at, rowid LIMIT 1",
        params![household_id],
        |row| row.get::<_, String>(0),
    )?;
    transaction.execute(
        "INSERT INTO frame_subscriptions (frame_id, channel_id) VALUES (?1, ?2)",
        params![frame_id, default_channel_id],
    )?;
    let updated = transaction.execute(
        "UPDATE device_enrollments
         SET status = 'claimed', household_id = ?1, frame_id = ?2,
             claimed_at = CURRENT_TIMESTAMP
         WHERE id = ?3 AND status = 'pending'",
        params![household_id, frame_id, enrollment_id],
    )?;
    if updated != 1 {
        return Err(BetaError::ClaimUnavailable);
    }
    transaction.commit()?;

    Ok(ClaimedFrame {
        household_id,
        frame_id,
        place_name: place_name.to_owned(),
    })
}

pub fn owner_context(
    connection: &Connection,
    owner_token: &str,
) -> Result<OwnerContext, BetaError> {
    let owner_token_hash = hash_secret(owner_token);
    let household = connection
        .query_row(
            "SELECT h.id, h.name
             FROM owner_access_grants g
             JOIN households h ON h.id = g.household_id
             WHERE g.token_hash = ?1
               AND g.revoked_at IS NULL
               AND (g.expires_at IS NULL OR g.expires_at > CURRENT_TIMESTAMP)",
            params![owner_token_hash.as_slice()],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()?
        .ok_or(BetaError::Unauthorized)?;
    let mut frame_statement = connection.prepare(
        "SELECT id, place_name
         FROM frames
         WHERE household_id = ?1
         ORDER BY created_at, rowid",
    )?;
    let frames = frame_statement
        .query_map(params![&household.0], |row| {
            Ok(OwnerFrame {
                frame_id: row.get(0)?,
                place_name: row.get(1)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut invitation_statement = connection.prepare(
        "SELECT id, label, created_at
         FROM sender_invitations
         WHERE household_id = ?1 AND revoked_at IS NULL
         ORDER BY created_at DESC, rowid DESC",
    )?;
    let invitations = invitation_statement
        .query_map(params![&household.0], |row| {
            Ok(OwnerInvitation {
                invitation_id: row.get(0)?,
                label: row.get(1)?,
                created_at: row.get(2)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    Ok(OwnerContext {
        household_name: household.1,
        frames,
        invitations,
    })
}

pub fn create_sender_invitation(
    connection: &Connection,
    owner_token: &str,
    label: &str,
) -> Result<CreatedInvitation, BetaError> {
    let label = bounded_text(label, "sender name", 80)?;
    let household_id = owner_household_id(connection, owner_token)?;
    let channel_id = connection
        .query_row(
            "SELECT c.id
             FROM channels c
             WHERE c.household_id = ?1
               AND EXISTS (
                   SELECT 1 FROM frames f WHERE f.household_id = c.household_id
               )
             ORDER BY c.created_at, c.rowid
             LIMIT 1",
            params![household_id],
            |row| row.get::<_, String>(0),
        )
        .optional()?
        .ok_or(BetaError::InvalidInput(
            "claim a frame before inviting senders",
        ))?;
    let invitation_id = Uuid::new_v4().to_string();
    let token = random_token();
    let token_hash = hash_secret(&token);
    connection.execute(
        "INSERT INTO sender_invitations
             (id, household_id, channel_id, label, token_hash)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            invitation_id,
            household_id,
            channel_id,
            label,
            token_hash.as_slice()
        ],
    )?;

    Ok(CreatedInvitation {
        invitation_id,
        label: label.to_owned(),
        token,
    })
}

pub fn revoke_sender_invitation(
    connection: &Connection,
    owner_token: &str,
    invitation_id: &str,
) -> Result<(), BetaError> {
    let household_id = owner_household_id(connection, owner_token)?;
    let changed = connection.execute(
        "UPDATE sender_invitations
         SET revoked_at = CURRENT_TIMESTAMP
         WHERE id = ?1 AND household_id = ?2 AND revoked_at IS NULL",
        params![invitation_id, household_id],
    )?;
    if changed != 1 {
        return Err(BetaError::NotFound);
    }
    Ok(())
}

pub fn sender_access(
    connection: &Connection,
    sender_token: &str,
) -> Result<SenderAccess, BetaError> {
    let sender_token_hash = hash_secret(sender_token);
    connection
        .query_row(
            "SELECT i.household_id, i.channel_id, f.place_name, i.label
             FROM sender_invitations i
             JOIN frames f ON f.household_id = i.household_id
             WHERE i.token_hash = ?1
               AND i.revoked_at IS NULL
             ORDER BY f.created_at, f.rowid
             LIMIT 1",
            params![sender_token_hash.as_slice()],
            |row| {
                Ok(SenderAccess {
                    household_id: row.get(0)?,
                    channel_id: row.get(1)?,
                    context: SenderContext {
                        place_name: row.get(2)?,
                        invitation_label: row.get(3)?,
                    },
                })
            },
        )
        .optional()?
        .ok_or(BetaError::Unauthorized)
}

fn owner_household_id(connection: &Connection, owner_token: &str) -> Result<String, BetaError> {
    let owner_token_hash = hash_secret(owner_token);
    connection
        .query_row(
            "SELECT household_id
             FROM owner_access_grants
             WHERE token_hash = ?1
               AND revoked_at IS NULL
               AND (expires_at IS NULL OR expires_at > CURRENT_TIMESTAMP)",
            params![owner_token_hash.as_slice()],
            |row| row.get(0),
        )
        .optional()?
        .ok_or(BetaError::Unauthorized)
}

fn bounded_text<'a>(
    value: &'a str,
    field_name: &'static str,
    max_characters: usize,
) -> Result<&'a str, BetaError> {
    let value = value.trim();
    if value.is_empty() || value.chars().count() > max_characters {
        return Err(BetaError::InvalidInput(field_name));
    }
    Ok(value)
}

fn validate_device_id(device_id: &str) -> Result<&str, BetaError> {
    Uuid::parse_str(device_id).map_err(|_| BetaError::InvalidInput("device ID"))?;
    Ok(device_id)
}

fn validate_device_code(device_code: &str) -> Result<&str, BetaError> {
    let valid = device_code.len() == 6
        && device_code
            .bytes()
            .all(|byte| HUMAN_ALPHABET.contains(&byte));
    if !valid {
        return Err(BetaError::InvalidInput("device code"));
    }
    Ok(device_code)
}

fn validate_device_credential(device_credential: &str) -> Result<(), BetaError> {
    validate_secret(device_credential, "device credential")
}

fn validate_secret(value: &str, name: &'static str) -> Result<(), BetaError> {
    let valid =
        (32..=512).contains(&value.len()) && value.bytes().all(|byte| !byte.is_ascii_whitespace());
    valid.then_some(()).ok_or(BetaError::InvalidInput(name))
}

fn hash_secret(secret: &str) -> [u8; 32] {
    Sha256::digest(secret.as_bytes()).into()
}

fn random_token() -> String {
    format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple())
}

fn random_claim_code() -> String {
    Uuid::new_v4()
        .as_bytes()
        .iter()
        .take(8)
        .map(|byte| HUMAN_ALPHABET[usize::from(*byte) % HUMAN_ALPHABET.len()] as char)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::initialize_database;

    fn device_credential(index: usize) -> String {
        format!("device-{index}-credential-abcdefghijklmnopqrstuvwxyz")
    }

    fn bootstrap_token(index: usize) -> String {
        format!("bootstrap-{index}-token-abcdefghijklmnopqrstuvwxyz0123456789")
    }

    #[test]
    fn bootstrap_claim_creates_owner_place_and_claimed_device() {
        let mut database = Connection::open_in_memory().expect("open database");
        initialize_database(&database).expect("initialize database");
        let device_id = Uuid::new_v4().to_string();
        let credential = device_credential(1);
        let token = bootstrap_token(1);

        let registration =
            register_bootstrap_device(&database, &device_id, "ABC2D3", &credential, &token)
                .expect("register bootstrap device");
        assert!(matches!(
            device_bootstrap_status(&database, &token).expect("read bootstrap status"),
            DeviceBootstrapState::Ready { device_code, .. } if device_code == "ABC2D3"
        ));

        let claim = claim_bootstrap_device(&mut database, &token, "Mom's living room")
            .expect("claim bootstrap device");
        assert_eq!(claim.frame.frame_id, device_id);
        assert_eq!(claim.frame.place_name, "Mom's living room");
        let context = owner_context(&database, &claim.owner_token).expect("read owner context");
        assert_eq!(context.household_name, "Mom's living room");
        assert_eq!(context.frames.len(), 1);
        assert_eq!(context.frames[0].frame_id, device_id);
        assert_eq!(context.frames[0].place_name, "Mom's living room");
        assert!(context.invitations.is_empty());
        assert!(matches!(
            enrollment_status(&database, &registration.enrollment_id, &credential)
                .expect("read enrollment")
                .state,
            DeviceEnrollmentState::Claimed { frame_id, .. } if frame_id == device_id
        ));
        assert_eq!(
            claimed_device_access(&database, &credential)
                .expect("authenticate claimed device")
                .frame_id,
            device_id
        );
        assert_eq!(
            device_bootstrap_status(&database, &token).expect("read consumed status"),
            DeviceBootstrapState::Claimed
        );
    }

    #[test]
    fn bootstrap_token_is_hashed_and_single_use() {
        let mut database = Connection::open_in_memory().expect("open database");
        initialize_database(&database).expect("initialize database");
        let token = bootstrap_token(2);
        register_bootstrap_device(
            &database,
            &Uuid::new_v4().to_string(),
            "ABC2D4",
            &device_credential(2),
            &token,
        )
        .expect("register bootstrap device");

        let stored: Vec<u8> = database
            .query_row("SELECT token_hash FROM device_bootstraps", [], |row| {
                row.get(0)
            })
            .expect("read bootstrap hash");
        assert_ne!(stored, token.as_bytes());
        assert_eq!(stored, hash_secret(&token));

        claim_bootstrap_device(&mut database, &token, "Kitchen").expect("first claim");
        assert!(matches!(
            claim_bootstrap_device(&mut database, &token, "Second place"),
            Err(BetaError::ClaimUnavailable)
        ));
        let household_count: i64 = database
            .query_row(
                "SELECT COUNT(*) FROM households WHERE id != 'demo-household'",
                [],
                |row| row.get(0),
            )
            .expect("count households");
        assert_eq!(household_count, 1);
    }

    #[test]
    fn unknown_and_expired_bootstrap_tokens_cannot_claim() {
        let mut database = Connection::open_in_memory().expect("open database");
        initialize_database(&database).expect("initialize database");
        let token = bootstrap_token(3);
        register_bootstrap_device(
            &database,
            &Uuid::new_v4().to_string(),
            "ABC2D5",
            &device_credential(3),
            &token,
        )
        .expect("register bootstrap device");

        let unknown = bootstrap_token(4);
        assert_eq!(
            device_bootstrap_status(&database, &unknown).expect("read unknown status"),
            DeviceBootstrapState::Waiting
        );
        assert!(matches!(
            claim_bootstrap_device(&mut database, &unknown, "Unknown place"),
            Err(BetaError::ClaimUnavailable)
        ));

        database
            .execute(
                "UPDATE device_bootstraps SET expires_at = datetime('now', '-1 minute')",
                [],
            )
            .expect("expire bootstrap");
        assert_eq!(
            device_bootstrap_status(&database, &token).expect("read expired status"),
            DeviceBootstrapState::Expired
        );
        assert!(matches!(
            claim_bootstrap_device(&mut database, &token, "Expired place"),
            Err(BetaError::ClaimUnavailable)
        ));
    }

    #[test]
    fn three_households_claim_distinct_devices() {
        let mut database = Connection::open_in_memory().expect("open database");
        initialize_database(&database).expect("initialize database");
        let mut owners = Vec::new();
        let mut registrations = Vec::new();

        for index in 1..=3 {
            owners.push(
                create_household_owner(
                    &mut database,
                    &format!("Household {index}"),
                    "Primary owner",
                )
                .expect("create household owner"),
            );
            registrations.push(
                register_device(
                    &database,
                    &Uuid::new_v4().to_string(),
                    &format!("ABC2D{}", index + 1),
                    &device_credential(index),
                )
                .expect("register device"),
            );
        }

        for index in 0..3 {
            let DeviceEnrollmentState::Pending { claim_code, .. } = &registrations[index].state
            else {
                panic!("new device should be pending");
            };
            let claimed = claim_device(
                &mut database,
                &owners[index].owner_token,
                claim_code,
                &format!("Tester {} living room", index + 1),
            )
            .expect("claim device");
            assert_eq!(claimed.household_id, owners[index].household_id);

            let status = enrollment_status(
                &database,
                &registrations[index].enrollment_id,
                &device_credential(index + 1),
            )
            .expect("read claimed status");
            assert_eq!(
                status.state,
                DeviceEnrollmentState::Claimed {
                    household_id: owners[index].household_id.clone(),
                    frame_id: claimed.frame_id,
                }
            );
        }

        let household_count: i64 = database
            .query_row(
                "SELECT COUNT(*) FROM households WHERE id != 'demo-household'",
                [],
                |row| row.get(0),
            )
            .expect("count households");
        assert_eq!(household_count, 3);
        let subscription_count: i64 = database
            .query_row(
                "SELECT COUNT(*)
                 FROM frame_subscriptions s
                 JOIN frames f ON f.id = s.frame_id
                 JOIN channels c ON c.id = s.channel_id
                 WHERE f.household_id = c.household_id
                   AND f.id != 'demo-frame'",
                [],
                |row| row.get(0),
            )
            .expect("count isolated subscriptions");
        assert_eq!(subscription_count, 3);
    }

    #[test]
    fn registration_is_idempotent_and_credentials_are_private() {
        let database = Connection::open_in_memory().expect("open database");
        initialize_database(&database).expect("initialize database");
        let device_id = Uuid::new_v4().to_string();
        let credential = device_credential(1);
        let first =
            register_device(&database, &device_id, "ABC2D3", &credential).expect("register device");
        let second = register_device(&database, &device_id, "ABC2D3", &credential)
            .expect("repeat registration");
        assert_eq!(first, second);

        let wrong_credential = enrollment_status(
            &database,
            &first.enrollment_id,
            "wrong-device-credential-abcdefghijklmnopqrstuvwxyz",
        );
        assert!(matches!(wrong_credential, Err(BetaError::Unauthorized)));

        let stored: Vec<u8> = database
            .query_row(
                "SELECT credential_hash FROM device_enrollments WHERE id = ?1",
                params![first.enrollment_id],
                |row| row.get(0),
            )
            .expect("read stored credential hash");
        assert_ne!(stored, credential.as_bytes());
        assert_eq!(stored.len(), 32);
    }

    #[test]
    fn a_claim_code_is_single_use() {
        let mut database = Connection::open_in_memory().expect("open database");
        initialize_database(&database).expect("initialize database");
        let first_owner =
            create_household_owner(&mut database, "First", "Owner").expect("create first owner");
        let second_owner =
            create_household_owner(&mut database, "Second", "Owner").expect("create second owner");
        let device_id = Uuid::new_v4().to_string();
        let registration = register_device(&database, &device_id, "ABC2D3", &device_credential(1))
            .expect("register device");
        let DeviceEnrollmentState::Pending { claim_code, .. } = registration.state else {
            panic!("device should be pending");
        };

        claim_device(
            &mut database,
            &first_owner.owner_token,
            &claim_code,
            "First living room",
        )
        .expect("claim device once");
        let second_claim = claim_device(
            &mut database,
            &second_owner.owner_token,
            &claim_code,
            "Second living room",
        );
        assert!(matches!(second_claim, Err(BetaError::ClaimUnavailable)));
    }

    #[test]
    fn sender_links_are_household_scoped_hashed_and_revocable() {
        let mut database = Connection::open_in_memory().expect("open database");
        initialize_database(&database).expect("initialize database");
        let owner = create_household_owner(&mut database, "First", "Owner").expect("create owner");
        let other_owner =
            create_household_owner(&mut database, "Second", "Owner").expect("create other owner");
        let device_id = Uuid::new_v4().to_string();
        let registration = register_device(&database, &device_id, "ABC2D3", &device_credential(1))
            .expect("register device");
        let DeviceEnrollmentState::Pending { claim_code, .. } = registration.state else {
            panic!("device should be pending");
        };
        claim_device(
            &mut database,
            &owner.owner_token,
            &claim_code,
            "Mom's living room",
        )
        .expect("claim device");
        let device_access = claimed_device_access(&database, &device_credential(1))
            .expect("authenticate claimed device");
        assert_eq!(device_access.household_id, owner.household_id);
        assert_eq!(device_access.frame_id, device_id);
        assert!(matches!(
            claimed_device_access(
                &database,
                "wrong-device-credential-abcdefghijklmnopqrstuvwxyz"
            ),
            Err(BetaError::Unauthorized)
        ));

        let invitation = create_sender_invitation(&database, &owner.owner_token, "Alice")
            .expect("create invitation");
        let access = sender_access(&database, &invitation.token).expect("use invitation");
        assert_eq!(access.household_id, owner.household_id);
        assert_eq!(access.channel_id, owner.default_channel_id);
        assert_eq!(access.context.place_name, "Mom's living room");
        assert_eq!(access.context.invitation_label, "Alice");

        let stored_hash: Vec<u8> = database
            .query_row(
                "SELECT token_hash FROM sender_invitations WHERE id = ?1",
                params![invitation.invitation_id],
                |row| row.get(0),
            )
            .expect("read invitation hash");
        assert_ne!(stored_hash, invitation.token.as_bytes());
        assert_eq!(stored_hash.len(), 32);

        let cross_household_revoke = revoke_sender_invitation(
            &database,
            &other_owner.owner_token,
            &invitation.invitation_id,
        );
        assert!(matches!(cross_household_revoke, Err(BetaError::NotFound)));
        revoke_sender_invitation(&database, &owner.owner_token, &invitation.invitation_id)
            .expect("revoke invitation");
        assert!(matches!(
            sender_access(&database, &invitation.token),
            Err(BetaError::Unauthorized)
        ));
    }
}
