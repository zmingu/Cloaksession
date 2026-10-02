//! Read-only identity orchestration. DB work stays on the launcher thread; CDP/network never do.
mod avatar;
mod extract;
#[cfg(test)]
mod tests;

use super::{LauncherCmd, TauriBrowserDriver};
use crate::registry::{ProfileRegistry, SessionSlot};
use avatar::{AvatarCache, AvatarRoute};
use cdp_driver::TaskCancel;
use multizen_core::{
    kuaishou_identity_allowed, BusinessProfileState, KuaishouIdentityObservation as Observation,
    KuaishouIdentitySnapshot as Snapshot, KuaishouIdentityStatus as Status, MultizenError, Profile,
    Result,
};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex, Weak,
};
use std::time::Duration;
use tokio::sync::{oneshot, OwnedSemaphorePermit, Semaphore};
use tokio::time::Instant;

const POLL: Duration = Duration::from_secs(5);
const COMMAND_WAIT: Duration = Duration::from_secs(2);
const DETECT_WAIT: Duration = Duration::from_secs(14);
const MAX_CONCURRENT: usize = 4;

type Context = (Profile, BusinessProfileState, Option<Observation>);
type Listed = Vec<(String, BusinessProfileState, Option<Observation>)>;
pub(super) enum IdentityCmd {
    List {
        resp: oneshot::Sender<Result<Listed>>,
    },
    Read {
        id: String,
        resp: oneshot::Sender<Result<Option<Context>>>,
    },
    Save {
        data: Observation,
        expected: BusinessProfileState,
        slot: Arc<SessionSlot>,
        stop: TaskCancel,
        deadline: Instant,
        resp: oneshot::Sender<Result<Option<Observation>>>,
    },
    Avatar {
        key: String,
        resp: oneshot::Sender<Result<bool>>,
    },
}

pub(super) async fn handle(
    cmd: IdentityCmd,
    pm: &profile_manager::ProfileManager,
    registry: &ProfileRegistry,
    launcher: &browser_launcher::BrowserLauncher,
) {
    let candidates = match &cmd {
        IdentityCmd::List { .. } => registry.ids().await,
        IdentityCmd::Read { id, .. } => vec![id.clone()],
        IdentityCmd::Save { data, .. } => vec![data.snapshot.profile_id.clone()],
        IdentityCmd::Avatar { .. } => Vec::new(),
    };
    for id in candidates {
        if !launcher.is_running_async(&id).await {
            registry.remove(&id).await;
        }
    }
    match cmd {
        IdentityCmd::List { resp } => {
            let result = (|| {
                pm.list()?
                    .into_iter()
                    .map(|p| {
                        Ok((
                            p.id.clone(),
                            pm.business_accounts_profile_state(&p.id)?,
                            pm.kuaishou_identity_observation(&p.id)?,
                        ))
                    })
                    .collect()
            })();
            let _ = resp.send(result);
        }
        IdentityCmd::Read { id, resp } => {
            let result = (|| {
                Ok(match pm.get(&id)? {
                    Some(p) => Some((
                        p,
                        pm.business_accounts_profile_state(&id)?,
                        pm.kuaishou_identity_observation(&id)?,
                    )),
                    None => None,
                })
            })();
            let _ = resp.send(result);
        }
        IdentityCmd::Save {
            data,
            expected,
            slot,
            stop,
            deadline,
            resp,
        } => {
            let id = data.snapshot.profile_id.clone();
            let result = registry
                .with_current(&id, &slot, || {
                    if resp.is_closed()
                        || stop.is_cancelled()
                        || Instant::now() >= deadline
                        || data.session_id.as_deref() != Some(slot.id.as_str())
                    {
                        return Ok(None);
                    }
                    pm.kuaishou_identity_save(data, &expected)
                })
                .await
                .unwrap_or(Ok(None));
            let _ = resp.send(result);
        }
        IdentityCmd::Avatar { key, resp } => {
            let _ = resp.send(pm.kuaishou_identity_avatar_referenced(&key));
        }
    }
}

