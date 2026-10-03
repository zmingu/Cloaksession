//! Scene play engine — Rust port of jieger `tasks/scenePlay/index.ts`.
//!
//! Model: each scene is an ordered list of lines `(message,
//! time_offset_sec, action_type)`; `play_scene` fans every line out to an
//! independent `tokio::spawn` sleeping until its absolute trigger timestamp.
//! Accounts come from the ready sub-account pool (round-robin); with
//! `allow_dynamic_pool` the pool is re-resolved at each line's fire time.
//!
//! Every dispatch appends one `sub_account_interactions` row and emits
//! `scene:started` / `scene:progress` / `scene:finished` (colon-style event
//! names per the Tauri IPC spec). Cancellation is cooperative via
//! `TaskCancel`: pending lines return without dispatching.
//!
//! SQLite access rides the dedicated launcher thread through
//! `LauncherCmd::Scene`, mirroring `account_init::InitCmd`. Browser actions
//! run small `Runtime.evaluate` scripts against the account session's active
//! page; like/follow check the already-done state first (idempotent).

use std::collections::HashMap;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc, Mutex as StdMutex, OnceLock,
};

use cdp_driver::TaskCancel;
use multizen_core::{BusinessAccountKind, MultizenError, Result};
use profile_manager::scenes::{
    RecordInteractionInput, Scene, SceneLineAction, SubAccountInteraction, TriggerMode,
};
use profile_manager::ProfileManager;
use serde::{Deserialize, Serialize};
use tokio::sync::{oneshot, Mutex as TokioMutex};

use super::{LauncherCmd, TauriBrowserDriver};

// ---------------------------------------------------------------------------
// Launcher-thread bridge
// ---------------------------------------------------------------------------

type SceneOperation = Box<dyn FnOnce(&ProfileManager) + Send>;

pub(super) struct SceneCmd {
    operation: SceneOperation,
}

pub(super) async fn handle(cmd: SceneCmd, pm: &ProfileManager) {
    (cmd.operation)(pm);
}

fn storage_error(message: &str) -> MultizenError {
    MultizenError::Mcp(message.into())
}

// ---------------------------------------------------------------------------
// Pure scheduling core (no IO; unit-tested)
// ---------------------------------------------------------------------------

/// One account ready to receive scene lines: a `kuaishou-sub` business
/// account whose browser session is alive and showing a live room.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScenePoolMember {
    pub account_id: String,
    pub profile_id: String,
    pub name: String,
}

/// A line pinned to an account and an absolute fire time (ms epoch).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScheduledItem {
    pub line_id: i64,
    pub ord: i64,
    pub account_id: String,
    pub profile_id: String,
    pub account_name: String,
    pub message: String,
    pub action_type: SceneLineAction,
    /// Absolute trigger timestamp (ms); the ±300ms jitter is already applied.
    pub trigger_at_ms: i64,
}

/// `local-time` mode: next absolute timestamp (ms) for today's
/// `secs_of_day`. A time that already passed today rolls over to tomorrow
/// (single shot, no repeat).
pub fn next_local_timestamp(secs_of_day: i64) -> i64 {
    let now = chrono::Local::now();
    let midnight = now
        .date_naive()
        .and_hms_opt(0, 0, 0)
        .and_then(|ndt| {
            use chrono::TimeZone;
            chrono::Local
                .from_local_datetime(&ndt)
                .single()
                .or_else(|| chrono::Local.from_local_datetime(&ndt).earliest())
        })
        .unwrap_or(now);
    let mut target = midnight + chrono::Duration::seconds(secs_of_day);
    if target <= now {
        target += chrono::Duration::days(1);
    }
    target.timestamp_millis()
}

pub fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

/// Per-line ±300ms jitter, softening the round-robin rhythm (jieger B1
/// anti-pattern note).
fn jitter_ms() -> i64 {
    use rand::Rng;
    rand::thread_rng().gen_range(0..600) - 300
}

fn base_trigger_ms(
    mode: TriggerMode,
    time_offset_sec: i64,
    start_ms: i64,
) -> i64 {
    match mode {
        TriggerMode::RelativeTime => {
            start_ms.saturating_add(time_offset_sec.saturating_mul(1000))
        }
        TriggerMode::LocalTime => next_local_timestamp(time_offset_sec),
    }
}

