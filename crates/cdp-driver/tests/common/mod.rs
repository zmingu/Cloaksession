//! Browser-free wire tests: a local HTTP/WebSocket CDP peer drives the real
//! chromiumoxide handler. No Chromium process or user endpoint is accessed.
//! The peer models routing/lifecycle/errors, not DOM or browser rendering.
#![allow(dead_code)] // Each integration binary uses a different subset of controls.
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use cdp_driver::session::BrowserSession;
use futures::{SinkExt, StreamExt};
use multizen_core::BrowserEngine;
use serde_json::{json, Value};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::task::JoinHandle;
use tokio_tungstenite::{accept_async, tungstenite::Message, WebSocketStream};

pub enum EvalReply {
    Value(Value),
    JsError,
    Stall,
}

#[derive(Default)]
struct State {
    commands: Vec<Value>,
    failure: Option<(String, Option<String>)>,
    stall: Option<(String, Option<String>)>,
    evaluations: VecDeque<EvalReply>,
    changed: Arc<tokio::sync::Notify>,
}

pub struct Peer {
    state: Arc<Mutex<State>>,
    task: JoinHandle<()>,
}

impl Drop for Peer {
    fn drop(&mut self) {
        self.task.abort();
    }
}

impl Peer {
    pub async fn connect() -> (Self, BrowserSession) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let state = Arc::new(Mutex::new(State::default()));
        let server_state = state.clone();
        let task = tokio::spawn(async move {
            let (mut http, _) = listener.accept().await.unwrap();
            let mut request = Vec::new();
            while !request.ends_with(b"\r\n\r\n") {
                let byte = http.read_u8().await.unwrap();
                request.push(byte);
                assert!(request.len() < 8192);
            }
            assert!(String::from_utf8_lossy(&request).starts_with("GET /json/version "));
            let body =
                json!({"webSocketDebuggerUrl": format!("ws://{addr}/devtools/browser/test")})
                    .to_string();
            http.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).as_bytes()).await.unwrap();
            drop(http);
            let (socket, _) = listener.accept().await.unwrap();
            socket.set_nodelay(true).unwrap();
            let ws = accept_async(socket).await.unwrap();
            serve(ws, server_state).await;
        });
        let peer = Self { state, task };
        let session = tokio::time::timeout(
            Duration::from_secs(5),
            BrowserSession::connect(&format!("http://{addr}"), BrowserEngine::Chromix),
        )
        .await
        .expect("connect deadline")
        .unwrap();
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                if session.browser.pages().await.unwrap().len() == 2 {
                    // An evaluation waits for actual target initialization as well.
                    session
                        .bind_page("a")
                        .await
                        .unwrap()
                        .evaluate("identity")
                        .await
                        .unwrap();
                    session
                        .bind_page("b")
                        .await
                        .unwrap()
                        .evaluate("identity")
                        .await
                        .unwrap();
                    break;
                }
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .expect("target initialization deadline");
        peer.clear();
        (peer, session)
    }

    pub fn clear(&self) {
        self.state.lock().unwrap().commands.clear();
    }
    pub fn commands(&self) -> Vec<Value> {
        self.state.lock().unwrap().commands.clone()
    }
    pub fn fail(&self, method: &str, kind: Option<&str>) {
        self.state.lock().unwrap().failure = Some((method.into(), kind.map(str::to_owned)));
    }
    pub fn stall(&self, method: &str, kind: Option<&str>) {
        self.state.lock().unwrap().stall = Some((method.into(), kind.map(str::to_owned)));
    }
    pub fn evaluations(&self, replies: Vec<EvalReply>) {
        self.state.lock().unwrap().evaluations = replies.into();
    }
    pub async fn wait_for(&self, method: &str, count: usize) {
        let changed = self.state.lock().unwrap().changed.clone();
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let notified = changed.notified();
                tokio::pin!(notified);
                notified.as_mut().enable();
                if self
                    .commands()
                    .iter()
                    .filter(|c| c["method"] == method)
                    .count()
                    >= count
                {
                    return;
                }
                notified.await;
            }
        })
        .await
        .expect("expected command did not reach peer");
    }
}

