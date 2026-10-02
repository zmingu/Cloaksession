//! Chromix-only readiness/observed-close regression for production open_slice/page.js.
//! Original H1 RED confirmed by parent: immediate control passed; delayed entry
//! rejected in 2ms; mounted at 1502ms; late-entry control passed. Keep both controls.
//! Explicit opt-in; isolated process/profile, all HTTP(S) intercepted or sunk.
//! No existing endpoint, live account, platform write, Edge fallback or download.
//! Run only by parent after setting RUN_ACCOUNT_INIT_CHROMIX_FIXTURE=1.
use super::*;
use crate::driver::business_tests::{fixture, launch_without_cdp};
use base64::{engine::general_purpose::STANDARD, Engine};
use cdp_driver::{session::BrowserSession, SelectorState};
use chromiumoxide::cdp::browser_protocol::{
    fetch::{EnableParams, EventRequestPaused, FailRequestParams, FulfillRequestParams},
    target::TargetId,
};
use futures::StreamExt;
use multizen_core::*;
use serde_json::json;
use std::{
    future::Future,
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::Arc,
    time::Instant,
};

const INSTALLED_CHROMIX: &str =
    "C:/Users/Administrator/.cache/chromix/v151.0.7922.173/win-x64/chromix/chrome.exe";
const LIMIT: Duration = Duration::from_secs(15);
#[path = "owned_page_fixture.rs"]
mod owned_page_fixture;
const HTML: &str = r#"<!doctype html><meta charset="utf-8"><link rel="icon" href="data:,"><title>Account init fixture</title>
<div class="username___fixture"><span class="id___fixture">ID:12345</span></div>
<div class="js-page-content"></div>
<div class="kwaishop-tianhe-shortVideoB-pc-drawer-content" style="display:none">
<div class="kwaishop-tianhe-shortVideoB-pc-drawer-header">
<h2 class="kwaishop-tianhe-shortVideoB-pc-drawer-title">直播切片托管设置</h2>
<div class="kwaishop-tianhe-shortVideoB-pc-drawer-extra"><span class="anticon anticon-system-close-medium-line" role="img">×</span></div>
</div>
<section class="WuFhZjfBowihPEVt5UUF"><div><h3 class="gh6LEC5pPxFLJTnYMjOy">全自动发布权限</h3><p>未开启</p></div>
<div class="collapseHeader"><label><input type="checkbox">直播中商品详解切片权限托管</label></div>
<div class="collapseHeader"><label><input type="checkbox">直播爆品切片返场权限托管</label></div>
<div class="collapseHeader"><label><input type="checkbox">直播引流片段发布权限托管</label></div>
<div class="collapseHeader"><label><input type="checkbox">切片个人主页展示位置</label></div>
</section></div>
<script>
window.__entryClicks = 0;
window.__permissionsFixture = document.querySelector('section.WuFhZjfBowihPEVt5UUF').cloneNode(true);
window.__closeClicks = 0;
document.querySelector('.kwaishop-tianhe-shortVideoB-pc-drawer-extra > span').onclick = () => {
  window.__closeClicks++;
  document.querySelector('.kwaishop-tianhe-shortVideoB-pc-drawer-content').style.display = 'none';
};
window.__armEntry = (delay, label = '修改设置') => {
  clearTimeout(window.__entryTimer);
  document.querySelector('.js-page-content').replaceChildren();
  document.querySelector('.kwaishop-tianhe-shortVideoB-pc-drawer-content').style.display = 'none';
  window.__entryClicks = 0;
  const mount = () => {
    const button = document.createElement('button'); button.type = 'button'; button.textContent = label;
    button.onclick = () => {
      window.__entryClicks++;
      const drawer = document.querySelector('.kwaishop-tianhe-shortVideoB-pc-drawer-content');
      drawer.style.display = 'block';
      drawer.querySelector('section.WuFhZjfBowihPEVt5UUF')?.remove();
      // The live sample also exposed a transient rejected read after opening.
      setTimeout(() => drawer.append(window.__permissionsFixture.cloneNode(true)), 100);
    };
    document.querySelector('.js-page-content').append(button);
  };
  if (delay === 0) mount(); else window.__entryTimer = setTimeout(mount, delay);
  return document.querySelector('.username___fixture') !== null && document.querySelector('.js-page-content button') === null;
};
// Actual observed zero-state reload uses this alternate exact settings entry.
window.__armEntry(0, '去设置');
</script>"#;