struct Retry {
    session: String,
    identity: String,
    url: String,
    until: Instant,
    failures: u32,
}
pub(super) struct IdentityRuntime {
    pub stop: TaskCancel,
    started: AtomicBool,
    active: Mutex<HashSet<String>>,
    // Weak registry: granted permits and queued acquisitions own the live locks.
    // Tokio's fair semaphore hands a released profile directly to an existing waiter.
    profile_locks: Mutex<HashMap<String, Weak<Semaphore>>>,
    permits: Arc<Semaphore>,
    retry: Mutex<HashMap<String, Retry>>,
    cache: AvatarCache,
}
impl IdentityRuntime {
    pub fn new(root: PathBuf) -> Self {
        Self {
            stop: TaskCancel::new(),
            started: AtomicBool::new(false),
            active: Mutex::new(HashSet::new()),
            profile_locks: Mutex::new(HashMap::new()),
            permits: Arc::new(Semaphore::new(MAX_CONCURRENT)),
            retry: Mutex::new(HashMap::new()),
            cache: AvatarCache::new(root),
        }
    }
    fn schedule(self: &Arc<Self>, mut ids: Vec<String>, cursor: &mut usize) -> Vec<Gate> {
        ids.sort();
        if !ids.is_empty() {
            let offset = *cursor % ids.len();
            ids.rotate_left(offset);
            *cursor = cursor.wrapping_add(MAX_CONCURRENT);
        }
        self.retry.lock().unwrap().retain(|id, _| ids.contains(id));
        ids.into_iter().filter_map(|id| self.enter(&id)).collect()
    }
    fn profile_lock(&self, id: &str) -> Arc<Semaphore> {
        let mut locks = self.profile_locks.lock().unwrap();
        locks.retain(|_, lock| lock.strong_count() > 0);
        if let Some(lock) = locks.get(id).and_then(Weak::upgrade) { return lock; }
        let lock = Arc::new(Semaphore::new(1));
        locks.insert(id.into(), Arc::downgrade(&lock));
        lock
    }
    /// Immediate reservation for fixtures; production workers wait fairly below.
    #[cfg(test)]
    pub(in crate::driver) fn reserve_initialization(self: &Arc<Self>, id: &str) -> Option<InitializationReservation> {
        if self.stop.is_cancelled() { return None; }
        let permit = self.profile_lock(id).try_acquire_owned().ok()?;
        if !self.active.lock().unwrap().insert(id.into()) { return None; }
        Some(InitializationReservation { runtime: self.clone(), id: id.into(), _profile: permit })
    }
    /// Caller bounds/cancels this wait and reserves its own initialization capacity.
    /// No global identity permit is consumed. Dropping the future removes its waiter.
    pub(in crate::driver) async fn reserve_initialization_wait(self: &Arc<Self>, id: &str) -> Option<InitializationReservation> {
        if self.stop.is_cancelled() { return None; }
        let permit = self.profile_lock(id).acquire_owned().await.ok()?;
        if self.stop.is_cancelled() || !self.active.lock().unwrap().insert(id.into()) { return None; }
        Some(InitializationReservation { runtime: self.clone(), id: id.into(), _profile: permit })
    }
    #[cfg(test)]
    pub(in crate::driver) fn test_hold_detection(self: &Arc<Self>, id: &str) -> Option<impl Send> { self.enter(id) }
    fn enter(self: &Arc<Self>, id: &str) -> Option<Gate> {
        if self.stop.is_cancelled() {
            return None;
        }
        let permit = self.permits.clone().try_acquire_owned().ok()?;
        // Nonwaiting detection must not overtake a queued initializer on this profile.
        let profile = self.profile_lock(id).try_acquire_owned().ok()?;
        if !self.active.lock().unwrap().insert(id.into()) {
            return None;
        }
        Some(Gate {
            runtime: self.clone(),
            id: id.into(),
            _profile: profile,
            _permit: permit,
        })
    }
}
pub(in crate::driver) struct InitializationReservation {
    runtime: Arc<IdentityRuntime>,
    id: String,
    _profile: OwnedSemaphorePermit,
}
impl Drop for InitializationReservation {
    fn drop(&mut self) { self.runtime.active.lock().unwrap().remove(&self.id); }
}
struct Gate {
    runtime: Arc<IdentityRuntime>,
    id: String,
    _profile: OwnedSemaphorePermit,
    _permit: OwnedSemaphorePermit,
}
impl Drop for Gate {
    fn drop(&mut self) {
        self.runtime.active.lock().unwrap().remove(&self.id);
    }
}

