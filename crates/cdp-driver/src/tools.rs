//! Legacy active-page tools. Task automation should use `BoundPage` instead.
use crate::page_ops::{OperationPolicy, PageOperations};
use multizen_core::{MultizenError, Result};

pub struct NavResult {
    pub url: String,
    pub title: String,
}

impl super::session::BrowserSession {
    pub async fn navigate(&self, url: &str, timeout_ms: u64) -> Result<NavResult> {
        // Preserve the legacy create-on-active-page-error behavior.
        let page = match self.active_page().await {
            Ok(p) => p,
            Err(_) => {
                let p = self
                    .browser
                    .new_page(url)
                    .await
                    .map_err(|e| MultizenError::Cdp(format!("new_page: {e}")))?;
                self.set_active_page(p.clone()).await;
                p
            }
        };
        PageOperations::new(self, &page, OperationPolicy::Legacy)
            .navigate(url, timeout_ms)
            .await
    }

    pub async fn screenshot(&self) -> Result<String> {
        let page = self.active_page().await?;
        PageOperations::new(self, &page, OperationPolicy::Legacy)
            .screenshot()
            .await
    }

    pub async fn evaluate(&self, expression: &str) -> Result<serde_json::Value> {
        let page = self.active_page().await?;
        PageOperations::new(self, &page, OperationPolicy::Legacy)
            .evaluate(expression)
            .await
    }

    pub async fn click(&self, selector: &str) -> Result<()> {
        let page = self.active_page().await?;
        PageOperations::new(self, &page, OperationPolicy::Legacy)
            .click(selector)
            .await
    }

    pub async fn type_text(&self, selector: &str, text: &str) -> Result<()> {
        let page = self.active_page().await?;
        PageOperations::new(self, &page, OperationPolicy::Legacy)
            .type_text(selector, text)
            .await
    }

    pub async fn extract(&self) -> Result<serde_json::Value> {
        let page = self.active_page().await?;
        PageOperations::new(self, &page, OperationPolicy::Legacy)
            .extract()
            .await
    }

    pub async fn close(mut self) {
        let _ = self.browser.close().await;
    }
}