/// Fixed-pool schedule: line `idx` goes to `pool[idx % pool.len()]`
/// (round-robin). Empty pool yields an empty schedule; the caller
/// (`play_scene`) rejects that case unless dynamic pooling is on.
pub fn build_schedule(
    scene: &Scene,
    pool: &[ScenePoolMember],
    start_ms: i64,
) -> Vec<ScheduledItem> {
    if pool.is_empty() {
        return Vec::new();
    }
    scene
        .lines
        .iter()
        .enumerate()
        .map(|(idx, line)| {
            let member = &pool[idx % pool.len()];
            ScheduledItem {
                line_id: line.id,
                ord: line.ord,
                account_id: member.account_id.clone(),
                profile_id: member.profile_id.clone(),
                account_name: member.name.clone(),
                message: line.message.clone(),
                action_type: line.action_type,
                trigger_at_ms: base_trigger_ms(scene.trigger_mode, line.time_offset_sec, start_ms)
                    + jitter_ms(),
            }
        })
        .collect()
}

/// Dynamic-pool schedule: same trigger math, but the account is left blank
/// (`account_id` = `__dynamic__`, as in jieger) and resolved at fire time.
pub fn build_dynamic_schedule(scene: &Scene, start_ms: i64) -> Vec<ScheduledItem> {
    scene
        .lines
        .iter()
        .map(|line| ScheduledItem {
            line_id: line.id,
            ord: line.ord,
            account_id: "__dynamic__".to_string(),
            profile_id: String::new(),
            account_name: "等待小号".to_string(),
            message: line.message.clone(),
            action_type: line.action_type,
            trigger_at_ms: base_trigger_ms(scene.trigger_mode, line.time_offset_sec, start_ms)
                + jitter_ms(),
        })
        .collect()
}

/// Kuaishou audience-side live-room URL (jieger `isKuaishouLiveRoomUrl`).
pub fn is_live_room_url(url: &str) -> bool {
    url.contains("live.kuaishou.com/") || url.contains("www.kuaishou.com/live/")
}

// ---------------------------------------------------------------------------
// Playback sessions
// ---------------------------------------------------------------------------

/// `play_scene` options (also the `scene_play` IPC payload).
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaySceneOptions {
    /// Anchor for `relative-time` mode; defaults to now.
    #[serde(default)]
    pub start_at_ms: Option<i64>,
    /// Start with no pool and resolve the latest pool at each fire time.
    #[serde(default)]
    pub allow_dynamic_pool: bool,
    /// Override the scene's own `group_id` filter (`None` = keep the scene's).
    #[serde(default)]
    pub group_id: Option<String>,
}

/// `play_scene` result (also the `scene_play` IPC return).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayStarted {
    pub scene_id: i64,
    pub scheduled_count: usize,
    pub schedule: Vec<ScheduledItem>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SceneStartedPayload {
    pub scene_id: i64,
    pub schedule: Vec<ScheduledItem>,
    pub started_at: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SceneProgressPayload {
    pub scene_id: i64,
    pub sent_count: usize,
    pub total_count: usize,
    pub last_item: ScheduledItem,
    pub ok: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SceneFinishedPayload {
    pub scene_id: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stopped: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

struct PlaySession {
    scene_id: i64,
    started_at_ms: i64,
    total: usize,
    sent: AtomicUsize,
    cancel: TaskCancel,
    dynamic_pool: bool,
    group_id: Option<String>,
    next_pool_index: StdMutex<usize>,
}

static SESSIONS: OnceLock<TokioMutex<HashMap<i64, Arc<PlaySession>>>> = OnceLock::new();

fn sessions() -> &'static TokioMutex<HashMap<i64, Arc<PlaySession>>> {
    SESSIONS.get_or_init(|| TokioMutex::new(HashMap::new()))
}

// ---------------------------------------------------------------------------
// Browser actions (active page of the account session)
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, Default)]
struct JsActionResult {
    #[serde(default)]
    ok: bool,
    #[serde(default)]
    already: bool,
    #[serde(default)]
    error: Option<String>,
}

fn parse_action_result(value: serde_json::Value) -> (bool, bool, Option<String>) {
    let text = match value {
        serde_json::Value::String(s) => s,
        other => other.to_string(),
    };
    match serde_json::from_str::<JsActionResult>(&text) {
        Ok(r) => (r.ok, r.already, r.error),
        Err(e) => (false, false, Some(format!("动作结果解析失败：{e}"))),
    }
}

