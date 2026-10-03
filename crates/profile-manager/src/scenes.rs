//! Scene play scripts: scenes + ordered lines + interaction log.
//!
//! Rust port of jieger `repositories/scene.ts` (`scenes` / `scene_lines`)
//! plus `subAccountInteraction.ts` (`sub_account_interactions`). Timestamps
//! are integer milliseconds (jieger `Date.now()`), line order is an explicit
//! `ord` column, and every dispatched line appends one interaction row for
//! history review and anti-abuse analysis.

use crate::ProfileManager;
use multizen_core::{MultizenError, Result};
use rusqlite::{params, Connection, OptionalExtension};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

pub(crate) fn migrate(conn: &Connection) -> Result<()> {
    let tx = conn.unchecked_transaction()?;
    tx.execute_batch(
        "CREATE TABLE IF NOT EXISTS scenes (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT NOT NULL CHECK(length(trim(name)) BETWEEN 1 AND 100),
            trigger_mode TEXT NOT NULL DEFAULT 'relative-time'
                CHECK(trigger_mode IN ('relative-time','local-time')),
            group_id TEXT,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS scene_lines (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            scene_id INTEGER NOT NULL REFERENCES scenes(id) ON DELETE CASCADE,
            ord INTEGER NOT NULL CHECK(ord >= 0),
            message TEXT NOT NULL DEFAULT '',
            time_offset_sec INTEGER NOT NULL DEFAULT 0 CHECK(time_offset_sec >= 0),
            action_type TEXT NOT NULL DEFAULT 'danmaku'
                CHECK(action_type IN ('danmaku','like','follow'))
        );
        CREATE INDEX IF NOT EXISTS idx_scene_lines_scene_ord
            ON scene_lines(scene_id, ord);
        CREATE TABLE IF NOT EXISTS sub_account_interactions (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            account_id TEXT NOT NULL,
            scene_id INTEGER REFERENCES scenes(id) ON DELETE SET NULL,
            action TEXT NOT NULL CHECK(action IN ('danmaku','like','follow')),
            message TEXT,
            live_room_url TEXT,
            ok INTEGER NOT NULL CHECK(ok IN (0,1)),
            error TEXT,
            duration_ms INTEGER,
            created_at INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_sub_account_interactions_scene
            ON sub_account_interactions(scene_id, created_at);
        CREATE INDEX IF NOT EXISTS idx_sub_account_interactions_account
            ON sub_account_interactions(account_id, created_at);",
    )?;
    tx.commit()?;
    Ok(())
}

/// Scene trigger mode. Wire strings match jieger (`local-time`/`relative-time`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TriggerMode {
    RelativeTime,
    LocalTime,
}

/// Per-line action. Wire strings match jieger (`danmaku`/`like`/`follow`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SceneLineAction {
    Danmaku,
    Like,
    Follow,
}

impl TriggerMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::RelativeTime => "relative-time",
            Self::LocalTime => "local-time",
        }
    }
}

