//! jieger 商品话术库：独立表 `shop_product_scripts` / `shop_product_script_lines`。
//! 不搬旧话术数据；行动作触发与提前量排期见 tauri-app 播放引擎。
use crate::ProfileManager;
use multizen_core::{MultizenError, Result};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

/// 话术行动作：上架 / 下架 / 讲解 / 取消讲解。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ScriptLineAction {
    OnShelf,
    OffShelf,
    Explain,
    CancelExplain,
}

impl ScriptLineAction {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::OnShelf => "on-shelf",
            Self::OffShelf => "off-shelf",
            Self::Explain => "explain",
            Self::CancelExplain => "cancel-explain",
        }
    }
}

fn decode_action(raw: String) -> rusqlite::Result<ScriptLineAction> {
    serde_json::from_value::<ScriptLineAction>(serde_json::Value::String(raw)).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
    })
}

/// 商品话术脚本（库条目）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShopProductScript {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// 脚本明细：脚本 + 按 `sort_order` 排序的话术行。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShopProductScriptDetail {
    pub script: ShopProductScript,
    pub lines: Vec<ShopProductScriptLine>,
}

/// 话术行：动作 + 商品 + 视频时间点 + 提前量 + 话术内容。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShopProductScriptLine {
    pub id: String,
    pub script_id: String,
    pub sort_order: i64,
    pub action: ScriptLineAction,
    pub goods_id: String,
    pub goods_name: Option<String>,
    pub video_time_sec: f64,
    pub lead_sec: f64,
    pub content: String,
    pub created_at: String,
    pub updated_at: String,
}

/// 创建脚本入参。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateShopProductScriptInput {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
}

/// 更新脚本入参：`description: Some(None)` 清空。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateShopProductScriptInput {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub description: Option<Option<String>>,
}

/// 新增话术行入参：`sort_order` 缺省时追加到末尾。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddShopProductScriptLineInput {
    pub script_id: String,
    pub action: ScriptLineAction,
    pub goods_id: String,
    #[serde(default)]
    pub goods_name: Option<String>,
    pub video_time_sec: f64,
    #[serde(default)]
    pub lead_sec: f64,
    #[serde(default)]
    pub content: String,
    #[serde(default)]
    pub sort_order: Option<i64>,
}

/// 更新话术行入参：缺省字段保持原值，`goods_name: Some(None)` 清空。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateShopProductScriptLineInput {
    #[serde(default)]
    pub action: Option<ScriptLineAction>,
    #[serde(default)]
    pub goods_id: Option<String>,
    #[serde(default)]
    pub goods_name: Option<Option<String>>,
    #[serde(default)]
    pub video_time_sec: Option<f64>,
    #[serde(default)]
    pub lead_sec: Option<f64>,
    #[serde(default)]
    pub content: Option<String>,
}

pub(crate) fn migrate(conn: &Connection) -> Result<()> {
    let tx = conn.unchecked_transaction()?;
    tx.execute_batch(
        "CREATE TABLE IF NOT EXISTS shop_product_scripts (
            id TEXT PRIMARY KEY NOT NULL,
            name TEXT NOT NULL CHECK(length(trim(name)) BETWEEN 1 AND 100),
            description TEXT,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS shop_product_script_lines (
            id TEXT PRIMARY KEY NOT NULL,
            script_id TEXT NOT NULL REFERENCES shop_product_scripts(id) ON DELETE CASCADE,
            sort_order INTEGER NOT NULL DEFAULT 0,
            action TEXT NOT NULL CHECK(action IN ('on-shelf','off-shelf','explain','cancel-explain')),
            goods_id TEXT NOT NULL CHECK(length(trim(goods_id)) BETWEEN 1 AND 128),
            goods_name TEXT,
            video_time_sec REAL NOT NULL CHECK(video_time_sec >= 0),
            lead_sec REAL NOT NULL DEFAULT 0 CHECK(lead_sec >= 0),
            content TEXT NOT NULL DEFAULT '',
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_shop_product_script_lines_script
            ON shop_product_script_lines(script_id, sort_order, id);",
    )?;
    tx.commit()?;
    Ok(())
}

fn config(message: impl Into<String>) -> MultizenError {
    MultizenError::Config(message.into())
}

fn now() -> String {
    chrono::Utc::now().to_rfc3339()
}

fn validate_name(name: &str) -> Result<String> {
    let trimmed = name.trim();
    if trimmed.is_empty() || trimmed.chars().count() > 100 || trimmed.chars().any(char::is_control)
    {
        return Err(config(
            "话术库名称不能为空，最多100个字符，且不能含控制字符",
        ));
    }
    Ok(trimmed.to_string())
}

fn validate_description(description: Option<String>) -> Result<Option<String>> {
    match description {
        None => Ok(None),
        Some(raw) => {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                return Ok(None);
            }
            if trimmed.chars().count() > 500 || trimmed.chars().any(char::is_control) {
                return Err(config("话术库描述最多500个字符，且不能含控制字符"));
            }
            Ok(Some(trimmed.to_string()))
        }
    }
}