fn info(id: &str, url: &str) -> Value {
    json!({"targetId":id,"type":"page","title":id,"url":url,"attached":true,"canAccessOpener":false})
}

fn frame(id: &str, url: &str, loader: &str) -> Value {
    let value = json!({"id":format!("frame-{id}"),"loaderId":loader,"url":url,
        "domainAndRegistry":"test.invalid","securityOrigin":"https://test.invalid","mimeType":"text/html",
        "secureContextType":"Secure","crossOriginIsolatedContextType":"NotIsolated","gatedAPIFeatures":[]});
    // The real handler ignores malformed initialization responses. Validate this
    // fixture against the same generated protocol type so drift fails promptly.
    serde_json::from_value::<chromiumoxide::cdp::browser_protocol::page::Frame>(value.clone())
        .unwrap();
    value
}

async fn send(ws: &mut WebSocketStream<TcpStream>, value: Value) {
    ws.send(Message::Text(value.to_string())).await.unwrap();
}

async fn loaded(ws: &mut WebSocketStream<TcpStream>, id: &str, loader: &str) {
    for name in ["init", "DOMContentLoaded", "load"] {
        send(ws, json!({"method":"Page.lifecycleEvent","sessionId":format!("session-{id}"),
            "params":{"frameId":format!("frame-{id}"),"loaderId":loader,"name":name,"timestamp":1.0}})).await;
    }
}

