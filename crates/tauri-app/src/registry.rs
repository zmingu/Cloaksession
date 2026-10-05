//! Current-session registry. Pending connections share one cell and cannot resurrect after remove.
use cdp_driver::{session::BrowserSession, TaskCancel};
use multizen_core::{BrowserEngine, ChromixSettings, MultizenError, Profile, Result};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{Mutex, OnceCell};

pub type ProfileId = String;

/// The launch-time configuration, not mutable Profile settings after launch.
#[derive(Clone)]
pub(crate) struct NetworkSnapshot {
    pub profile: Profile,
    pub engine: BrowserEngine,
    pub chromix: ChromixSettings,
    pub environment_uncertain: bool,
}

pub(crate) struct SessionSlot {
    pub id: String,
    pub cancel: TaskCancel,
    pub network: Option<NetworkSnapshot>,
    endpoint: String,
    engine: BrowserEngine,
    marker: String,
    session: OnceCell<Arc<BrowserSession>>,
}
impl SessionSlot {
    pub fn session(&self) -> Option<Arc<BrowserSession>> {
        self.session.get().cloned()
    }
    #[cfg(test)]
    pub(crate) fn install_test_session(&self, session: Arc<BrowserSession>) {
        assert!(self.session.set(session).is_ok(), "fixture slot already initialized");
    }
}

pub struct ProfileRegistry {
    sessions: Mutex<HashMap<ProfileId, Arc<SessionSlot>>>,
}
impl Default for ProfileRegistry {
    fn default() -> Self {
        Self::new()
    }
}
impl ProfileRegistry {
    pub fn new() -> Self {
        Self {
            sessions: Mutex::new(HashMap::new()),
        }
    }

    /// Called on launcher success before replying. Repeated launch keeps the original proxy snapshot.
    pub(crate) async fn prepare(
        &self,
        profile_id: &str,
        endpoint: &str,
        engine: BrowserEngine,
        marker: &str,
        network: Option<NetworkSnapshot>,
    ) -> Arc<SessionSlot> {
        let mut entries = self.sessions.lock().await;
        if let Some(slot) = entries.get(profile_id) {
            if slot.endpoint == endpoint
                && slot.engine == engine
                && slot.marker == marker
                && !slot.cancel.is_cancelled()
            {
                return slot.clone();
            }
            slot.cancel.cancel();
        }
        let slot = Arc::new(SessionSlot {
            id: uuid::Uuid::new_v4().to_string(),
            cancel: TaskCancel::new(),
            network,
            endpoint: endpoint.into(),
            engine,
            marker: marker.into(),
            session: OnceCell::new(),
        });
        entries.insert(profile_id.into(), slot.clone());
        slot
    }

    async fn connect_slot(
        &self,
        profile_id: &str,
        slot: Arc<SessionSlot>,
    ) -> Result<Arc<BrowserSession>> {
        let session = tokio::select! {
            biased;
            _ = slot.cancel.cancelled() => return Err(MultizenError::Cdp("session closed during attachment".into())),
            value = tokio::time::timeout(std::time::Duration::from_secs(15),slot.session.get_or_try_init(|| async {
                BrowserSession::connect(&slot.endpoint,slot.engine).await.map(Arc::new)
            })) => value.map_err(|_| MultizenError::Cdp("session attachment deadline".into()))??.clone(),
        };
        if !self.is_current(profile_id, &slot).await {
            return Err(MultizenError::Cdp(
                "session changed during attachment".into(),
            ));
        }
        Ok(session)
    }

    pub async fn get_or_connect(
        &self,
        profile_id: &str,
        endpoint: &str,
        engine: BrowserEngine,
    ) -> Result<Arc<BrowserSession>> {
        let slot = self
            .prepare(profile_id, endpoint, engine, endpoint, None)
            .await;
        self.connect_slot(profile_id, slot).await
    }

    pub(crate) async fn prepared_slot(
        &self,
        profile_id: &str,
        marker: &str,
    ) -> Result<Arc<SessionSlot>> {
        self.sessions
            .lock()
            .await
            .get(profile_id)
            .filter(|s| s.marker == marker && !s.cancel.is_cancelled())
            .cloned()
            .ok_or_else(|| MultizenError::Cdp("launch session was invalidated".into()))
    }