fn project(
    id: &str,
    old: Option<&Observation>,
    business: &BusinessProfileState,
    slot: Option<&Arc<SessionSlot>>,
) -> Snapshot {
    let mut snapshot = old
        .map(|o| o.snapshot.clone())
        .unwrap_or_else(|| Snapshot::empty(id, Status::Unknown));
    match slot {
        None => {
            snapshot.status = Status::Closed;
            snapshot.message = Some("环境未运行；保留的是上次观察，不会自动启动浏览器".into());
        }
        Some(_) if !kuaishou_identity_allowed(business) => {
            snapshot.status = Status::Skipped;
            snapshot.message = Some("金牛scope或非小店业务绑定，已跳过".into());
        }
        Some(s) if old.and_then(|o| o.session_id.as_deref()) != Some(s.id.as_str()) => {
            snapshot.status = Status::Unknown;
            snapshot.message = Some("当前运行会话尚未检测；历史资料不表示当前登录".into());
        }
        Some(s) if s.cancel.is_cancelled() => {
            snapshot.status = Status::Closed;
            snapshot.message = Some("会话已关闭，历史资料不表示当前登录".into());
        }
        Some(_)
            if snapshot.status == Status::Detected
                && business
                    .account
                    .as_ref()
                    .and_then(|a| a.platform_user_id.as_deref())
                    .is_some_and(|manual| snapshot.platform_user_id.as_deref() != Some(manual)) =>
        {
            snapshot.status = Status::Conflict;
            snapshot.message = Some("页面观察与手工登记ID不一致".into());
        }
        Some(_) => {}
    }
    snapshot
}

impl TauriBrowserDriver {
    async fn identity_request<T>(
        &self,
        build: impl FnOnce(oneshot::Sender<Result<T>>) -> IdentityCmd,
    ) -> Result<T> {
        tokio::time::timeout(COMMAND_WAIT, async {
            let (resp, receive) = oneshot::channel();
            self.launcher_tx
                .send(LauncherCmd::Identity(build(resp)))
                .await
                .map_err(|_| MultizenError::Mcp("身份存储线程已关闭".into()))?;
            receive
                .await
                .map_err(|_| MultizenError::Mcp("身份存储响应已取消".into()))?
        })
        .await
        .map_err(|_| MultizenError::Mcp("身份存储等待超时，请稍后重试".into()))?
    }

    pub async fn kuaishou_identity_list(&self) -> Result<Vec<Snapshot>> {
        tokio::time::timeout(Duration::from_secs(4), self.identity_list_inner())
            .await
            .map_err(|_| MultizenError::Mcp("身份列表读取超时".into()))?
    }
    async fn identity_list_inner(&self) -> Result<Vec<Snapshot>> {
        let rows = self
            .identity_request(|resp| IdentityCmd::List { resp })
            .await?;
        let mut result = Vec::with_capacity(rows.len());
        for (id, business, old) in rows {
            let slot = self.registry.slot(&id).await;
            result.push(project(&id, old.as_ref(), &business, slot.as_ref()));
        }
        Ok(result)
    }

    pub async fn kuaishou_identity_avatar(&self, key: &str) -> Option<String> {
        if !avatar::valid_key(key) {
            return None;
        }
        let referenced = self
            .identity_request(|resp| IdentityCmd::Avatar {
                key: key.into(),
                resp,
            })
            .await
            .ok()?;
        if !referenced {
            return None;
        }
        tokio::time::timeout(COMMAND_WAIT, self.identity.cache.read(key))
            .await
            .ok()
            .flatten()
    }

    /// No launch, navigation, active-page switch or authentication storage access.
    pub async fn kuaishou_identity_detect(&self, id: &str) -> Snapshot {
        tokio::time::timeout(Duration::from_secs(20), self.identity_detect_inner(id))
            .await
            .unwrap_or_else(|_| {
                let mut snapshot = Snapshot::empty(id, Status::Error);
                snapshot.message = Some("身份检测总等待超时，请稍后重试".into());
                snapshot
            })
    }