async fn serve(mut ws: WebSocketStream<TcpStream>, state: Arc<Mutex<State>>) {
    let mut pages: HashMap<String, String> = ["a", "b"]
        .into_iter()
        .map(|id| (id.to_owned(), format!("https://test.invalid/{id}")))
        .collect();
    let mut next_id = 0;
    while let Some(Ok(message)) = ws.next().await {
        let Message::Text(text) = message else {
            continue;
        };
        let command: Value = serde_json::from_str(&text).unwrap();
        let method = command["method"].as_str().unwrap();
        let params = &command["params"];
        let target = command["sessionId"]
            .as_str()
            .unwrap_or("")
            .strip_prefix("session-")
            .unwrap_or("");
        let (failure, stall) = {
            let mut s = state.lock().unwrap();
            s.commands.push(command.clone());
            s.changed.notify_waiters();
            let matches = |rule: &Option<(String, Option<String>)>| {
                rule.as_ref().is_some_and(|(m, kind)| {
                    m == method
                        && kind
                            .as_ref()
                            .is_none_or(|k| params["type"].as_str() == Some(k.as_str()))
                })
            };
            let failure = matches(&s.failure);
            let stall = matches(&s.stall);
            if failure {
                s.failure.take();
            }
            if stall {
                s.stall.take();
            }
            (failure, stall)
        };
        if stall {
            continue;
        }
        let mut response = json!({"id":command["id"],"result":{}});
        if let Some(session) = command.get("sessionId") {
            response["sessionId"] = session.clone();
        }
        if failure || (!target.is_empty() && !pages.contains_key(target)) {
            response.as_object_mut().unwrap().remove("result");
            response["error"] = json!({"code":-32000,"message":"mock CDP failure"});
            send(&mut ws, response).await;
            continue;
        }
        match method {
            "Target.setDiscoverTargets" => {
                for (id, url) in &pages {
                    send(&mut ws, json!({"method":"Target.targetCreated","params":{"targetInfo":info(id,url)}})).await;
                }
            }
            "Target.attachToTarget" => {
                let id = params["targetId"].as_str().unwrap();
                response["result"] = json!({"sessionId":format!("session-{id}")});
                send(&mut ws, json!({"method":"Target.attachedToTarget","params":{
                    "sessionId":format!("session-{id}"),"targetInfo":info(id,&pages[id]),"waitingForDebugger":false}})).await;
            }
            "Target.createTarget" => {
                next_id += 1;
                let id = format!("new-{next_id}");
                let url = params["url"].as_str().unwrap().to_owned();
                send(
                    &mut ws,
                    json!({"method":"Target.targetCreated","params":{"targetInfo":info(&id,&url)}}),
                )
                .await;
                pages.insert(id.clone(), url);
                response["result"] = json!({"targetId":id});
            }
            "Page.close" => {
                pages.remove(target);
                send(
                    &mut ws,
                    json!({"method":"Target.targetDestroyed","params":{"targetId":target}}),
                )
                .await;
            }
            "Target.closeTarget" => {
                let id = params["targetId"].as_str().unwrap();
                pages.remove(id);
                send(
                    &mut ws,
                    json!({"method":"Target.targetDestroyed","params":{"targetId":id}}),
                )
                .await;
                response["result"] = json!({"success":true});
            }
            "Target.getTargets" => {
                response["result"] = json!({"targetInfos": pages.iter()
                    .map(|(id, url)| info(id, url)).collect::<Vec<_>>()});
            }
            "Page.getFrameTree" => {
                response["result"] =
                    json!({"frameTree":{"frame":frame(target,&pages[target],"initial")}});
            }
            "Page.setLifecycleEventsEnabled" => {
                loaded(&mut ws, target, "initial").await;
            }
            "Page.addScriptToEvaluateOnNewDocument" => {
                response["result"] = json!({"identifier":"script"})
            }
            "Page.createIsolatedWorld" => response["result"] = json!({"executionContextId":1}),
            "Page.navigate" => {
                let url = params["url"].as_str().unwrap();
                if url.ends_with("/stall") {
                    continue;
                }
                pages.insert(target.to_owned(), url.to_owned());
                response["result"] =
                    json!({"frameId":format!("frame-{target}"),"loaderId":"navigated"});
                send(
                    &mut ws,
                    json!({"method":"Page.frameNavigated","sessionId":format!("session-{target}"),
                    "params":{"frame":frame(target,url,"navigated"),"type":"Navigation"}}),
                )
                .await;
                loaded(&mut ws, target, "navigated").await;
            }
            "Page.captureScreenshot" => {
                response["result"] = json!({"data":if target == "a" {"QQ=="} else {"Qg=="}})
            }
            "Runtime.evaluate" | "Runtime.callFunctionOn" => {
                let expr = params["expression"]
                    .as_str()
                    .or_else(|| params["functionDeclaration"].as_str())
                    .unwrap();
                let scripted = state.lock().unwrap().evaluations.pop_front();
                if matches!(scripted, Some(EvalReply::Stall)) {
                    continue;
                }
                let js_error = matches!(scripted, Some(EvalReply::JsError));
                let value = if let Some(EvalReply::Value(value)) = scripted {
                    value
                } else if expr.contains("getBoundingClientRect") {
                    if expr.contains("#missing") {
                        Value::Null
                    } else {
                        json!({"x":10,"y":20})
                    }
                } else if expr.contains("el.focus()") {
                    json!(!expr.contains("#missing"))
                } else if expr.contains("location.href") {
                    json!({"url":pages[target],"title":target})
                } else if expr.contains("innerText") {
                    json!(format!("text-{target}"))
                } else {
                    json!(target)
                };
                let kind = if value.is_boolean() {
                    "boolean"
                } else if value.is_string() {
                    "string"
                } else {
                    "object"
                };
                response["result"] = json!({"result":{"type":kind,"value":value}});
                if expr == "throw_error" || js_error {
                    response["result"] = json!({"result":{"type":"undefined"},"exceptionDetails":{
                        "exceptionId":1,"text":"mock JS exception","lineNumber":0,"columnNumber":0}});
                }
            }
            // Explicitly enumerated initialization and input commands; unexpected
            // new protocol requirements fail the peer instead of silently passing.
            "Page.enable"
            | "Runtime.enable"
            | "Network.enable"
            | "Security.setIgnoreCertificateErrors"
            | "Network.setCacheDisabled"
            | "Target.setAutoAttach"
            | "Performance.enable"
            | "Log.enable"
            | "Page.setInterceptFileChooserDialog"
            | "Target.activateTarget"
            | "Input.dispatchMouseEvent"
            | "Input.dispatchKeyEvent" => {}
            other => panic!("unexpected CDP method: {other}"),
        }
        send(&mut ws, response).await;
    }
}