    /// Unlike get_or_connect this never creates a slot after close/delete raced the launch reply.
    pub(crate) async fn connect_prepared(
        &self,
        profile_id: &str,
        marker: &str,
    ) -> Result<Arc<BrowserSession>> {
        let slot = self.prepared_slot(profile_id, marker).await?;
        self.connect_slot(profile_id, slot).await
    }
    pub(crate) async fn slot(&self, profile_id: &str) -> Option<Arc<SessionSlot>> {
        self.sessions
            .lock()
            .await
            .get(profile_id)
            .filter(|s| s.session.get().is_some() && !s.cancel.is_cancelled())
            .cloned()
    }
    /// Unfiltered slot lookup (no session/cancel filters). The running monitor
    /// snapshots the exact generation it is about to probe, so `remove_current`
    /// can reject a slot a relaunch has already replaced.
    pub(crate) async fn raw_slot(&self, profile_id: &str) -> Option<Arc<SessionSlot>> {
        self.sessions.lock().await.get(profile_id).cloned()
    }
    pub async fn get(&self, profile_id: &str) -> Option<Arc<BrowserSession>> {
        self.slot(profile_id).await.and_then(|s| s.session())
    }
    pub(crate) async fn is_current(&self, profile_id: &str, expected: &Arc<SessionSlot>) -> bool {
        self.with_current(profile_id, expected, || ())
            .await
            .is_some()
    }
    /// Synchronous commit closure runs while remove/replacement is excluded. Never await in it.
    pub(crate) async fn with_current<T>(
        &self,
        profile_id: &str,
        expected: &Arc<SessionSlot>,
        commit: impl FnOnce() -> T,
    ) -> Option<T> {
        let entries = self.sessions.lock().await;
        let current = entries.get(profile_id)?;
        if expected.cancel.is_cancelled() || !Arc::ptr_eq(current, expected) {
            return None;
        }
        Some(commit())
    }
    /// An old close reply cannot clear the cache/events of a newer prepared launch.
    pub(crate) async fn with_absent<T>(
        &self,
        profile_id: &str,
        commit: impl FnOnce() -> T,
    ) -> Option<T> {
        let entries = self.sessions.lock().await;
        if entries.contains_key(profile_id) {
            return None;
        }
        Some(commit())
    }

    pub(crate) async fn remove_current(
        &self,
        profile_id: &str,
        expected: &Arc<SessionSlot>,
    ) -> bool {
        let mut entries = self.sessions.lock().await;
        if !entries
            .get(profile_id)
            .is_some_and(|slot| Arc::ptr_eq(slot, expected))
        {
            return false;
        }
        entries.remove(profile_id);
        expected.cancel.cancel();
        true
    }

    pub async fn remove(&self, profile_id: &str) {
        if let Some(slot) = self.sessions.lock().await.remove(profile_id) {
            slot.cancel.cancel();
        }
    }
    pub(crate) async fn clear(&self) {
        for (_, slot) in self.sessions.lock().await.drain() {
            slot.cancel.cancel();
        }
    }
    pub async fn ids(&self) -> Vec<String> {
        self.sessions
            .lock()
            .await
            .iter()
            .filter(|(_, s)| s.session.get().is_some() && !s.cancel.is_cancelled())
            .map(|(id, _)| id.clone())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn same_generation_reuses_slot_and_remove_invalidates_commit() {
        let reg = ProfileRegistry::new();
        let a = reg
            .prepare(
                "p",
                "http://127.0.0.1:1",
                BrowserEngine::Chromix,
                "first",
                None,
            )
            .await;
        let b = reg
            .prepare(
                "p",
                "http://127.0.0.1:1",
                BrowserEngine::Chromix,
                "first",
                None,
            )
            .await;
        assert!(Arc::ptr_eq(&a, &b));
        reg.remove("p").await;
        assert!(a.cancel.is_cancelled());
        let c = reg
            .prepare(
                "p",
                "http://127.0.0.1:1",
                BrowserEngine::Chromix,
                "second",
                None,
            )
            .await;
        assert_ne!(a.id, c.id);
        assert!(reg
            .with_current("p", &a, || panic!("stale commit"))
            .await
            .is_none());
        assert!(reg.connect_prepared("p", "first").await.is_err());
        reg.clear().await;
        assert!(c.cancel.is_cancelled());
    }
    #[tokio::test]
    async fn remove_cancels_inflight_attach_without_resurrection() {
        let reg = ProfileRegistry::new();
        let slot = reg
            .prepare(
                "p",
                "http://127.0.0.1:1",
                BrowserEngine::Chromix,
                "first",
                None,
            )
            .await;
        let connect = reg.connect_prepared("p", "first");
        tokio::pin!(connect);
        tokio::select! { _ = &mut connect => panic!("attach returned before cancellation"), _ = tokio::time::sleep(std::time::Duration::from_millis(20)) => {} }
        reg.remove("p").await;
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(100), connect)
                .await
                .unwrap()
                .is_err()
        );
        assert!(slot.cancel.is_cancelled());
        assert!(reg.ids().await.is_empty());
    }
}
