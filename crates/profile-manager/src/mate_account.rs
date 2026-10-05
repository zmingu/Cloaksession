//! 直播伴侣账号表（`mate_accounts`）。
//!
//! 与 `business_accounts` 不同，直播伴侣账号**不绑定浏览器环境**：登录是纯 HTTP 4 步
//! （`qr.kuaishou.com` / `id.kwaixiaodian.com`），开播取推流码走 `node:https` 签名直调
//! 快手 App API，全程零 playwright。因此本表没有 `profile_id` 列，也不进
//! `business_accounts`（后者强制 `profile_id`）。
//!
//! jieger 对应模型：`electron/main/services/database/repositories/account.ts` 的
//! `accounts(usage='mate')` 行；Cloaksession 把 mate 专用字段独立成表，避免污染
//! 通用业务账号表。登录成功后 `mate_account_record_login` 写入平台身份 + 推流 token
//! + `login_at`（对应 jieger `updateAccount(accountId, {...})`）。
use crate::ProfileManager;
use multizen_core::{MultizenError, Result};
use rusqlite::{params, Connection, OptionalExtension};
use uuid::Uuid;

pub(crate) fn migrate(conn: &Connection) -> Result<()> {
    let tx = conn.unchecked_transaction()?;
    tx.execute_batch(
        "CREATE TABLE IF NOT EXISTS mate_accounts (
            id TEXT PRIMARY KEY NOT NULL,
            label TEXT NOT NULL,
            platform_user_id TEXT,
            user_name TEXT,
            avatar_url TEXT,
            mate_pass_token TEXT,
            mate_token TEXT,
            mate_st TEXT,
            mate_h5_st TEXT,
            mate_lmtoken TEXT,
            login_at INTEGER,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );",
    )?;
    tx.commit()?;
    Ok(())
}

/// `receive` 步骤返回的推流凭证（jieger `ReceiveResp` 的 token 字段）。
///
/// 平台原始 key：`passToken` / `token` / `lmtoken` / `kuaishou.live.mate_st` /
/// `kuaishou.live.mate.h5_st`。仅在登录成功时写入，**不写日志**。
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MateLoginTokens {
    pub pass_token: Option<String>,
    pub token: Option<String>,
    pub lmtoken: Option<String>,
    pub mate_st: Option<String>,
    pub mate_h5_st: Option<String>,
}

/// 一行直播伴侣账号（无环境绑定）。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MateAccount {
    pub id: String,
    pub label: String,
    pub platform_user_id: Option<String>,
    pub user_name: Option<String>,
    pub avatar_url: Option<String>,
    pub mate_pass_token: Option<String>,
    pub mate_token: Option<String>,
    pub mate_st: Option<String>,
    pub mate_h5_st: Option<String>,
    pub mate_lmtoken: Option<String>,
    pub login_at: Option<i64>,
    pub created_at: String,
    pub updated_at: String,
}

/// 新建账号时的占位别名。前端「添加账号」不再预填名字，先落一行占位，
/// 待首次扫码登录成功后由 `mate_account_record_login` 用平台昵称覆盖。
pub const MATE_PLACEHOLDER_LABEL: &str = "未命名伴侣";

fn now() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

fn invalid(message: impl Into<String>) -> MultizenError {
    MultizenError::Config(message.into())
}

fn row_to_account(r: &rusqlite::Row<'_>) -> rusqlite::Result<MateAccount> {
    Ok(MateAccount {
        id: r.get(0)?,
        label: r.get(1)?,
        platform_user_id: r.get(2)?,
        user_name: r.get(3)?,
        avatar_url: r.get(4)?,
        mate_pass_token: r.get(5)?,
        mate_token: r.get(6)?,
        mate_st: r.get(7)?,
        mate_h5_st: r.get(8)?,
        mate_lmtoken: r.get(9)?,
        login_at: r.get(10)?,
        created_at: r.get(11)?,
        updated_at: r.get(12)?,
    })
}

const SELECT_COLUMNS: &str = "id, label, platform_user_id, user_name, avatar_url,
     mate_pass_token, mate_token, mate_st, mate_h5_st, mate_lmtoken,
     login_at, created_at, updated_at";

