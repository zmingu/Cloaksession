//! 磁力金牛大户状态表（`jinniu_account_state`）。
//!
//! 账号即环境：一个大户 = 一行 `business_accounts(kind='jinniu')` + 一个专属
//! Profile。通用业务账号表只承载 id/kind/display_name/profile_id 等公共字段；
//! 金牛特有的派生缓存（主账号名/ID/头像、上次进入的子户）放进本侧表，避免污染
//! 通用表结构（`business_accounts` 被小店/直播/伴侣/小号共用）。
//!
//! 单活跃约束：`is_active` 上的部分唯一索引在数据库层强制「同时最多一个大户
//! 活跃」，与运行时「同时只允许一个金牛会话存活」互为兜底。
use crate::ProfileManager;
use multizen_core::{BusinessAccount, BusinessAccountKind, MultizenError, Result};
use rusqlite::{params, Connection, OptionalExtension};

pub(crate) fn migrate(conn: &Connection) -> Result<()> {
    let tx = conn.unchecked_transaction()?;
    tx.execute_batch(
        "CREATE TABLE IF NOT EXISTS jinniu_account_state (
            business_account_id TEXT PRIMARY KEY NOT NULL
                REFERENCES business_accounts(id) ON DELETE CASCADE,
            master_name TEXT,
            master_id TEXT,
            master_avatar_url TEXT,
            last_sub_account_id TEXT,
            last_sub_account_name TEXT,
            is_active INTEGER NOT NULL DEFAULT 0 CHECK(is_active IN (0,1)),
            updated_at TEXT NOT NULL
        );
        CREATE UNIQUE INDEX IF NOT EXISTS idx_jinniu_account_active
            ON jinniu_account_state(is_active) WHERE is_active = 1;",
    )?;
    tx.commit()?;
    Ok(())
}

/// 金牛大户的派生状态缓存（jieger `JinniuAccount` 中除 id/label/storageStatePath
/// 之外的字段）。`storageStatePath` 在 Cloaksession 中等价于该大户专属 Profile 的
/// 数据目录（账号即环境），因此不单独存列。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JinniuAccountState {
    pub business_account_id: String,
    pub master_name: Option<String>,
    pub master_id: Option<String>,
    pub master_avatar_url: Option<String>,
    pub last_sub_account_id: Option<String>,
    pub last_sub_account_name: Option<String>,
    pub is_active: bool,
}

/// 一个金牛大户 = 通用业务账号行 + 金牛状态行。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JinniuAccountRecord {
    pub account: BusinessAccount,
    pub state: JinniuAccountState,
}

fn now() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

fn invalid(message: impl Into<String>) -> MultizenError {
    MultizenError::Config(message.into())
}

fn state_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<JinniuAccountState> {
    Ok(JinniuAccountState {
        business_account_id: r.get(0)?,
        master_name: r.get(1)?,
        master_id: r.get(2)?,
        master_avatar_url: r.get(3)?,
        last_sub_account_id: r.get(4)?,
        last_sub_account_name: r.get(5)?,
        is_active: r.get::<_, i64>(6)? != 0,
    })
}

