//! Account rows and persistent cookie-scope reservations share the profile connection.
//! Liveness and effective browser-directory checks belong to the launcher-thread caller.
use crate::ProfileManager;
use multizen_core::{
    BusinessAccount, BusinessAccountKind, BusinessProfileScope, BusinessProfileState,
    MultizenError, Result, SaveBusinessAccountInput,
};
use rusqlite::{params, Connection, OptionalExtension};

pub(crate) fn migrate(conn: &Connection) -> Result<()> {
    let tx = conn.unchecked_transaction()?;
    tx.execute_batch(
        "CREATE TABLE IF NOT EXISTS business_accounts (
            id TEXT PRIMARY KEY NOT NULL,
            kind TEXT NOT NULL CHECK(kind IN ('kuaishou-shop','kuaishou-live','kuaishou-mate','kuaishou-sub','jinniu')),
            display_name TEXT NOT NULL CHECK(length(trim(display_name)) BETWEEN 1 AND 100),
            platform_user_id TEXT CHECK(platform_user_id IS NULL OR length(platform_user_id) BETWEEN 1 AND 128),
            profile_id TEXT UNIQUE REFERENCES profiles(id) ON DELETE SET NULL,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );
        CREATE UNIQUE INDEX IF NOT EXISTS idx_business_kind_platform_id
            ON business_accounts(kind, platform_user_id) WHERE platform_user_id IS NOT NULL;
        CREATE TABLE IF NOT EXISTS business_profile_scopes (
            profile_id TEXT PRIMARY KEY NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,
            scope TEXT NOT NULL CHECK(scope IN ('jinniu','kuaishou'))
        );"
    )?;
    tx.commit()?;
    Ok(())
}

fn config(message: impl Into<String>) -> MultizenError {
    MultizenError::Config(message.into())
}

fn decode<T: serde::de::DeserializeOwned>(text: String) -> rusqlite::Result<T> {
    serde_json::from_value(serde_json::Value::String(text)).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
    })
}

fn row(r: &rusqlite::Row<'_>) -> rusqlite::Result<BusinessAccount> {
    Ok(BusinessAccount {
        id: r.get(0)?,
        kind: decode::<BusinessAccountKind>(r.get(1)?)?,
        display_name: r.get(2)?,
        platform_user_id: r.get(3)?,
        profile_id: r.get(4)?,
        created_at: r.get(5)?,
        updated_at: r.get(6)?,
    })
}

const PROJECTION: &str = "SELECT id,kind,display_name,platform_user_id,profile_id,created_at,updated_at FROM business_accounts";

fn get(conn: &Connection, id: &str) -> Result<Option<BusinessAccount>> {
    Ok(conn
        .query_row(&format!("{PROJECTION} WHERE id=?"), [id], row)
        .optional()?)
}

fn scope(conn: &Connection, profile_id: &str) -> Result<Option<BusinessProfileScope>> {
    Ok(conn
        .query_row(
            "SELECT scope FROM business_profile_scopes WHERE profile_id=?",
            [profile_id],
            |r| decode(r.get(0)?),
        )
        .optional()?)
}

fn require_profile(conn: &Connection, id: &str) -> Result<()> {
    let exists: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM profiles WHERE id=?)",
        [id],
        |r| r.get(0),
    )?;
    if !exists {
        return Err(MultizenError::NotFound(format!("Profile {id}")));
    }
    Ok(())
}