    async fn identity_detect_inner(&self, id: &str) -> Snapshot {
        let gate = self.identity.enter(id);
        if gate.is_none() {
            let context = self
                .identity_request(|resp| IdentityCmd::Read {
                    id: id.into(),
                    resp,
                })
                .await;
            if let Ok(Some((_, business, old))) = context {
                let slot = self.registry.slot(id).await;
                let mut snapshot = project(id, old.as_ref(), &business, slot.as_ref());
                if slot.is_none() {
                    return snapshot;
                }
                snapshot.status = Status::Unknown;
                snapshot.message = Some("检测正在进行或并发已满，请稍后重试".into());
                return snapshot;
            }
            let mut snapshot = Snapshot::empty(id, Status::Error);
            snapshot.message = Some("身份状态不可用，请稍后重试".into());
            return snapshot;
        }
        self.identity_detect_guarded(id, gate.unwrap()).await
    }

    async fn identity_detect_guarded(&self, id: &str, _gate: Gate) -> Snapshot {
        let context = self
            .identity_request(|resp| IdentityCmd::Read {
                id: id.into(),
                resp,
            })
            .await;
        let (_profile, business, old) = match context {
            Ok(Some(c)) => c,
            Ok(None) => {
                let mut s = Snapshot::empty(id, Status::Closed);
                s.message = Some("Profile不存在或已删除".into());
                return s;
            }
            Err(e) => {
                let mut s = Snapshot::empty(id, Status::Error);
                s.message = Some(e.to_string());
                return s;
            }
        };
        let slot = self.registry.slot(id).await;
        let mut snapshot = project(id, old.as_ref(), &business, slot.as_ref());
        snapshot.checked_at = Some(chrono::Utc::now().to_rfc3339());
        let Some(slot) = slot else {
            return snapshot;
        };
        if !kuaishou_identity_allowed(&business) {
            return snapshot;
        }
        let Some(session) = slot.session() else {
            snapshot.status = Status::Closed;
            return snapshot;
        };
        let work = async {
            let (result, avatar_target) = tokio::time::timeout(
                Duration::from_secs(8),
                extract::detect_with_avatar_target(&session, slot.cancel.clone()),
            )
            .await
            .map_err(|_| "小店页面检测整体超时".to_string())??;
            let mut observed = Snapshot::empty(id, Status::NotDetected);
            observed.checked_at = Some(chrono::Utc::now().to_rfc3339());
            let mut avatar_url = None;
            match result {
                extract::Detection::NoPage => {
                    observed.message = Some("没有打开准确域名的小店页面；未导航或新建页面".into())
                }
                extract::Detection::NoId => {
                    observed.message = Some("小店账号区未检测到有效ID，请确认登录或稍后重试".into())
                }
                extract::Detection::Conflict => {
                    observed.status = Status::Conflict;
                    observed.message = Some("多个小店页面读到不同ID，未选择任一账号".into());
                }
                extract::Detection::Found(page) => {
                    let identity = page.platform_user_id.unwrap();
                    if business
                        .account
                        .as_ref()
                        .and_then(|a| a.platform_user_id.as_deref())
                        .is_some_and(|manual| manual != identity)
                    {
                        observed.status = Status::Conflict;
                        observed.message =
                            Some("页面ID与手工登记平台ID不一致；未修改人工档案或绑定".into());
                    } else {
                        observed.status = Status::Detected;
                        observed.platform_user_id = Some(identity.clone());
                        observed.nickname = page.nickname;
                        if let Some(url) = page.avatar_url {
                            let cached = old.as_ref().filter(|o| {
                                o.snapshot.platform_user_id.as_deref() == Some(&identity)
                                    && o.avatar_url.as_deref() == Some(&url)
                            });
                            let key = cached.and_then(|o| o.snapshot.avatar_key.as_deref());
                            if let Some(key) = key {
                                if self.identity.cache.read(key).await.is_some() {
                                    observed.avatar_key = Some(key.into());
                                    avatar_url = Some(url.clone());
                                    observed.message =
                                        Some("ID已读到；头像复用已验证的本地缓存".into());
                                }
                            }
                            if observed.avatar_key.is_none() {
                                let retry_allowed =
                                    self.identity.retry.lock().unwrap().get(id).is_none_or(|r| {
                                        r.session != slot.id
                                            || r.identity != identity
                                            || r.url != url
                                            || Instant::now() >= r.until
                                    });
                                if retry_allowed {
                                    let route = slot
                                        .network
                                        .as_ref()
                                        .map(|n| {
                                            AvatarRoute::from_profile(
                                                &n.profile,
                                                n.engine,
                                                &n.chromix,
                                                n.environment_uncertain,
                                            )
                                        })
                                        .unwrap_or(AvatarRoute::Skip);
                                    let load = async {
                                        if !self.registry.is_current(id, &slot).await {
                                            return Err("会话已变化，已跳过头像".into());
                                        }
                                        let target =
                                            avatar_target.as_deref().ok_or("头像页面已不可用")?;
                                        let mut page = extract::AvatarPage::open(
                                            &session,
                                            target,
                                            &identity,
                                            &url,
                                            slot.cancel.clone(),
                                        )
                                        .await?;
                                        let local = match page.content() {
                                            Some(content) => {
                                                self.identity.cache.store_base64(content).await.ok()
                                            }
                                            None => None,
                                        };
                                        let from_browser =
                                            local.is_some() || matches!(route, AvatarRoute::Skip);
                                        let result = match local {
                                            Some(key) => Ok(key),
                                            None if matches!(route, AvatarRoute::Skip) => {
                                                let content = page.read_network().await?;
                                                self.identity.cache.store_base64(&content).await
                                            }
                                            None => {
                                                self.identity.cache.download(&url, &route).await
                                            }
                                        };
                                        // Keep the target lease while either local IO or the explicit proxy waits.
                                        // Neither an account switch nor a replaced session may acquire these bytes.
                                        page.revalidate().await?;
                                        if slot.cancel.is_cancelled()
                                            || !self.registry.is_current(id, &slot).await
                                        {
                                            return Err("会话已变化，已丢弃头像".into());
                                        }
                                        result.map(|key| (key, from_browser))
                                    };
                                    match tokio::time::timeout(Duration::from_secs(5), load)
                                        .await
                                        .unwrap_or_else(|_| Err("ID已读到；头像缓存超时".into()))
                                    {
                                        Ok((key, from_browser)) => {
                                            observed.avatar_key = Some(key);
                                            avatar_url = Some(url);
                                            observed.message = Some(if from_browser {
                                                "ID已读到；头像经浏览器读取并保存本地缓存（必要时仅请求公开头像）"
                                                    .into()
                                            } else {
                                                "ID已读到；头像经明确代理下载，已保存本地缓存"
                                                    .into()
                                            });
                                            self.identity.retry.lock().unwrap().remove(id);
                                        }
                                        Err(warning) => {
                                            let mut retries = self.identity.retry.lock().unwrap();
                                            let failures = retries
                                                .get(id)
                                                .filter(|r| {
                                                    r.session == slot.id
                                                        && r.identity == identity
                                                        && r.url == url
                                                })
                                                .map_or(1, |r| r.failures.saturating_add(1).min(5));
                                            retries.insert(
                                                id.into(),
                                                Retry {
                                                    session: slot.id.clone(),
                                                    identity,
                                                    url,
                                                    failures,
                                                    until: Instant::now()
                                                        + Duration::from_secs(10u64 << failures),
                                                },
                                            );
                                            observed.message = Some(warning);
                                        }
                                    }
                                } else {
                                    observed.message =
                                        Some("ID已读到；头像缓存失败后退避中，稍后重试".into());
                                }
                            }
                        } else {
                            observed.message = Some("ID已读到；页面暂无可用头像".into());
                        }
                    }
                }
            }
            Ok::<_, String>(Observation {
                snapshot: observed,
                session_id: Some(slot.id.clone()),
                avatar_url,
            })
        };
        let result = tokio::select! {
            biased;
            _=self.identity.stop.cancelled()=>Err("身份检测已停止".to_string()),
            _=slot.cancel.cancelled()=>Err("运行会话已关闭或替换".to_string()),
            r=tokio::time::timeout(DETECT_WAIT,work)=>r.unwrap_or_else(|_|Err("身份检测整体超时".into())),
        };
        let observation = match result {
            Ok(o) => o,
            Err(message) => {
                snapshot.status = Status::Error;
                snapshot.message = Some(message);
                Observation {
                    snapshot,
                    session_id: Some(slot.id.clone()),
                    avatar_url: None,
                }
            }
        };
        let result = self
            .identity_request(|resp| IdentityCmd::Save {
                data: observation,
                expected: business.clone(),
                slot: slot.clone(),
                stop: self.identity.stop.clone(),
                deadline: Instant::now() + COMMAND_WAIT,
                resp,
            })
            .await;
        match result {
            Ok(Some(saved)) if self.registry.is_current(id, &slot).await => saved.snapshot,
            Ok(_) => {
                let current = self.registry.slot(id).await;
                let mut s = project(id, old.as_ref(), &business, current.as_ref());
                if current.is_some() {
                    s.status = Status::Unknown;
                }
                s.message = Some("环境或业务绑定已变化，已丢弃迟到的检测结果".into());
                s
            }
            Err(e) => {
                let mut s = project(id, old.as_ref(), &business, Some(&slot));
                s.status = Status::Error;
                s.message = Some(e.to_string());
                s
            }
        }
    }

