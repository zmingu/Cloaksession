//! Real DOM acceptance for TaskPage, separate from the wire peer and identity tests.
//!
//! PowerShell (from repository root; use an already installed Chromium executable):
//! $env:RUN_TASK_BROWSER_FIXTURE = '1'
//! $env:TASK_TEST_CHROMIUM = 'C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe'
//! cargo test -p cdp-driver --locked --test task_browser task_page_in_disposable_chromium -- --ignored --exact --nocapture
//!
//! Never accepts a CDP endpoint, downloads a browser, or loads an external fixture.
//! All documents are embedded data URLs. Background traffic is directed to an owned
//! non-forwarding proxy sink, with DNS disabled. This is Chromium acceptance, not
//! proof of every engine's safety, full actionability, or browser-side rollback.
//!
//! Parent-run evidence, 2026-10-01, installed Microsoft Edge:
//! - Full acceptance FAILED: hidden-A click exceeded its 15-second deadline.
//! - Stage probe: geometry/typing worked; move-0 timed out with no DOM events.
//! - Disabling the three background-throttling flags did not resolve move-0.
//! - Foreground diagnostic PASSED (1/0, 1.78s): all mouse events acknowledged
//!   and delivered. Foreground activation is diagnostic only, NOT a workaround.
//! - Hidden-A non-pointer acceptance PASSED (1/0, 4.02s): all included assertions.
//! - Independent raw page WebSocket reproduced move-0 timeout (3.005s), with
//!   visibility hidden, no DOM mouse events, and output unchanged at idle.
//! This reproduces the limitation outside chromiumoxide's command handler on
//! this installed Edge; it is not an all-engine claim. The full background-mouse
//! acceptance gate remains FAILED, pending a semantics decision. No production
//! fix, silent activation, longer deadline, or automatic input retry is applied.

