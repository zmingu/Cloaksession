//! Bounded liveness monitor for the sync `running` cache.
//!
//! `launch`/`close` are the only writers of
//! [`TauriBrowserDriver::running`](super::TauriBrowserDriver), so a browser the
//! user closes directly (or one that crashes) leaves the cache stale forever —
//! the frontend keeps showing "Stop" until something else refreshes it.
//!
//! This monitor closes that gap with a bounded, non-blocking poll: every tick
//! it takes the cached ids and asks the launcher thread (the only owner of the
//! [`BrowserLauncher`](browser_launcher::BrowserLauncher)) whether each one is
//! genuinely alive, via [`LauncherCmd::HealthCheck`]. When the answer is `false`
//! the id is evicted from the cache, its session slot is dropped, and the same
//! `profiles:running-changed` `Closed` / `chromium:status` `stopped` events that
//! `close()` emits are pushed.
//!
//! Invariants this module must preserve:
//!
//! * **No duplicate emits** — the commit removes the exact slot generation
//!   snapshotted before the probe (`registry.remove_current`) and is gated by the
//!   cache-removal result, so a user `close()` (or a fast relaunch) that raced
//!   the tick is never re-announced by the monitor.
//! * **Never emit for an environment that never ran** — the candidate set is
//!   exactly the `running` cache, which only receives an id after a successful
//!   launch.
//! * **Bounded and deadlock-free** — each probe is a bounded oneshot round trip
//!   and no `tokio` lock is held across an `await` that emits.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use super::{ChromiumStatus, LauncherCmd, RunningStateChange, TauriBrowserDriver};

/// Same cadence as the sub-account login poller. Cheap by construction: one
/// O(1) hash lookup per cached profile per tick, so a window the user closes
/// directly converges in about 3 seconds.
const POLL: Duration = Duration::from_secs(3);

/// Bound on a single `HealthCheck` round trip. The launcher thread serves
/// commands serially (a `Launch` can occupy it for a while), so a probe that
/// does not answer in time is abandoned as `None` — the monitor then leaves
/// the cache untouched rather than guessing the process is dead.
const HEALTH_WAIT: Duration = Duration::from_secs(2);

/// Cancellation + once-guard state for the running monitor task. Mirrors
/// `InitRuntime`/`IdentityRuntime` so `shutdown()` and the `RunEvent::Exit`
/// hook stop it the same way.
pub(in crate::driver) struct RunningMonitorRuntime {
    pub(in crate::driver) stop: cdp_driver::TaskCancel,
    started: AtomicBool,
}

impl RunningMonitorRuntime {
    pub fn new() -> Self {
        Self {
            stop: cdp_driver::TaskCancel::new(),
            started: AtomicBool::new(false),
        }
    }
}

impl Default for RunningMonitorRuntime {
    fn default() -> Self {
        Self::new()
    }
}

impl TauriBrowserDriver {
    /// Ask the launcher thread whether `id` is genuinely alive. `None` when the
    /// probe could not be completed (channel closed, reply dropped, or the
    /// bounded wait elapsed) — callers must treat that as "unknown", never as
    /// "dead".
    async fn health_check(&self, id: &str) -> Option<bool> {
        let (resp, receive) = tokio::sync::oneshot::channel();
        self.launcher_tx
            .send(LauncherCmd::HealthCheck {
                profile_id: id.to_string(),
                resp,
            })
            .await
            .ok()?;
        tokio::time::timeout(HEALTH_WAIT, receive).await.ok()?.ok()
    }

    /// One bounded reconciliation pass over `ids` (the caller passes a snapshot
    /// of the `running` cache). Returns the ids whose process had exited and
    /// were converged this tick — the seam tests assert idempotence on.
    pub(in crate::driver) async fn running_monitor_tick(&self, ids: Vec<String>) -> Vec<String> {
        let mut converged = Vec::new();
        for id in ids {
            // Snapshot the slot generation *before* the probe. A relaunch that
            // replaces the slot during the probe changes this Arc, so the commit
            // below rejects it instead of announcing the fresh instance closed.
            let slot = self.registry.raw_slot(&id).await;
            if self.health_check(&id).await != Some(false) {
                continue;
            }
            if self.evict_exited(&id, slot.as_ref()).await {
                converged.push(id);
            }
        }
        converged
    }

    /// Evict an id whose process is gone, emitting exactly once.
    ///
    /// A running browser always owns a session slot, so the commit is
    /// **generation-checked, not absent-checked**: `remove_current` only removes
    /// the exact slot snapshotted before the probe and returns `false` when a
    /// relaunch already replaced it. That is the duplicate guard — a user
    /// `close()` (which removes the slot and clears the cache first) makes the
    /// later tick's `remove_current` fail, and a fast relaunch is never
    /// announced as closed. Returns whether this call performed the convergence.
    pub(in crate::driver) async fn evict_exited(
        &self,
        id: &str,
        slot: Option<&Arc<crate::registry::SessionSlot>>,
    ) -> bool {
        // Drop the dead generation's slot (and its CDP session) first; a relaunch
        // that raced the probe keeps its newer slot. No slot (a launch reply that
        // never prepared one) still converges through the cache removal below.
        if let Some(slot) = slot {
            if !self.registry.remove_current(id, slot).await {
                return false;
            }
        }
        // Single atomic commit point: whoever removes the cache entry owns the
        // announcement. `close()` also clears the cache before emitting, so a
        // tick that loses this race emits nothing.
        if !self.running.lock().unwrap().remove(id) {
            return false;
        }
        self.emit(
            "profiles:running-changed",
            &RunningStateChange::Closed {
                profile_id: id.to_string(),
                reason: "external-exit",
            },
        );
        self.emit(
            "chromium:status",
            &ChromiumStatus {
                profile_id: id.to_string(),
                status: "stopped".into(),
                error: None,
            },
        );
        true
    }

    /// Start the app-level liveness monitor. One task, weak idle ownership and
    /// cancellation through `running_monitor.stop` — the same shape as the
    /// identity and sub-account monitors, so an app exit or `shutdown()` stops
    /// it deterministically.
    pub fn start_running_monitor(self: &Arc<Self>) {
        if self.running_monitor.started.swap(true, Ordering::AcqRel) {
            return;
        }
        let weak = Arc::downgrade(self);
        let stop = self.running_monitor.stop.clone();
        // Called from the Tauri setup hook (no Tokio runtime context), so use
        // `tauri::async_runtime::spawn` — plain `tokio::spawn` panics there.
        tauri::async_runtime::spawn(async move {
            let mut interval = tokio::time::interval(POLL);
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                tokio::select! {
                    biased;
                    _ = stop.cancelled() => break,
                    _ = interval.tick() => {}
                }
                let Some(driver) = weak.upgrade() else {
                    break;
                };
                // Snapshot the cache without holding the lock across the awaits
                // below; ids only ever enter it after a successful launch.
                let ids: Vec<String> = driver.running.lock().unwrap().iter().cloned().collect();
                driver.running_monitor_tick(ids).await;
            }
        });
    }

    pub fn stop_running_monitor(&self) {
        self.running_monitor.stop.cancel();
    }
}