    pub fn stop_kuaishou_identity_monitor(&self) {
        self.identity.stop.cancel();
    }

    /// Fresh read-only re-check across every shop page of this session. Used by
    /// write-capable callers right before they act: no page, no ID, or two pages
    /// showing different accounts are all refusals, never an arbitrary pick.
    pub(super) async fn current_kuaishou_account(
        &self,
        session: &cdp_driver::session::BrowserSession,
        cancel: TaskCancel,
    ) -> Result<String> {
        let (detection, _) = extract::detect_with_avatar_target(session, cancel)
            .await
            .map_err(MultizenError::Mcp)?;
        match detection {
            extract::Detection::Found(page) => page
                .platform_user_id
                .ok_or_else(|| MultizenError::Mcp("当前小店页面未读出账号ID".into())),
            extract::Detection::NoPage => Err(MultizenError::Mcp("当前没有已打开的小店页面".into())),
            extract::Detection::NoId => Err(MultizenError::Mcp("当前小店页面未读出账号ID".into())),
            extract::Detection::Conflict => {
                Err(MultizenError::Mcp("多个小店页面显示不同账号，已拒绝".into()))
            }
        }
    }

    pub(super) async fn validate_kuaishou_task_identity(
        &self,
        session: &cdp_driver::session::BrowserSession,
        owned: &mut cdp_driver::TaskPage<'_>,
        expected: &str,
        cancel: TaskCancel,
    ) -> Result<()> {
        tokio::time::timeout(Duration::from_secs(8), extract::verify_initialization_identity(session, owned, expected, cancel))
            .await.map_err(|_| MultizenError::Mcp("初始化身份重验超时".into()))?
            .map_err(MultizenError::Mcp)
    }

