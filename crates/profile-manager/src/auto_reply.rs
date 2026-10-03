//! jieger auto-reply reply records (`auto_reply_records`).
use crate::ProfileManager;
use multizen_core::*;
use rusqlite::{params, Connection};

pub(crate) fn migrate(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS auto_reply_records (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            account_id TEXT NOT NULL,
            content TEXT NOT NULL,
            reply TEXT NOT NULL DEFAULT '',
            source TEXT NOT NULL,
            goods_id TEXT,
            created_at TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_auto_reply_account
            ON auto_reply_records(account_id, id);",
    )?;
    Ok(())
}

/// One persisted auto-reply resolution. `source` is one of
/// `knowledge`/`ai`/`template`; `reply` may be empty for unanswered misses.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AutoReplyRecord {
    pub id: i64,
    pub account_id: String,
    pub content: String,
    pub reply: String,
    pub source: String,
    pub goods_id: Option<String>,
    pub created_at: String,
}

fn invalid(message: &str) -> MultizenError {
    MultizenError::Config(message.into())
}

fn now() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

impl ProfileManager {
    /// Persist one auto-reply resolution (record_reply 落库).
    pub fn auto_reply_record(
        &self,
        account_id: &str,
        content: &str,
        reply: &str,
        source: &str,
        goods_id: Option<&str>,
    ) -> Result<AutoReplyRecord> {
        let account_id = account_id.trim();
        let content = content.trim();
        if account_id.is_empty() || content.is_empty() {
            return Err(invalid("账号或评论内容不能为空"));
        }
        if account_id.chars().count() > 128
            || content.chars().count() > 2000
            || reply.chars().count() > 2000
            || source.chars().count() > 32
        {
            return Err(invalid("自动回复记录字段长度超出范围"));
        }
        let created_at = now();
        self.conn.execute(
            "INSERT INTO auto_reply_records(account_id,content,reply,source,goods_id,created_at)
             VALUES (?,?,?,?,?,?)",
            params![account_id, content, reply, source, goods_id, created_at],
        )?;
        Ok(AutoReplyRecord {
            id: self.conn.last_insert_rowid(),
            account_id: account_id.into(),
            content: content.into(),
            reply: reply.into(),
            source: source.into(),
            goods_id: goods_id.map(str::to_string),
            created_at,
        })
    }

    /// Recent records, newest first. Empty `account_id` lists all accounts.
    /// `limit` is clamped to 1..=200.
    pub fn auto_reply_list(&self, account_id: &str, limit: u64) -> Result<Vec<AutoReplyRecord>> {
        let limit = limit.clamp(1, 200) as i64;
        let account_id = account_id.trim();
        let sql = if account_id.is_empty() {
            "SELECT id,account_id,content,reply,source,goods_id,created_at
             FROM auto_reply_records ORDER BY id DESC LIMIT ?"
        } else {
            "SELECT id,account_id,content,reply,source,goods_id,created_at
             FROM auto_reply_records WHERE account_id=? ORDER BY id DESC LIMIT ?"
        };
        let mut stmt = self.conn.prepare(sql)?;
        let rows = if account_id.is_empty() {
            stmt.query_map([limit], map_row)?
        } else {
            stmt.query_map(params![account_id, limit], map_row)?
        };
        rows.collect::<rusqlite::Result<Vec<_>>>().map_err(Into::into)
    }
}

fn map_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<AutoReplyRecord> {
    Ok(AutoReplyRecord {
        id: r.get(0)?,
        account_id: r.get(1)?,
        content: r.get(2)?,
        reply: r.get(3)?,
        source: r.get(4)?,
        goods_id: r.get(5)?,
        created_at: r.get(6)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn open_manager() -> (tempfile::TempDir, ProfileManager) {
        let dir = tempfile::tempdir().unwrap();
        let pm = ProfileManager::new(&dir.path().join("profiles.db"), &dir.path().join("profiles"))
            .unwrap();
        (dir, pm)
    }

    #[test]
    fn migration_creates_table_and_is_idempotent() {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        migrate(&conn).unwrap();
        let count: i64 = conn
            .query_row(
                "SELECT count(*) FROM sqlite_master
                 WHERE type='table' AND name='auto_reply_records'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn record_and_list_roundtrip() {
        let (_dir, pm) = open_manager();
        let first = pm
            .auto_reply_record("a1", "多少钱", "价格：99元", "knowledge", Some("g1"))
            .unwrap();
        assert!(first.id > 0);
        assert!(!first.created_at.is_empty());
        pm.auto_reply_record("a1", "主播好", "", "template", None)
            .unwrap();
        pm.auto_reply_record("a2", "怎么用", "冷榨直饮", "knowledge", Some("g1"))
            .unwrap();

        let mine = pm.auto_reply_list("a1", 10).unwrap();
        assert_eq!(mine.len(), 2);
        assert!(mine[0].id > mine[1].id); // newest first
        assert_eq!(mine[0].content, "主播好");
        assert_eq!(mine[1].goods_id.as_deref(), Some("g1"));

        let all = pm.auto_reply_list("", 10).unwrap();
        assert_eq!(all.len(), 3);

        let one = pm.auto_reply_list("a1", 1).unwrap();
        assert_eq!(one.len(), 1);
    }

    #[test]
    fn record_rejects_empty_or_oversized_fields() {
        let (_dir, pm) = open_manager();
        assert!(pm.auto_reply_record("", "x", "", "knowledge", None).is_err());
        assert!(pm.auto_reply_record("a", "  ", "", "knowledge", None).is_err());
        let long = "x".repeat(2001);
        assert!(pm
            .auto_reply_record("a", &long, "", "knowledge", None)
            .is_err());
    }
}
