use super::{diagnostics, Guard, TauriBrowserDriver};
use cdp_driver::TaskPage;
use multizen_core::KuaishouInitErrorCode as Code;
use serde::Deserialize;
use std::time::Duration;
const WAIT: Duration = Duration::from_secs(8);
pub(super) const SUBJECT_URL: &str = "https://s.kwaixiaodian.com/zone/shop/info/qualification";
pub(super) const SLICE_URL: &str = "https://s.kwaixiaodian.com/zone/short-video-b/slice";
const SCRIPT: &str = include_str!("page.js");
const IDENTITY: &str = include_str!("../identity/extract.js");
#[cfg(all(test, windows))]
#[path = "chromix_entry_fixture.rs"]
mod chromix_entry_fixture;
#[derive(Deserialize)]
pub(super) struct Subject {
    pub name: String,
    pub card: String,
    pub pictures: Vec<String>,
}

async fn live_identity(
    driver: &TauriBrowserDriver,
    guard: &Guard,
    page: &mut TaskPage<'_>,
) -> Result<(), Code> {
    driver.validate_init(guard).await?;
    let session = guard.slot.session().ok_or(Code::ContextChanged)?;
    driver
        .validate_kuaishou_task_identity(
            &session,
            page,
            &guard.context.platform_user_id,
            guard.slot.cancel.clone(),
        )
        .await
        .map_err(|_| Code::ContextChanged)?;
    driver.validate_init(guard).await
}
pub(super) async fn execute(
    driver: &TauriBrowserDriver,
    guard: &Guard,
    page: &mut TaskPage<'_>,
    action: &str,
    argument: serde_json::Value,
) -> Result<serde_json::Value, Code> {
    let mut probe = diagnostics::Probe::slice_action(action);
    live_identity(driver, guard, page)
        .await
        .inspect_err(|_code| {
            diagnostics::finish(&mut probe, "identity-before-rejected");
        })?;
    let expression = format!(
        "({SCRIPT})({},{},{},()=>{IDENTITY})",
        serde_json::to_string(action).unwrap(),
        serde_json::to_string(&guard.context.platform_user_id).unwrap(),
        argument
    );
    let result = page.evaluate(&expression, WAIT).await.map_err(|error| {
        diagnostics::finish(&mut probe, diagnostics::task_error(&error));
        Code::PageUnsupported
    })?;
    live_identity(driver, guard, page)
        .await
        .inspect_err(|_code| {
            diagnostics::finish(&mut probe, "identity-after-rejected");
        })?;
    // Each invocation checks exact origin/account before AND after synchronous DOM mutation.
    let value = result.get("result").cloned().ok_or_else(|| {
        diagnostics::finish(&mut probe, "response-shape");
        Code::PageUnsupported
    })?;
    diagnostics::finish(&mut probe, "none");
    Ok(value)
}
pub(super) async fn identity(
    driver: &TauriBrowserDriver,
    guard: &Guard,
    page: &mut TaskPage<'_>,
) -> Result<(), Code> {
    execute(driver, guard, page, "identity", serde_json::Value::Null)
        .await
        .map(|_| ())
}
pub(super) async fn navigate(
    driver: &TauriBrowserDriver,
    guard: &Guard,
    page: &mut TaskPage<'_>,
    url: &str,
) -> Result<(), Code> {
    let mut probe = (url == SLICE_URL)
        .then(|| diagnostics::Probe::new(multizen_core::KuaishouInitStep::Slice, "slice-navigate"));
    identity(driver, guard, page).await.inspect_err(|_code| {
        diagnostics::finish(&mut probe, "identity-before-rejected");
    })?;
    page.navigate(url, Duration::from_secs(20))
        .await
        .map_err(|error| {
            diagnostics::finish(&mut probe, diagnostics::task_error(&error));
            Code::PageUnsupported
        })?;
    wait_identity(driver, guard, page)
        .await
        .inspect_err(|_code| {
            diagnostics::finish(&mut probe, "identity-ready-rejected");
        })?;
    diagnostics::finish(&mut probe, "none");
    Ok(())
}
pub(super) async fn wait_identity(
    driver: &TauriBrowserDriver,
    guard: &Guard,
    page: &mut TaskPage<'_>,
) -> Result<(), Code> {
    // Fast polls first (responsive pages resolve in <1s), then slower fallback.
    for i in 0..20 {
        if identity(driver, guard, page).await.is_ok() {
            return Ok(());
        }
        driver.validate_init(guard).await?;
        tokio::time::sleep(Duration::from_millis(if i < 5 { 100 } else { 250 })).await;
    }
    Err(Code::ContextChanged)
}
pub(super) async fn subject(
    driver: &TauriBrowserDriver,
    guard: &Guard,
    page: &mut TaskPage<'_>,
    tab: &str,
) -> Result<Subject, Code> {
    execute(driver, guard, page, "tab", tab.into()).await?;
    for i in 0..12 {
        if let Ok(value) = execute(driver, guard, page, "subject", tab.into()).await {
            let result: Subject =
                serde_json::from_value(value).map_err(|_| Code::PageUnsupported)?;
            if result.pictures.len() > 8
                || result
                    .pictures
                    .iter()
                    .any(|p| p.len() > local_ocr::MAX_IMAGE_BYTES.div_ceil(3) * 4)
            {
                return Err(Code::AttachmentUnavailable);
            }
            return Ok(result);
        }
        tokio::time::sleep(Duration::from_millis(if i < 4 { 100 } else { 250 })).await;
    }
    Err(Code::AttachmentUnavailable)
}
async fn read_slice(
    driver: &TauriBrowserDriver,
    guard: &Guard,
    page: &mut TaskPage<'_>,
) -> Result<[bool; 4], Code> {
    let value = execute(driver, guard, page, "slice", serde_json::Value::Null).await?;
    serde_json::from_value(value).map_err(|_| Code::PageUnsupported)
}
async fn wait_settings_entry(
    driver: &TauriBrowserDriver,
    guard: &Guard,
    page: &mut TaskPage<'_>,
) -> Result<(), Code> {
    let mut probe =
        diagnostics::Probe::new(multizen_core::KuaishouInitStep::Slice, "slice-entry-wait");
    let result = tokio::time::timeout(WAIT, async {
        loop {
            // Only a missing/disabled entry is retried. Ambiguity, identity changes,
            // transport errors and unknown dialogs propagate without dispatching a click.
            let ready = execute(driver, guard, page, "open-ready", serde_json::Value::Null).await?;
            match ready.as_bool() {
                Some(true) => return Ok(()),
                Some(false) => tokio::time::sleep(Duration::from_millis(100)).await,
                None => return Err(Code::PageUnsupported),
            }
        }
    })
    .await
    .unwrap_or(Err(Code::TimedOut));
    probe.finish(
        result
            .as_ref()
            .err()
            .copied()
            .map(diagnostics::step_error)
            .unwrap_or("none"),
    );
    result
}
async fn open_slice(
    driver: &TauriBrowserDriver,
    guard: &Guard,
    page: &mut TaskPage<'_>,
) -> Result<[bool; 4], Code> {
    wait_settings_entry(driver, guard, page).await?;
    // Revalidate then dispatch exactly once. Never retry a potentially sent click.
    execute(driver, guard, page, "open", serde_json::Value::Null).await?;
    for _ in 0..12 {
        if let Ok(state) = read_slice(driver, guard, page).await {
            return Ok(state);
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    Err(Code::PageUnsupported)
}
pub(super) async fn slice(
    driver: &TauriBrowserDriver,
    guard: &Guard,
    page: &mut TaskPage<'_>,
) -> Result<[bool; 4], Code> {
    // Always navigate first: cancellation recovery reads server state rather than toggling stale DOM.
    navigate(driver, guard, page, SLICE_URL).await?;
    let initial = open_slice(driver, guard, page).await?;
    // Batch close: turn off all still-enabled checkboxes in one pass, then
    // wait for the platform to persist before a single re-read verification.
    // This matches the platform's async save model better than per-checkbox
    // read-write-verify cycles and reduces retry-triggering false failures.
    for (index, disabled) in initial.into_iter().enumerate() {
        if disabled {
            continue;
        }
        read_slice(driver, guard, page).await?; // all-four shape/count revalidation before each write
        execute(driver, guard, page, "off", index.into()).await?;
    }
    // Wait for the platform to persist all toggles, then verify once.
    tokio::time::sleep(Duration::from_millis(300)).await;
    let after_close = read_slice(driver, guard, page).await?;
    if after_close != [true; 4] {
        return Err(Code::PersistenceUnverified);
    }
    execute(driver, guard, page, "close", serde_json::Value::Null).await?;
    // No invented save contract. Full document navigation then reopen and read all four.
    navigate(driver, guard, page, SLICE_URL).await?;
    let persisted = open_slice(driver, guard, page).await?;
    if persisted != [true; 4] {
        return Err(Code::PersistenceUnverified);
    }
    execute(driver, guard, page, "close", serde_json::Value::Null).await?;
    Ok(persisted)
}
