//! Jinniu creator-authorize records (`jinniu_authorize_records`).
//!
//! Mirrors jieger
//! `electron/main/services/database/repositories/jinniuAuthorize.ts`
//! (`getByJinniuId` / `upsertBatch`). Independent table: a creator
//! authorization is a many-to-many relation under one jinniu sub-account,
//! not a single `business_accounts` row, so it is not folded into the
//! business-accounts scope table.
//!
//! `jinniu_id` is the jieger jinniu account id (business key). It is NOT a
//! Cloaksession browser `profile_id`; the browser session is resolved by the
//! caller via `profile_id` and only the business key is persisted here.

use crate::ProfileManager;
use multizen_core::{MultizenError, Result};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

/// One creator-authorize row as exchanged with the jinniu authorize page.
///
/// `serde` is camelCase so the same shape can travel over Tauri IPC without
/// a second mapping layer; storage uses snake_case columns.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JinniuAuthorizeItem {
    pub user_id: String,
    pub user_name: String,
    pub status: String,
    pub authorize_time: String,
}

/// Stored record with row id + sync timestamp.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JinniuAuthorizeRecord {
    pub id: i64,
    pub jinniu_id: String,
    pub user_id: String,
    pub user_name: String,
    pub status: String,
    pub authorize_time: String,
    pub synced_at: i64,
}

pub(crate) fn migrate(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS jinniu_authorize_records (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            jinniu_id TEXT NOT NULL,
            user_id TEXT NOT NULL,
            user_name TEXT NOT NULL DEFAULT '',
            status TEXT NOT NULL DEFAULT '',
            authorize_time TEXT NOT NULL DEFAULT '',
            synced_at INTEGER NOT NULL,
            UNIQUE (jinniu_id, user_id)
        );
        CREATE INDEX IF NOT EXISTS idx_jinniu_authorize_jinniu_id
            ON jinniu_authorize_records(jinniu_id);",
    )?;
    Ok(())
}

fn row(r: &rusqlite::Row<'_>) -> rusqlite::Result<JinniuAuthorizeRecord> {
    Ok(JinniuAuthorizeRecord {
        id: r.get(0)?,
        jinniu_id: r.get(1)?,
        user_id: r.get(2)?,
        user_name: r.get(3)?,
        status: r.get(4)?,
        authorize_time: r.get(5)?,
        synced_at: r.get(6)?,
    })
}

fn validate_jinniu_id(jinniu_id: &str) -> Result<()> {
    let trimmed = jinniu_id.trim();
    if trimmed.is_empty() || trimmed.chars().count() > 128 || trimmed.chars().any(char::is_control)
    {
        return Err(MultizenError::Config(
            "金牛账号ID不能为空，最多128个字符，且不能含控制字符".into(),
        ));
    }
    Ok(())
}

impl ProfileManager {
    /// All authorize records for one jinniu account, newest first
    /// (jieger `ORDER BY id DESC`).
    pub fn jinniu_authorize_get(&self, jinniu_id: &str) -> Result<Vec<JinniuAuthorizeRecord>> {
        validate_jinniu_id(jinniu_id)?;
        let mut stmt = self.conn.prepare(
            "SELECT id, jinniu_id, user_id, user_name, status, authorize_time, synced_at
             FROM jinniu_authorize_records WHERE jinniu_id = ? ORDER BY id DESC",
        )?;
        let rows = stmt.query_map([jinniu_id.trim()], row)?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Insert-or-update a batch of scraped authorize rows.
    /// Empty `user_id` rows are skipped (the page sometimes renders
    /// placeholder rows); an empty batch is a no-op success.
    pub fn jinniu_authorize_upsert_batch(
        &self,
        jinniu_id: &str,
        items: &[JinniuAuthorizeItem],
    ) -> Result<()> {
        validate_jinniu_id(jinniu_id)?;
        let jinniu_id = jinniu_id.trim();
        let tx = self.conn.unchecked_transaction()?;
        let now = chrono::Utc::now().timestamp_millis();
        {
            let mut stmt = tx.prepare(
                "INSERT INTO jinniu_authorize_records
                 (jinniu_id, user_id, user_name, status, authorize_time, synced_at)
                 VALUES (?, ?, ?, ?, ?, ?)
                 ON CONFLICT(jinniu_id, user_id) DO UPDATE SET
                   user_name = excluded.user_name,
                   status = excluded.status,
                   authorize_time = excluded.authorize_time,
                   synced_at = excluded.synced_at",
            )?;
            for item in items {
                let user_id = item.user_id.trim();
                if user_id.is_empty() {
                    continue;
                }
                stmt.execute(params![
                    jinniu_id,
                    user_id,
                    item.user_name,
                    item.status,
                    item.authorize_time,
                    now
                ])?;
            }
        }
        tx.commit()?;
        Ok(())
    }
}