use base64::{engine::general_purpose::STANDARD, Engine as _};
use cdp_driver::{
    session::BrowserSession, BoundPage, SelectorState, TaskCancel, TaskError, TaskPage,
};
use chromiumoxide::cdp::browser_protocol::input::{
    DispatchMouseEventParams, DispatchMouseEventType, MouseButton,
};
use multizen_core::BrowserEngine;
use std::{
    path::PathBuf,
    process::{Child, Command, Stdio},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

const WAIT: Duration = Duration::from_secs(15);
const POLL: Duration = Duration::from_millis(20);
const SHORT: Duration = Duration::from_millis(200);

// Deliberately independent of identity_browser.rs: no identity/platform reader,
// URL, opt-in, or cross-crate source inclusion belongs in this acceptance test.
// Ownership follows that fixture's Child + private DevToolsActivePort pattern.
struct OwnedBrowser {
    child: Option<Child>,
    dir: PathBuf,
    _network_sink: std::net::TcpListener,
}

impl OwnedBrowser {
    fn start() -> Self {
        Self::start_with_args(&[])
    }

    fn start_with_args(extra_args: &[&str]) -> Self {
        let exe = std::env::var_os("TASK_TEST_CHROMIUM")
            .expect("set TASK_TEST_CHROMIUM to an installed Chromium executable");
        assert!(PathBuf::from(&exe).is_file(), "Chromium executable missing");
        let dir = std::env::temp_dir().join(format!(
            "cloaksession-task-fixture-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let sink = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let proxy = sink.local_addr().unwrap();
        std::fs::create_dir(&dir).expect("create a new, never reused profile directory");
        let mut owned = Self {
            child: None,
            dir,
            _network_sink: sink,
        };
        owned.child = Some(
            Command::new(&exe)
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
                    "--window-size=900,700",
                ])
                .args(extra_args)
                .arg(format!("--proxy-server=http://{proxy}"))
                .arg(format!("--user-data-dir={}", owned.dir.display()))
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .expect("spawn owned Chromium"),
        );
        println!(
            "executable={} owned_pid={} temp={}",
            PathBuf::from(exe).display(),
            owned.child.as_ref().unwrap().id(),
            owned.dir.display()
        );
        owned
    }

    async fn endpoint(&mut self) -> String {
        tokio::time::timeout(WAIT, async {
            loop {
                assert!(
                    self.child.as_mut().unwrap().try_wait().unwrap().is_none(),
                    "owned Chromium exited before CDP startup"
                );
                if let Ok(text) = std::fs::read_to_string(self.dir.join("DevToolsActivePort")) {
                    if let Some(port) = text.lines().next().and_then(|v| v.parse::<u16>().ok()) {
                        assert_ne!(port, 0);
                        return format!("http://127.0.0.1:{port}");
                    }
                }
                tokio::time::sleep(POLL).await;
            }
        })
        .await
        .expect("owned CDP startup deadline")
    }

    fn cleanup(&mut self) {
        // Only the process we spawned is eligible for termination. No process-name
        // search, existing endpoint, taskkill /IM, or user profile deletion.
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            if let Err(error) = child.wait() {
                eprintln!("failed to reap owned Chromium: {error}");
            }
        }
        for _ in 0..100 {
            if !self.dir.exists() || std::fs::remove_dir_all(&self.dir).is_ok() {
                return;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        eprintln!("temporary profile cleanup failed: {}", self.dir.display());
    }
}

impl Drop for OwnedBrowser {
    fn drop(&mut self) {
        self.cleanup();
    }
}

fn document(name: &str) -> String {
    let html = format!(
        r#"<!doctype html><meta charset="utf-8"><link rel="icon" href="data:,"><title>Task {name}</title>
        <main>{name}</main><input id="entry"><button id="press" onclick="document.querySelector('output').textContent = 'clicked'">Press</button><output>idle</output>"#
    );
    format!("data:text/html;base64,{}", STANDARD.encode(html))
}

// The independent BoundPage is a fixture actor, intentionally outside the
// cooperative lease. Start from the opposite state, then mutate real DOM while
// TaskPage is polling; no scripted SelectorObservation payloads are involved.
async fn transition(
    task: &mut TaskPage<'_>,
    actor: &BoundPage<'_>,
    selector: &str,
    before: SelectorState,
    after: SelectorState,
    mutation: &str,
) {
    task.wait_for_selector(selector, before, WAIT, POLL)
        .await
        .unwrap();
    let (result, ()) = tokio::join!(task.wait_for_selector(selector, after, WAIT, POLL), async {
        tokio::time::sleep(SHORT).await;
        tokio::time::timeout(WAIT, actor.evaluate(mutation))
            .await
            .unwrap()
            .unwrap();
    });
    result.unwrap();
}

async fn acceptance(session: &BrowserSession, include_pointer: bool) {
    use SelectorState::{Attached, Detached, Hidden, Visible};

    let a = session.new_bound_page("about:blank").await.unwrap();
    let b = session.new_bound_page("about:blank").await.unwrap();
    let a_id = a.target_id().to_owned();
    let b_id = b.target_id().to_owned();
    b.navigate(&document("B"), 15_000).await.unwrap();
    let mut task = session
        .task_page(&a_id, TaskCancel::new(), WAIT)
        .await
        .unwrap();
    session.activate_page(&b_id).await.unwrap();
    let url = document("A");
    let nav = task.navigate(&url, WAIT).await.unwrap();
    assert_eq!(nav.url, url);
    assert_eq!(nav.title, "Task A");
    assert_eq!(task.target_id(), a_id);
    assert_eq!(session.evaluate("document.title").await.unwrap(), "Task B");
    assert_eq!(task.extract(WAIT).await.unwrap()["title"], "Task A");
    assert!(task
        .screenshot(WAIT)
        .await
        .unwrap()
        .starts_with("iVBORw0KGgo"));

    task.type_text("#entry", "local123", WAIT).await.unwrap();
    if include_pointer {
        task.click("#press", WAIT).await.unwrap();
    } else {
        assert_eq!(
            task.evaluate("document.visibilityState", WAIT)
                .await
                .unwrap(),
            "hidden"
        );
        println!(
            "non-pointer acceptance: hidden A; pointer gate intentionally excluded, NOT passed"
        );
    }
    assert_eq!(
        task.evaluate("document.querySelector('#entry').value", WAIT)
            .await
            .unwrap(),
        "local123"
    );
    assert_eq!(
        task.evaluate("document.querySelector('output').textContent", WAIT)
            .await
            .unwrap(),
        if include_pointer { "clicked" } else { "idle" }
    );
    assert_eq!(
        b.evaluate("document.querySelector('#entry').value")
            .await
            .unwrap(),
        ""
    );
    assert_eq!(
        b.evaluate("document.querySelector('output').textContent")
            .await
            .unwrap(),
        "idle"
    );
    assert_eq!(session.evaluate("document.title").await.unwrap(), "Task B");
    println!("fixed target: navigation/evaluate/extract/PNG/input remain on A; legacy stays B");

    // Quotes, backslashes and Unicode exercise JSON-to-JS selector encoding.
    let selector = r#"[data-probe="quote\"slash\\中"]"#;
    transition(&mut task, &a, selector, Detached, Attached,
        r#"(() => { const el = document.createElement('div'); el.id='probe'; el.dataset.probe='quote"slash\\中'; el.style.cssText='display:none;width:40px;height:20px'; document.body.append(el); return true; })()"#).await;
    transition(
        &mut task,
        &a,
        selector,
        Hidden,
        Visible,
        "document.querySelector('#probe').style.display='block'",
    )
    .await;
    transition(
        &mut task,
        &a,
        selector,
        Visible,
        Hidden,
        "document.querySelector('#probe').style.visibility='hidden'",
    )
    .await;
    transition(
        &mut task,
        &a,
        selector,
        Hidden,
        Visible,
        "document.querySelector('#probe').style.visibility='visible'",
    )
    .await;
    transition(
        &mut task,
        &a,
        selector,
        Visible,
        Hidden,
        "document.querySelector('#probe').style.width='0px'",
    )
    .await;
    transition(
        &mut task,
        &a,
        selector,
        Hidden,
        Visible,
        "document.querySelector('#probe').style.width='40px'",
    )
    .await;
    transition(
        &mut task,
        &a,
        selector,
        Attached,
        Detached,
        "document.querySelector('#probe').remove(); true",
    )
    .await;
    task.wait_for_selector(selector, Hidden, WAIT, POLL)
        .await
        .unwrap();
    assert!(matches!(
        task.wait_for_selector("[", Attached, WAIT, POLL).await,
        Err(TaskError::Driver(_))
    ));
    assert_eq!(
        task.evaluate("document.title", WAIT).await.unwrap(),
        "Task A"
    );
    println!(
        "real DOM: attached/detached/display/visibility/zero-size/hidden-absent and invalid CSS"
    );

    // Distinct bindings must queue on the same target; B is still independently usable.
    assert!(matches!(
        session
            .bind_page(&a_id)
            .await
            .unwrap()
            .into_task(TaskCancel::new(), SHORT)
            .await,
        Err(TaskError::TimedOut)
    ));
    let mut other = session
        .task_page(&b_id, TaskCancel::new(), WAIT)
        .await
        .unwrap();
    assert_eq!(
        other.evaluate("document.title", WAIT).await.unwrap(),
        "Task B"
    );
    other.release();
    task.release();

    let cancel = TaskCancel::new();
    let mut task = session
        .task_page(&a_id, cancel.clone(), WAIT)
        .await
        .unwrap();
    let (result, ()) = tokio::join!(
        task.wait_for_selector("#never-created", Attached, WAIT, POLL),
        async {
            tokio::time::sleep(SHORT).await;
            cancel.cancel();
        }
    );
    assert!(matches!(result, Err(TaskError::Cancelled)));
    assert!(matches!(
        task.evaluate("window.forbidden = true", WAIT).await,
        Err(TaskError::Cancelled)
    ));
    drop(task);

    let cancel = TaskCancel::new();
    let mut task = session
        .task_page(&a_id, cancel.clone(), WAIT)
        .await
        .unwrap();
    let (result, ()) = tokio::join!(
        task.evaluate("window.pendingStarted = true; new Promise(() => {})", WAIT),
        async {
            // Observe actual renderer dispatch before cancelling the slow await.
            tokio::time::timeout(WAIT, async {
                loop {
                    if a.evaluate("window.pendingStarted === true").await.unwrap() == true {
                        break;
                    }
                    tokio::time::sleep(POLL).await;
                }
            })
            .await
            .expect("pending evaluation reached the renderer");
            cancel.cancel();
        }
    );
    assert!(matches!(result, Err(TaskError::Cancelled)));
    task.release();

    let mut task = session
        .task_page(&a_id, TaskCancel::new(), WAIT)
        .await
        .unwrap();
    assert!(matches!(
        task.wait_for_selector("#never-created", Attached, SHORT, POLL)
            .await,
        Err(TaskError::TimedOut)
    ));
    drop(task);

    let mut task = session
        .task_page(&a_id, TaskCancel::new(), WAIT)
        .await
        .unwrap();
    // A never-resolving Promise stalls only this CDP await, not the renderer thread.
    // It is read-only; cancellation does not imply that browser work was recalled.
    assert!(matches!(
        task.evaluate("new Promise(() => {})", SHORT).await,
        Err(TaskError::TimedOut)
    ));
    assert!(matches!(
        task.click("#press", WAIT).await,
        Err(TaskError::TimedOut)
    ));
    task.release();
    let mut task = session
        .task_page(&a_id, TaskCancel::new(), WAIT)
        .await
        .unwrap();
    assert_eq!(
        task.evaluate("typeof window.forbidden", WAIT)
            .await
            .unwrap(),
        "undefined"
    );
    let next = document("A after recovery");
    let nav = task.navigate(&next, WAIT).await.unwrap();
    assert_eq!(nav.url, next);
    assert_eq!(nav.title, "Task A after recovery");
    assert_eq!(session.evaluate("document.title").await.unwrap(), "Task B");
    assert_eq!(session.browser.pages().await.unwrap().len(), 2);
    task.release();
    println!("leases: same-target queue deadline, different-target independence, cancel/timeout terminality, drop/release and reacquisition");
}

#[tokio::test]
#[ignore = "explicit RUN_TASK_BROWSER_FIXTURE=1 and TASK_TEST_CHROMIUM required; owns a new disposable browser"]
async fn task_page_in_disposable_chromium() {
    run_acceptance(true).await;
}

#[tokio::test]
#[ignore = "explicit isolated-browser opt-in; excludes the known failing hidden-pointer gate"]
async fn task_page_non_pointer_in_disposable_chromium() {
    run_acceptance(false).await;
}

async fn run_acceptance(include_pointer: bool) {
    assert_eq!(
        std::env::var("RUN_TASK_BROWSER_FIXTURE").as_deref(),
        Ok("1"),
        "explicit isolated-browser opt-in required"
    );
    let mut owned = OwnedBrowser::start();
    let endpoint = owned.endpoint().await;
    let session = tokio::time::timeout(
        WAIT,
        BrowserSession::connect(&endpoint, BrowserEngine::Chromix),
    )
    .await
    .expect("connect deadline")
    .expect("connect only to the owned process");
    // Chromix here selects the existing engine observation policy, not a Node
    // bridge or installed engine claim. No fingerprint bootstrap is performed.
    let result = tokio::time::timeout(
        Duration::from_secs(120),
        acceptance(&session, include_pointer),
    )
    .await;
    // Close only this owned session, then reap via the process guard. On panic,
    // the same guard still kills/reaps the owned child and removes its profile.
    let _ = tokio::time::timeout(WAIT, session.close()).await;
    owned.cleanup();
    assert!(!owned.dir.exists(), "temporary profile must be removed");
    result.expect("whole acceptance deadline");
    println!(
        "cleanup: owned browser reaped; temporary profile removed; only embedded local documents"
    );
}

type RawSocket =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

async fn raw_command(
    socket: &mut RawSocket,
    id: u64,
    method: &str,
    params: serde_json::Value,
) -> serde_json::Value {
    use futures::{SinkExt, StreamExt};
    use tokio_tungstenite::tungstenite::Message;
    socket
        .send(Message::Text(
            serde_json::json!({
                "id": id, "method": method, "params": params,
            })
            .to_string(),
        ))
        .await
        .unwrap();
    while let Some(message) = socket.next().await {
        match message.unwrap() {
            Message::Text(text) => {
                let response: serde_json::Value = serde_json::from_str(&text).unwrap();
                if response["id"].as_u64() == Some(id) {
                    return response;
                }
                println!("raw other message={response}");
            }
            Message::Ping(bytes) => socket.send(Message::Pong(bytes)).await.unwrap(),
            Message::Close(reason) => panic!("raw socket closed: {reason:?}"),
            _ => {}
        }
    }
    panic!("raw socket ended before command response");
}

// Fresh process; Chromiumoxide is used only to prepare the same two documents.
// The observed Runtime and Input commands use a separate direct page WebSocket,
// without chromiumoxide serialization, request bookkeeping or handler pumping.
#[tokio::test]
#[ignore = "diagnostic only; raw WebSocket hidden-A mouse move in an owned browser"]
async fn task_click_raw_hidden_probe() {
    use serde_json::json;
    assert_eq!(
        std::env::var("RUN_TASK_BROWSER_FIXTURE").as_deref(),
        Ok("1")
    );
    let mut owned = OwnedBrowser::start();
    let endpoint = owned.endpoint().await;
    let session = tokio::time::timeout(
        WAIT,
        BrowserSession::connect(&endpoint, BrowserEngine::Chromix),
    )
    .await
    .unwrap()
    .unwrap();
    let a = session.new_bound_page("about:blank").await.unwrap();
    let b = session.new_bound_page("about:blank").await.unwrap();
    b.navigate(&document("B"), 15_000).await.unwrap();
    let mut task = session
        .task_page(a.target_id(), TaskCancel::new(), WAIT)
        .await
        .unwrap();
    session.activate_page(b.target_id()).await.unwrap();
    task.navigate(&document("A"), WAIT).await.unwrap();
    task.extract(WAIT).await.unwrap();
    task.screenshot(WAIT).await.unwrap();
    task.type_text("#entry", "local123", WAIT).await.unwrap();
    // Construct only this owned process's local page endpoint. No configurable
    // remote endpoint, external URL, or system proxy participates in this socket.
    let ws = format!(
        "{}/devtools/page/{}",
        endpoint.replacen("http://", "ws://", 1),
        a.target_id()
    );
    let (mut socket, _) = tokio::time::timeout(WAIT, tokio_tungstenite::connect_async(&ws))
        .await
        .unwrap()
        .unwrap();
    let setup = tokio::time::timeout(WAIT, raw_command(&mut socket, 1, "Runtime.evaluate", json!({
        "expression": "(() => { window.rawMouseProbe = []; document.addEventListener('mousemove', e => window.rawMouseProbe.push([e.clientX,e.clientY]), true); const el = document.querySelector('#press'); el.scrollIntoView({block:'center'}); const r = el.getBoundingClientRect(); return {x:r.x+r.width/2,y:r.y+r.height/2,visibility:document.visibilityState,entry:document.querySelector('#entry').value}; })()",
        "returnByValue": true
    }))).await.unwrap();
    println!("raw setup={setup}");
    assert!(setup.get("error").is_none());
    let state = &setup["result"]["result"]["value"];
    assert_eq!(
        state["visibility"], "hidden",
        "raw comparison requires hidden A"
    );
    assert_eq!(state["entry"], "local123");
    let x = state["x"].as_f64().unwrap();
    let y = state["y"].as_f64().unwrap();
    let (x, y) =
        behavioral::mouse::humanized_path((x - 4.0, y - 4.0), (x, y), x.to_bits() ^ y.to_bits())[0];
    let start = std::time::Instant::now();
    let result = tokio::time::timeout(
        Duration::from_secs(3),
        raw_command(
            &mut socket,
            2,
            "Input.dispatchMouseEvent",
            json!({"type":"mouseMoved","button":"none","x":x,"y":y}),
        ),
    )
    .await;
    println!(
        "raw move-0 elapsed={:?} response={result:?}",
        start.elapsed()
    );
    // Only read after timeout; never repeat the possibly delivered mouse move.
    let dom = tokio::time::timeout(Duration::from_secs(3), raw_command(
        &mut socket, 3, "Runtime.evaluate", json!({
            "expression":"({events:window.rawMouseProbe,visibility:document.visibilityState,output:document.querySelector('output').textContent})",
            "returnByValue":true
        })
    )).await;
    println!("raw post-move DOM={dom:?}");
    assert_eq!(session.evaluate("document.title").await.unwrap(), "Task B");
    drop(socket);
    drop(task);
    drop(a);
    drop(b);
    let _ = tokio::time::timeout(WAIT, session.close()).await;
    owned.cleanup();
    assert!(!owned.dir.exists());
    let response = result.expect(
        "raw hidden mouseMoved acknowledgement deadline; inspect log for browser-side evidence",
    );
    assert!(response.get("error").is_none(), "raw CDP error: {response}");
}

// Parent foreground result (2026-10-01): 1 passed / 0 failed in 1.78s;
// visible A acknowledged all 12 moves in 8-17ms, press/release below 1ms,
// and DOM received mousemove/mousedown/mouseup/click. This proves visibility
// dependence through chromiumoxide, not yet an engine-versus-library attribution.

// Diagnostic only: spells out page_ops::click's exact dispatch stages to locate
// a browser stall. This is not an alternate implementation or acceptance pass.
// Run the same command above with the exact filter task_click_stage_probe.
#[tokio::test]
#[ignore = "diagnostic only; same explicit isolated-browser opt-in as acceptance"]
async fn task_click_stage_probe() {
    click_stage_probe(&[], false).await;
}

// Controlled comparison only. These switches must not silently become the
// acceptance defaults: success here would establish a launch-policy limitation.
#[tokio::test]
#[ignore = "diagnostic only; disables Chromium background scheduling in its owned process"]
async fn task_click_no_background_throttling_probe() {
    click_stage_probe(
        &[
            "--disable-backgrounding-occluded-windows",
            "--disable-renderer-backgrounding",
            "--disable-background-timer-throttling",
        ],
        false,
    )
    .await;
}

// Parent-observed Edge results (2026-10-01): baseline and no-throttle probes
// both stalled on move-0, with no DOM mouse events. Geometry and typing worked;
// A had visibilityState=hidden. The switches above did not resolve the failure.
// This explicit foreground comparison is diagnostic, never an acceptance fix.
#[tokio::test]
#[ignore = "diagnostic only; explicitly brings A to foreground in a fresh owned browser"]
async fn task_click_foreground_probe() {
    click_stage_probe(&[], true).await;
}

async fn click_stage_probe(extra_args: &[&str], foreground: bool) {
    assert_eq!(
        std::env::var("RUN_TASK_BROWSER_FIXTURE").as_deref(),
        Ok("1")
    );
    println!("probe extra browser switches={extra_args:?}");
    let mut owned = OwnedBrowser::start_with_args(extra_args);
    let endpoint = owned.endpoint().await;
    let session = tokio::time::timeout(
        WAIT,
        BrowserSession::connect(&endpoint, BrowserEngine::Chromix),
    )
    .await
    .unwrap()
    .unwrap();
    let a = session.new_bound_page("about:blank").await.unwrap();
    let b = session.new_bound_page("about:blank").await.unwrap();
    b.navigate(&document("B"), 15_000).await.unwrap();
    let mut task = session
        .task_page(a.target_id(), TaskCancel::new(), WAIT)
        .await
        .unwrap();
    session.activate_page(b.target_id()).await.unwrap();
    task.navigate(&document("A"), WAIT).await.unwrap();
    task.extract(WAIT).await.unwrap();
    task.screenshot(WAIT).await.unwrap();
    task.type_text("#entry", "local123", WAIT).await.unwrap();
    let page = session
        .browser
        .pages()
        .await
        .unwrap()
        .into_iter()
        .find(|p| p.target_id().as_ref() == a.target_id())
        .unwrap();
    if foreground {
        println!("probe explicitly activating A in the browser; legacy cache remains B");
        tokio::time::timeout(WAIT, page.activate())
            .await
            .unwrap()
            .unwrap();
        tokio::time::timeout(WAIT, async {
            loop {
                if a.evaluate("document.visibilityState").await.unwrap() == "visible" {
                    break;
                }
                tokio::time::sleep(POLL).await;
            }
        })
        .await
        .expect("foreground comparison requires visible A");
    }
    // Record real DOM delivery as well as CDP acknowledgements. Never retry a
    // timed-out mouse command: it may already have been dispatched.
    task.evaluate(r#"(() => {
        window.mouseProbe = [];
        for (const type of ['mousemove','mousedown','mouseup','click']) {
            document.addEventListener(type, e => window.mouseProbe.push({type:e.type,x:e.clientX,y:e.clientY}), true);
        }
        return true;
    })()"#, WAIT).await.unwrap();
    println!("probe before geometry: {}", task.evaluate(
        "({visibility:document.visibilityState,focus:document.hasFocus(),active:document.activeElement.id,entry:document.querySelector('#entry').value})", WAIT
    ).await.unwrap());
    let start = std::time::Instant::now();
    // Identical to page_ops's find + scrollIntoView expression for #press.
    let geometry = task
        .evaluate(
            r##"(function() {
        var el = document.querySelector("#press");
        if (!el) return null;
        el.scrollIntoView({block:'center'});
        var r = el.getBoundingClientRect();
        return {x: r.x + r.width/2, y: r.y + r.height/2};
    })()"##,
            WAIT,
        )
        .await;
    println!(
        "probe geometry elapsed={:?} result={geometry:?}",
        start.elapsed()
    );
    let geometry = geometry.unwrap();
    let x = geometry["x"].as_f64().unwrap();
    let y = geometry["y"].as_f64().unwrap();
    let seed = x.to_bits() ^ y.to_bits();
    let mut stages: Vec<(String, DispatchMouseEventType, MouseButton, f64, f64)> =
        behavioral::mouse::humanized_path((x - 4.0, y - 4.0), (x, y), seed)
            .into_iter()
            .enumerate()
            .map(|(index, (x, y))| {
                (
                    format!("move-{index}"),
                    DispatchMouseEventType::MouseMoved,
                    MouseButton::None,
                    x,
                    y,
                )
            })
            .collect();
    stages.push((
        "press".into(),
        DispatchMouseEventType::MousePressed,
        MouseButton::Left,
        x,
        y,
    ));
    stages.push((
        "release".into(),
        DispatchMouseEventType::MouseReleased,
        MouseButton::Left,
        x,
        y,
    ));
    for (stage, kind, button, x, y) in stages {
        let mut builder = DispatchMouseEventParams::builder()
            .x(x)
            .y(y)
            .button(button)
            .r#type(kind);
        if stage == "press" || stage == "release" {
            builder = builder.click_count(1);
        }
        let command = builder.build().unwrap();
        let start = std::time::Instant::now();
        println!("probe dispatch {stage} x={x} y={y}");
        let result = tokio::time::timeout(Duration::from_secs(3), async {
            page.execute(command).await.map(|_| ())
        })
        .await;
        println!(
            "probe ack {stage} elapsed={:?} result={result:?}",
            start.elapsed()
        );
        if !matches!(&result, Ok(Ok(_))) {
            // A read-only observation after timeout helps distinguish absent
            // delivery, delivered-but-unacknowledged input and a wedged renderer.
            let state = tokio::time::timeout(Duration::from_secs(3), a.evaluate(
                "({events:window.mouseProbe,visibility:document.visibilityState,focus:document.hasFocus(),output:document.querySelector('output').textContent})"
            )).await;
            println!("probe first failure={stage}; DOM={state:?}");
            panic!("first failed click stage: {stage}; foreground comparison={foreground}; no retry performed");
        }
    }
    println!(
        "probe completed DOM={}",
        a.evaluate("window.mouseProbe").await.unwrap()
    );
    assert_eq!(session.evaluate("document.title").await.unwrap(), "Task B");
    drop(task);
    drop(a);
    drop(b);
    let _ = tokio::time::timeout(WAIT, session.close()).await;
    owned.cleanup();
    assert!(!owned.dir.exists());
}
