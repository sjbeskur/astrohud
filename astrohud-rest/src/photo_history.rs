use crate::{Photo, Playback};
use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct PhotoSender {
    pub key: String,
    pub label: String,
}
#[derive(Debug, Serialize)]
pub struct SenderCategory {
    pub key: String,
    pub label: String,
    pub count: usize,
}
#[derive(Debug, Serialize)]
pub struct OwnerGalleryPage {
    pub frame_id: String,
    pub place_name: String,
    pub photos: Vec<Photo>,
    pub playback: Playback,
    pub senders: Vec<SenderCategory>,
    pub next_cursor: Option<i64>,
}
#[derive(Debug, Serialize)]
pub struct SenderHistoryPage {
    pub photos: Vec<Photo>,
    pub next_cursor: Option<i64>,
}

const COLUMNS: &str = "p.id,p.channel_id,p.storage_key,p.mime_type,p.created_at,p.location_label,
    COALESCE(pc.sender_key,'earlier'),COALESCE(i.label,pc.sender_label,'Earlier uploads'),p.rowid";
const SOURCE: &str = "FROM photos p JOIN channels c ON c.id=p.channel_id
    LEFT JOIN photo_contributors pc ON pc.photo_id=p.id
    LEFT JOIN sender_invitations i ON i.id=pc.invitation_id";
// Owner browsing obeys the same explicit-delivery / legacy-feed policy as
// the device manifest, including frame-local removals. Sender history below
// deliberately depends on authorship instead of current device visibility.
const VISIBLE: &str = "JOIN frames f ON f.id=?1
    WHERE f.household_id=?2 AND c.household_id=?2
    AND ((EXISTS (SELECT 1 FROM photo_delivery_plans d WHERE d.photo_id=p.id)
          AND EXISTS (SELECT 1 FROM photo_frame_deliveries d WHERE d.photo_id=p.id AND d.frame_id=f.id))
      OR (NOT EXISTS (SELECT 1 FROM photo_delivery_plans d WHERE d.photo_id=p.id)
          AND EXISTS (SELECT 1 FROM frame_subscriptions s WHERE s.frame_id=f.id AND s.channel_id=p.channel_id)))
    AND NOT EXISTS (SELECT 1 FROM frame_photo_removals r WHERE r.photo_id=p.id AND r.frame_id=f.id)";

fn photo_row(row: &rusqlite::Row<'_>, prefix: &str) -> rusqlite::Result<(Photo, i64)> {
    let id: String = row.get(0)?;
    Ok((
        Photo {
            url: format!("{prefix}/{id}"),
            id,
            channel_id: row.get(1)?,
            mime_type: row.get(3)?,
            created_at: row.get(4)?,
            location_label: row.get(5)?,
            sender: Some(PhotoSender {
                key: row.get(6)?,
                label: row.get(7)?,
            }),
        },
        row.get(8)?,
    ))
}
fn finish_page(mut rows: Vec<(Photo, i64)>) -> (Vec<Photo>, Option<i64>) {
    let more = rows.len() > 50;
    rows.truncate(50);
    let cursor = more.then(|| rows.last().unwrap().1);
    (rows.into_iter().map(|row| row.0).collect(), cursor)
}

pub(crate) fn owner_gallery_page(
    db: &Connection,
    household: &str,
    frame: &str,
    before: Option<i64>,
    sender: Option<&str>,
) -> rusqlite::Result<Option<OwnerGalleryPage>> {
    let name: Option<String> = db
        .query_row(
            "SELECT place_name FROM frames WHERE id=?1 AND household_id=?2",
            params![frame, household],
            |row| row.get(0),
        )
        .optional()?;
    let Some(place_name) = name else {
        return Ok(None);
    };
    let sql = format!(
        "SELECT {COLUMNS} {SOURCE} {VISIBLE}
        AND (?3 IS NULL OR p.rowid<?3) AND (?4 IS NULL OR COALESCE(pc.sender_key,'earlier')=?4)
        ORDER BY p.rowid DESC LIMIT 51"
    );
    let prefix = format!("/api/beta/owner/frames/{frame}/photos");
    let mut statement = db.prepare(&sql)?;
    let rows = statement
        .query_map(params![frame, household, before, sender], |row| {
            photo_row(row, &prefix)
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let (photos, next_cursor) = finish_page(rows);
    let sql = format!("SELECT COALESCE(pc.sender_key,'earlier'),COALESCE(i.label,pc.sender_label,'Earlier uploads'),COUNT(*)
        {SOURCE} {VISIBLE} GROUP BY COALESCE(pc.sender_key,'earlier') ORDER BY 2 COLLATE NOCASE,1");
    let mut statement = db.prepare(&sql)?;
    let senders = statement
        .query_map(params![frame, household], |row| {
            Ok(SenderCategory {
                key: row.get(0)?,
                label: row.get(1)?,
                count: row.get(2)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(Some(OwnerGalleryPage {
        frame_id: frame.to_owned(),
        place_name,
        photos,
        playback: crate::frame_playback(db, frame)?,
        senders,
        next_cursor,
    }))
}

pub(crate) fn sender_history_page(
    db: &Connection,
    invitation: &str,
    household: &str,
    channel: &str,
    before: Option<i64>,
) -> rusqlite::Result<SenderHistoryPage> {
    let sql = format!(
        "SELECT {COLUMNS} {SOURCE}
        WHERE pc.invitation_id=?1 AND c.household_id=?2 AND c.id=?3 AND (?4 IS NULL OR p.rowid<?4)
        ORDER BY p.rowid DESC LIMIT 51"
    );
    let mut statement = db.prepare(&sql)?;
    let rows = statement
        .query_map(params![invitation, household, channel, before], |row| {
            photo_row(row, "/api/beta/sender/photos")
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let (photos, next_cursor) = finish_page(rows);
    Ok(SenderHistoryPage {
        photos,
        next_cursor,
    })
}
pub(crate) fn sender_history_media(
    db: &Connection,
    invitation: &str,
    household: &str,
    channel: &str,
    photo: &str,
) -> rusqlite::Result<Option<String>> {
    db.query_row(
        "SELECT p.storage_key FROM photos p JOIN channels c ON c.id=p.channel_id
        JOIN photo_contributors pc ON pc.photo_id=p.id
        WHERE pc.invitation_id=?1 AND c.household_id=?2 AND c.id=?3 AND p.id=?4",
        params![invitation, household, channel, photo],
        |row| row.get(0),
    )
    .optional()
}
