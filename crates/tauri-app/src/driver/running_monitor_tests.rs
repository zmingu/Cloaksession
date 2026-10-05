//! Unit tests for the bounded running monitor (`driver/running_monitor.rs`).
//!
//! The fixture has no `AppHandle`, so `emit` is a no-op; the monitor's tick
//! return value (the ids it converged) is the observable seam for
//! "exactly one event per external exit".

use super::*;
use crate::driver::business_tests::{fixture, launch_without_cdp};
use mcp_server::driver::BrowserDriver;
use multizen_core::{ChromixSettings, CreateProfileInput, Profile};

async fn profile(d: &TauriBrowserDriver, name: &str) -> Profile {
    d.create_profile(CreateProfileInput {
        name: name.into(),
        ..Default::default()
    })
    .await
    .unwrap()
}

/// A cache entry whose process is already gone (never registered with the
/// launcher) converges exactly once: the id leaves the cache, the session slot
/// is dropped, and a second pass — even one handed the same stale snapshot — is
/// a no-op.
#[tokio::test]
async fn exited_process_converges_once_and_is_idempotent() {
    let (_dir, d) = fixture(ChromixSettings::default());
    let p = profile(&d, "exited").await;
    // Simulate "launch succeeded, then the process died on its own": the sync
    // cache still holds the id while the launcher's registry does not.
    d.running.lock().unwrap().insert(p.id.clone());
    assert!(d.is_running(&p.id));

    let converged = d.running_monitor_tick(vec![p.id.clone()]).await;
    assert_eq!(converged, vec![p.id.clone()]);
    assert!(!d.is_running(&p.id));
    assert!(d.registry.get(&p.id).await.is_none());

    // Second tick: nothing left to converge, so no duplicate emit.
    assert!(d.running_monitor_tick(vec![p.id.clone()]).await.is_empty());
    d.shutdown().await;
}

/// An environment that never ran is never reported: the monitor's candidate set
/// is the cache, so an empty cache (or a snapshot naming a never-cached id)
/// converges nothing.
#[tokio::test]
async fn never_running_environment_is_never_converged() {
    let (_dir, d) = fixture(ChromixSettings::default());
    assert!(d
        .running_monitor_tick(vec!["never-launched".into()])
        .await
        .is_empty());
    assert!(!d.is_running("never-launched"));
    assert!(d.registry.get("never-launched").await.is_none());
    d.shutdown().await;
}

/// A genuinely alive process is left alone: the launcher's authoritative probe
/// reports true, so the cache entry survives the tick.
#[tokio::test]
async fn alive_process_is_not_converged() {
    let (_dir, d) = fixture(ChromixSettings::default());
    let p = profile(&d, "alive").await;
    launch_without_cdp(&d, &p).await.unwrap();
    d.running.lock().unwrap().insert(p.id.clone());
    assert!(d.running_monitor_tick(vec![p.id.clone()]).await.is_empty());
    assert!(d.is_running(&p.id));
    d.close(&p.id).await.unwrap();
    // close() already cleared the cache; a later tick must not re-announce it.
    assert!(d.running_monitor_tick(vec![p.id.clone()]).await.is_empty());
    d.shutdown().await;
}

/// The user's own `close()` owns the `Closed` event; the monitor must not
/// re-emit for the same environment afterwards.
#[tokio::test]
async fn user_close_is_not_re_emitted_by_the_monitor() {
    let (_dir, d) = fixture(ChromixSettings::default());
    let p = profile(&d, "closed").await;
    launch_without_cdp(&d, &p).await.unwrap();
    d.running.lock().unwrap().insert(p.id.clone());
    d.close(&p.id).await.unwrap();
    assert!(!d.is_running(&p.id));
    assert!(d.running_monitor_tick(vec![p.id.clone()]).await.is_empty());
    d.shutdown().await;
}

/// The real external-exit path: a running browser owns a session slot, and the
/// user closes the window directly (no `close()` call). The slot lingers until
/// the monitor removes the exact generation it probed, so the environment does
/// converge instead of being permanently skipped.
#[tokio::test]
async fn external_exit_with_a_slot_converges_and_drops_the_slot() {
    let (_dir, d) = fixture(ChromixSettings::default());
    let p = profile(&d, "external").await;
    // "Launch succeeded, then the process died on its own": a prepared slot plus
    // a cache entry, but no live launcher handle (the fixture never registers one).
    d.registry
        .prepare(&p.id, "http://127.0.0.1:1", d.engine, "prepared", None)
        .await;
    d.running.lock().unwrap().insert(p.id.clone());

    assert_eq!(
        d.running_monitor_tick(vec![p.id.clone()]).await,
        vec![p.id.clone()]
    );
    assert!(!d.is_running(&p.id));
    assert!(d.registry.get(&p.id).await.is_none());
    // Idempotent: a second pass emits nothing.
    assert!(d.running_monitor_tick(vec![p.id.clone()]).await.is_empty());
    d.shutdown().await;
}

/// A relaunch that replaced the slot between the probe snapshot and the commit
/// must never be announced as closed: `remove_current` rejects the stale Arc.
#[tokio::test]
async fn a_relaunch_that_replaced_the_slot_is_never_announced_closed() {
    let (_dir, d) = fixture(ChromixSettings::default());
    let p = profile(&d, "relaunch").await;
    let stale = d
        .registry
        .prepare(&p.id, "http://127.0.0.1:1", d.engine, "first", None)
        .await;
    // A relaunch replaces the slot; `stale` is now a superseded generation.
    d.registry
        .prepare(&p.id, "http://127.0.0.1:2", d.engine, "second", None)
        .await;
    d.running.lock().unwrap().insert(p.id.clone());

    assert!(
        !d.evict_exited(&p.id, Some(&stale)).await,
        "a stale generation must not commit"
    );
    assert!(d.is_running(&p.id), "the newer generation must survive");
    assert!(
        d.registry.raw_slot(&p.id).await.is_some(),
        "the replaced slot must remain registered"
    );
    d.shutdown().await;
}

/// The app-level task starts once (idempotent guard) and stops on demand.
#[tokio::test]
async fn monitor_starts_once_and_stops() {
    let (_dir, d) = fixture(ChromixSettings::default());
    let d = Arc::new(d);
    d.start_running_monitor();
    d.start_running_monitor();
    d.stop_running_monitor();
    assert!(d.running_monitor.stop.is_cancelled());
    d.shutdown().await;
}
