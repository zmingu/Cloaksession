//! Durable log for live-room comment events (`live_events` table).
//!
//! This table is independent from business/identity archives: one row per
//! observed live event, keyed by the page-reported stable message id
//! (`msg_id`). Writers insert with `INSERT OR IGNORE` so re-delivered
//! observer payloads and observer/polling overlaps never duplicate rows.

use std::path::Path;

use multizen_core::{MultizenError, Result};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

/// One persisted live event. Column names match the `live_events` schema
/// (`msg_id` PK / `type` / `user_id` / `nickname` / `content` / `time` /
/// `account_id`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LiveEventRow {
    pub msg_id: String,
    /// Live event kind: `Comment` / `RoomEnter` / `RoomLike` / `RoomFollow` /
    /// `LiveOrder` / `LivePaid` (same vocabulary as the listener's
    /// `CommentEventType`, stored as its plain variant name).
    #[serde(rename = "type")]
    pub event_type: String,
    pub user_id: Option<String>,
    pub nickname: Option<String>,
    pub content: String,
    /// Wall-clock time (RFC 3339) when the event was observed.
    pub time: String,
    /// Listening account (profile id) that observed the event.
    pub account_id: String,
}

pub(crate) fn migrate(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS live_events (
            msg_id TEXT PRIMARY KEY,
            type TEXT NOT NULL,
            user_id TEXT,
            nickname TEXT,
            content TEXT NOT NULL DEFAULT '',
            time TEXT NOT NULL,
            account_id TEXT NOT NULL DEFAULT ''
        );
        CREATE INDEX IF NOT EXISTS idx_live_events_account_time
            ON live_events(account_id, time);",
    )?;
    Ok(())
}

/// Open a short-lived writer/reader connection to the profiles database.
///
/// Each call opens its own `Connection` (with a 5s busy timeout) instead of
/// sharing `ProfileManager`'s launcher-thread connection, so background
/// listener tasks can persist without crossing the `!Sync` boundary. The
/// table creation is idempotent, matching `migrate`.
fn open_db(db_path: &Path) -> Result<Connection> {
    if let Some(parent) = db_path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(MultizenError::Io)?;
        }
    }
    let conn = Connection::open(db_path)?;
    conn.busy_timeout(std::time::Duration::from_millis(5_000))?;
    migrate(&conn)?;
    Ok(conn)
}

/// Validate a row before writing: `msg_id` is the primary key and must be
/// non-empty; `event_type` must be one of the six known live kinds.
fn validate(row: &LiveEventRow) -> Result<()> {
    if row.msg_id.is_empty() {
        return Err(MultizenError::Config("live event msg_id is empty".into()));
    }
    match row.event_type.as_str() {
        "Comment" | "RoomEnter" | "RoomLike" | "RoomFollow" | "LiveOrder" | "LivePaid" => Ok(()),
        other => Err(MultizenError::Config(format!(
            "unknown live event type: {other}"
        ))),
    }
}

/// Insert one event; a repeated `msg_id` is ignored (re-delivery safe).
pub fn append_event(db_path: &Path, row: &LiveEventRow) -> Result<()> {
    validate(row)?;
    let conn = open_db(db_path)?;
    conn.execute(
        "INSERT OR IGNORE INTO live_events
            (msg_id, type, user_id, nickname, content, time, account_id)
         VALUES (?, ?, ?, ?, ?, ?, ?)",
        params![
            row.msg_id,
            row.event_type,
            row.user_id,
            row.nickname,
            row.content,
            row.time,
            row.account_id,
        ],
    )?;
    Ok(())
}

/// Latest events for one account, newest first, bounded by `limit`.
pub fn recent_events(
    db_path: &Path,
    account_id: &str,
    limit: u32,
) -> Result<Vec<LiveEventRow>> {
    let conn = open_db(db_path)?;
    let mut stmt = conn.prepare(
        "SELECT msg_id, type, user_id, nickname, content, time, account_id
         FROM live_events WHERE account_id = ?
         ORDER BY time DESC, rowid DESC LIMIT ?",
    )?;
    let rows = stmt
        .query_map(params![account_id, limit], |r| {
            Ok(LiveEventRow {
                msg_id: r.get(0)?,
                event_type: r.get(1)?,
                user_id: r.get(2)?,
                nickname: r.get(3)?,
                content: r.get(4)?,
                time: r.get(5)?,
                account_id: r.get(6)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

/// Total rows for one account (used by status/health checks).
pub fn count_events(db_path: &Path, account_id: &str) -> Result<u64> {
    let conn = open_db(db_path)?;
    Ok(conn.query_row(
        "SELECT COUNT(*) FROM live_events WHERE account_id = ?",
        params![account_id],
        |r| r.get(0),
    )?)
}