fn validate_label(label: &str) -> Result<&str> {
    let label = label.trim();
    if label.is_empty() || label.chars().count() > 100 {
        return Err(invalid("账号别名不能为空，最多100个字符"));
    }
    Ok(label)
}

impl ProfileManager {
    /// 所有直播伴侣账号，按创建时间升序。
    pub fn mate_accounts_list(&self) -> Result<Vec<MateAccount>> {
        let mut stmt = self
            .conn
            .prepare(&format!("SELECT {SELECT_COLUMNS} FROM mate_accounts ORDER BY created_at, id"))?;
        let rows = stmt.query_map([], row_to_account)?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// 单个账号；不存在返回 `None`。
    pub fn mate_account_get(&self, id: &str) -> Result<Option<MateAccount>> {
        Ok(self
            .conn
            .query_row(
                &format!("SELECT {SELECT_COLUMNS} FROM mate_accounts WHERE id = ?"),
                [id],
                row_to_account,
            )
            .optional()?)
    }

    /// 新建账号（UUID 主键）。`label` 为 `None` 时先用占位别名
    /// [`MATE_PLACEHOLDER_LABEL`]，待首次登录成功后由 `mate_account_record_login`
    /// 以平台昵称覆盖。对应 jieger `addMateAccount`。
    pub fn mate_account_add(&self, label: Option<&str>) -> Result<MateAccount> {
        let label = match label {
            Some(raw) => validate_label(raw)?,
            None => MATE_PLACEHOLDER_LABEL,
        };
        let id = Uuid::new_v4().to_string();
        let ts = now();
        self.conn.execute(
            "INSERT INTO mate_accounts(id, label, created_at, updated_at) VALUES (?, ?, ?, ?)",
            params![id, label, ts, ts],
        )?;
        self.mate_account_get(&id)?
            .ok_or_else(|| MultizenError::NotFound(format!("直播伴侣账号 {id} 写入后未找到")))
    }

    /// 删除账号。对应 jieger `removeMateAccount`（运行时会话由驱动层负责取消）。
    pub fn mate_account_remove(&self, id: &str) -> Result<()> {
        let changed = self
            .conn
            .execute("DELETE FROM mate_accounts WHERE id = ?", [id])?;
        if changed == 0 {
            return Err(MultizenError::NotFound(format!("直播伴侣账号 {id} 不存在")));
        }
        Ok(())
    }

    /// 重命名别名。对应 jieger `renameMateAccount`。
    pub fn mate_account_rename(&self, id: &str, label: &str) -> Result<MateAccount> {
        let label = validate_label(label)?;
        let changed = self.conn.execute(
            "UPDATE mate_accounts SET label = ?, updated_at = ? WHERE id = ?",
            params![label, now(), id],
        )?;
        if changed == 0 {
            return Err(MultizenError::NotFound(format!("直播伴侣账号 {id} 不存在")));
        }
        self.mate_account_get(id)?
            .ok_or_else(|| MultizenError::NotFound(format!("直播伴侣账号 {id} 不存在")))
    }

    /// 登录成功后写入平台身份 + 推流 token + 登录时间。
    /// 对应 jieger `updateAccount(accountId, { profileId, profileName, avatarUrl,
    /// matePassToken, mateToken, mateSt, mateH5St, mateLmtoken, mateLoginAt, status })`。
    pub fn mate_account_record_login(
        &self,
        id: &str,
        platform_user_id: &str,
        user_name: &str,
        avatar_url: Option<&str>,
        tokens: &MateLoginTokens,
        login_at: i64,
    ) -> Result<MateAccount> {
        // 首次登录自动命名：仅当旧行 `login_at IS NULL`（从未登录过）时才用平台昵称
        // 覆盖 label；已登录过的账号保持用户可能已手动重命名后的值。
        let next_label: Option<String> = {
            let trimmed = user_name.trim();
            if trimmed.is_empty() || trimmed.chars().count() > 100 {
                None
            } else {
                Some(trimmed.to_string())
            }
        };
        let changed = self.conn.execute(
            "UPDATE mate_accounts
             SET platform_user_id = ?, user_name = ?, avatar_url = ?,
                 label = CASE WHEN login_at IS NULL THEN COALESCE(?, label) ELSE label END,
                 mate_pass_token = ?, mate_token = ?, mate_st = ?, mate_h5_st = ?, mate_lmtoken = ?,
                 login_at = ?, updated_at = ?
             WHERE id = ?",
            params![
                platform_user_id,
                user_name,
                avatar_url,
                next_label,
                tokens.pass_token,
                tokens.token,
                tokens.mate_st,
                tokens.mate_h5_st,
                tokens.lmtoken,
                login_at,
                now(),
                id
            ],
        )?;
        if changed == 0 {
            return Err(MultizenError::NotFound(format!(
                "直播伴侣账号 {id} 不存在，登录结果未落库"
            )));
        }
        self.mate_account_get(id)?
            .ok_or_else(|| MultizenError::NotFound(format!("直播伴侣账号 {id} 不存在")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn open() -> (tempfile::TempDir, ProfileManager) {
        let dir = tempfile::tempdir().unwrap();
        let pm = ProfileManager::new(&dir.path().join("p.db"), &dir.path().join("profiles")).unwrap();
        (dir, pm)
    }

    #[test]
    fn migration_is_idempotent() {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        migrate(&conn).unwrap();
    }

    #[test]
    fn crud_roundtrip() {
        let (_dir, pm) = open();
        assert!(pm.mate_accounts_list().unwrap().is_empty());

        let a = pm.mate_account_add(Some("伴侣甲")).unwrap();
        let b = pm.mate_account_add(Some("伴侣乙")).unwrap();
        assert_ne!(a.id, b.id);
        assert_eq!(a.label, "伴侣甲");
        assert!(a.platform_user_id.is_none());
        assert!(a.login_at.is_none());

        let listed = pm.mate_accounts_list().unwrap();
        assert_eq!(listed.len(), 2);
        assert_eq!(pm.mate_account_get(&a.id).unwrap().unwrap().label, "伴侣甲");

        let renamed = pm.mate_account_rename(&a.id, " 伴侣甲-改 ").unwrap();
        assert_eq!(renamed.label, "伴侣甲-改");

        pm.mate_account_remove(&b.id).unwrap();
        assert_eq!(pm.mate_accounts_list().unwrap().len(), 1);
        assert!(pm.mate_account_get(&b.id).unwrap().is_none());
    }

    #[test]
    fn rejects_empty_label_and_unknown_id() {
        let (_dir, pm) = open();
        assert!(pm.mate_account_add(Some("   ")).is_err());
        assert!(pm.mate_account_add(Some(&"x".repeat(101))).is_err());
        assert!(pm.mate_account_rename("ghost", "n").is_err());
        assert!(pm.mate_account_remove("ghost").is_err());
    }

    #[test]
    fn record_login_persists_identity_and_tokens() {
        let (_dir, pm) = open();
        let account = pm.mate_account_add(Some("伴侣甲")).unwrap();
        let tokens = MateLoginTokens {
            pass_token: Some("pass".into()),
            token: Some("tok".into()),
            lmtoken: Some("lm".into()),
            mate_st: Some("st".into()),
            mate_h5_st: Some("h5".into()),
        };
        let saved = pm
            .mate_account_record_login(
                &account.id,
                "u1001",
                "测试主播",
                Some("http://example.com/a.png"),
                &tokens,
                1_700_000_000_000,
            )
            .unwrap();
        assert_eq!(saved.platform_user_id.as_deref(), Some("u1001"));
        assert_eq!(saved.user_name.as_deref(), Some("测试主播"));
        assert_eq!(saved.avatar_url.as_deref(), Some("http://example.com/a.png"));
        assert_eq!(saved.mate_pass_token.as_deref(), Some("pass"));
        assert_eq!(saved.mate_token.as_deref(), Some("tok"));
        assert_eq!(saved.mate_lmtoken.as_deref(), Some("lm"));
        assert_eq!(saved.mate_st.as_deref(), Some("st"));
        assert_eq!(saved.mate_h5_st.as_deref(), Some("h5"));
        assert_eq!(saved.login_at, Some(1_700_000_000_000));
        // 首次登录（旧行 login_at 为 NULL）会用平台昵称覆盖 label。
        assert_eq!(saved.label, "测试主播");

        // 落库可经列表再次读回。
        let reread = pm.mate_account_get(&account.id).unwrap().unwrap();
        assert_eq!(reread.mate_token.as_deref(), Some("tok"));

        assert!(pm
            .mate_account_record_login("ghost", "u", "n", None, &tokens, 0)
            .is_err());
    }

    fn sample_tokens() -> MateLoginTokens {
        MateLoginTokens {
            pass_token: Some("pass".into()),
            token: Some("tok".into()),
            lmtoken: Some("lm".into()),
            mate_st: Some("st".into()),
            mate_h5_st: Some("h5".into()),
        }
    }

    #[test]
    fn add_none_uses_placeholder_label() {
        let (_dir, pm) = open();
        let a = pm.mate_account_add(None).unwrap();
        assert_eq!(a.label, MATE_PLACEHOLDER_LABEL);
        assert_eq!(a.label, "未命名伴侣");
        assert!(a.login_at.is_none());
        assert!(a.platform_user_id.is_none());
    }

    #[test]
    fn add_blank_label_is_rejected() {
        let (_dir, pm) = open();
        assert!(pm.mate_account_add(Some("   ")).is_err());
    }

    #[test]
    fn first_login_overwrites_placeholder_label() {
        let (_dir, pm) = open();
        let account = pm.mate_account_add(None).unwrap();
        let saved = pm
            .mate_account_record_login(
                &account.id,
                "u1001",
                "测试主播",
                Some("http://example.com/a.png"),
                &sample_tokens(),
                1_700_000_000_000,
            )
            .unwrap();
        assert_eq!(saved.label, "测试主播");
        assert_eq!(saved.platform_user_id.as_deref(), Some("u1001"));
        assert_eq!(saved.user_name.as_deref(), Some("测试主播"));
        assert_eq!(saved.mate_token.as_deref(), Some("tok"));
        assert_eq!(saved.login_at, Some(1_700_000_000_000));

        // 落库后再次读回仍是平台昵称。
        let reread = pm.mate_account_get(&account.id).unwrap().unwrap();
        assert_eq!(reread.label, "测试主播");
    }

    #[test]
    fn second_login_does_not_overwrite_label() {
        let (_dir, pm) = open();
        let account = pm.mate_account_add(None).unwrap();
        pm.mate_account_record_login(
            &account.id,
            "u1001",
            "测试主播",
            None,
            &sample_tokens(),
            1_700_000_000_000,
        )
        .unwrap();
        let second = pm
            .mate_account_record_login(
                &account.id,
                "u1001",
                "新昵称",
                None,
                &sample_tokens(),
                1_700_000_100_000,
            )
            .unwrap();
        assert_eq!(second.label, "测试主播");
        assert_eq!(second.user_name.as_deref(), Some("新昵称"));
        assert_eq!(second.login_at, Some(1_700_000_100_000));
    }

    #[test]
    fn first_login_with_blank_name_keeps_placeholder() {
        let (_dir, pm) = open();
        let account = pm.mate_account_add(None).unwrap();
        let saved = pm
            .mate_account_record_login(&account.id, "u1001", "   ", None, &sample_tokens(), 1)
            .unwrap();
        assert_eq!(saved.label, MATE_PLACEHOLDER_LABEL);
        assert_eq!(saved.login_at, Some(1));
    }

    #[test]
    fn rename_then_relogin_keeps_user_label() {
        let (_dir, pm) = open();
        let account = pm.mate_account_add(None).unwrap();
        pm.mate_account_record_login(
            &account.id,
            "u1001",
            "测试主播",
            None,
            &sample_tokens(),
            1_700_000_000_000,
        )
        .unwrap();
        let renamed = pm.mate_account_rename(&account.id, "我的主播号").unwrap();
        assert_eq!(renamed.label, "我的主播号");
        let relogin = pm
            .mate_account_record_login(
                &account.id,
                "u1001",
                "平台新昵称",
                None,
                &sample_tokens(),
                1_700_000_100_000,
            )
            .unwrap();
        assert_eq!(relogin.label, "我的主播号");
    }
}