async fn active_page_url(session: &cdp_driver::session::BrowserSession) -> Option<String> {
    session
        .evaluate("location.href")
        .await
        .ok()?
        .as_str()
        .map(|s| s.to_string())
}

/// Send one danmaku from the session's active page: pick the best chat
/// input, fill it with input events, then click the send button (Enter-key
/// fallback when the button is missing).
async fn send_danmaku(
    session: &cdp_driver::session::BrowserSession,
    message: &str,
) -> (bool, bool, Option<String>) {
    const BODY: &str = r#"(function(M){
function out(ok,error){return JSON.stringify({ok:!!ok,already:false,error:error||null});}
if(typeof M!=='string'||!M.trim())return out(false,'消息内容为空');
try{
var scope=(document.body&&document.body.innerText||'')+' '+(document.title||'');
var keys=['拖动滑块','滑块验证','请完成验证','安全验证','行为验证','拼图验证'];
for(var i=0;i<keys.length;i++){if(scope.indexOf(keys[i])>=0)return out(false,'检测到平台安全验证，请先在浏览器完成验证后再试');}
var nodes=document.querySelectorAll('textarea,input,[contenteditable="true"]');
var best=null,bestScore=-1e12;
for(var j=0;j<nodes.length;j++){var el=nodes[j];
try{
var r=el.getBoundingClientRect();if(!r||r.width<20||r.height<8)continue;
var s=r.top;
var ph=el.getAttribute? (el.getAttribute('placeholder')||''):'';
if(ph.indexOf('弹幕')>=0||ph.indexOf('说点什么')>=0||ph.indexOf('评论')>=0)s+=2000;
if(el.isContentEditable)s+=500;
var tag=(el.tagName||'').toLowerCase();if(tag==='textarea'||tag==='input')s+=200;
if(s>bestScore){bestScore=s;best=el;}
}catch(e){}}
if(!best)return out(false,'未找到评论输入框');
try{best.focus();}catch(e){}
try{
if(best.isContentEditable){
best.textContent='';
try{best.dispatchEvent(new InputEvent('input',{bubbles:true}));}catch(e){best.dispatchEvent(new Event('input',{bubbles:true}));}
best.textContent=M;
try{best.dispatchEvent(new InputEvent('input',{bubbles:true,data:M,inputType:'insertText'}));}catch(e){best.dispatchEvent(new Event('input',{bubbles:true}));}
}else if('value' in best){
best.value='';
best.dispatchEvent(new Event('input',{bubbles:true}));
best.value=M;
best.dispatchEvent(new Event('input',{bubbles:true}));
best.dispatchEvent(new Event('change',{bubbles:true}));
}else{best.textContent=M;}
}catch(e){return out(false,'写入评论失败：'+((e&&e.message)||e));}
var send=null;var btns=document.querySelectorAll('button');
for(var k=0;k<btns.length;k++){var t=(btns[k].innerText||'').trim();
if(t==='发送'||t.indexOf('发送')>=0){try{var br=btns[k].getBoundingClientRect();if(br&&br.width>0&&br.height>0){send=btns[k];break;}}catch(e){}}}
try{if(send){send.click();}else if(best){best.dispatchEvent(new KeyboardEvent('keydown',{key:'Enter',code:'Enter',keyCode:13,bubbles:true}));}}catch(e){}
return out(true);
}catch(e){return out(false,'弹幕执行异常：'+((e&&e.message)||e));}
})"#;
    let msg_json = serde_json::to_string(message).unwrap_or_else(|_| "\"\"".to_string());
    match session.evaluate(&format!("{BODY}({msg_json})")).await {
        Ok(v) => parse_action_result(v),
        Err(e) => (false, false, Some(e.to_string())),
    }
}