async fn stage<T, E>(
    phase: &'static str,
    work: impl Future<Output = std::result::Result<T, E>>,
) -> T {
    let start = Instant::now();
    match tokio::time::timeout(LIMIT, work).await {
        Ok(Ok(value)) => {
            println!(
                "step=slice phase={phase} error=none elapsed_ms={}",
                start.elapsed().as_millis()
            );
            value
        }
        Ok(Err(_)) => panic!("step=slice phase={phase} error=fixture-operation-failed"),
        Err(_) => panic!("step=slice phase={phase} error=fixture-deadline"),
    }
}

struct OwnedChromix {
    child: Child,
    dir: tempfile::TempDir,
    _sink: std::net::TcpListener,
}
impl OwnedChromix {
    fn start() -> Self {
        assert_eq!(
            std::env::var("RUN_ACCOUNT_INIT_CHROMIX_FIXTURE").as_deref(),
            Ok("1"),
            "explicit Chromix fixture opt-in required"
        );
        let binary = PathBuf::from(INSTALLED_CHROMIX);
        assert!(
            binary.is_file(),
            "required installed Chromix binary missing; no browser fallback"
        );
        let dir = tempfile::Builder::new()
            .prefix("account-init-chromix-h1-")
            .tempdir()
            .unwrap_or_else(|_| panic!("fixture temp directory failed"));
        let sink = std::net::TcpListener::bind("127.0.0.1:0")
            .unwrap_or_else(|_| panic!("fixture network sink failed"));
        let address = sink.local_addr().unwrap();
        let child = Command::new(binary)
            .args([
                "--headless=new",
                "--remote-debugging-address=127.0.0.1",
                "--remote-debugging-port=0",
                "--no-first-run",
                "--no-default-browser-check",
                "--no-startup-window",
                "--disable-background-networking",
                "--disable-component-update",
                "--disable-sync",
                "--disable-default-apps",
                "--disable-extensions",
                "--disable-quic",
                "--metrics-recording-only",
                "--safebrowsing-disable-auto-update",
                "--host-resolver-rules=MAP * ~NOTFOUND, EXCLUDE 127.0.0.1",
                "--proxy-bypass-list=<-loopback>",
            ])
            .arg(format!("--proxy-server=http://{address}"))
            .arg(format!("--user-data-dir={}", dir.path().display()))
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap_or_else(|_| panic!("installed Chromix fixture launch failed"));
        Self {
            child,
            dir,
            _sink: sink,
        }
    }
    async fn endpoint(&mut self) -> String {
        stage("fixture-startup", async {
            loop {
                if self.child.try_wait().map_err(|_| ())?.is_some() {
                    return Err(());
                }
                if let Ok(text) =
                    std::fs::read_to_string(self.dir.path().join("DevToolsActivePort"))
                {
                    if let Some(port) = text
                        .lines()
                        .next()
                        .and_then(|line| line.parse::<u16>().ok())
                        .filter(|port| *port != 0)
                    {
                        return Ok(format!("http://127.0.0.1:{port}"));
                    }
                }
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
        })
        .await
    }
}
impl Drop for OwnedChromix {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        // Only this freshly created temporary profile is eligible for cleanup.
        for _ in 0..40 {
            if !self.dir.path().exists() || std::fs::remove_dir_all(self.dir.path()).is_ok() {
                break;
            }
            std::thread::sleep(Duration::from_millis(25));
        }
    }
}
struct Interceptor {
    task: tokio::task::JoinHandle<()>,
    documents: Arc<std::sync::atomic::AtomicUsize>,
}
impl Drop for Interceptor {
    fn drop(&mut self) {
        self.task.abort();
    }
}
async fn intercept(page: &chromiumoxide::Page) -> Interceptor {
    let documents = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let count = documents.clone();
    let mut events = stage(
        "fixture-listener",
        page.event_listener::<EventRequestPaused>(),
    )
    .await;
    let actor = page.clone();
    let task = tokio::spawn(async move {
        while let Some(event) = events.next().await {
            // Fulfill only the synthetic route. Never continue an intercepted request.
            if event.request.url == SLICE_URL && event.request.method == "GET" {
                count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                let params: FulfillRequestParams = serde_json::from_value(json!({
                    "requestId": event.request_id, "responseCode": 200,
                    "responseHeaders": [{"name":"Content-Type","value":"text/html; charset=utf-8"}, {"name":"Cache-Control","value":"no-store"}],
                    "body": STANDARD.encode(HTML),
                })).unwrap();
                let _ = tokio::time::timeout(LIMIT, actor.execute(params)).await;
            } else {
                let params: FailRequestParams = serde_json::from_value(
                    json!({"requestId":event.request_id,"errorReason":"BlockedByClient"}),
                )
                .unwrap();
                let _ = tokio::time::timeout(LIMIT, actor.execute(params)).await;
            }
        }
    });
    let enable: EnableParams =
        serde_json::from_value(json!({"patterns":[{"urlPattern":"*","requestStage":"Request"}]}))
            .unwrap();
    stage("fixture-interceptor", page.execute(enable)).await;
    Interceptor { task, documents }
}

