//! Platform identity archives are independent from business_accounts and survive profile deletion.
use crate::ProfileManager;
use multizen_core::{
    kuaishou_identity_allowed, valid_kuaishou_user_id, BusinessProfileState,
    KuaishouIdentityObservation as Observation, KuaishouIdentityStatus as Status, MultizenError,
    Result,
};
use rusqlite::{params, Connection, OptionalExtension};

pub(crate) fn migrate(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS kuaishou_identities (
        platform_user_id TEXT PRIMARY KEY NOT NULL,
        nickname TEXT, avatar_key TEXT, avatar_url TEXT, last_seen_at TEXT NOT NULL
    );
    CREATE TABLE IF NOT EXISTS kuaishou_identity_observations (
        profile_id TEXT PRIMARY KEY NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,
        observation TEXT NOT NULL CHECK(json_valid(observation))
    );",
    )?;
    Ok(())
}

fn read(conn: &Connection, id: &str) -> Result<Option<Observation>> {
    let raw: Option<String> = conn
        .query_row(
            "SELECT observation FROM kuaishou_identity_observations WHERE profile_id=?",
            [id],
            |r| r.get(0),
        )
        .optional()?;
    raw.map(|s| serde_json::from_str(&s).map_err(Into::into))
        .transpose()
}

impl ProfileManager {
    pub fn kuaishou_identity_observations(&self) -> Result<Vec<Observation>> {
        let mut stmt = self.conn.prepare(
            "SELECT observation FROM kuaishou_identity_observations ORDER BY profile_id",
        )?;
        let raw = stmt
            .query_map([], |r| r.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        raw.into_iter()
            .map(|s| serde_json::from_str(&s).map_err(Into::into))
            .collect()
    }

    pub fn kuaishou_identity_observation(&self, profile_id: &str) -> Result<Option<Observation>> {
        read(&self.conn, profile_id)
    }

    /// Caller also holds the current registry session lease through this synchronous transaction.
    pub fn kuaishou_identity_save(
        &self,
        mut observation: Observation,
        expected_business: &BusinessProfileState,
    ) -> Result<Option<Observation>> {
        let tx = self.conn.unchecked_transaction()?;
        let id = &observation.snapshot.profile_id;
        let exists: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM profiles WHERE id=?)",
            [id],
            |r| r.get(0),
        )?;
        if !exists {
            return Ok(None);
        }
        // Same connection: this read is inside the transaction. No stale unbind/rebind result wins.
        let current = self.business_accounts_profile_state(id)?;
        if &current != expected_business {
            return Ok(None);
        }
        if !kuaishou_identity_allowed(&current) {
            observation.snapshot.status = Status::Skipped;
            observation.snapshot.message = Some("金牛scope或非小店业务绑定，已跳过身份检测".into());
        }
        if observation.snapshot.status == Status::Detected {
            let platform_id = observation
                .snapshot
                .platform_user_id
                .as_deref()
                .ok_or_else(|| MultizenError::Config("detected observation has no ID".into()))?;
            if !valid_kuaishou_user_id(platform_id)
                || observation.session_id.as_deref().is_none_or(str::is_empty)
            {
                return Err(MultizenError::Config("invalid identity observation".into()));
            }
            if current
                .account
                .as_ref()
                .and_then(|a| a.platform_user_id.as_deref())
                .is_some_and(|manual| manual != platform_id)
            {
                observation.snapshot.status = Status::Conflict;
                observation.snapshot.message =
                    Some("页面ID与手工登记平台ID不一致；未修改人工档案或绑定".into());
            }
        }
        let old = read(&tx, id)?;
        if observation.snapshot.status != Status::Detected {
            observation.snapshot.platform_user_id = old
                .as_ref()
                .and_then(|o| o.snapshot.platform_user_id.clone());
            observation.snapshot.nickname = old.as_ref().and_then(|o| o.snapshot.nickname.clone());
            observation.snapshot.avatar_key =
                old.as_ref().and_then(|o| o.snapshot.avatar_key.clone());
            observation.snapshot.last_seen_at =
                old.as_ref().and_then(|o| o.snapshot.last_seen_at.clone());
            observation.avatar_url = old.and_then(|o| o.avatar_url);
        } else {
            observation.snapshot.last_seen_at = observation.snapshot.checked_at.clone();
            if observation.snapshot.checked_at.is_none() {
                return Err(MultizenError::Config(
                    "identity observation missing time".into(),
                ));
            }
            if observation.snapshot.avatar_key.is_none() {
                observation.avatar_url = None;
            }
            tx.execute("INSERT INTO kuaishou_identities(platform_user_id,nickname,avatar_key,avatar_url,last_seen_at) VALUES (?,?,?,?,?) ON CONFLICT(platform_user_id) DO UPDATE SET nickname=excluded.nickname,avatar_key=COALESCE(excluded.avatar_key,kuaishou_identities.avatar_key),avatar_url=COALESCE(excluded.avatar_url,kuaishou_identities.avatar_url),last_seen_at=excluded.last_seen_at",
                params![observation.snapshot.platform_user_id,observation.snapshot.nickname,observation.snapshot.avatar_key,observation.avatar_url,observation.snapshot.last_seen_at])?;
        }
        tx.execute("INSERT INTO kuaishou_identity_observations(profile_id,observation) VALUES (?,?) ON CONFLICT(profile_id) DO UPDATE SET observation=excluded.observation",params![id,serde_json::to_string(&observation)?])?;
        tx.commit()?;
        Ok(Some(observation))
    }

    pub fn kuaishou_identity_avatar_referenced(&self, key: &str) -> Result<bool> {
        Ok(self.conn.query_row("SELECT EXISTS(SELECT 1 FROM kuaishou_identities WHERE avatar_key=? UNION ALL SELECT 1 FROM kuaishou_identity_observations WHERE json_extract(observation,'$.snapshot.avatarKey')=?)",params![key,key],|r| r.get(0))?)
    }
}