/// Click like once. Idempotent: an already-liked indicator (`aria-pressed`
/// / liked class) returns `already=true` without clicking.
async fn click_like(
    session: &cdp_driver::session::BrowserSession,
) -> (bool, bool, Option<String>) {
    const BODY: &str = r#"(function(){
function out(ok,already,error){return JSON.stringify({ok:!!ok,already:!!already,error:error||null});}
try{
var btn=null;
var labelled=document.querySelectorAll('[aria-label]');
for(var i=0;i<labelled.length;i++){var a=labelled[i].getAttribute('aria-label')||'';
if(a.indexOf('点赞')>=0||a==='赞'){btn=labelled[i];break;}}
if(!btn){var btns=document.querySelectorAll('button');
for(var j=0;j<btns.length;j++){var t=(btns[j].innerText||'').trim();
if(t==='赞'||t==='点赞'){btn=btns[j];break;}}}
if(!btn)return out(false,false,'未找到点赞按钮');
var pressed=btn.getAttribute?btn.getAttribute('aria-pressed'):null;
var cls='';try{cls=(btn.className||'').toString();}catch(e){}
if(pressed==='true'||cls.indexOf('liked')>=0||cls.indexOf('is-liked')>=0)return out(true,true);
try{btn.click();}catch(e){return out(false,false,'点赞点击失败：'+((e&&e.message)||e));}
return out(true,false);
}catch(e){return out(false,false,'点赞执行异常：'+((e&&e.message)||e));}
})()"#;
    match session.evaluate(BODY).await {
        Ok(v) => parse_action_result(v),
        Err(e) => (false, false, Some(e.to_string())),
    }
}

/// Follow the room's anchor. Idempotent: a visible 已关注 badge returns
/// `already=true` without clicking.
async fn click_follow(
    session: &cdp_driver::session::BrowserSession,
) -> (bool, bool, Option<String>) {
    const BODY: &str = r#"(function(){
function out(ok,already,error){return JSON.stringify({ok:!!ok,already:!!already,error:error||null});}
try{
function visible(el){try{var r=el.getBoundingClientRect();return r&&r.width>0&&r.height>0;}catch(e){return false;}}
var all=document.querySelectorAll('button,[role="button"]');
for(var i=0;i<all.length;i++){var t=(all[i].innerText||'').trim();
if(t.indexOf('已关注')>=0&&visible(all[i]))return out(true,true);}
var btn=null;
for(var j=0;j<all.length;j++){var s=(all[j].innerText||'').trim();
if((s==='关注'||s.indexOf('关注主播')>=0)&&visible(all[j])){btn=all[j];break;}}
if(!btn){var labelled=document.querySelectorAll('[aria-label]');
for(var k=0;k<labelled.length;k++){var a=labelled[k].getAttribute('aria-label')||'';
if(a.indexOf('关注')>=0&&a.indexOf('已')<0&&visible(labelled[k])){btn=labelled[k];break;}}}
if(!btn)return out(false,false,'未找到关注按钮');
try{btn.click();}catch(e){return out(false,false,'关注点击失败：'+((e&&e.message)||e));}
return out(true,false);
}catch(e){return out(false,false,'关注执行异常：'+((e&&e.message)||e));}
})()"#;
    match session.evaluate(BODY).await {
        Ok(v) => parse_action_result(v),
        Err(e) => (false, false, Some(e.to_string())),
    }
}

// ---------------------------------------------------------------------------
// Driver implementation
// ---------------------------------------------------------------------------