#[tokio::test]
#[ignore = "Chromix-only regression; parent opt-in, no existing browser endpoints"]
async fn chromix_delayed_slice_entry_after_ready_identity() {
    assert!(
        tokio::time::timeout(Duration::from_secs(90), run_fixture())
            .await
            .is_ok(),
        "step=slice phase=fixture-total error=fixture-deadline"
    );
}

async fn run_fixture() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter("off,tauri_app::driver::account_init=info")
        .with_ansi(false)
        .with_target(false)
        .try_init();
    let mut browser = OwnedChromix::start();
    let endpoint = browser.endpoint().await;
    let session = Arc::new(
        stage(
            "fixture-connect",
            BrowserSession::connect(&endpoint, BrowserEngine::Chromix),
        )
        .await,
    );
    let bound = stage("fixture-create", session.new_bound_page("about:blank")).await;
    let page = stage(
        "fixture-target",
        session.browser.get_page(TargetId::new(bound.target_id())),
    )
    .await;
    let _interceptor = intercept(&page).await;
    stage("fixture-document", bound.navigate(SLICE_URL, 10_000)).await;

    // Real launcher-thread context guards, but a protocol-only fake launcher owns
    // the temporary Profile. The installed Chromix above is the sole page engine.
    let (_data, driver) = fixture(ChromixSettings::default());
    let driver = Arc::new(driver);
    let profile = stage(
        "fixture-profile",
        driver.create_profile(CreateProfileInput {
            name: "synthetic H1 fixture".into(),
            ..Default::default()
        }),
    )
    .await;
    let launched = stage("fixture-launcher", launch_without_cdp(&driver, &profile)).await;
    let slot = stage(
        "fixture-slot",
        driver.registry.prepared_slot(
            &profile.id,
            &format!("{}:{}", launched.started_at, launched.pid),
        ),
    )
    .await;
    slot.install_test_session(session.clone());
    let business = BusinessProfileState {
        account: None,
        scope: None,
    };
    let mut snapshot =
        KuaishouIdentitySnapshot::empty(&profile.id, KuaishouIdentityStatus::Detected);
    snapshot.platform_user_id = Some("12345".into());
    snapshot.checked_at = Some(chrono::Utc::now().to_rfc3339());
    let observation = KuaishouIdentityObservation {
        snapshot,
        session_id: Some(slot.id.clone()),
        avatar_url: None,
    };
    let expected = business.clone();
    stage(
        "fixture-observation",
        driver.init_db(None, move |pm| {
            pm.kuaishou_identity_save(observation, &expected)?;
            Ok(())
        }),
    )
    .await;
    let guard = Guard {
        context: KuaishouInitContext {
            platform_user_id: "12345".into(),
            profile_id: profile.id.clone(),
            session_id: slot.id.clone(),
            expected_business: business,
        },
        slot: slot.clone(),
    };
    let _reservation = driver
        .identity
        .reserve_initialization(&profile.id)
        .expect("fixture reservation failed");
    let mut task = stage(
        "fixture-acquire",
        session.task_page(
            bound.target_id(),
            slot.cancel.clone(),
            Duration::from_secs(2),
        ),
    )
    .await;
    stage("identity-ready", wait_identity(&driver, &guard, &mut task)).await;

    stage(
        "immediate-entry-arm",
        task.evaluate("window.__armEntry(0)", WAIT),
    )
    .await;
    let control = stage(
        "immediate-entry-control",
        open_slice(&driver, &guard, &mut task),
    )
    .await;
    assert_eq!(
        control, [true; 4],
        "fixture control did not open/read expected synthetic drawer"
    );

    let absent = stage(
        "delayed-entry-arm",
        task.evaluate("window.__armEntry(1500)", WAIT),
    )
    .await;
    assert_eq!(
        absent,
        json!(true),
        "H1 precondition: header must be ready while entry is absent"
    );
    let start = Instant::now();
    // Invoke production orchestration directly, not a copied click loop.
    let outcome = tokio::time::timeout(
        Duration::from_secs(8),
        open_slice(&driver, &guard, &mut task),
    )
    .await;
    let outcome_code = match &outcome {
        Ok(Ok(_)) => "none",
        Ok(Err(code)) => diagnostics::step_error(*code),
        Err(_) => "fixture-deadline",
    };
    println!(
        "step=slice phase=delayed-entry-attempt error={outcome_code} elapsed_ms={}",
        start.elapsed().as_millis()
    );
    stage(
        "delayed-entry-eventual-ready",
        task.wait_for_selector(
            ".js-page-content button",
            SelectorState::Attached,
            Duration::from_secs(5),
            Duration::from_millis(25),
        ),
    )
    .await;
    let clicks = stage(
        "delayed-entry-click-check",
        task.evaluate("window.__entryClicks", WAIT),
    )
    .await;
    let passed = matches!(outcome, Ok(Ok([true, true, true, true]))) && clicks == json!(1);
    let h1_red = matches!(outcome, Ok(Err(Code::PageUnsupported))) && clicks == json!(0);
    if h1_red {
        // Parent diagnostic control only: after the entry exists, the same production
        // seam must work. This is not a product retry or a real-platform action.
        assert_eq!(
            stage("late-entry-control", open_slice(&driver, &guard, &mut task)).await,
            [true; 4]
        );
    }
    // Faithful observed close shape: no button, only the header/extra's named SPAN.
    let no_button = stage("header-icon-precondition", task.evaluate("document.querySelectorAll('.kwaishop-tianhe-shortVideoB-pc-drawer-content button[aria-label=Close],button.kwaishop-tianhe-shortVideoB-pc-drawer-close').length === 0", WAIT)).await;
    assert_eq!(no_button, json!(true));
    stage(
        "header-icon-close",
        execute(&driver, &guard, &mut task, "close", serde_json::Value::Null),
    )
    .await;
    assert_eq!(stage("header-icon-closed", task.evaluate("window.__closeClicks === 1 && getComputedStyle(document.querySelector('.kwaishop-tianhe-shortVideoB-pc-drawer-content')).display === 'none'", WAIT)).await, json!(true));
    stage("ambiguous-close-arm", task.evaluate("window.__armEntry(0); const extra = document.querySelector('.kwaishop-tianhe-shortVideoB-pc-drawer-extra'); extra.append(extra.firstElementChild.cloneNode(true)); true", WAIT)).await;
    stage(
        "ambiguous-close-open",
        open_slice(&driver, &guard, &mut task),
    )
    .await;
    assert_eq!(
        execute(&driver, &guard, &mut task, "close", serde_json::Value::Null).await,
        Err(Code::PageUnsupported)
    );
    assert_eq!(stage("ambiguous-close-unchanged", task.evaluate("window.__closeClicks === 1 && getComputedStyle(document.querySelector('.kwaishop-tianhe-shortVideoB-pc-drawer-content')).display !== 'none'", WAIT)).await, json!(true));
    let documents_before = _interceptor
        .documents
        .load(std::sync::atomic::Ordering::SeqCst);
    assert_eq!(
        stage("already-off-full-slice", slice(&driver, &guard, &mut task)).await,
        [true; 4]
    );
    assert_eq!(
        _interceptor
            .documents
            .load(std::sync::atomic::Ordering::SeqCst)
            - documents_before,
        2,
        "production sequence must navigate, close, navigate again, reopen and re-read"
    );
    assert_eq!(stage("zero-reload-entry", task.evaluate("document.querySelector('.js-page-content button').textContent === '去设置' && window.__entryClicks === 1 && window.__closeClicks === 1", WAIT)).await, json!(true));
    drop(task);
    driver.shutdown().await;
    assert!(
        passed || h1_red,
        "fixture inconclusive: inspect sanitized phase diagnostics, not H1 evidence"
    );
    assert!(passed, "H1 RED: production open_slice failed before a delayed settings entry became available; immediate and late-entry controls passed");
}