    fn identity_monitor_tick(self: &Arc<Self>, ids: Vec<String>, cursor: &mut usize) -> usize {
        let gates = self.identity.schedule(ids, cursor);
        let scheduled = gates.len();
        for gate in gates {
            let id = gate.id.clone();
            let driver = self.clone();
            tauri::async_runtime::spawn(async move {
                let _ = tokio::time::timeout(Duration::from_secs(20), driver.identity_detect_guarded(&id, gate)).await;
            });
        }
        scheduled
    }

    /// One app-level poller, weak driver while idle. Each tick drops overflow instead of queueing it.
    pub fn start_kuaishou_identity_monitor(self: &Arc<Self>) {
        if self.identity.started.swap(true, Ordering::AcqRel) {
            return;
        }
        let weak = Arc::downgrade(self);
        let stop = self.identity.stop.clone();
        tauri::async_runtime::spawn(async move {
            let mut interval = tokio::time::interval(POLL);
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            let mut cursor = 0usize;
            loop {
                tokio::select! { biased; _=stop.cancelled()=>break, _=interval.tick()=>{} }
                let Some(driver) = weak.upgrade() else {
                    break;
                };
                let ids = tokio::select! { biased; _=stop.cancelled()=>break, result=tokio::time::timeout(COMMAND_WAIT,driver.registry.ids())=>match result { Ok(ids)=>ids,Err(_)=>continue } };
                driver.identity_monitor_tick(ids, &mut cursor);
            }
        });
    }
}