impl TauriBrowserDriver {
    async fn scene_db<T: Send + 'static>(
        &self,
        operation: impl FnOnce(&ProfileManager) -> Result<T> + Send + 'static,
    ) -> Result<T> {
        let (resp, receive) = oneshot::channel();
        let cmd = SceneCmd {
            operation: Box::new(move |pm| {
                if resp.is_closed() {
                    return;
                }
                let _ = resp.send(operation(pm));
            }),
        };
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            self.launcher_tx
                .send(LauncherCmd::Scene(cmd))
                .await
                .map_err(|_| storage_error("场景存储线程不可用"))?;
            receive
                .await
                .map_err(|_| storage_error("场景存储响应已取消"))?
        })
        .await
        .map_err(|_| storage_error("场景存储等待超时"))?
    }

    // --- Scene / line CRUD (thin launcher-thread forwards) -----------------

    pub async fn scene_create(
        &self,
        name: String,
        trigger_mode: TriggerMode,
        group_id: Option<String>,
    ) -> Result<Scene> {
        self.scene_db(move |pm| pm.scene_create(&name, trigger_mode, group_id))
            .await
    }

    pub async fn scene_get(&self, id: i64) -> Result<Option<Scene>> {
        self.scene_db(move |pm| pm.scene_get(id)).await
    }

    pub async fn scene_list(&self) -> Result<Vec<Scene>> {
        self.scene_db(move |pm| pm.scene_list()).await
    }

    pub async fn scene_update(
        &self,
        id: i64,
        name: Option<String>,
        trigger_mode: Option<TriggerMode>,
        group_id: Option<Option<String>>,
    ) -> Result<Scene> {
        self.scene_db(move |pm| pm.scene_update(id, name, trigger_mode, group_id))
            .await
    }

    pub async fn scene_delete(&self, id: i64) -> Result<()> {
        self.scene_db(move |pm| pm.scene_delete(id)).await
    }

    pub async fn scene_add_line(
        &self,
        scene_id: i64,
        message: String,
        time_offset_sec: i64,
        action_type: SceneLineAction,
    ) -> Result<profile_manager::scenes::SceneLine> {
        self.scene_db(move |pm| pm.scene_add_line(scene_id, &message, time_offset_sec, action_type))
            .await
    }

    pub async fn scene_update_line(
        &self,
        id: i64,
        message: Option<String>,
        time_offset_sec: Option<i64>,
        action_type: Option<SceneLineAction>,
    ) -> Result<profile_manager::scenes::SceneLine> {
        self.scene_db(move |pm| pm.scene_update_line(id, message, time_offset_sec, action_type))
            .await
    }

    pub async fn scene_delete_line(&self, id: i64) -> Result<()> {
        self.scene_db(move |pm| pm.scene_delete_line(id)).await
    }

    pub async fn scene_reorder_lines(
        &self,
        scene_id: i64,
        ordered_ids: Vec<i64>,
    ) -> Result<Vec<profile_manager::scenes::SceneLine>> {
        self.scene_db(move |pm| pm.scene_reorder_lines(scene_id, &ordered_ids))
            .await
    }

    pub async fn scene_record_interaction(
        &self,
        input: RecordInteractionInput,
    ) -> Result<SubAccountInteraction> {
        self.scene_db(move |pm| pm.record_interaction(&input)).await
    }

    // --- Ready pool --------------------------------------------------------
    //
    // jieger `getReadySubAccountPool`: `usage='sub'` accounts whose
    // BrowserSession is connected and whose page shows a live room, filtered
    // by group. Here `kind=kuaishou-sub` business accounts bound to a profile
    // provide the account set; "logged in" is a live registry session and
    // "in live room" is any tab whose URL matches the audience-side pattern.

    pub async fn get_ready_sub_account_pool(
        &self,
        group_id: Option<&str>,
    ) -> Result<Vec<ScenePoolMember>> {
        let accounts = self.business_accounts_list().await?;
        let groups: HashMap<String, Option<String>> = self
            .list_profiles()
            .await?
            .into_iter()
            .map(|p| (p.id, p.group))
            .collect();
        let mut pool = Vec::new();
        for account in accounts
            .iter()
            .filter(|a| a.kind == BusinessAccountKind::KuaishouSub)
        {
            let Some(profile_id) = account.profile_id.as_deref() else {
                continue;
            };
            if let Some(want) = group_id {
                let actual = groups.get(profile_id).and_then(|g| g.as_deref());
                if actual != Some(want) {
                    continue;
                }
            }
            let Some(session) = self.registry.get(profile_id).await else {
                continue;
            };
            if !session_in_live_room(&session).await {
                continue;
            }
            pool.push(ScenePoolMember {
                account_id: account.id.clone(),
                profile_id: profile_id.to_string(),
                name: account.display_name.clone(),
            });
        }
        // Deterministic order for round-robin stability across polls.
        pool.sort_by(|a, b| a.profile_id.cmp(&b.profile_id));
        Ok(pool)
    }

    // --- Playback ----------------------------------------------------------

    /// Start playing a scene. Each line gets an independent task sleeping
    /// until its absolute trigger time; tasks select against the session
    /// cancel flag. Emits `scene:started`.
    pub async fn play_scene(
        self: &Arc<Self>,
        scene_id: i64,
        options: PlaySceneOptions,
    ) -> Result<PlayStarted> {
        let scene = self
            .scene_db(move |pm| pm.scene_get(scene_id))
            .await?
            .ok_or_else(|| MultizenError::NotFound(format!("场景 {scene_id} 不存在")))?;
        if scene.lines.is_empty() {
            return Err(MultizenError::Config("场景没有台词".into()));
        }
        {
            let map = sessions().lock().await;
            if map.contains_key(&scene_id) {
                return Err(MultizenError::Config("场景正在播放中".into()));
            }
        }
        let group_id = options.group_id.clone().or(scene.group_id.clone());
        let pool = if options.allow_dynamic_pool {
            Vec::new()
        } else {
            self.get_ready_sub_account_pool(group_id.as_deref()).await?
        };
        if !options.allow_dynamic_pool && pool.is_empty() {
            return Err(MultizenError::Config(
                match group_id {
                    Some(g) => format!("没有已登录且已进入直播间的小号（分组：{g}）"),
                    None => "没有已登录且已进入直播间的小号".to_string(),
                },
            ));
        }
        let start_ms = options.start_at_ms.unwrap_or_else(now_ms);
        let schedule = if options.allow_dynamic_pool {
            build_dynamic_schedule(&scene, start_ms)
        } else {
            build_schedule(&scene, &pool, start_ms)
        };
        let session = Arc::new(PlaySession {
            scene_id,
            started_at_ms: start_ms,
            total: schedule.len(),
            sent: AtomicUsize::new(0),
            cancel: TaskCancel::new(),
            dynamic_pool: options.allow_dynamic_pool,
            group_id,
            next_pool_index: StdMutex::new(0),
        });
        sessions().lock().await.insert(scene_id, session.clone());
        for item in schedule.clone() {
            let (driver, sess) = (Arc::clone(self), session.clone());
            tokio::spawn(async move {
                run_scheduled_line(driver, sess, item).await;
            });
        }
        self.emit(
            "scene:started",
            &SceneStartedPayload {
                scene_id,
                schedule: schedule.clone(),
                started_at: start_ms,
            },
        );
        tracing::info!(
            scene_id,
            lines = schedule.len(),
            dynamic = options.allow_dynamic_pool,
            "scene play started"
        );
        Ok(PlayStarted {
            scene_id,
            scheduled_count: schedule.len(),
            schedule,
        })
    }

    /// Stop a playing scene. Pending lines are cancelled; already-fired
    /// lines keep their recorded results. Emits `scene:finished stopped`.
    pub async fn stop_scene(&self, scene_id: i64) -> Result<bool> {
        let removed = sessions().lock().await.remove(&scene_id);
        match removed {
            Some(session) => {
                session.cancel.cancel();
                self.emit(
                    "scene:finished",
                    &SceneFinishedPayload {
                        scene_id,
                        stopped: Some(true),
                        reason: None,
                    },
                );
                tracing::info!(scene_id, "scene play stopped");
                Ok(true)
            }
            None => Ok(false),
        }
    }

    /// Dispatch one resolved line against its account session, returning
    /// `(ok, error, live_room_url)`.
    async fn dispatch_item(
        &self,
        item: &ScheduledItem,
    ) -> (bool, Option<String>, Option<String>) {
        let Some(session) = self.registry.get(&item.profile_id).await else {
            return (
                false,
                Some(format!("小号 {} 会话不存在", item.account_name)),
                None,
            );
        };
        let url = active_page_url(&session).await;
        let Some(url) = url else {
            return (false, Some("页面不可读或已关闭".to_string()), None);
        };
        if !is_live_room_url(&url) {
            return (
                false,
                Some(format!("非直播间页面（当前 URL：{url}）")),
                Some(url),
            );
        }
        let (ok, _already, error) = match item.action_type {
            SceneLineAction::Danmaku => {
                if item.message.trim().is_empty() {
                    (false, false, Some("消息内容为空".to_string()))
                } else {
                    send_danmaku(&session, &item.message).await
                }
            }
            SceneLineAction::Like => click_like(&session).await,
            SceneLineAction::Follow => click_follow(&session).await,
        };
        (ok, error, Some(url))
    }
}

