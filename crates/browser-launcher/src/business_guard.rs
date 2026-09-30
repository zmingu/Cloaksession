use crate::{
    data_dir::{effective_data_dir, verify_data_dir, VerifiedDataDir},
    BrowserLauncher,
};
use multizen_core::{
    BrowserEngine, BusinessAccount, BusinessProfileScope, ChromixSettings, MultizenError, Profile,
    Result, SaveBusinessAccountInput, UpdateProfileInput,
};

fn conflict(id: &str) -> MultizenError {
    MultizenError::Config(format!(
        "金牛独立环境的数据目录与Profile {id} 重叠，请配置不重叠的独立userDataDir"
    ))
}

impl BrowserLauncher {
    pub async fn require_stopped(&self, profile_id: &str) -> Result<()> {
        if self.is_running_async(profile_id).await {
            return Err(MultizenError::Config(
                "请先关闭Profile；运行中不能绑定、编辑或解绑业务账号".into(),
            ));
        }
        Ok(())
    }

    /// Called on the serialized launcher thread, before mutations or ANY launch side effects.
    /// `global` is the startup settings snapshot, NOT a config already merged for another profile.
    pub async fn validate_business_directory(
        &self,
        candidate: &Profile,
        engine: BrowserEngine,
        global: &ChromixSettings,
        proposed_scope: Option<BusinessProfileScope>,
    ) -> Result<()> {
        let own_scope = proposed_scope.or(self.pm.business_profile_scope(&candidate.id)?);
        let is_jinniu = own_scope == Some(BusinessProfileScope::Jinniu);
        let profiles = self.pm.list()?;
        let mut peers = Vec::new();
        for item in profiles {
            if item.id == candidate.id {
                continue;
            }
            if is_jinniu
                || self.pm.business_profile_scope(&item.id)? == Some(BusinessProfileScope::Jinniu)
            {
                if let Some(profile) = self.pm.get(&item.id)? {
                    peers.push(profile);
                }
            }
        }
        let mut live = Vec::new();
        for id in self.registry.ids().await {
            if id == candidate.id {
                continue;
            }
            if let Some(snapshot) = self
                .registry
                .with(&id, |h| {
                    (
                        h.data_dir.clone(),
                        h.verified_data_dir.clone(),
                        h.business_scope,
                    )
                })
                .await
            {
                if is_jinniu || snapshot.2 == Some(BusinessProfileScope::Jinniu) {
                    live.push((id, snapshot));
                }
            }
        }
        // Ordinary profiles keep their previous path behavior unless a Jinniu reservation
        // or live Jinniu directory actually needs protecting.
        if !is_jinniu && peers.is_empty() && live.is_empty() {
            return Ok(());
        }
        let config = global.with_profile_options(&candidate.chromix_options);
        let path = verify_data_dir(&effective_data_dir(candidate, engine, &config)?)?;
        for peer in peers {
            let config = global.with_profile_options(&peer.chromix_options);
            let other = verify_data_dir(&effective_data_dir(&peer, engine, &config)?)?;
            if path.overlaps(&other) {
                return Err(conflict(&peer.id));
            }
        }
        // A live handle retains both initial canonical identity and the original path. Compare
        // both so neither metadata edits nor an observed symlink remap hides the occupied tree.
        for (id, (raw, original, _)) in live {
            let original = original.ok_or_else(|| {
                MultizenError::Config(format!(
                    "无法验证运行中Profile {id} 的原始目录，请关闭该Profile后重试"
                ))
            })?;
            if path.overlaps(&original) || path.overlaps(&verify_data_dir(&raw)?) {
                return Err(conflict(&id));
            }
        }
        Ok(())
    }

    pub async fn save_business_account(
        &self,
        input: SaveBusinessAccountInput,
        engine: BrowserEngine,
        global: &ChromixSettings,
    ) -> Result<BusinessAccount> {
        self.require_stopped(&input.profile_id).await?;
        if let Some(id) = input.id.as_deref() {
            if let Some(account) = self.pm.business_account_get(id)? {
                if let Some(bound) = account.profile_id {
                    self.require_stopped(&bound).await?;
                }
            }
        }
        let profile = self
            .pm
            .get(&input.profile_id)?
            .ok_or_else(|| MultizenError::NotFound(input.profile_id.clone()))?;
        self.validate_business_directory(&profile, engine, global, Some(input.kind.scope()))
            .await?;
        self.pm.business_accounts_save(input)
    }

    pub async fn unbind_business_account(&self, id: &str) -> Result<()> {
        let account = self
            .pm
            .business_account_get(id)?
            .ok_or_else(|| MultizenError::NotFound(id.into()))?;
        if let Some(profile_id) = account.profile_id {
            self.require_stopped(&profile_id).await?;
        }
        self.pm.business_accounts_unbind(id)
    }

    pub async fn update_profile_guarded(
        &self,
        id: &str,
        patch: UpdateProfileInput,
        engine: BrowserEngine,
        global: &ChromixSettings,
    ) -> Result<Profile> {
        if let Some(options) = &patch.chromix_options {
            let mut candidate = self
                .pm
                .get(id)?
                .ok_or_else(|| MultizenError::NotFound(id.into()))?;
            // Same-options autosaves and all non-directory metadata edits remain usable.
            if candidate.chromix_options != *options {
                let old_options = candidate.chromix_options.clone();
                candidate.chromix_options = options.clone();
                if self.pm.business_profile_scope(id)?.is_some() && self.is_running_async(id).await
                {
                    let old_config = global.with_profile_options(&old_options);
                    let old =
                        verify_data_dir(&effective_data_dir(&candidate, engine, &old_config)?)?;
                    let new_config = global.with_profile_options(options);
                    let new =
                        verify_data_dir(&effective_data_dir(&candidate, engine, &new_config)?)?;
                    let live: Option<Option<VerifiedDataDir>> = self
                        .registry
                        .with(id, |h| h.verified_data_dir.clone())
                        .await;
                    if !old.same_directory(&new)
                        || !live.flatten().is_some_and(|v| v.same_directory(&new))
                    {
                        return Err(MultizenError::Config(
                            "持有业务scope的Profile正在运行，不能改变有效数据目录；请先关闭".into(),
                        ));
                    }
                }
                self.validate_business_directory(&candidate, engine, global, None)
                    .await?;
            }
        }
        self.pm.update(id, patch)
    }
}