impl SceneLineAction {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Danmaku => "danmaku",
            Self::Like => "like",
            Self::Follow => "follow",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SceneLine {
    pub id: i64,
    pub scene_id: i64,
    pub ord: i64,
    pub message: String,
    pub time_offset_sec: i64,
    pub action_type: SceneLineAction,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Scene {
    pub id: i64,
    pub name: String,
    pub trigger_mode: TriggerMode,
    pub group_id: Option<String>,
    pub lines: Vec<SceneLine>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubAccountInteraction {
    pub id: i64,
    pub account_id: String,
    pub scene_id: Option<i64>,
    pub action: SceneLineAction,
    pub message: Option<String>,
    pub live_room_url: Option<String>,
    pub ok: bool,
    pub error: Option<String>,
    pub duration_ms: Option<i64>,
    pub created_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordInteractionInput {
    pub account_id: String,
    pub scene_id: Option<i64>,
    pub action: SceneLineAction,
    pub message: Option<String>,
    pub live_room_url: Option<String>,
    pub ok: bool,
    pub error: Option<String>,
    pub duration_ms: Option<i64>,
}

/// Local-time mode uses a seconds-of-day clock (`0..=86399`, e.g. `73800` =
/// 20:30:00). Relative-time mode counts seconds from playback start.
pub const MAX_LOCAL_TIME_OFFSET_SEC: i64 = 86_399;

fn config(message: &str) -> MultizenError {
    MultizenError::Config(message.into())
}

fn decode<T: DeserializeOwned>(text: String) -> rusqlite::Result<T> {
    serde_json::from_value(serde_json::Value::String(text)).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
    })
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

fn check_name(name: &str) -> Result<String> {
    let trimmed = name.trim();
    if trimmed.is_empty()
        || trimmed.chars().count() > 100
        || trimmed.chars().any(char::is_control)
    {
        return Err(config("场景名不能为空，最多100个字符，且不能含控制字符"));
    }
    Ok(trimmed.to_string())
}

/// Danmaku lines require a non-empty message; like/follow lines may carry an
/// empty message (the line itself is the action marker, as in jieger).
fn check_message(action: SceneLineAction, message: &str) -> Result<String> {
    if message.chars().any(char::is_control) {
        return Err(config("台词不能含控制字符"));
    }
    let trimmed = message.trim();
    if action == SceneLineAction::Danmaku && trimmed.is_empty() {
        return Err(config("弹幕台词不能为空"));
    }
    Ok(trimmed.to_string())
}

fn check_offset(offset: i64, mode: TriggerMode) -> Result<i64> {
    if offset < 0 {
        return Err(config("触发时间不能为负数"));
    }
    if mode == TriggerMode::LocalTime && offset > MAX_LOCAL_TIME_OFFSET_SEC {
        return Err(config("local-time 模式的触发时间必须在当天 0~86399 秒内"));
    }
    Ok(offset)
}

fn row_to_line(r: &rusqlite::Row<'_>) -> rusqlite::Result<SceneLine> {
    Ok(SceneLine {
        id: r.get(0)?,
        scene_id: r.get(1)?,
        ord: r.get(2)?,
        message: r.get(3)?,
        time_offset_sec: r.get(4)?,
        action_type: decode::<SceneLineAction>(r.get(5)?)?,
    })
}

const LINE_PROJECTION: &str = "SELECT id, scene_id, ord, message, time_offset_sec, action_type
    FROM scene_lines";

fn lines_of(conn: &Connection, scene_id: i64) -> Result<Vec<SceneLine>> {
    let mut stmt = conn.prepare(&format!(
        "{LINE_PROJECTION} WHERE scene_id = ? ORDER BY ord, id"
    ))?;
    let rows = stmt.query_map([scene_id], row_to_line)?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

fn scene_row(
    conn: &Connection,
    id: i64,
) -> Result<Option<(i64, String, TriggerMode, Option<String>, i64, i64)>> {
    let row = conn
        .query_row(
            "SELECT id, name, trigger_mode, group_id, created_at, updated_at
             FROM scenes WHERE id = ?",
            [id],
            |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, Option<String>>(3)?,
                    r.get::<_, i64>(4)?,
                    r.get::<_, i64>(5)?,
                ))
            },
        )
        .optional()?;
    row.map(
        |(id, name, mode_raw, group_id, created_at, updated_at)| {
            decode::<TriggerMode>(mode_raw)
                .map(|trigger_mode| {
                    (id, name, trigger_mode, group_id, created_at, updated_at)
                })
                .map_err(MultizenError::from)
        },
    )
    .transpose()
}

fn assemble(
    conn: &Connection,
    row: (i64, String, TriggerMode, Option<String>, i64, i64),
) -> Result<Scene> {
    let (id, name, trigger_mode, group_id, created_at, updated_at) = row;
    Ok(Scene {
        id,
        name,
        trigger_mode,
        group_id,
        lines: lines_of(conn, id)?,
        created_at,
        updated_at,
    })
}

impl ProfileManager {
    pub fn scene_create(
        &self,
        name: &str,
        trigger_mode: TriggerMode,
        group_id: Option<String>,
    ) -> Result<Scene> {
        let name = check_name(name)?;
        let now = now_ms();
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "INSERT INTO scenes (name, trigger_mode, group_id, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?)",
            params![name, trigger_mode.as_str(), group_id, now, now],
        )?;
        let id = tx.last_insert_rowid();
        let scene = scene_row(&tx, id)?
            .map(|row| assemble(&tx, row))
            .transpose()?
            .ok_or_else(|| config("场景写入后未找到记录"))?;
        tx.commit()?;
        Ok(scene)
    }