/// True when any tab of the session shows a Kuaishou audience-side live room.
async fn session_in_live_room(session: &cdp_driver::session::BrowserSession) -> bool {
    let pages = match session.browser.pages().await {
        Ok(p) => p,
        Err(_) => return false,
    };
    for page in &pages {
        if let Ok(Some(url)) = page.url().await {
            if is_live_room_url(&url) {
                return true;
            }
        }
    }
    false
}

async fn run_scheduled_line(
    driver: Arc<TauriBrowserDriver>,
    session: Arc<PlaySession>,
    item: ScheduledItem,
) {
    let delay_ms = item.trigger_at_ms.saturating_sub(now_ms()).max(0) as u64;
    tokio::select! {
        biased;
        _ = session.cancel.cancelled() => return,
        _ = tokio::time::sleep(std::time::Duration::from_millis(delay_ms)) => {}
    }
    if session.cancel.is_cancelled() {
        return;
    }
    let started = now_ms();
    // Dynamic pool resolves the latest ready accounts at fire time.
    let resolved = if session.dynamic_pool {
        match driver
            .get_ready_sub_account_pool(session.group_id.as_deref())
            .await
        {
            Ok(pool) if !pool.is_empty() => {
                let idx = {
                    let mut guard = session.next_pool_index.lock().unwrap();
                    let idx = *guard;
                    *guard = guard.wrapping_add(1);
                    idx
                };
                let member = &pool[idx % pool.len()];
                (
                    ScheduledItem {
                        account_id: member.account_id.clone(),
                        profile_id: member.profile_id.clone(),
                        account_name: member.name.clone(),
                        ..item.clone()
                    },
                    None,
                )
            }
            Ok(_) => (item.clone(), Some("到点时没有已进入直播间的小号".to_string())),
            Err(e) => (item.clone(), Some(e.to_string())),
        }
    } else {
        (item.clone(), None)
    };
    let (item, pre_error) = resolved;
    let (ok, error, live_room_url) = match pre_error {
        Some(e) => (false, Some(e), None),
        None => driver.dispatch_item(&item).await,
    };
    let duration_ms = now_ms().saturating_sub(started);
    let message = (item.action_type == SceneLineAction::Danmaku)
        .then(|| item.message.clone());
    let input = RecordInteractionInput {
        // Unresolved dynamic lines keep jieger's `__dynamic__` marker so the
        // failure is still attributable in history.
        account_id: if item.account_id.is_empty() {
            "__dynamic__".to_string()
        } else {
            item.account_id.clone()
        },
        scene_id: Some(session.scene_id),
        action: item.action_type,
        message,
        live_room_url,
        ok,
        error: error.clone(),
        duration_ms: Some(duration_ms),
    };
    if let Err(e) = driver.scene_db(move |pm| pm.record_interaction(&input)).await {
        tracing::warn!(scene_id = session.scene_id, error = %e, "scene record_interaction failed");
    }
    let sent = session.sent.fetch_add(1, Ordering::SeqCst) + 1;
    driver.emit(
        "scene:progress",
        &SceneProgressPayload {
            scene_id: session.scene_id,
            sent_count: sent,
            total_count: session.total,
            last_item: item,
            ok,
            error,
        },
    );
    if sent >= session.total {
        let mut map = sessions().lock().await;
        if map
            .get(&session.scene_id)
            .is_some_and(|s| Arc::ptr_eq(s, &session))
        {
            map.remove(&session.scene_id);
        }
        drop(map);
        driver.emit(
            "scene:finished",
            &SceneFinishedPayload {
                scene_id: session.scene_id,
                stopped: None,
                reason: None,
            },
        );
        tracing::info!(
            scene_id = session.scene_id,
            total = session.total,
            started_at = session.started_at_ms,
            "scene play finished"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use multizen_core::BrowserEngine;
    use std::path::PathBuf;
    use tempfile::TempDir;

    fn test_scene(mode: TriggerMode, offsets: &[i64]) -> Scene {
        Scene {
            id: 1,
            name: "t".into(),
            trigger_mode: mode,
            group_id: None,
            lines: offsets
                .iter()
                .enumerate()
                .map(|(i, o)| profile_manager::scenes::SceneLine {
                    id: i as i64 + 1,
                    scene_id: 1,
                    ord: i as i64,
                    message: format!("m{i}"),
                    time_offset_sec: *o,
                    action_type: SceneLineAction::Danmaku,
                })
                .collect(),
            created_at: 0,
            updated_at: 0,
        }
    }

    fn pool(n: usize) -> Vec<ScenePoolMember> {
        (0..n)
            .map(|i| ScenePoolMember {
                account_id: format!("acc-{i}"),
                profile_id: format!("p-{i}"),
                name: format!("n-{i}"),
            })
            .collect()
    }

    #[test]
    fn live_room_url_matcher() {
        assert!(is_live_room_url("https://live.kuaishou.com/u/abc"));
        assert!(is_live_room_url("https://www.kuaishou.com/live/xyz?x=1"));
        assert!(!is_live_room_url("https://www.kuaishou.com/"));
        assert!(!is_live_room_url("https://login.kwaixiaodian.com/"));
        assert!(!is_live_room_url(""));
    }

    #[test]
    fn next_local_timestamp_is_future_within_a_day() {
        for secs in [0, 1, 7_200, 73_800, 86_399] {
            let target = next_local_timestamp(secs);
            let delta = target - now_ms();
            assert!(delta > 0, "secs={secs} delta={delta}");
            assert!(delta <= 86_400_000, "secs={secs} delta={delta}");
        }
    }

    #[test]
    fn relative_schedule_round_robins_with_bounded_jitter() {
        let scene = test_scene(TriggerMode::RelativeTime, &[10, 20, 30, 40, 50]);
        let start = now_ms();
        let schedule = build_schedule(&scene, &pool(2), start);
        assert_eq!(schedule.len(), 5);
        let expect = ["p-0", "p-1", "p-0", "p-1", "p-0"];
        for (item, (want_profile, offset)) in
            schedule.iter().zip(expect.iter().zip([10, 20, 30, 40, 50]))
        {
            assert_eq!(&item.profile_id, want_profile);
            let base = start + offset * 1000;
            assert!(
                (item.trigger_at_ms - base).abs() <= 300,
                "jitter out of bounds: {}",
                item.trigger_at_ms - base
            );
        }
    }

    #[test]
    fn local_schedule_uses_next_timestamp() {
        let scene = test_scene(TriggerMode::LocalTime, &[0, 43_200]);
        let schedule = build_schedule(&scene, &pool(1), now_ms());
        assert_eq!(schedule.len(), 2);
        for item in &schedule {
            assert!(item.trigger_at_ms > now_ms() - 1000);
        }
    }

    #[test]
    fn empty_pool_builds_empty_schedule() {
        let scene = test_scene(TriggerMode::RelativeTime, &[1]);
        assert!(build_schedule(&scene, &[], now_ms()).is_empty());
        let dynamic = build_dynamic_schedule(&scene, now_ms());
        assert_eq!(dynamic.len(), 1);
        assert_eq!(dynamic[0].account_name, "等待小号");
    }

    fn fixture() -> (TempDir, TauriBrowserDriver) {
        let dir = TempDir::new().unwrap();
        let driver = TauriBrowserDriver::start(
            dir.path().join("p.db"),
            dir.path().join("profiles"),
            dir.path().join("extensions"),
            Arc::new(crate::registry::ProfileRegistry::new()),
            BrowserEngine::Chromix,
            PathBuf::new(),
            None,
        )
        .unwrap();
        (dir, driver)
    }

    /// Offline playback: dynamic pool with no live sessions fails every line
    /// at fire time, records each result, and cleans the session up.
    #[tokio::test]
    async fn dynamic_play_without_pool_records_failures_and_finishes() {
        let (_dir, driver) = fixture();
        let driver = Arc::new(driver);
        let scene = driver
            .scene_create("t".into(), TriggerMode::RelativeTime, None)
            .await
            .unwrap();
        driver
            .scene_add_line(scene.id, "hi".into(), 0, SceneLineAction::Danmaku)
            .await
            .unwrap();
        driver
            .scene_add_line(scene.id, "".into(), 0, SceneLineAction::Like)
            .await
            .unwrap();
        let started = driver
            .play_scene(
                scene.id,
                PlaySceneOptions {
                    allow_dynamic_pool: true,
                    ..Default::default()
                },
            )
            .await
            .unwrap();
        assert_eq!(started.scheduled_count, 2);
        // Second play while running is rejected.
        assert!(driver
            .play_scene(scene.id, PlaySceneOptions::default())
            .await
            .is_err());
        // Wait for both lines to fire (jitter <= 300ms) and finish.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        loop {
            let rows = driver
                .scene_db(move |pm| {
                    pm.list_interactions(Some(scene.id), None, 100, 0)
                })
                .await
                .unwrap();
            if rows.len() >= 2 || std::time::Instant::now() > deadline {
                assert_eq!(rows.len(), 2);
                assert!(rows.iter().all(|r| !r.ok));
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
        // Session cleaned up after the last line.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            if !sessions().lock().await.contains_key(&scene.id)
                || std::time::Instant::now() > deadline
            {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
        assert!(!sessions().lock().await.contains_key(&scene.id));
        // Empty fixed pool is rejected when dynamic pooling is off.
        let err = driver
            .play_scene(scene.id, PlaySceneOptions::default())
            .await
            .unwrap_err()
            .to_string();
        assert!(err.contains("没有已登录"), "{err}");
        // Stopping a non-playing scene returns false.
        assert!(!driver.stop_scene(scene.id).await.unwrap());
    }
}
