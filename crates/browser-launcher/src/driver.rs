use std::path::{Path, PathBuf};
use std::sync::Arc;

use multizen_core::{BrowserEngine, ChromixSettings, LaunchedProfile, MultizenError, Result};
use profile_manager::ProfileManager;

use crate::registry::RunningRegistry;

pub struct BrowserHandle {
    pub profile_id: String,
    pub cdp_endpoint: String,
    pub pid: u32,
    pub started_at: String,
    // Raw path plus canonical identity at launch time: later DB edits must not hide live cookies.
    pub(crate) data_dir: PathBuf,
    pub(crate) verified_data_dir: Option<crate::data_dir::VerifiedDataDir>,
    pub(crate) business_scope: Option<multizen_core::BusinessProfileScope>,
    chromix: Option<crate::chromix::ChromixProcess>,
}

impl BrowserHandle {
    pub(crate) fn is_alive(&self) -> bool {
        self.chromix
            .as_ref()
            .is_some_and(|process| process.is_alive())
    }

    pub fn endpoint_info(&self) -> (String, String, u32) {
        (self.profile_id.clone(), self.cdp_endpoint.clone(), self.pid)
    }
}

pub struct BrowserLauncher {
    pub(crate) pm: Arc<ProfileManager>,
    pub(crate) registry: RunningRegistry,
    chromix_launch: tokio::sync::Mutex<()>,
}

impl BrowserLauncher {
    pub fn new(pm: Arc<ProfileManager>) -> Self {
        Self {
            pm,
            registry: RunningRegistry::new(),
            chromix_launch: tokio::sync::Mutex::new(()),
        }
    }

    pub async fn is_running_async(&self, profile_id: &str) -> bool {
        self.registry.contains(profile_id).await
    }

    /// Launch through the bundled official SDK; an empty binary path enables SDK resolution.
    pub async fn launch_with_chromix(
        &self,
        profile_id: &str,
        binary_path: &Path,
        companion_dir: Option<&Path>,
        config: &ChromixSettings,
        runtime_dir: &Path,
        skip_download: bool,
        hidden: bool,
    ) -> Result<LaunchedProfile> {
        let _launch = self.chromix_launch.lock().await;
        // Hidden launch: force the SDK window off-screen. Per-profile options
        // are a shallow merge, so `launchOptions` has to be merged by hand to
        // avoid dropping the profile's own `launchOptions`.
        let hidden_config;
        let config = if hidden {
            let mut options = config.options.clone();
            let mut launch_options = options
                .get("launchOptions")
                .and_then(|value| value.as_object())
                .cloned()
                .unwrap_or_default();
            // Append to any existing args instead of replacing the array, so a
            // profile that already sets `launchOptions.args` keeps them.
            let mut args = launch_options
                .get("args")
                .and_then(|value| value.as_array())
                .cloned()
                .unwrap_or_default();
            args.push(serde_json::json!("--window-position=-32000,-32000"));
            launch_options.insert("args".into(), serde_json::Value::Array(args));
            options.insert("launchOptions".into(), serde_json::Value::Object(launch_options));
            hidden_config = ChromixSettings {
                node_path: config.node_path.clone(),
                options,
                environment: config.environment.clone(),
            };
            &hidden_config
        } else {
            config
        };
        if let Some(existing) = self
            .registry
            .with(profile_id, |handle| LaunchedProfile {
                id: handle.profile_id.clone(),
                cdp_endpoint: handle.cdp_endpoint.clone(),
                pid: handle.pid,
                started_at: handle.started_at.clone(),
            })
            .await
        {
            return Ok(existing);
        }
        self.close(profile_id).await?;
        let profile = self
            .pm
            .get(profile_id)
            .map_err(|error| MultizenError::Launch(format!("profile get: {error}")))?
            .ok_or_else(|| MultizenError::NotFound(profile_id.into()))?;
        let data_dir =
            crate::data_dir::effective_data_dir(&profile, BrowserEngine::Chromix, config)?;
        let verified_data_dir = crate::data_dir::verify_data_dir(&data_dir).ok();
        let business_scope = self.pm.business_profile_scope(profile_id)?;
        // 互动账号（小号）多开优化：按业务身份判定，向桥接层下发
        // `resourceProfile=sub`，使其阻断媒体/字体并静音暂停。
        let sub_account = matches!(
            self.pm
                .business_accounts_profile_state(profile_id)?
                .account
                .map(|account| account.kind),
            Some(multizen_core::BusinessAccountKind::KuaishouSub)
        );
        let process = crate::chromix::start(
            &profile,
            binary_path,
            companion_dir,
            config,
            runtime_dir,
            skip_download,
            sub_account,
        )
        .await?;
        if let Err(error) = self.pm.mark_opened(profile_id) {
            process.close().await;
            return Err(MultizenError::Launch(format!("mark_opened: {error}")));
        }
        let launched = LaunchedProfile {
            id: profile_id.into(),
            cdp_endpoint: process.endpoint.clone(),
            pid: process.pid,
            started_at: chrono::Utc::now().to_rfc3339(),
        };
        self.registry
            .insert(BrowserHandle {
                profile_id: launched.id.clone(),
                cdp_endpoint: launched.cdp_endpoint.clone(),
                pid: launched.pid,
                started_at: launched.started_at.clone(),
                data_dir,
                verified_data_dir,
                business_scope,
                chromix: Some(process),
            })
            .await;
        Ok(launched)
    }

    pub async fn close(&self, profile_id: &str) -> Result<()> {
        let mut handle = match self.registry.remove(profile_id).await {
            Some(h) => h,
            None => return Ok(()),
        };
        if let Some(process) = handle.chromix.take() {
            process.close().await;
        }
        Ok(())
    }

    pub async fn close_all(&self) {
        let ids = self.registry.ids().await;
        for id in ids {
            let _ = self.close(&id).await;
        }
    }
}
