//! Opt-in, disposable real Chromium test. Never attaches to a supplied/user endpoint.
//! Node/Playwright may locate the executable, but all browser work is Rust/CDP.
#[path = "../../tauri-app/src/driver/identity/extract.rs"]
mod reader;

use cdp_driver::{session::BrowserSession, TaskCancel};
use chromiumoxide::{
    cdp::browser_protocol::{
        fetch::{EnableParams, EventRequestPaused, FailRequestParams, FulfillRequestParams},
        network::EventRequestWillBeSent,
        page::{GetResourceContentParams, GetResourceTreeParams},
    },
    Page,
};
use futures::StreamExt;
use multizen_core::BrowserEngine;
use serde_json::{json, Value};
use std::{
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};

const EXTRACTOR: &str = include_str!("../../tauri-app/src/driver/identity/extract.js");
const WAIT: Duration = Duration::from_secs(10);
const PNG: &str =
    "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+aX1sAAAAASUVORK5CYII=";
const IMAGE: &str = "https://fixture.yximgs.com/avatar.png";

struct OwnedBrowser {
    child: Option<Child>,
    dir: PathBuf,
    // Held exclusively for the browser lifetime. No accept/forward operation exists.
    network_sink: std::net::TcpListener,
}
impl OwnedBrowser {
    fn start() -> Self {
        let exe = std::env::var_os("IDENTITY_TEST_CHROMIUM")
            .expect("set IDENTITY_TEST_CHROMIUM to a dedicated test Chromium executable");
        assert!(PathBuf::from(&exe).is_file(), "Chromium executable missing");
        let dir = std::env::temp_dir().join(format!(
            "cloaksession-identity-fixture-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let network_sink = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        std::fs::create_dir(&dir).unwrap();
        let mut owned = Self {
            child: None,
            dir,
            network_sink,
        };
        owned.child = Some(
            Command::new(exe)
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
                .arg(format!(
                    "--proxy-server=http://{}",
                    owned.network_sink.local_addr().unwrap()
                ))
                .arg(format!("--user-data-dir={}", owned.dir.display()))
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .expect("spawn own Chromium"),
        );
        owned
    }
    async fn endpoint(&mut self) -> String {
        tokio::time::timeout(WAIT, async {
            loop {
                assert!(
                    self.child.as_mut().unwrap().try_wait().unwrap().is_none(),
                    "owned Chromium exited"
                );
                if let Ok(text) = std::fs::read_to_string(self.dir.join("DevToolsActivePort")) {
                    if let Some(port) = text.lines().next().and_then(|v| v.parse::<u16>().ok()) {
                        return format!("http://127.0.0.1:{port}");
                    }
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        })
        .await
        .expect("owned CDP startup deadline")
    }
    fn cleanup(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            child.wait().expect("reap owned child");
        }
        for _ in 0..100 {
            if !self.dir.exists() || std::fs::remove_dir_all(&self.dir).is_ok() {
                return;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        eprintln!("temporary directory cleanup failed: {}", self.dir.display());
    }
}
impl Drop for OwnedBrowser {
    fn drop(&mut self) {
        self.cleanup();
    }
}

// Avoid a manifest/lock change for this standalone harness.
fn base64(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let n = ((chunk[0] as u32) << 16)
            | ((chunk.get(1).copied().unwrap_or(0) as u32) << 8)
            | chunk.get(2).copied().unwrap_or(0) as u32;
        out.push(TABLE[((n >> 18) & 63) as usize] as char);
        out.push(TABLE[((n >> 12) & 63) as usize] as char);
        out.push(if chunk.len() > 1 {
            TABLE[((n >> 6) & 63) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            TABLE[(n & 63) as usize] as char
        } else {
            '='
        });
    }
    out
}
fn html(id: &str, nickname: &str) -> String {
    format!(
        r#"<!doctype html><meta charset="utf-8"><link rel="icon" href="data:,"><title>Isolated identity fixture</title><div class="username___test"><span class="id___test">{id}</span><span class="nickName___test">{nickname}</span></div><div class="avatar___test"><div class="seller-main-avatar"><img src="{IMAGE}"></div></div><img id="cors" crossorigin="anonymous" src="https://fixture.yximgs.com/cors.png"><main>商品 ID:9999999999 <span class="id___product">ID:8888888888</span></main>"#
    )
}
#[derive(Default)]
struct Counts {
    avatar_mode: AtomicUsize,
    avatar_gets: AtomicUsize,
    paused: AtomicUsize,
    fulfilled: AtomicUsize,
    failed: AtomicUsize,
    network: AtomicUsize,
}
struct Interception {
    tasks: Vec<tokio::task::JoinHandle<()>>,
    counts: Arc<Counts>,
}
impl Drop for Interception {
    fn drop(&mut self) {
        for task in &self.tasks {
            task.abort();
        }
    }
}
async fn intercept(page: &Page) -> Interception {
    let counts = Arc::new(Counts::default());
    let mut requests = page.event_listener::<EventRequestPaused>().await.unwrap();
    let mut network = page
        .event_listener::<EventRequestWillBeSent>()
        .await
        .unwrap();
    let c = counts.clone();
    let network_task = tokio::spawn(async move {
        while network.next().await.is_some() {
            c.network.fetch_add(1, Ordering::SeqCst);
        }
    });
    let p = page.clone();
    let c = counts.clone();
    let fetch_task = tokio::spawn(async move {
        while let Some(event) = requests.next().await {
            c.paused.fetch_add(1, Ordering::SeqCst);
            let url = event.request.url.as_str();
            println!("Fetch.requestPaused url={url}");
            if (url == IMAGE || url == "https://fixture.yximgs.com/proxy-proof.png")
                && event.resource_type
                    != chromiumoxide::cdp::browser_protocol::network::ResourceType::Image
            {
                c.avatar_gets.fetch_add(1, Ordering::SeqCst);
                let mode = c.avatar_mode.load(Ordering::SeqCst);
                if mode == 6 {
                    p.execute(
                        chromiumoxide::cdp::browser_protocol::fetch::ContinueRequestParams::new(
                            event.request_id.clone(),
                        ),
                    )
                    .await
                    .unwrap();
                    continue;
                }
                if mode == 4 || mode == 5 {
                    tokio::time::sleep(Duration::from_millis(if mode == 4 { 500 } else { 3500 }))
                        .await;
                }
                if (1..=3).contains(&mode) {
                    let (status, headers, body) = if mode == 1 {
                        (
                            302,
                            vec![
                                json!({"name":"Location","value":"https://unexpected.invalid/never"}),
                            ],
                            String::new(),
                        )
                    } else {
                        let mut headers = vec![json!({"name":"Content-Type","value":"image/png"})];
                        if mode == 2 {
                            headers.push(json!({"name":"Content-Length","value":"3000000"}));
                        }
                        (200, headers, base64(&vec![1; 3_000_000]))
                    };
                    let cmd: FulfillRequestParams = serde_json::from_value(json!({"requestId":event.request_id,"responseCode":status,"responseHeaders":headers,"body":body})).unwrap();
                    let _ = p.execute(cmd).await;
                    c.fulfilled.fetch_add(1, Ordering::SeqCst);
                    continue;
                }
            }
            let document = match url {
                "https://s.kwaixiaodian.com/a" => Some(html("ID:12345678", "  Fixture   Alpha  ")),
                "https://s.kwaixiaodian.com/b" => Some(html("ID：87654321", "Fixture Beta")),
                "https://s.kwaixiaodian.com/no-id" => Some(html("", "Not an account")),
                "https://s.kwaixiaodian.com/invalid" => Some(html("ID:12345x", "Invalid")),
                "https://s.kwaixiaodian.com.evil.invalid/wrong" => {
                    Some(html("ID:12345678", "Wrong domain"))
                }
                _ => None,
            };
            let response = if let Some(body) = document {
                Some(("text/html; charset=utf-8", base64(body.as_bytes())))
            } else if url == IMAGE || url == "https://fixture.yximgs.com/cors.png" {
                Some(("image/png", PNG.to_owned()))
            } else {
                None
            };
            if let Some((mime, body)) = response {
                let mut headers = vec![
                    json!({"name":"Content-Type","value":mime}),
                    json!({"name":"Cache-Control","value":"public, max-age=3600"}),
                ];
                if url.ends_with("/cors.png") {
                    headers.push(json!({"name":"Access-Control-Allow-Origin","value":"*"}));
                }
                let command: FulfillRequestParams = serde_json::from_value(json!({"requestId":event.request_id,"responseCode":200,"responseHeaders":headers,"body":body})).unwrap();
                // Cancellation can invalidate a deliberately delayed fixture request.
                let _ = p.execute(command).await;
                c.fulfilled.fetch_add(1, Ordering::SeqCst);
            } else {
                let command: FailRequestParams = serde_json::from_value(
                    json!({"requestId":event.request_id,"errorReason":"BlockedByClient"}),
                )
                .unwrap();
                p.execute(command).await.unwrap();
                c.failed.fetch_add(1, Ordering::SeqCst);
            }
        }
    });
    page.execute(
        serde_json::from_value::<EnableParams>(
            json!({"patterns":[{"urlPattern":"*","requestStage":"Request"}]}),
        )
        .unwrap(),
    )
    .await
    .unwrap();
    Interception {
        tasks: vec![network_task, fetch_task],
        counts,
    }
}
async fn read(session: &BrowserSession, page: &Page) -> Value {
    let mut task = session
        .task_page(page.target_id().as_ref(), TaskCancel::new(), WAIT)
        .await
        .unwrap();
    task.evaluate(EXTRACTOR, WAIT).await.unwrap()
}
async fn loaded(page: &Page, url: &str) {
    page.goto(url).await.unwrap();
    tokio::time::timeout(WAIT, async {
        loop {
            let ready = page
                .evaluate(
                    "document.images.length === 2 && Array.from(document.images).every(i => i.complete && i.naturalWidth === 1)",
                )
                .await
                .unwrap()
                .into_value::<bool>()
                .unwrap();
            if ready {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("fixture PNG decoded");
}

#[tokio::test]
#[ignore = "explicit isolated browser opt-in"]
async fn browser_native_avatar_get_probe() {
    assert_eq!(
        std::env::var("RUN_IDENTITY_BROWSER_FIXTURE").as_deref(),
        Ok("1")
    );
    let mut owned = OwnedBrowser::start();
    let endpoint = owned.endpoint().await;
    let session = BrowserSession::connect(&endpoint, BrowserEngine::Chromix)
        .await
        .unwrap();
    let page = session.browser.new_page("about:blank").await.unwrap();
    let _interception = intercept(&page).await;
    loaded(&page, "https://s.kwaixiaodian.com/a").await;
    let reader = BrowserSession::connect(&endpoint, BrowserEngine::Chromix)
        .await
        .unwrap();
    let p = tokio::time::timeout(WAIT, async {
        loop {
            if let Some(p) = reader
                .browser
                .pages()
                .await
                .unwrap()
                .into_iter()
                .find(|p| p.target_id() == page.target_id())
            {
                break p;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    let mut task = reader
        .task_page(p.target_id().as_ref(), TaskCancel::new(), WAIT)
        .await
        .unwrap();
    assert_eq!(task.read_public_avatar(IMAGE, WAIT).await.unwrap(), PNG);
    println!("production native GET response stream: PNG without CORS");
    let c = &_interception.counts;
    assert_eq!(c.avatar_gets.load(Ordering::SeqCst), 1);
    let before = c.paused.load(Ordering::SeqCst);
    assert!(task
        .read_public_avatar("https://evil.invalid/a", WAIT)
        .await
        .is_err());
    assert_eq!(c.paused.load(Ordering::SeqCst), before);
    for mode in [1, 2, 3] {
        c.avatar_mode.store(mode, Ordering::SeqCst);
        let started = std::time::Instant::now();
        assert!(
            task.read_public_avatar(IMAGE, WAIT).await.is_err(),
            "reject redirect/oversized mode={mode}"
        );
        if mode != 1 {
            assert!(
                started.elapsed() < Duration::from_secs(2),
                "oversize must reject before the request timeout"
            );
        }
    }
    assert_eq!(
        c.failed.load(Ordering::SeqCst),
        0,
        "redirect destination must never be requested"
    );
    drop(task);
    c.avatar_mode.store(4, Ordering::SeqCst);
    let mut avatar = reader::AvatarPage::open(
        &reader,
        p.target_id().as_ref(),
        "12345678",
        IMAGE,
        TaskCancel::new(),
    )
    .await
    .unwrap();
    assert!(avatar.content().is_none());
    let (result, _) = tokio::join!(avatar.read_network(), async {
        tokio::time::sleep(Duration::from_millis(100)).await;
        page.evaluate("document.querySelector('.id___test').textContent='ID:22222222'")
            .await
            .unwrap();
    });
    assert!(result.is_err(), "account change rejects downloaded bytes");
    drop(avatar);
    page.evaluate("document.querySelector('.id___test').textContent='ID:12345678'")
        .await
        .unwrap();
    let mut avatar = reader::AvatarPage::open(
        &reader,
        p.target_id().as_ref(),
        "12345678",
        IMAGE,
        TaskCancel::new(),
    )
    .await
    .unwrap();
    let (result, _) = tokio::join!(avatar.read_network(), async {
        tokio::time::sleep(Duration::from_millis(100)).await;
        loaded(&page, "https://s.kwaixiaodian.com/a").await;
    });
    assert!(result.is_err(), "same URL reload rejects bytes");
    drop(avatar);
    let cancel = TaskCancel::new();
    let mut task = reader
        .task_page(p.target_id().as_ref(), cancel.clone(), WAIT)
        .await
        .unwrap();
    let (result, _) = tokio::join!(task.read_public_avatar(IMAGE, WAIT), async {
        tokio::time::sleep(Duration::from_millis(100)).await;
        cancel.cancel();
    });
    assert!(matches!(result, Err(cdp_driver::TaskError::Cancelled)));
    drop(task);
    tokio::time::sleep(Duration::from_millis(700)).await;
    c.avatar_mode.store(5, Ordering::SeqCst);
    let mut task = reader
        .task_page(p.target_id().as_ref(), TaskCancel::new(), WAIT)
        .await
        .unwrap();
    assert!(
        task.read_public_avatar(IMAGE, WAIT).await.is_err(),
        "stalled response deadline"
    );
    drop(task);
    tokio::time::sleep(Duration::from_millis(700)).await;
    c.avatar_mode.store(0, Ordering::SeqCst);
    let mut avatar = reader::AvatarPage::open(
        &reader,
        p.target_id().as_ref(),
        "12345678",
        IMAGE,
        TaskCancel::new(),
    )
    .await
    .unwrap();
    assert_eq!(
        avatar.read_network().await.unwrap(),
        PNG,
        "cleanup permits another successful operation"
    );
    assert_eq!(reader.browser.pages().await.unwrap().len(), 1);
    assert_eq!(
        reader.evaluate("location.href").await.unwrap(),
        "https://s.kwaixiaodian.com/a"
    );
    assert_eq!(c.failed.load(Ordering::SeqCst), 0);
    println!("production avatar: no-CORS success; untrusted/redirect/header oversize/stream oversize/account switch/reload/cancel/deadline rejected; next operation succeeds; no unexpected URL or new target");
    drop(avatar);
    // Positive proxy-chain proof: allow only this public fixture GET past the
    // fixture interceptor, into our non-forwarding local HTTP proxy listener.
    owned.network_sink.set_nonblocking(true).unwrap();
    // Do not close pre-existing sink sockets: doing so can mark the proxy bad.
    c.avatar_mode.store(6, Ordering::SeqCst);
    let mut task = reader
        .task_page(p.target_id().as_ref(), TaskCancel::new(), WAIT)
        .await
        .unwrap();
    let mut probe_responses = p.event_listener::<EventRequestPaused>().await.unwrap();
    let mut failed_network = p
        .event_listener::<chromiumoxide::cdp::browser_protocol::network::EventLoadingFailed>()
        .await
        .unwrap();
    assert!(task
        .read_public_avatar("https://fixture.yximgs.com/proxy-proof.png", WAIT)
        .await
        .is_err());
    if let Ok(Some(e)) = tokio::time::timeout(Duration::from_secs(1), failed_network.next()).await {
        println!("proxy probe failure: {}", e.error_text);
    }
    if let Ok(Some(e)) = tokio::time::timeout(Duration::from_secs(1), probe_responses.next()).await
    {
        println!(
            "proxy response: {:?} {:?}",
            e.response_status_code, e.response_error_reason
        );
    }
    let mut reached_proxy = false;
    while let Ok((mut socket, _)) = owned.network_sink.accept() {
        use std::io::Read;
        socket
            .set_read_timeout(Some(Duration::from_millis(100)))
            .unwrap();
        let mut buf = [0_u8; 2048];
        if let Ok(n) = socket.read(&mut buf) {
            reached_proxy |=
                String::from_utf8_lossy(&buf[..n]).starts_with("CONNECT fixture.yximgs.com:443 ");
        }
    }
    assert!(
        reached_proxy,
        "browser GET must reach the configured proxy, not a host direct client"
    );
    println!("proxy proof: browser sent CONNECT fixture.yximgs.com:443 to owned non-forwarding sink; DNS blocked; no external request forwarded");
}

#[tokio::test]
#[ignore = "explicit IDENTITY_TEST_CHROMIUM required; launches only a new isolated browser"]
async fn identity_in_disposable_chromium() {
    assert_eq!(
        std::env::var("RUN_IDENTITY_BROWSER_FIXTURE").as_deref(),
        Ok("1"),
        "explicit opt-in required"
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
    let version: Value = reqwest::Client::builder()
        .no_proxy()
        .build()
        .unwrap()
        .get(format!("{endpoint}/json/version"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    println!(
        "browser={} executable={} owned_pid={} temp={}",
        version["Browser"],
        std::env::var("IDENTITY_TEST_CHROMIUM").unwrap(),
        owned.child.as_ref().unwrap().id(),
        owned.dir.display()
    );
    let a = session.browser.new_page("about:blank").await.unwrap();
    let b = session.browser.new_page("about:blank").await.unwrap();
    let ia = intercept(&a).await;
    let ib = intercept(&b).await;
    loaded(&a, "https://s.kwaixiaodian.com/a").await;
    loaded(&b, "https://s.kwaixiaodian.com/b").await;
    session.activate_page(a.target_id().as_ref()).await.unwrap(); // only test setup activates
    let before_focus = a
        .evaluate("document.visibilityState")
        .await
        .unwrap()
        .into_value::<String>()
        .unwrap();
    let requests_before = (
        ia.counts.paused.load(Ordering::SeqCst),
        ib.counts.paused.load(Ordering::SeqCst),
    );
    let (ra, rb) = tokio::join!(read(&session, &a), read(&session, &b));
    assert!(matches!(
        reader::detect(&session, TaskCancel::new()).await.unwrap(),
        reader::Detection::Conflict
    ));
    assert_eq!(
        (
            ia.counts.paused.load(Ordering::SeqCst),
            ib.counts.paused.load(Ordering::SeqCst)
        ),
        requests_before,
        "production identity detection must not request anything"
    );
    assert_eq!(ra["platformUserId"], "12345678");
    assert_eq!(ra["nickname"], "Fixture Alpha");
    assert_eq!(ra["avatarUrl"], IMAGE);
    assert_eq!(rb["platformUserId"], "87654321");
    assert_eq!(rb["nickname"], "Fixture Beta");
    assert_ne!(ra["platformUserId"], rb["platformUserId"]);
    assert_eq!(
        session.evaluate("location.href").await.unwrap(),
        "https://s.kwaixiaodian.com/a"
    );
    assert_eq!(
        a.evaluate("document.visibilityState")
            .await
            .unwrap()
            .into_value::<String>()
            .unwrap(),
        before_focus
    );
    assert_eq!(session.browser.pages().await.unwrap().len(), 2);

    // Primitive evidence for a future cache API; no navigation or fetch fallback.
    tokio::time::sleep(Duration::from_millis(200)).await;
    let request_before = ia.counts.network.load(Ordering::SeqCst);
    let paused_before = ia.counts.paused.load(Ordering::SeqCst);
    let tree = a
        .execute(GetResourceTreeParams::default())
        .await
        .unwrap()
        .result
        .frame_tree;
    assert!(tree.resources.iter().any(|r| r.url == IMAGE));
    assert_eq!(ia.counts.paused.load(Ordering::SeqCst), paused_before);
    let resource = a
        .execute(GetResourceContentParams::new(tree.frame.id.clone(), IMAGE))
        .await
        .unwrap()
        .result;
    assert!(resource.base64_encoded);
    assert_eq!(resource.content, PNG);
    tokio::time::sleep(Duration::from_millis(200)).await;
    let after_resource = ia.counts.paused.load(Ordering::SeqCst);
    println!("getResourceContent HIT: Fetch before={paused_before} after={after_resource}; Network before={request_before} after={}", ia.counts.network.load(Ordering::SeqCst));
    // Regression evidence: Chromium re-requests the main document while retrieving
    // this loaded cross-origin image. Network events alone miss this request.
    // This primitive is NOT a cache-only production API.
    assert_eq!(after_resource, paused_before + 1);
    let miss = tokio::time::timeout(
        WAIT,
        a.execute(GetResourceContentParams::new(
            tree.frame.id,
            "https://fixture.yximgs.com/missing.png",
        )),
    )
    .await
    .expect("resource miss deadline");
    assert!(miss.is_err());
    tokio::time::sleep(Duration::from_millis(200)).await;
    println!(
        "getResourceContent MISS: error={:?} Fetch before={} after={}",
        miss.err(),
        after_resource,
        ia.counts.paused.load(Ordering::SeqCst)
    );
    assert_eq!(
        ia.counts.paused.load(Ordering::SeqCst),
        after_resource,
        "missing resource unexpectedly triggered Fetch"
    );
    let paused_before = ia.counts.paused.load(Ordering::SeqCst);
    let request_before = ia.counts.network.load(Ordering::SeqCst);
    let canvas: Value = a.evaluate(r#"(() => { const read = img => { try { const c=document.createElement('canvas');c.width=img.naturalWidth;c.height=img.naturalHeight;c.getContext('2d').drawImage(img,0,0);return c.toDataURL('image/png'); } catch(e) { return e.name; } }; return {tainted:read(document.images[0]),cors:read(document.getElementById('cors'))}; })()"#).await.unwrap().into_value().unwrap();
    assert_eq!(canvas["tainted"], "SecurityError");
    assert!(canvas["cors"]
        .as_str()
        .unwrap()
        .starts_with("data:image/png;base64,"));
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(
        ia.counts.network.load(Ordering::SeqCst),
        request_before,
        "cache/canvas caused a new Network request"
    );
    assert_eq!(
        ia.counts.paused.load(Ordering::SeqCst),
        paused_before,
        "cache/canvas caused a new Fetch request"
    );
    println!("cache resource: base64=true bytes(base64)={} network_before={} network_after={} canvas_no_cors=SecurityError canvas_cors=PNG", resource.content.len(), request_before, ia.counts.network.load(Ordering::SeqCst));

    // Exercise the actual production avatar reader, not a copy of its script.
    let before_avatar = (
        ia.counts.paused.load(Ordering::SeqCst),
        ia.counts.network.load(Ordering::SeqCst),
    );
    {
        let mut avatar = reader::AvatarPage::open(
            &session,
            a.target_id().as_ref(),
            "12345678",
            IMAGE,
            TaskCancel::new(),
        )
        .await
        .unwrap();
        assert!(
            avatar.content().is_none(),
            "tainted avatar must be unavailable, not refetched"
        );
        avatar.revalidate().await.unwrap();
    }
    // Move an ALREADY loaded CORS image into the fixture header; never assign src.
    a.evaluate("window.fixtureOriginal = document.querySelector('.seller-main-avatar img'); window.fixtureOriginal.replaceWith(document.getElementById('cors'))").await.unwrap();
    {
        let mut avatar = reader::AvatarPage::open(
            &session,
            a.target_id().as_ref(),
            "12345678",
            "https://fixture.yximgs.com/cors.png",
            TaskCancel::new(),
        )
        .await
        .unwrap();
        assert!(avatar.content().unwrap().starts_with("iVBORw0KGgo"));
        avatar.revalidate().await.unwrap();
        // Simulate an independent page actor changing identity while the lease lives.
        a.evaluate("document.querySelector('.id___test').textContent = 'ID:22222222'")
            .await
            .unwrap();
        assert!(avatar.revalidate().await.is_err());
    }
    a.evaluate("document.querySelector('.id___test').textContent = 'ID:12345678'; const cors=document.getElementById('cors'); cors.replaceWith(window.fixtureOriginal); document.body.append(cors); delete window.fixtureOriginal").await.unwrap();
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(
        (
            ia.counts.paused.load(Ordering::SeqCst),
            ia.counts.network.load(Ordering::SeqCst)
        ),
        before_avatar,
        "production avatar reader must not issue any request"
    );
    println!("production AvatarPage: no-CORS=None; loaded CORS=PNG; changed ID rejected; Fetch/Network unchanged");

    for path in ["no-id", "invalid"] {
        loaded(&b, &format!("https://s.kwaixiaodian.com/{path}")).await;
        let result = read(&session, &b).await;
        assert!(result["platformUserId"].is_null());
        assert!(result["nickname"].is_null());
        assert!(result["avatarUrl"].is_null());
    }
    for invalid in [
        "ID:1234",
        "ID:12345x",
        "ID:１２３４５",
        "ID:123456789012345678901234567890123",
        "商品 ID:12345678",
    ] {
        let script = format!(
            "document.querySelector('.id___test').textContent = {}",
            serde_json::to_string(invalid).unwrap()
        );
        b.evaluate(script).await.unwrap();
        assert!(
            read(&session, &b).await["platformUserId"].is_null(),
            "{invalid}"
        );
    }
    a.evaluate("document.querySelector('.id___test').textContent = ''")
        .await
        .unwrap();
    assert!(matches!(
        reader::detect(&session, TaskCancel::new()).await.unwrap(),
        reader::Detection::NoId
    ));
    // Restore only local fixture DOM; production detector never mutates it.
    a.evaluate("document.querySelector('.id___test').textContent = 'ID:12345678'")
        .await
        .unwrap();
    loaded(&b, "https://s.kwaixiaodian.com.evil.invalid/wrong").await;
    let wrong = read(&session, &b).await;
    assert!(wrong["platformUserId"].is_null());
    assert!(reader::decode(wrong).is_err());
    {
        let mut stale = reader::AvatarPage::open(
            &session,
            a.target_id().as_ref(),
            "12345678",
            IMAGE,
            TaskCancel::new(),
        )
        .await
        .unwrap();
        // Simulate an independent actor reloading the same synthetic URL/account.
        loaded(&a, "https://s.kwaixiaodian.com/a").await;
        let before = ia.counts.paused.load(Ordering::SeqCst);
        assert!(
            stale.revalidate().await.is_err(),
            "new document epoch must invalidate the old observation"
        );
        assert_eq!(ia.counts.paused.load(Ordering::SeqCst), before);
        println!("production AvatarPage: same-URL reload rejected via documentEpoch; revalidation issued no request");
    }
    // Unknown URL must be failed, never continued onto a real network.
    assert_eq!(
        a.evaluate(
            "fetch('https://blocked.invalid/probe').then(() => 'unexpected', () => 'blocked')"
        )
        .await
        .unwrap()
        .into_value::<String>()
        .unwrap(),
        "blocked"
    );
    assert_eq!(ia.counts.failed.load(Ordering::SeqCst), 1);
    assert_eq!(
        session.evaluate("location.href").await.unwrap(),
        "https://s.kwaixiaodian.com/a"
    );
    assert_eq!(session.browser.pages().await.unwrap().len(), 2);
    for (name, interception) in [("a", &ia), ("b", &ib)] {
        let c = &interception.counts;
        assert_eq!(
            c.paused.load(Ordering::SeqCst),
            c.fulfilled.load(Ordering::SeqCst) + c.failed.load(Ordering::SeqCst)
        );
        assert!(c.fulfilled.load(Ordering::SeqCst) >= 3);
        assert!(
            interception.tasks.iter().all(|t| !t.is_finished()),
            "interception worker exited"
        );
        println!(
            "target={name} paused={} fulfilled={} failed={} network={}",
            c.paused.load(Ordering::SeqCst),
            c.fulfilled.load(Ordering::SeqCst),
            c.failed.load(Ordering::SeqCst),
            c.network.load(Ordering::SeqCst)
        );
    }
    drop(ia);
    drop(ib);
    drop(session);
    owned.cleanup();
    assert!(!owned.dir.exists(), "temporary profile must be removed");
    println!(
        "cleanup: own child killed/reaped; temporary profile removed; no screenshots or user data"
    );
}