    pub fn scene_get(&self, id: i64) -> Result<Option<Scene>> {
        scene_row(&self.conn, id)?.map(|row| assemble(&self.conn, row)).transpose()
    }

    pub fn scene_list(&self) -> Result<Vec<Scene>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, trigger_mode, group_id, created_at, updated_at
             FROM scenes ORDER BY created_at, id",
        )?;
        let raws = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, Option<String>>(3)?,
                    r.get::<_, i64>(4)?,
                    r.get::<_, i64>(5)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let mut all_lines: std::collections::HashMap<i64, Vec<SceneLine>> =
            std::collections::HashMap::new();
        if !raws.is_empty() {
            let mut stmt = self
                .conn
                .prepare(&format!("{LINE_PROJECTION} ORDER BY scene_id, ord, id"))?;
            for line in stmt.query_map([], row_to_line)? {
                let line = line?;
                all_lines.entry(line.scene_id).or_default().push(line);
            }
        }
        let mut out = Vec::with_capacity(raws.len());
        for (id, name, mode_raw, group_id, created_at, updated_at) in raws {
            out.push(Scene {
                id,
                name,
                trigger_mode: decode::<TriggerMode>(mode_raw)?,
                group_id,
                lines: all_lines.remove(&id).unwrap_or_default(),
                created_at,
                updated_at,
            });
        }
        Ok(out)
    }

    /// `name`/`trigger_mode` are keep-on-`None`; `group_id` is tri-state:
    /// `None` keeps, `Some(None)` clears, `Some(Some(_))` sets.
    pub fn scene_update(
        &self,
        id: i64,
        name: Option<String>,
        trigger_mode: Option<TriggerMode>,
        group_id: Option<Option<String>>,
    ) -> Result<Scene> {
        let tx = self.conn.unchecked_transaction()?;
        let current = scene_row(&tx, id)?.ok_or_else(|| MultizenError::NotFound(id.to_string()))?;
        let mut fields: Vec<&str> = Vec::new();
        let mut values: Vec<rusqlite::types::Value> = Vec::new();
        let mut new_mode = current.2;
        if let Some(name) = name {
            let name = check_name(&name)?;
            fields.push("name = ?");
            values.push(rusqlite::types::Value::Text(name));
        }
        if let Some(mode) = trigger_mode {
            // Existing lines must still fit the new mode's clock.
            for line in lines_of(&tx, id)? {
                check_offset(line.time_offset_sec, mode)?;
            }
            new_mode = mode;
            fields.push("trigger_mode = ?");
            values.push(rusqlite::types::Value::Text(mode.as_str().to_string()));
        }
        if let Some(group) = group_id {
            fields.push("group_id = ?");
            values.push(match group {
                Some(g) => rusqlite::types::Value::Text(g),
                None => rusqlite::types::Value::Null,
            });
        }
        let _ = new_mode;
        fields.push("updated_at = ?");
        values.push(rusqlite::types::Value::Integer(now_ms()));
        values.push(rusqlite::types::Value::Integer(id));
        let params: Vec<&dyn rusqlite::ToSql> =
            values.iter().map(|v| v as &dyn rusqlite::ToSql).collect();
        tx.execute(
            &format!("UPDATE scenes SET {} WHERE id = ?", fields.join(", ")),
            params.as_slice(),
        )?;
        let scene = scene_row(&tx, id)?
            .map(|row| assemble(&tx, row))
            .transpose()?
            .ok_or_else(|| MultizenError::NotFound(id.to_string()))?;
        tx.commit()?;
        Ok(scene)
    }

    /// Absent ids succeed, matching profile `delete` semantics. Interaction
    /// rows keep their history (`scene_id` becomes NULL via `ON DELETE SET
    /// NULL`); lines are removed by `ON DELETE CASCADE`.
    pub fn scene_delete(&self, id: i64) -> Result<()> {
        self.conn
            .execute("DELETE FROM scenes WHERE id = ?", [id])?;
        Ok(())
    }

    fn touch_scene(tx: &Connection, scene_id: i64) -> Result<()> {
        tx.execute(
            "UPDATE scenes SET updated_at = ? WHERE id = ?",
            params![now_ms(), scene_id],
        )?;
        Ok(())
    }

    pub fn scene_add_line(
        &self,
        scene_id: i64,
        message: &str,
        time_offset_sec: i64,
        action_type: SceneLineAction,
    ) -> Result<SceneLine> {
        let message = check_message(action_type, message)?;
        let tx = self.conn.unchecked_transaction()?;
        let row = scene_row(&tx, scene_id)?
            .ok_or_else(|| MultizenError::NotFound(scene_id.to_string()))?;
        check_offset(time_offset_sec, row.2)?;
        let ord: i64 = tx.query_row(
            "SELECT COALESCE(MAX(ord), -1) + 1 FROM scene_lines WHERE scene_id = ?",
            [scene_id],
            |r| r.get(0),
        )?;
        tx.execute(
            "INSERT INTO scene_lines (scene_id, ord, message, time_offset_sec, action_type)
             VALUES (?, ?, ?, ?, ?)",
            params![scene_id, ord, message, time_offset_sec, action_type.as_str()],
        )?;
        let id = tx.last_insert_rowid();
        Self::touch_scene(&tx, scene_id)?;
        let line = tx
            .query_row(
                &format!("{LINE_PROJECTION} WHERE id = ?"),
                [id],
                row_to_line,
            )
            .optional()?
            .ok_or_else(|| config("台词写入后未找到记录"))?;
        tx.commit()?;
        Ok(line)
    }

    pub fn scene_line_get(&self, id: i64) -> Result<Option<SceneLine>> {
        Ok(self
            .conn
            .query_row(&format!("{LINE_PROJECTION} WHERE id = ?"), [id], row_to_line)
            .optional()?)
    }

    pub fn scene_update_line(
        &self,
        id: i64,
        message: Option<String>,
        time_offset_sec: Option<i64>,
        action_type: Option<SceneLineAction>,
    ) -> Result<SceneLine> {
        let tx = self.conn.unchecked_transaction()?;
        let current = tx
            .query_row(&format!("{LINE_PROJECTION} WHERE id = ?"), [id], row_to_line)
            .optional()?
            .ok_or_else(|| MultizenError::NotFound(id.to_string()))?;
        let scene = scene_row(&tx, current.scene_id)?
            .ok_or_else(|| MultizenError::NotFound(current.scene_id.to_string()))?;
        let action = action_type.unwrap_or(current.action_type);
        let message = match message {
            Some(m) => check_message(action, &m)?,
            None => {
                if action == SceneLineAction::Danmaku && current.message.trim().is_empty() {
                    return Err(config("弹幕台词不能为空"));
                }
                current.message.clone()
            }
        };
        let offset = match time_offset_sec {
            Some(o) => check_offset(o, scene.2)?,
            None => {
                check_offset(current.time_offset_sec, scene.2)?;
                current.time_offset_sec
            }
        };
        tx.execute(
            "UPDATE scene_lines SET message = ?, time_offset_sec = ?, action_type = ? WHERE id = ?",
            params![message, offset, action.as_str(), id],
        )?;
        Self::touch_scene(&tx, current.scene_id)?;
        let line = tx
            .query_row(&format!("{LINE_PROJECTION} WHERE id = ?"), [id], row_to_line)
            .optional()?
            .ok_or_else(|| MultizenError::NotFound(id.to_string()))?;
        tx.commit()?;
        Ok(line)
    }

    pub fn scene_delete_line(&self, id: i64) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        let parent: Option<i64> = tx
            .query_row("SELECT scene_id FROM scene_lines WHERE id = ?", [id], |r| {
                r.get(0)
            })
            .optional()?;
        if let Some(scene_id) = parent {
            tx.execute("DELETE FROM scene_lines WHERE id = ?", [id])?;
            Self::touch_scene(&tx, scene_id)?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Replace a scene's line order atomically. `ordered_ids` must contain
    /// exactly the scene's current line ids (no missing, no extras, no
    /// duplicates); ord is reassigned `0..N` in the given order.
    pub fn scene_reorder_lines(
        &self,
        scene_id: i64,
        ordered_ids: &[i64],
    ) -> Result<Vec<SceneLine>> {
        let tx = self.conn.unchecked_transaction()?;
        if scene_row(&tx, scene_id)?.is_none() {
            return Err(MultizenError::NotFound(scene_id.to_string()));
        }
        let current = lines_of(&tx, scene_id)?;
        if ordered_ids.len() != current.len() {
            return Err(config("台词数量与场景现有台词不一致"));
        }
        let mut seen = std::collections::HashSet::new();
        let known: std::collections::HashSet<i64> = current.iter().map(|l| l.id).collect();
        for id in ordered_ids {
            if !known.contains(id) || !seen.insert(id) {
                return Err(config("排序包含了未知或重复的台词"));
            }
        }
        for (ord, id) in ordered_ids.iter().enumerate() {
            tx.execute(
                "UPDATE scene_lines SET ord = ? WHERE id = ? AND scene_id = ?",
                params![ord as i64, id, scene_id],
            )?;
        }
        Self::touch_scene(&tx, scene_id)?;
        let lines = lines_of(&tx, scene_id)?;
        tx.commit()?;
        Ok(lines)
    }

    /// Append one dispatch result. Never throws for business reasons; only
    /// DB/IO failures propagate (play path logs them and continues).
    pub fn record_interaction(&self, input: &RecordInteractionInput) -> Result<SubAccountInteraction> {
        if input.account_id.trim().is_empty() {
            return Err(config("账号ID不能为空"));
        }
        let now = now_ms();
        self.conn.execute(
            "INSERT INTO sub_account_interactions
                (account_id, scene_id, action, message, live_room_url, ok, error, duration_ms, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
            params![
                input.account_id,
                input.scene_id,
                input.action.as_str(),
                input.message,
                input.live_room_url,
                if input.ok { 1 } else { 0 },
                input.error,
                input.duration_ms,
                now,
            ],
        )?;
        let id = self.conn.last_insert_rowid();
        Ok(SubAccountInteraction {
            id,
            account_id: input.account_id.clone(),
            scene_id: input.scene_id,
            action: input.action,
            message: input.message.clone(),
            live_room_url: input.live_room_url.clone(),
            ok: input.ok,
            error: input.error.clone(),
            duration_ms: input.duration_ms,
            created_at: now,
        })
    }

    /// Newest-first interaction history, optionally filtered. `limit` is
    /// clamped to `1..=500` (jieger `listInteractions` contract).
    pub fn list_interactions(
        &self,
        scene_id: Option<i64>,
        account_id: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<SubAccountInteraction>> {
        let limit = limit.clamp(1, 500);
        let offset = offset.max(0);
        let mut sql = String::from(
            "SELECT id, account_id, scene_id, action, message, live_room_url, ok, error,
                    duration_ms, created_at
             FROM sub_account_interactions",
        );
        let mut clauses: Vec<&str> = Vec::new();
        let mut values: Vec<rusqlite::types::Value> = Vec::new();
        if let Some(scene_id) = scene_id {
            clauses.push("scene_id = ?");
            values.push(rusqlite::types::Value::Integer(scene_id));
        }
        if let Some(account_id) = account_id {
            clauses.push("account_id = ?");
            values.push(rusqlite::types::Value::Text(account_id.to_string()));
        }
        if !clauses.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(&clauses.join(" AND "));
        }
        sql.push_str(" ORDER BY created_at DESC, id DESC LIMIT ? OFFSET ?");
        values.push(rusqlite::types::Value::Integer(limit));
        values.push(rusqlite::types::Value::Integer(offset));
        let params: Vec<&dyn rusqlite::ToSql> =
            values.iter().map(|v| v as &dyn rusqlite::ToSql).collect();
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params.as_slice(), |r| {
            Ok(SubAccountInteraction {
                id: r.get(0)?,
                account_id: r.get(1)?,
                scene_id: r.get(2)?,
                action: decode::<SceneLineAction>(r.get::<_, String>(3)?)?,
                message: r.get(4)?,
                live_room_url: r.get(5)?,
                ok: r.get::<_, i64>(6)? == 1,
                error: r.get(7)?,
                duration_ms: r.get(8)?,
                created_at: r.get(9)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }
}
