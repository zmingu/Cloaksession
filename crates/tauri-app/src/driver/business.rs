use super::{LauncherCmd, TauriBrowserDriver};
use multizen_core::{
    BusinessAccount, BusinessProfileState, MultizenError, Result, SaveBusinessAccountInput,
};
use tokio::sync::oneshot;

impl TauriBrowserDriver {
    /// Read-only guard for the explicit shop-login action, including unbound reservations.
    pub async fn require_kuaishou_login_scope(&self, profile_id: &str) -> Result<()> {
        let state = self.business_accounts_profile_state(profile_id).await?;
        if state.scope == Some(multizen_core::BusinessProfileScope::Jinniu) {
            return Err(MultizenError::Launch(
                "金牛专用环境不能通过小店扫码入口启动，请使用普通启动或选择快手环境".into(),
            ));
        }
        Ok(())
    }

    pub async fn business_accounts_list(&self) -> Result<Vec<BusinessAccount>> {
        let (resp, receive) = oneshot::channel();
        self.launcher_tx
            .send(LauncherCmd::BusinessAccountsList { resp })
            .await
            .map_err(|_| MultizenError::Mcp("launcher thread closed".into()))?;
        receive
            .await
            .map_err(|_| MultizenError::Mcp("launcher thread dropped response".into()))?
    }

    pub async fn business_accounts_profile_state(
        &self,
        profile_id: &str,
    ) -> Result<BusinessProfileState> {
        let (resp, receive) = oneshot::channel();
        self.launcher_tx
            .send(LauncherCmd::BusinessProfileState {
                profile_id: profile_id.into(),
                resp,
            })
            .await
            .map_err(|_| MultizenError::Mcp("launcher thread closed".into()))?;
        receive
            .await
            .map_err(|_| MultizenError::Mcp("launcher thread dropped response".into()))?
    }

    pub async fn business_accounts_save(
        &self,
        input: SaveBusinessAccountInput,
    ) -> Result<BusinessAccount> {
        let (resp, receive) = oneshot::channel();
        self.launcher_tx
            .send(LauncherCmd::SaveBusinessAccount {
                input,
                engine: self.engine,
                chromix: self.chromix.clone(),
                resp,
            })
            .await
            .map_err(|_| MultizenError::Mcp("launcher thread closed".into()))?;
        receive
            .await
            .map_err(|_| MultizenError::Mcp("launcher thread dropped response".into()))?
    }

    pub async fn business_accounts_unbind(&self, id: &str) -> Result<()> {
        let (resp, receive) = oneshot::channel();
        self.launcher_tx
            .send(LauncherCmd::UnbindBusinessAccount {
                id: id.into(),
                resp,
            })
            .await
            .map_err(|_| MultizenError::Mcp("launcher thread closed".into()))?;
        receive
            .await
            .map_err(|_| MultizenError::Mcp("launcher thread dropped response".into()))?
    }

    /// Permanently delete a business-account record (not just unbind). The
    /// launcher guard rejects the call while a bound profile is still running.
    pub async fn delete_business_account(&self, id: &str) -> Result<()> {
        let (resp, receive) = oneshot::channel();
        self.launcher_tx
            .send(LauncherCmd::DeleteBusinessAccount {
                id: id.into(),
                resp,
            })
            .await
            .map_err(|_| MultizenError::Mcp("launcher thread closed".into()))?;
        receive
            .await
            .map_err(|_| MultizenError::Mcp("launcher thread dropped response".into()))?
    }
}