fn validate_goods_id(goods_id: &str) -> Result<String> {
    let trimmed = goods_id.trim();
    if trimmed.is_empty() || trimmed.chars().count() > 128 || trimmed.chars().any(char::is_control)
    {
        return Err(config("商品ID不能为空，最多128个字符，且不能含控制字符"));
    }
    Ok(trimmed.to_string())
}

fn validate_goods_name(goods_name: Option<String>) -> Result<Option<String>> {
    match goods_name {
        None => Ok(None),
        Some(raw) => {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                return Ok(None);
            }
            if trimmed.chars().count() > 200 || trimmed.chars().any(char::is_control) {
                return Err(config("商品名称最多200个字符，且不能含控制字符"));
            }
            Ok(Some(trimmed.to_string()))
        }
    }
}

fn validate_time(video_time_sec: f64, lead_sec: f64) -> Result<()> {
    if !video_time_sec.is_finite() || video_time_sec < 0.0 {
        return Err(config("视频时间点必须为不小于0的数字（秒）"));
    }
    if !lead_sec.is_finite() || lead_sec < 0.0 {
        return Err(config("提前量必须为不小于0的数字（秒）"));
    }
    Ok(())
}

fn validate_content(content: &str) -> Result<String> {
    if content.chars().count() > 2000 {
        return Err(config("话术内容最多2000个字符"));
    }
    Ok(content.to_string())
}

fn row_script(r: &rusqlite::Row<'_>) -> rusqlite::Result<ShopProductScript> {
    Ok(ShopProductScript {
        id: r.get(0)?,
        name: r.get(1)?,
        description: r.get(2)?,
        created_at: r.get(3)?,
        updated_at: r.get(4)?,
    })
}

fn row_line(r: &rusqlite::Row<'_>) -> rusqlite::Result<ShopProductScriptLine> {
    Ok(ShopProductScriptLine {
        id: r.get(0)?,
        script_id: r.get(1)?,
        sort_order: r.get(2)?,
        action: decode_action(r.get(3)?)?,
        goods_id: r.get(4)?,
        goods_name: r.get(5)?,
        video_time_sec: r.get(6)?,
        lead_sec: r.get(7)?,
        content: r.get(8)?,
        created_at: r.get(9)?,
        updated_at: r.get(10)?,
    })
}

const SCRIPT_PROJECTION: &str =
    "SELECT id,name,description,created_at,updated_at FROM shop_product_scripts";
const LINE_PROJECTION: &str = "SELECT id,script_id,sort_order,action,goods_id,goods_name,video_time_sec,lead_sec,content,created_at,updated_at FROM shop_product_script_lines";

fn get_script(conn: &Connection, id: &str) -> Result<Option<ShopProductScript>> {
    Ok(conn
        .query_row(&format!("{SCRIPT_PROJECTION} WHERE id=?"), [id], row_script)
        .optional()?)
}