impl ProfileManager {
    pub fn business_accounts_list(&self) -> Result<Vec<BusinessAccount>> {
        let mut stmt = self
            .conn
            .prepare(&format!("{PROJECTION} ORDER BY updated_at DESC,id"))?;
        let rows = stmt.query_map([], row)?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn business_account_get(&self, id: &str) -> Result<Option<BusinessAccount>> {
        get(&self.conn, id)
    }

    pub fn business_profile_scope(&self, profile_id: &str) -> Result<Option<BusinessProfileScope>> {
        scope(&self.conn, profile_id)
    }

    pub fn business_accounts_profile_state(
        &self,
        profile_id: &str,
    ) -> Result<BusinessProfileState> {
        require_profile(&self.conn, profile_id)?;
        let account = self
            .conn
            .query_row(
                &format!("{PROJECTION} WHERE profile_id=?"),
                [profile_id],
                row,
            )
            .optional()?;
        Ok(BusinessProfileState {
            account,
            scope: scope(&self.conn, profile_id)?,
        })
    }

    /// Atomic metadata + binding + reservation write. Never clears any browser data.
    pub fn business_accounts_save(
        &self,
        input: SaveBusinessAccountInput,
    ) -> Result<BusinessAccount> {
        let name = input.display_name.trim();
        if name.is_empty() || name.chars().count() > 100 || name.chars().any(char::is_control) {
            return Err(config("账号别名不能为空，最多100个字符，且不能含控制字符"));
        }
        let platform_id = input
            .platform_user_id
            .as_deref()
            .map(str::trim)
            .filter(|v| !v.is_empty());
        if platform_id.is_some_and(|v| v.chars().count() > 128 || v.chars().any(char::is_control)) {
            return Err(config("平台ID最多128个字符，且不能含控制字符"));
        }
        let tx = self.conn.unchecked_transaction()?;
        require_profile(&tx, &input.profile_id)?;
        let existing = match input.id.as_deref() {
            Some(id) => Some(get(&tx, id)?.ok_or_else(|| {
                MultizenError::NotFound(format!(
                    "业务账号 {id} 不存在，请刷新后选择原档案；新建请省略id或使用null"
                ))
            })?),
            None => None,
        };
        if let Some(account) = &existing {
            if account.kind != input.kind {
                return Err(config("已有账号的业务类型不能修改；请解绑后另建记录"));
            }
            if account
                .profile_id
                .as_deref()
                .is_some_and(|id| id != input.profile_id)
            {
                return Err(config(
                    "账号已绑定其他Profile，请先显式解绑；不会自动移动账号",
                ));
            }
        }
        let id = existing
            .as_ref()
            .map(|a| a.id.clone())
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        let occupied: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM business_accounts WHERE profile_id=? AND id<>?)",
            params![input.profile_id, id],
            |r| r.get(0),
        )?;
        if occupied {
            return Err(config("此Profile已绑定业务账号，不能覆盖；请先显式解绑"));
        }
        let duplicate: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM business_accounts WHERE kind=? AND platform_user_id=? AND id<>?)",
            params![input.kind.as_str(), platform_id, id], |r| r.get(0))?;
        if duplicate {
            return Err(config(
                "该业务类型的平台ID已登记，请选择原记录编辑或重新绑定；不会覆盖已有档案",
            ));
        }
        let wanted = input.kind.scope();
        if scope(&tx, &input.profile_id)?.is_some_and(|v| v != wanted) {
            return Err(config(
                "Profile保留了其他业务scope及Cookie，不能跨scope绑定；请创建独立Profile",
            ));
        }
        let now = chrono::Utc::now().to_rfc3339();
        let created = existing
            .as_ref()
            .map(|a| a.created_at.clone())
            .unwrap_or_else(|| now.clone());
        tx.execute("INSERT INTO business_profile_scopes(profile_id,scope) VALUES (?,?) ON CONFLICT(profile_id) DO NOTHING",
            params![input.profile_id,wanted.as_str()])?;
        if existing.is_some() {
            tx.execute("UPDATE business_accounts SET display_name=?,platform_user_id=?,profile_id=?,updated_at=? WHERE id=?",
                params![name,platform_id,input.profile_id,now,id])?;
        } else {
            tx.execute("INSERT INTO business_accounts(id,kind,display_name,platform_user_id,profile_id,created_at,updated_at) VALUES (?,?,?,?,?,?,?)",
                params![id,input.kind.as_str(),name,platform_id,input.profile_id,created,now])?;
        }
        let account = get(&tx, &id)?.ok_or_else(|| config("账号写入后未找到记录"))?;
        tx.commit()?;
        Ok(account)
    }

    pub fn business_accounts_unbind(&self, id: &str) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        let account =
            get(&tx, id)?.ok_or_else(|| MultizenError::NotFound(format!("业务账号 {id}")))?;
        if account.profile_id.is_some() {
            tx.execute(
                "UPDATE business_accounts SET profile_id=NULL,updated_at=? WHERE id=?",
                params![chrono::Utc::now().to_rfc3339(), id],
            )?;
        }
        tx.commit()?;
        Ok(())
    }
}
