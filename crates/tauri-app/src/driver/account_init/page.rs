use super::{Guard, TauriBrowserDriver};
use cdp_driver::TaskPage;
use multizen_core::KuaishouInitErrorCode as Code;
use serde::Deserialize;
use std::time::Duration;
const WAIT: Duration = Duration::from_secs(8);
pub(super) const SUBJECT_URL: &str = "https://s.kwaixiaodian.com/zone/shop/info/qualification";
pub(super) const SLICE_URL: &str = "https://s.kwaixiaodian.com/zone/short-video-b/slice";
const SCRIPT: &str = include_str!("page.js");
const IDENTITY: &str = include_str!("../identity/extract.js");
#[derive(Deserialize)]
pub(super) struct Subject { pub name: String, pub card: String, pub pictures: Vec<String> }

pub(super) async fn execute(driver: &TauriBrowserDriver, guard: &Guard, page: &mut TaskPage<'_>, action: &str, argument: serde_json::Value) -> Result<serde_json::Value, Code> {
    driver.validate_init(guard).await?;
    let expression = format!("({SCRIPT})({},{},{},()=>{IDENTITY})", serde_json::to_string(action).unwrap(), serde_json::to_string(&guard.context.platform_user_id).unwrap(), argument);
    let result = page.evaluate(&expression, WAIT).await.map_err(|_| Code::PageUnsupported)?;
    driver.validate_init(guard).await?;
    // Each invocation checks exact origin/account before AND after synchronous DOM mutation.
    result.get("result").cloned().ok_or(Code::PageUnsupported)
}
pub(super) async fn identity(driver: &TauriBrowserDriver, guard: &Guard, page: &mut TaskPage<'_>) -> Result<(), Code> {
    execute(driver, guard, page, "identity", serde_json::Value::Null).await.map(|_| ())
}
pub(super) async fn navigate(driver: &TauriBrowserDriver, guard: &Guard, page: &mut TaskPage<'_>, url: &str) -> Result<(), Code> {
    identity(driver, guard, page).await?;
    page.navigate(url, Duration::from_secs(20)).await.map_err(|_| Code::PageUnsupported)?;
    wait_identity(driver, guard, page).await
}
pub(super) async fn wait_identity(driver: &TauriBrowserDriver, guard: &Guard, page: &mut TaskPage<'_>) -> Result<(), Code> {
    for _ in 0..20 {
        if identity(driver, guard, page).await.is_ok() { return Ok(()); }
        driver.validate_init(guard).await?;
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    Err(Code::ContextChanged)
}
pub(super) async fn subject(driver: &TauriBrowserDriver, guard: &Guard, page: &mut TaskPage<'_>, tab: &str) -> Result<Subject, Code> {
    execute(driver, guard, page, "tab", tab.into()).await?;
    for _ in 0..12 {
        if let Ok(value) = execute(driver, guard, page, "subject", tab.into()).await {
            let result: Subject = serde_json::from_value(value).map_err(|_| Code::PageUnsupported)?;
            if result.pictures.len() > 8 || result.pictures.iter().any(|p| p.len() > local_ocr::MAX_IMAGE_BYTES.div_ceil(3) * 4) { return Err(Code::AttachmentUnavailable); }
            return Ok(result);
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    Err(Code::AttachmentUnavailable)
}
async fn read_slice(driver: &TauriBrowserDriver, guard: &Guard, page: &mut TaskPage<'_>) -> Result<[bool; 4], Code> {
    let value = execute(driver, guard, page, "slice", serde_json::Value::Null).await?;
    serde_json::from_value(value).map_err(|_| Code::PageUnsupported)
}
async fn open_slice(driver: &TauriBrowserDriver, guard: &Guard, page: &mut TaskPage<'_>) -> Result<[bool; 4], Code> {
    execute(driver, guard, page, "open", serde_json::Value::Null).await?;
    for _ in 0..12 {
        if let Ok(state) = read_slice(driver, guard, page).await { return Ok(state); }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    Err(Code::PageUnsupported)
}
pub(super) async fn slice(driver: &TauriBrowserDriver, guard: &Guard, page: &mut TaskPage<'_>) -> Result<[bool; 4], Code> {
    // Always navigate first: cancellation recovery reads server state rather than toggling stale DOM.
    navigate(driver, guard, page, SLICE_URL).await?;
    let initial = open_slice(driver, guard, page).await?;
    for (index, disabled) in initial.into_iter().enumerate() {
        if disabled { continue; }
        read_slice(driver, guard, page).await?; // all-four shape/count revalidation before each write
        execute(driver, guard, page, "off", index.into()).await?;
        tokio::time::sleep(Duration::from_millis(350)).await;
        if !read_slice(driver, guard, page).await?[index] { return Err(Code::PersistenceUnverified); }
    }
    execute(driver, guard, page, "close", serde_json::Value::Null).await?;
    // No invented save contract. Full document navigation then reopen and read all four.
    navigate(driver, guard, page, SLICE_URL).await?;
    let persisted = open_slice(driver, guard, page).await?;
    if persisted != [true; 4] { return Err(Code::PersistenceUnverified); }
    execute(driver, guard, page, "close", serde_json::Value::Null).await?;
    Ok(persisted)
}