fn lines_of(conn: &Connection, script_id: &str) -> Result<Vec<ShopProductScriptLine>> {
    let mut stmt = conn.prepare(&format!(
        "{LINE_PROJECTION} WHERE script_id=? ORDER BY sort_order,id"
    ))?;
    let rows = stmt.query_map([script_id], row_line)?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

fn get_line(conn: &Connection, id: &str) -> Result<Option<ShopProductScriptLine>> {
    Ok(conn
        .query_row(&format!("{LINE_PROJECTION} WHERE id=?"), [id], row_line)
        .optional()?)
}

impl ProfileManager {
    /// 新建话术脚本。
    pub fn shop_product_script_create(
        &self,
        input: CreateShopProductScriptInput,
    ) -> Result<ShopProductScript> {
        let name = validate_name(&input.name)?;
        let description = validate_description(input.description)?;
        let id = uuid::Uuid::new_v4().to_string();
        let now = now();
        self.conn.execute(
            "INSERT INTO shop_product_scripts(id,name,description,created_at,updated_at) VALUES (?,?,?,?,?)",
            params![id, name, description, now, now],
        )?;
        get_script(&self.conn, &id)?.ok_or_else(|| config("话术脚本写入后未找到记录"))
    }

    /// 读取脚本明细（脚本 + 按序排列的话术行）；不存在返回 `Ok(None)`。
    pub fn shop_product_script_get(&self, id: &str) -> Result<Option<ShopProductScriptDetail>> {
        match get_script(&self.conn, id)? {
            None => Ok(None),
            Some(script) => Ok(Some(ShopProductScriptDetail {
                lines: lines_of(&self.conn, id)?,
                script,
            })),
        }
    }

    /// 按更新时间倒序列出全部脚本（不含行）。
    pub fn shop_product_scripts_list(&self) -> Result<Vec<ShopProductScript>> {
        let mut stmt = self
            .conn
            .prepare(&format!("{SCRIPT_PROJECTION} ORDER BY updated_at DESC,id"))?;
        let rows = stmt.query_map([], row_script)?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// 更新脚本名称/描述；脚本不存在返回 `NotFound`。
    pub fn shop_product_script_update(
        &self,
        id: &str,
        patch: UpdateShopProductScriptInput,
    ) -> Result<ShopProductScript> {
        let existing = get_script(&self.conn, id)?
            .ok_or_else(|| MultizenError::NotFound(format!("话术脚本 {id}")))?;
        let name = match patch.name {
            Some(n) => validate_name(&n)?,
            None => existing.name,
        };
        let description = match patch.description {
            Some(d) => validate_description(d)?,
            None => existing.description,
        };
        self.conn.execute(
            "UPDATE shop_product_scripts SET name=?,description=?,updated_at=? WHERE id=?",
            params![name, description, now(), id],
        )?;
        get_script(&self.conn, id)?.ok_or_else(|| config("话术脚本更新后未找到记录"))
    }

    /// 删除脚本；行通过外键级联删除。不存在的 id 视为成功（与 profiles.delete 一致）。
    pub fn shop_product_script_delete(&self, id: &str) -> Result<()> {
        self.conn
            .execute("DELETE FROM shop_product_scripts WHERE id=?", [id])?;
        Ok(())
    }

    /// 向脚本追加/插入话术行；脚本不存在返回 `NotFound`。
    pub fn shop_product_script_add_line(
        &self,
        input: AddShopProductScriptLineInput,
    ) -> Result<ShopProductScriptLine> {
        validate_time(input.video_time_sec, input.lead_sec)?;
        let goods_id = validate_goods_id(&input.goods_id)?;
        let goods_name = validate_goods_name(input.goods_name)?;
        let content = validate_content(&input.content)?;
        if get_script(&self.conn, &input.script_id)?.is_none() {
            return Err(MultizenError::NotFound(format!(
                "话术脚本 {}",
                input.script_id
            )));
        }
        let sort_order = match input.sort_order {
            Some(v) => v,
            None => self.conn.query_row(
                "SELECT COALESCE(MAX(sort_order), -1) + 1 FROM shop_product_script_lines WHERE script_id=?",
                [&input.script_id],
                |r| r.get::<_, i64>(0),
            )?,
        };
        let id = uuid::Uuid::new_v4().to_string();
        let now = now();
        self.conn.execute(
            "INSERT INTO shop_product_script_lines(id,script_id,sort_order,action,goods_id,goods_name,video_time_sec,lead_sec,content,created_at,updated_at) VALUES (?,?,?,?,?,?,?,?,?,?,?)",
            params![id, input.script_id, sort_order, input.action.as_str(), goods_id, goods_name, input.video_time_sec, input.lead_sec, content, now, now],
        )?;
        get_line(&self.conn, &id)?.ok_or_else(|| config("话术行写入后未找到记录"))
    }

    /// 更新话术行；行不存在返回 `NotFound`。
    pub fn shop_product_script_update_line(
        &self,
        id: &str,
        patch: UpdateShopProductScriptLineInput,
    ) -> Result<ShopProductScriptLine> {
        let existing = get_line(&self.conn, id)?
            .ok_or_else(|| MultizenError::NotFound(format!("话术行 {id}")))?;
        let action = patch.action.unwrap_or(existing.action);
        let goods_id = match patch.goods_id {
            Some(g) => validate_goods_id(&g)?,
            None => existing.goods_id,
        };
        let goods_name = match patch.goods_name {
            Some(g) => validate_goods_name(g)?,
            None => existing.goods_name,
        };
        let video_time_sec = patch.video_time_sec.unwrap_or(existing.video_time_sec);
        let lead_sec = patch.lead_sec.unwrap_or(existing.lead_sec);
        validate_time(video_time_sec, lead_sec)?;
        let content = match patch.content {
            Some(c) => validate_content(&c)?,
            None => existing.content,
        };
        self.conn.execute(
            "UPDATE shop_product_script_lines SET action=?,goods_id=?,goods_name=?,video_time_sec=?,lead_sec=?,content=?,updated_at=? WHERE id=?",
            params![action.as_str(), goods_id, goods_name, video_time_sec, lead_sec, content, now(), id],
        )?;
        get_line(&self.conn, id)?.ok_or_else(|| config("话术行更新后未找到记录"))
    }

    /// 删除话术行；不存在的 id 视为成功。
    pub fn shop_product_script_delete_line(&self, id: &str) -> Result<()> {
        self.conn
            .execute("DELETE FROM shop_product_script_lines WHERE id=?", [id])?;
        Ok(())
    }

    /// 整组重排：`ordered_ids` 必须恰好是该脚本的全部行 id（集合一致），按数组下标重写
    /// `sort_order`。任一 id 不属于该脚本、缺失或重复都拒绝且不落库。
    pub fn shop_product_script_reorder_lines(
        &self,
        script_id: &str,
        ordered_ids: Vec<String>,
    ) -> Result<Vec<ShopProductScriptLine>> {
        if get_script(&self.conn, script_id)?.is_none() {
            return Err(MultizenError::NotFound(format!("话术脚本 {script_id}")));
        }
        let current = lines_of(&self.conn, script_id)?;
        let mut have: Vec<&str> = current.iter().map(|l| l.id.as_str()).collect();
        have.sort_unstable();
        let mut want: Vec<&str> = ordered_ids.iter().map(String::as_str).collect();
        want.sort_unstable();
        if have != want {
            return Err(config(
                "重排的话术行ID必须与该脚本的全部行一一对应，不能缺失、重复或混入其他脚本的行",
            ));
        }
        let tx = self.conn.unchecked_transaction()?;
        for (index, id) in ordered_ids.iter().enumerate() {
            tx.execute(
                "UPDATE shop_product_script_lines SET sort_order=?,updated_at=? WHERE id=? AND script_id=?",
                params![index as i64, now(), id, script_id],
            )?;
        }
        tx.commit()?;
        lines_of(&self.conn, script_id)
    }
}