impl ProfileManager {
    /// 所有金牛大户（含未创建状态行的历史行），按创建时间升序，供 UI 列表使用。
    pub fn jinniu_accounts_list(&self) -> Result<Vec<JinniuAccountRecord>> {
        let mut stmt = self.conn.prepare(
            "SELECT ba.id, ba.display_name, ba.platform_user_id, ba.profile_id,
                    ba.created_at, ba.updated_at,
                    s.master_name, s.master_id, s.master_avatar_url,
                    s.last_sub_account_id, s.last_sub_account_name, COALESCE(s.is_active, 0)
             FROM business_accounts ba
             LEFT JOIN jinniu_account_state s ON s.business_account_id = ba.id
             WHERE ba.kind = 'jinniu'
             ORDER BY ba.created_at, ba.id",
        )?;
        let rows = stmt.query_map([], |r| {
            let id: String = r.get(0)?;
            Ok(JinniuAccountRecord {
                account: BusinessAccount {
                    id: id.clone(),
                    kind: BusinessAccountKind::Jinniu,
                    display_name: r.get(1)?,
                    platform_user_id: r.get(2)?,
                    profile_id: r.get(3)?,
                    created_at: r.get(4)?,
                    updated_at: r.get(5)?,
                },
                state: JinniuAccountState {
                    business_account_id: id,
                    master_name: r.get(6)?,
                    master_id: r.get(7)?,
                    master_avatar_url: r.get(8)?,
                    last_sub_account_id: r.get(9)?,
                    last_sub_account_name: r.get(10)?,
                    is_active: r.get::<_, i64>(11)? != 0,
                },
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn jinniu_account_state(&self, id: &str) -> Result<Option<JinniuAccountState>> {
        Ok(self
            .conn
            .query_row(
                "SELECT business_account_id, master_name, master_id, master_avatar_url,
                        last_sub_account_id, last_sub_account_name, is_active
                 FROM jinniu_account_state WHERE business_account_id = ?",
                [id],
                state_row,
            )
            .optional()?)
    }

    /// 确保金牛状态行存在（新建大户后调用）。拒绝把非金牛业务账号写入本表。
    pub fn jinniu_account_ensure_state(&self, id: &str) -> Result<()> {
        let kind: Option<String> = self
            .conn
            .query_row("SELECT kind FROM business_accounts WHERE id = ?", [id], |r| {
                r.get(0)
            })
            .optional()?;
        match kind.as_deref() {
            Some("jinniu") => {}
            Some(_) => return Err(invalid(format!("业务账号 {id} 不是金牛大户，已拒绝写入金牛状态"))),
            None => return Err(MultizenError::NotFound(format!("金牛大户 {id} 不存在"))),
        }
        self.conn.execute(
            "INSERT INTO jinniu_account_state(business_account_id, is_active, updated_at)
             VALUES (?, 0, ?) ON CONFLICT(business_account_id) DO NOTHING",
            params![id, now()],
        )?;
        Ok(())
    }

    /// 登录后捕获到主账号信息时回写缓存。头像仅在传入非空时覆盖（jieger 语义）。
    pub fn jinniu_account_update_master(
        &self,
        id: &str,
        master_name: &str,
        master_id: &str,
        master_avatar_url: Option<&str>,
    ) -> Result<()> {
        self.jinniu_account_ensure_state(id)?;
        self.conn.execute(
            "UPDATE jinniu_account_state
             SET master_name = ?, master_id = ?,
                 master_avatar_url = COALESCE(?, master_avatar_url), updated_at = ?
             WHERE business_account_id = ?",
            params![master_name, master_id, master_avatar_url, now(), id],
        )?;
        Ok(())
    }

    /// URL 出现 `__accountId__` 时记录子户 ID；`sub_account_name` 为空时保留旧名。
    pub fn jinniu_account_update_sub(
        &self,
        id: &str,
        sub_account_id: &str,
        sub_account_name: Option<&str>,
    ) -> Result<()> {
        self.jinniu_account_ensure_state(id)?;
        self.conn.execute(
            "UPDATE jinniu_account_state
             SET last_sub_account_id = ?,
                 last_sub_account_name = COALESCE(?, last_sub_account_name), updated_at = ?
             WHERE business_account_id = ?",
            params![sub_account_id, sub_account_name, now(), id],
        )?;
        Ok(())
    }

    /// 当前活跃大户 ID（0 或 1 个，由部分唯一索引保证）。
    pub fn jinniu_account_active(&self) -> Result<Option<String>> {
        Ok(self
            .conn
            .query_row(
                "SELECT business_account_id FROM jinniu_account_state WHERE is_active = 1",
                [],
                |r| r.get(0),
            )
            .optional()?)
    }

    /// 单选切换：先清旧活跃，再设新活跃（同一事务，避免短暂违反唯一索引）。
    /// `None` 清空活跃指针。
    pub fn jinniu_account_set_active(&self, id: Option<&str>) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        match id {
            None => {
                tx.execute(
                    "UPDATE jinniu_account_state SET is_active = 0, updated_at = ? WHERE is_active = 1",
                    params![now()],
                )?;
            }
            Some(id) => {
                let exists: bool = tx.query_row(
                    "SELECT EXISTS(SELECT 1 FROM business_accounts WHERE id = ? AND kind = 'jinniu')",
                    [id],
                    |r| r.get(0),
                )?;
                if !exists {
                    return Err(MultizenError::NotFound(format!(
                        "金牛大户 {id} 不存在，无法设为活跃"
                    )));
                }
                tx.execute(
                    "INSERT INTO jinniu_account_state(business_account_id, is_active, updated_at)
                     VALUES (?, 0, ?) ON CONFLICT(business_account_id) DO NOTHING",
                    params![id, now()],
                )?;
                tx.execute(
                    "UPDATE jinniu_account_state SET is_active = 0, updated_at = ?
                     WHERE is_active = 1 AND business_account_id <> ?",
                    params![now(), id],
                )?;
                tx.execute(
                    "UPDATE jinniu_account_state SET is_active = 1, updated_at = ?
                     WHERE business_account_id = ?",
                    params![now(), id],
                )?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// 若当前无活跃大户，则把第一个（最早上架）大户设为活跃。用于删除活跃大户后的补位。
    pub fn jinniu_account_ensure_active(&self) -> Result<Option<String>> {
        if let Some(active) = self.jinniu_account_active()? {
            return Ok(Some(active));
        }
        let next: Option<String> = self
            .conn
            .query_row(
                "SELECT business_account_id FROM jinniu_account_state ORDER BY rowid LIMIT 1",
                [],
                |r| r.get(0),
            )
            .optional()?;
        if let Some(id) = next.as_deref() {
            self.jinniu_account_set_active(Some(id))?;
        }
        Ok(next)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use multizen_core::SaveBusinessAccountInput;

    fn open() -> (tempfile::TempDir, ProfileManager) {
        let dir = tempfile::tempdir().unwrap();
        let pm = ProfileManager::new(&dir.path().join("p.db"), &dir.path().join("profiles")).unwrap();
        (dir, pm)
    }

    fn profile(pm: &ProfileManager, name: &str) -> String {
        pm.create(multizen_core::CreateProfileInput {
            name: name.into(),
            ..Default::default()
        })
        .unwrap()
        .id
    }

    fn add(pm: &ProfileManager, name: &str) -> String {
        let pid = profile(pm, name);
        let account = pm
            .business_accounts_save(SaveBusinessAccountInput {
                id: None,
                profile_id: pid,
                kind: BusinessAccountKind::Jinniu,
                display_name: name.into(),
                platform_user_id: None,
            })
            .unwrap();
        pm.jinniu_account_ensure_state(&account.id).unwrap();
        account.id
    }

    #[test]
    fn migration_is_idempotent_and_active_index_is_unique() {
        let conn = Connection::open_in_memory().unwrap();
        conn.pragma_update(None, "foreign_keys", "ON").unwrap();
        conn.execute_batch(
            "CREATE TABLE business_accounts (
                id TEXT PRIMARY KEY NOT NULL,
                kind TEXT NOT NULL,
                display_name TEXT NOT NULL,
                platform_user_id TEXT,
                profile_id TEXT,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL);",
        )
        .unwrap();
        migrate(&conn).unwrap();
        migrate(&conn).unwrap();
        conn.execute_batch(
            "INSERT INTO business_accounts VALUES('a','jinniu','A',NULL,NULL,'t','t');
             INSERT INTO business_accounts VALUES('b','jinniu','B',NULL,NULL,'t','t');",
        )
        .unwrap();
        conn.execute(
            "INSERT INTO jinniu_account_state(business_account_id,is_active,updated_at) VALUES('a',1,'t')",
            [],
        )
        .unwrap();
        // 第二个 is_active=1 必须被部分唯一索引拒绝。
        assert!(conn
            .execute(
                "INSERT INTO jinniu_account_state(business_account_id,is_active,updated_at) VALUES('b',1,'t')",
                [],
            )
            .is_err());
        // 多个 is_active=0 允许。
        conn.execute(
            "INSERT INTO jinniu_account_state(business_account_id,is_active,updated_at) VALUES('b',0,'t')",
            [],
        )
        .unwrap();
    }

    #[test]
    fn crud_single_active_and_master_sub_cache() {
        let (_dir, pm) = open();
        let a = add(&pm, "大户甲");
        let b = add(&pm, "大户乙");
        let listed = pm.jinniu_accounts_list().unwrap();
        assert_eq!(listed.len(), 2);
        assert!(listed.iter().all(|r| !r.state.is_active));

        // 单选切换：设置 b 活跃后 a 自动失活。
        pm.jinniu_account_set_active(Some(&a)).unwrap();
        assert_eq!(pm.jinniu_account_active().unwrap().as_deref(), Some(a.as_str()));
        pm.jinniu_account_set_active(Some(&b)).unwrap();
        assert_eq!(pm.jinniu_account_active().unwrap().as_deref(), Some(b.as_str()));
        assert!(pm
            .jinniu_accounts_list()
            .unwrap()
            .iter()
            .filter(|r| r.state.is_active)
            .count()
            == 1);

        // 状态缓存：主账号 + 子户。
        pm.jinniu_account_update_master(&a, "甲主号", "1001", Some("http://a/x.png"))
            .unwrap();
        pm.jinniu_account_update_master(&a, "甲主号", "1001", None).unwrap();
        pm.jinniu_account_update_sub(&a, "sub-9", None).unwrap();
        let state = pm.jinniu_account_state(&a).unwrap().unwrap();
        assert_eq!(state.master_name.as_deref(), Some("甲主号"));
        assert_eq!(state.master_id.as_deref(), Some("1001"));
        assert_eq!(state.master_avatar_url.as_deref(), Some("http://a/x.png"));
        assert_eq!(state.last_sub_account_id.as_deref(), Some("sub-9"));

        // 清空活跃。
        pm.jinniu_account_set_active(None).unwrap();
        assert!(pm.jinniu_account_active().unwrap().is_none());
        // 补位：ensure_active 选第一个。
        assert_eq!(
            pm.jinniu_account_ensure_active().unwrap().as_deref(),
            Some(a.as_str())
        );
    }

    #[test]
    fn set_active_rejects_unknown_and_non_jinniu() {
        let (_dir, pm) = open();
        assert!(pm.jinniu_account_set_active(Some("ghost")).is_err());
        let pid = profile(&pm, "shop");
        let shop = pm
            .business_accounts_save(SaveBusinessAccountInput {
                id: None,
                profile_id: pid,
                kind: BusinessAccountKind::KuaishouShop,
                display_name: "Shop".into(),
                platform_user_id: None,
            })
            .unwrap();
        assert!(pm.jinniu_account_set_active(Some(&shop.id)).is_err());
        assert!(pm.jinniu_account_ensure_state(&shop.id).is_err());
        assert!(pm.jinniu_accounts_list().unwrap().is_empty());
    }
}
