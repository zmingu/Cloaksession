//! jieger 商品话术库播放引擎：排期计算 + 动作派发 + 播放状态广播。
//!
//! shop-helper（`goods_on_shelf` / `goods_off_shelf`）与 goodsList
//!（`explain_goods` / `cancel_explain_goods`）由 [`DriverShopActionDispatcher`]
//! 接入真实原语；trait [`ShopActionDispatcher`] 保留为可替换缝，测试用 mock。
//! 商品知识扫描（供 auto-popup 调用）只留 [`ProductScriptSource`] 接口。
use std::collections::HashMap;
use std::sync::{Arc, Mutex as StdMutex, OnceLock};

use super::{LauncherCmd, TauriBrowserDriver};
use cdp_driver::TaskCancel;
use multizen_core::{MultizenError, Result};
use profile_manager::{
    AddShopProductScriptLineInput, CreateShopProductScriptInput, ScriptLineAction,
    ShopProductScript, ShopProductScriptDetail, ShopProductScriptLine,
    UpdateShopProductScriptInput, UpdateShopProductScriptLineInput,
};
use serde::{Deserialize, Serialize};
use tokio::sync::{oneshot, watch};

/// 播放状态广播事件名。
pub const PRODUCT_SCRIPT_STATE_EVENT: &str = "product-script-state-changed";

/// 触发提前量：`trigger_offset_sec = video_time_sec - lead_sec`。
/// `video_time_sec` 为商品在视频中的出现时间（秒），`lead_sec`
/// 为提前量（秒）；返回相对视频开始的触发偏移（秒）。
pub fn trigger_offset_sec(video_time_sec: f64, lead_sec: f64) -> f64 {
    video_time_sec - lead_sec
}

/// 排期条目：某一行被触发的绝对时间（毫秒）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScheduledLine {
    pub line_id: String,
    pub script_id: String,
    pub action: ScriptLineAction,
    pub goods_id: String,
    pub trigger_offset_sec: f64,
    pub trigger_at_ms: i64,
}

/// 排期：`trigger_at = started_at + (video_time_sec - lead_sec) * 1000`，
/// 按 `trigger_at` 升序。负偏移钳制到 `started_at`（立即触发），
/// 保证播放循环无需处理过去的时间点。
pub fn build_schedule(lines: &[ShopProductScriptLine], started_at_ms: i64) -> Vec<ScheduledLine> {
    let mut out: Vec<ScheduledLine> = lines
        .iter()
        .map(|line| {
            let offset = trigger_offset_sec(line.video_time_sec, line.lead_sec);
            let trigger_at_ms = started_at_ms
                .saturating_add((offset * 1000.0).round() as i64)
                .max(started_at_ms);
            ScheduledLine {
                line_id: line.id.clone(),
                script_id: line.script_id.clone(),
                action: line.action,
                goods_id: line.goods_id.clone(),
                trigger_offset_sec: offset,
                trigger_at_ms,
            }
        })
        .collect();
    out.sort_by(|a, b| {
        a.trigger_at_ms
            .cmp(&b.trigger_at_ms)
            .then_with(|| a.line_id.cmp(&b.line_id))
    });
    out
}

/// shop-helper / goodsList 动作派发占位接口。
///
/// - `OnShelf` → `goods_on_shelf`（shop-helper）
/// - `OffShelf` → `goods_off_shelf`（shop-helper）
/// - `Explain` → `explain_goods`（goodsList）
/// - `CancelExplain` → `cancel_explain_goods`（goodsList）
#[async_trait::async_trait]
pub trait ShopActionDispatcher: Send + Sync {
    async fn goods_on_shelf(&self, goods_id: &str) -> Result<()>;
    async fn goods_off_shelf(&self, goods_id: &str) -> Result<()>;
    async fn explain_goods(&self, goods_id: &str) -> Result<()>;
    async fn cancel_explain_goods(&self, goods_id: &str) -> Result<()>;
}

/// 按行动作把一条排期派发到 `dispatcher`。
pub async fn dispatch_line_action(
    dispatcher: &impl ShopActionDispatcher,
    line: &ScheduledLine,
) -> Result<()> {
    match line.action {
        ScriptLineAction::OnShelf => dispatcher.goods_on_shelf(&line.goods_id).await,
        ScriptLineAction::OffShelf => dispatcher.goods_off_shelf(&line.goods_id).await,
        ScriptLineAction::Explain => dispatcher.explain_goods(&line.goods_id).await,
        ScriptLineAction::CancelExplain => dispatcher.cancel_explain_goods(&line.goods_id).await,
    }
}

/// 真实 dispatcher：把排期动作派发到 shop-helper（上车/下车）与 goodsList
///（讲解/取消讲解）原语。
///
/// 持有 driver 引用 + `profile_id` + `TaskCancel`（三者都是播放期间必需），
/// 不自己取 `TaskPage`：
/// - 上车/下车直接调 [`TauriBrowserDriver::goods_on_shelf`] /
///   [`TauriBrowserDriver::goods_off_shelf`]，二者内部自行 `find_helper_target`
///   + 租约（幂等）。
/// - 讲解/取消讲解调 [`TauriBrowserDriver::auto_popup_explain_goods`] /
///   [`TauriBrowserDriver::auto_popup_cancel_explain_goods`]（driver 级封装，
///   内部走 `console_task_page` 合作式租约）。
///
/// `TaskCancel` 同时作为任务取消令牌传给底层原语，`stop` 播放时一并中断。
///
/// 持有 `Arc<TauriBrowserDriver>`（而非借用）以便整段播放跑在独立
/// `tokio::spawn` 上，命令立即返回、不阻塞调用方。
pub struct DriverShopActionDispatcher {
    driver: Arc<TauriBrowserDriver>,
    profile_id: String,
    cancel: TaskCancel,
}

impl DriverShopActionDispatcher {
    pub fn new(
        driver: Arc<TauriBrowserDriver>,
        profile_id: impl Into<String>,
        cancel: TaskCancel,
    ) -> Self {
        Self {
            driver,
            profile_id: profile_id.into(),
            cancel,
        }
    }
}

#[async_trait::async_trait]
impl ShopActionDispatcher for DriverShopActionDispatcher {
    async fn goods_on_shelf(&self, goods_id: &str) -> Result<()> {
        self.driver
            .goods_on_shelf(&self.profile_id, goods_id, self.cancel.clone())
            .await
            .map(|_| ())
    }

    async fn goods_off_shelf(&self, goods_id: &str) -> Result<()> {
        self.driver
            .goods_off_shelf(&self.profile_id, goods_id, self.cancel.clone())
            .await
            .map(|_| ())
    }

    async fn explain_goods(&self, goods_id: &str) -> Result<()> {
        self.driver
            .auto_popup_explain_goods(&self.profile_id, goods_id, self.cancel.clone())
            .await
            .map_err(MultizenError::Mcp)
    }

    async fn cancel_explain_goods(&self, goods_id: &str) -> Result<()> {
        let _ = goods_id;
        self.driver
            .auto_popup_cancel_explain_goods(&self.profile_id, self.cancel.clone())
            .await
            .map_err(MultizenError::Mcp)
    }
}

/// 商品知识（供 auto-popup 匹配话术行用）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProductGoods {
    pub goods_id: String,
    pub title: String,
}

/// 商品知识来源：扫描当前可售商品知识，供 auto-popup 调用匹配话术行。
/// 只留接口，不在此实现扫描逻辑。
pub trait ProductScriptSource: Send + Sync {
    fn scan_goods_knowledge(&self) -> Vec<ProductGoods>;
}

/// 只保留商品知识中存在的商品对应的行（保持原排期顺序）。
pub fn select_lines_for_goods(
    schedule: &[ScheduledLine],
    known: &[ProductGoods],
) -> Vec<ScheduledLine> {
    schedule
        .iter()
        .filter(|line| known.iter().any(|g| g.goods_id == line.goods_id))
        .cloned()
        .collect()
}

/// 播放状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProductScriptPlayStatus {
    Idle,
    Playing,
    Finished,
    Cancelled,
}

/// 播放状态快照，通过 [`PRODUCT_SCRIPT_STATE_EVENT`] 广播。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProductScriptPlayState {
    pub script_id: String,
    pub status: ProductScriptPlayStatus,
    pub current_line_id: Option<String>,
    pub current_index: usize,
    pub total: usize,
    pub started_at_ms: i64,
}

/// 播放器：持有状态广播通道，按排期触发派发。
pub struct ProductScriptPlayer {
    state_tx: tokio::sync::broadcast::Sender<ProductScriptPlayState>,
}

impl Default for ProductScriptPlayer {
    fn default() -> Self {
        Self::new()
    }
}

impl ProductScriptPlayer {
    pub fn new() -> Self {
        let (state_tx, _) = tokio::sync::broadcast::channel(32);
        Self { state_tx }
    }

    pub fn subscribe(&self) -> tokio::sync::broadcast::Receiver<ProductScriptPlayState> {
        self.state_tx.subscribe()
    }

    fn emit(&self, state: ProductScriptPlayState) {
        let _ = self.state_tx.send(state);
    }

    fn snapshot(
        script_id: &str,
        status: ProductScriptPlayStatus,
        current_line_id: Option<String>,
        current_index: usize,
        total: usize,
        started_at_ms: i64,
    ) -> ProductScriptPlayState {
        ProductScriptPlayState {
            script_id: script_id.into(),
            status,
            current_line_id,
            current_index,
            total,
            started_at_ms,
        }
    }

    /// 按排期播放：到达 `trigger_at` 即派发并广播 `Playing`；
    /// 全部完成广播 `Finished`。`stop` 置位即中断并广播 `Cancelled`。
    /// 等待使用相对延时（相对调用时刻），`trigger_at <= started_at`
    /// 的条目立即触发，便于测试与追播。
    pub async fn play(
        &self,
        script_id: &str,
        schedule: Vec<ScheduledLine>,
        dispatcher: &impl ShopActionDispatcher,
        started_at_ms: i64,
        stop: tokio::sync::watch::Receiver<bool>,
    ) -> ProductScriptPlayStatus {
        let total = schedule.len();
        let wall_start = std::time::Instant::now();
        for (index, line) in schedule.iter().enumerate() {
            if *stop.borrow() {
                let state = Self::snapshot(
                    script_id,
                    ProductScriptPlayStatus::Cancelled,
                    None,
                    index,
                    total,
                    started_at_ms,
                );
                self.emit(state);
                return ProductScriptPlayStatus::Cancelled;
            }
            let delay_ms = (line.trigger_at_ms - started_at_ms).max(0) as u64;
            let elapsed_ms = wall_start.elapsed().as_millis().min(u64::MAX as u128) as u64;
            if delay_ms > elapsed_ms {
                let mut stop = stop.clone();
                tokio::select! {
                    _ = tokio::time::sleep(std::time::Duration::from_millis(delay_ms - elapsed_ms)) => {},
                    _ = stop.wait_for(|v| *v) => {
                        let state = Self::snapshot(
                            script_id,
                            ProductScriptPlayStatus::Cancelled,
                            None,
                            index,
                            total,
                            started_at_ms,
                        );
                        self.emit(state);
                        return ProductScriptPlayStatus::Cancelled;
                    }
                }
            }
            if dispatch_line_action(dispatcher, line).await.is_err() {
                continue;
            }
            self.emit(Self::snapshot(
                script_id,
                ProductScriptPlayStatus::Playing,
                Some(line.line_id.clone()),
                index + 1,
                total,
                started_at_ms,
            ));
        }
        let state = Self::snapshot(
            script_id,
            ProductScriptPlayStatus::Finished,
            None,
            total,
            total,
            started_at_ms,
        );
        self.emit(state);
        ProductScriptPlayStatus::Finished
    }
}

impl TauriBrowserDriver {
    fn shop_script_closed() -> MultizenError {
        MultizenError::Mcp("launcher thread closed".into())
    }

    fn shop_script_dropped() -> MultizenError {
        MultizenError::Mcp("launcher thread dropped response".into())
    }

    /// 广播话术播放状态（`product-script-state-changed`）；无 AppHandle 时静默跳过。
    pub fn emit_product_script_state(&self, state: &ProductScriptPlayState) {
        self.emit(PRODUCT_SCRIPT_STATE_EVENT, state);
    }

    pub async fn shop_product_scripts_list(&self) -> Result<Vec<ShopProductScript>> {
        let (resp, receive) = oneshot::channel();
        self.launcher_tx
            .send(LauncherCmd::ShopProductScriptsList { resp })
            .await
            .map_err(|_| Self::shop_script_closed())?;
        receive.await.map_err(|_| Self::shop_script_dropped())?
    }

    pub async fn shop_product_script_get(
        &self,
        id: &str,
    ) -> Result<Option<ShopProductScriptDetail>> {
        let (resp, receive) = oneshot::channel();
        self.launcher_tx
            .send(LauncherCmd::ShopProductScriptGet {
                id: id.into(),
                resp,
            })
            .await
            .map_err(|_| Self::shop_script_closed())?;
        receive.await.map_err(|_| Self::shop_script_dropped())?
    }

    pub async fn shop_product_script_create(
        &self,
        input: CreateShopProductScriptInput,
    ) -> Result<ShopProductScript> {
        let (resp, receive) = oneshot::channel();
        self.launcher_tx
            .send(LauncherCmd::ShopProductScriptCreate { input, resp })
            .await
            .map_err(|_| Self::shop_script_closed())?;
        receive.await.map_err(|_| Self::shop_script_dropped())?
    }

    pub async fn shop_product_script_update(
        &self,
        id: &str,
        patch: UpdateShopProductScriptInput,
    ) -> Result<ShopProductScript> {
        let (resp, receive) = oneshot::channel();
        self.launcher_tx
            .send(LauncherCmd::ShopProductScriptUpdate {
                id: id.into(),
                patch,
                resp,
            })
            .await
            .map_err(|_| Self::shop_script_closed())?;
        receive.await.map_err(|_| Self::shop_script_dropped())?
    }

    pub async fn shop_product_script_delete(&self, id: &str) -> Result<()> {
        let (resp, receive) = oneshot::channel();
        self.launcher_tx
            .send(LauncherCmd::ShopProductScriptDelete {
                id: id.into(),
                resp,
            })
            .await
            .map_err(|_| Self::shop_script_closed())?;
        receive.await.map_err(|_| Self::shop_script_dropped())?
    }

    pub async fn shop_product_script_add_line(
        &self,
        input: AddShopProductScriptLineInput,
    ) -> Result<ShopProductScriptLine> {
        let (resp, receive) = oneshot::channel();
        self.launcher_tx
            .send(LauncherCmd::ShopProductScriptAddLine { input, resp })
            .await
            .map_err(|_| Self::shop_script_closed())?;
        receive.await.map_err(|_| Self::shop_script_dropped())?
    }

    pub async fn shop_product_script_update_line(
        &self,
        id: &str,
        patch: UpdateShopProductScriptLineInput,
    ) -> Result<ShopProductScriptLine> {
        let (resp, receive) = oneshot::channel();
        self.launcher_tx
            .send(LauncherCmd::ShopProductScriptUpdateLine {
                id: id.into(),
                patch,
                resp,
            })
            .await
            .map_err(|_| Self::shop_script_closed())?;
        receive.await.map_err(|_| Self::shop_script_dropped())?
    }

    pub async fn shop_product_script_delete_line(&self, id: &str) -> Result<()> {
        let (resp, receive) = oneshot::channel();
        self.launcher_tx
            .send(LauncherCmd::ShopProductScriptDeleteLine {
                id: id.into(),
                resp,
            })
            .await
            .map_err(|_| Self::shop_script_closed())?;
        receive.await.map_err(|_| Self::shop_script_dropped())?
    }

    pub async fn shop_product_script_reorder_lines(
        &self,
        script_id: &str,
        ordered_ids: Vec<String>,
    ) -> Result<Vec<ShopProductScriptLine>> {
        let (resp, receive) = oneshot::channel();
        self.launcher_tx
            .send(LauncherCmd::ShopProductScriptReorderLines {
                script_id: script_id.into(),
                ordered_ids,
                resp,
            })
            .await
            .map_err(|_| Self::shop_script_closed())?;
        receive.await.map_err(|_| Self::shop_script_dropped())?
    }

    /// 播放话术脚本：读脚本行 → 排期 → 真实 dispatcher 逐条派发，状态经
    /// `product-script-state-changed` 广播。
    ///
    /// - `goods_ids` 为 `Some` 且非空时先经 [`select_lines_for_goods`] 过滤，
    ///   只播放这些商品的行；`None` 或空表则播放全部。
    /// - `started_at_ms` 缺省用当前墙钟；`trigger_at <= started_at` 的行立即触发。
    /// - 播放期间同一脚本不可重复播放（`AlreadyExists`）。
    /// - 返回的 [`ShopScriptPlayback`] 含脚本 id / 起始时刻 / 排期条数；
    ///   用 [`TauriBrowserDriver::stop_shop_product_script`] 停止。
    pub async fn play_shop_product_script(
        self: &Arc<Self>,
        profile_id: &str,
        script_id: &str,
        goods_ids: Option<Vec<String>>,
        started_at_ms: Option<i64>,
    ) -> Result<ShopScriptPlayback> {
        if play_sessions().lock().unwrap().contains_key(script_id) {
            return Err(MultizenError::AlreadyExists(format!(
                "话术脚本 {script_id} 正在播放中"
            )));
        }
        let detail = self
            .shop_product_script_get(script_id)
            .await?
            .ok_or_else(|| MultizenError::NotFound(format!("话术脚本 {script_id} 不存在")))?;
        if detail.lines.is_empty() {
            return Err(MultizenError::Config("话术脚本没有话术行".into()));
        }
        let started_at_ms = started_at_ms.unwrap_or_else(now_ms);
        let schedule = build_schedule(&detail.lines, started_at_ms);
        let schedule = match goods_ids {
            Some(ids) if !ids.is_empty() => {
                let known: Vec<ProductGoods> = ids
                    .into_iter()
                    .map(|goods_id| ProductGoods {
                        goods_id,
                        title: String::new(),
                    })
                    .collect();
                select_lines_for_goods(&schedule, &known)
            }
            _ => schedule,
        };
        if schedule.is_empty() {
            return Err(MultizenError::Config("话术排期为空".into()));
        }
        let scheduled_count = schedule.len();
        let cancel = TaskCancel::new();
        let (stop_tx, stop_rx) = watch::channel(false);
        play_sessions().lock().unwrap().insert(
            script_id.to_string(),
            ShopScriptPlaySession {
                stop_tx,
                cancel: cancel.clone(),
            },
        );
        // 启动快照：`current_line_id` 为空表示尚未派发任何行。
        self.emit_product_script_state(&ProductScriptPlayState {
            script_id: script_id.to_string(),
            status: ProductScriptPlayStatus::Playing,
            current_line_id: None,
            current_index: 0,
            total: scheduled_count,
            started_at_ms,
        });
        let player = self.script_player();
        let script = script_id.to_string();
        let dispatcher =
            DriverShopActionDispatcher::new(Arc::clone(self), profile_id, cancel);
        tokio::spawn(async move {
            player
                .play(&script, schedule, &dispatcher, started_at_ms, stop_rx)
                .await;
            play_sessions().lock().unwrap().remove(&script);
        });
        Ok(ShopScriptPlayback {
            script_id: script_id.to_string(),
            started_at_ms,
            scheduled_count,
        })
    }

    /// 停止播放：置位 stop 通道 + 取消底层任务；返回是否确有正在播放的脚本。
    pub fn stop_shop_product_script(&self, script_id: &str) -> bool {
        let removed = play_sessions().lock().unwrap().remove(script_id);
        match removed {
            Some(session) => {
                session.cancel.cancel();
                let _ = session.stop_tx.send(true);
                true
            }
            None => false,
        }
    }

    /// 进程级播放器：持有状态广播通道，供前端/测试订阅。
    pub fn script_player(&self) -> Arc<ProductScriptPlayer> {
        script_player().clone()
    }
}

/// 播放句柄：脚本 id / 起始时刻 / 排期条数。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShopScriptPlayback {
    pub script_id: String,
    pub started_at_ms: i64,
    pub scheduled_count: usize,
}

/// 播放中的脚本运行态：`stop_tx` 置位停止循环，`cancel` 取消底层动作。
struct ShopScriptPlaySession {
    stop_tx: watch::Sender<bool>,
    cancel: TaskCancel,
}

fn play_sessions() -> &'static StdMutex<HashMap<String, ShopScriptPlaySession>> {
    static SESSIONS: OnceLock<StdMutex<HashMap<String, ShopScriptPlaySession>>> = OnceLock::new();
    SESSIONS.get_or_init(|| StdMutex::new(HashMap::new()))
}

fn script_player() -> &'static Arc<ProductScriptPlayer> {
    static PLAYER: OnceLock<Arc<ProductScriptPlayer>> = OnceLock::new();
    PLAYER.get_or_init(|| Arc::new(ProductScriptPlayer::new()))
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis().min(i64::MAX as u128) as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use multizen_core::BrowserEngine;
    use std::path::PathBuf;
    use std::sync::Mutex;
    use tempfile::TempDir;

    fn fixture() -> (TempDir, Arc<TauriBrowserDriver>) {
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
        (dir, Arc::new(driver))
    }

    fn line(
        id: &str,
        script_id: &str,
        action: ScriptLineAction,
        goods_id: &str,
        video_time_sec: f64,
        lead_sec: f64,
    ) -> ShopProductScriptLine {
        ShopProductScriptLine {
            id: id.into(),
            script_id: script_id.into(),
            sort_order: 0,
            action,
            goods_id: goods_id.into(),
            goods_name: None,
            video_time_sec,
            lead_sec,
            content: String::new(),
            created_at: "t".into(),
            updated_at: "t".into(),
        }
    }

    #[test]
    fn trigger_offset_subtracts_lead_from_video_time() {
        assert_eq!(trigger_offset_sec(60.0, 5.0), 55.0);
        assert_eq!(trigger_offset_sec(10.0, 0.0), 10.0);
        assert_eq!(trigger_offset_sec(3.0, 5.0), -2.0);
    }

    #[test]
    fn build_schedule_applies_lead_and_sorts_by_trigger() {
        let lines = vec![
            line("b", "s", ScriptLineAction::Explain, "g2", 60.0, 5.0),
            line("a", "s", ScriptLineAction::OnShelf, "g1", 10.0, 0.0),
        ];
        let schedule = build_schedule(&lines, 1_000);
        assert_eq!(schedule.len(), 2);
        // a: 1000 + 10*1000 = 11000; b: 1000 + 55*1000 = 56000.
        assert_eq!(schedule[0].line_id, "a");
        assert_eq!(schedule[0].trigger_at_ms, 11_000);
        assert_eq!(schedule[0].trigger_offset_sec, 10.0);
        assert_eq!(schedule[1].line_id, "b");
        assert_eq!(schedule[1].trigger_at_ms, 56_000);
        assert_eq!(schedule[1].trigger_offset_sec, 55.0);
    }

    #[test]
    fn build_schedule_clamps_negative_offset_to_started_at() {
        let lines = vec![line("x", "s", ScriptLineAction::OffShelf, "g", 3.0, 5.0)];
        let schedule = build_schedule(&lines, 7_000);
        assert_eq!(schedule[0].trigger_offset_sec, -2.0);
        assert_eq!(schedule[0].trigger_at_ms, 7_000);
    }

    #[derive(Default)]
    struct MockDispatcher {
        calls: Mutex<Vec<(String, String)>>,
        fail_on: Mutex<Vec<String>>,
    }

    impl MockDispatcher {
        fn record(&self, method: &str, goods_id: &str) -> Result<()> {
            self.calls
                .lock()
                .unwrap()
                .push((method.into(), goods_id.into()));
            if self.fail_on.lock().unwrap().iter().any(|g| g == goods_id) {
                return Err(MultizenError::Mcp("mock dispatch failure".into()));
            }
            Ok(())
        }
    }

    #[async_trait::async_trait]
    impl ShopActionDispatcher for MockDispatcher {
        async fn goods_on_shelf(&self, goods_id: &str) -> Result<()> {
            self.record("goods_on_shelf", goods_id)
        }
        async fn goods_off_shelf(&self, goods_id: &str) -> Result<()> {
            self.record("goods_off_shelf", goods_id)
        }
        async fn explain_goods(&self, goods_id: &str) -> Result<()> {
            self.record("explain_goods", goods_id)
        }
        async fn cancel_explain_goods(&self, goods_id: &str) -> Result<()> {
            self.record("cancel_explain_goods", goods_id)
        }
    }

    fn scheduled(action: ScriptLineAction, goods_id: &str) -> ScheduledLine {
        ScheduledLine {
            line_id: format!("{action:?}-{goods_id}"),
            script_id: "s".into(),
            action,
            goods_id: goods_id.into(),
            trigger_offset_sec: 0.0,
            trigger_at_ms: 0,
        }
    }

    #[tokio::test]
    async fn mock_dispatch_routes_each_action_to_its_method() {
        let dispatcher = MockDispatcher::default();
        for (action, method) in [
            (ScriptLineAction::OnShelf, "goods_on_shelf"),
            (ScriptLineAction::OffShelf, "goods_off_shelf"),
            (ScriptLineAction::Explain, "explain_goods"),
            (ScriptLineAction::CancelExplain, "cancel_explain_goods"),
        ] {
            dispatch_line_action(&dispatcher, &scheduled(action, "g1"))
                .await
                .unwrap();
            assert_eq!(
                dispatcher.calls.lock().unwrap().last().unwrap(),
                &(method.to_string(), "g1".to_string())
            );
        }
        assert_eq!(dispatcher.calls.lock().unwrap().len(), 4);
    }

    #[tokio::test]
    async fn mock_dispatch_propagates_dispatcher_error() {
        let dispatcher = MockDispatcher {
            fail_on: Mutex::new(vec!["bad".into()]),
            ..Default::default()
        };
        assert!(
            dispatch_line_action(&dispatcher, &scheduled(ScriptLineAction::Explain, "bad"))
                .await
                .is_err()
        );
    }

    struct StaticSource(Vec<ProductGoods>);

    impl ProductScriptSource for StaticSource {
        fn scan_goods_knowledge(&self) -> Vec<ProductGoods> {
            self.0.clone()
        }
    }

    #[test]
    fn select_lines_keeps_only_known_goods_in_schedule_order() {
        let schedule = vec![
            scheduled(ScriptLineAction::OnShelf, "g1"),
            scheduled(ScriptLineAction::Explain, "g2"),
            scheduled(ScriptLineAction::Explain, "g3"),
        ];
        let source = StaticSource(vec![
            ProductGoods {
                goods_id: "g3".into(),
                title: "third".into(),
            },
            ProductGoods {
                goods_id: "g1".into(),
                title: "first".into(),
            },
        ]);
        let known = source.scan_goods_knowledge();
        let selected = select_lines_for_goods(&schedule, &known);
        assert_eq!(selected.len(), 2);
        assert_eq!(selected[0].goods_id, "g1");
        assert_eq!(selected[1].goods_id, "g3");
    }

    #[tokio::test]
    async fn player_emits_playing_then_finished_for_immediate_schedule() {
        let player = ProductScriptPlayer::new();
        let mut rx = player.subscribe();
        let dispatcher = MockDispatcher::default();
        let schedule = vec![
            scheduled(ScriptLineAction::OnShelf, "g1"),
            scheduled(ScriptLineAction::Explain, "g2"),
        ];
        let (_tx, stop) = tokio::sync::watch::channel(false);
        let status = player.play("s", schedule, &dispatcher, 0, stop).await;
        assert_eq!(status, ProductScriptPlayStatus::Finished);
        let mut states = Vec::new();
        while let Ok(state) = rx.try_recv() {
            states.push(state);
        }
        assert_eq!(states.len(), 3);
        assert_eq!(states[0].status, ProductScriptPlayStatus::Playing);
        assert_eq!(states[0].current_line_id.as_deref(), Some("OnShelf-g1"));
        assert_eq!(states[1].current_index, 2);
        assert_eq!(states[2].status, ProductScriptPlayStatus::Finished);
        assert_eq!(states[2].current_index, 2);
        assert_eq!(dispatcher.calls.lock().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn player_stops_before_first_line_when_requested() {
        let player = ProductScriptPlayer::new();
        let mut rx = player.subscribe();
        let dispatcher = MockDispatcher::default();
        let schedule = vec![scheduled(ScriptLineAction::OnShelf, "g1")];
        let (_tx, stop) = tokio::sync::watch::channel(true);
        let status = player.play("s", schedule, &dispatcher, 0, stop).await;
        assert_eq!(status, ProductScriptPlayStatus::Cancelled);
        assert!(dispatcher.calls.lock().unwrap().is_empty());
        let state = rx.try_recv().unwrap();
        assert_eq!(state.status, ProductScriptPlayStatus::Cancelled);
    }

    #[tokio::test]
    async fn player_skips_failed_lines_and_continues() {
        let player = ProductScriptPlayer::new();
        let dispatcher = MockDispatcher {
            fail_on: Mutex::new(vec!["g1".into()]),
            ..Default::default()
        };
        let schedule = vec![
            scheduled(ScriptLineAction::OnShelf, "g1"),
            scheduled(ScriptLineAction::OnShelf, "g2"),
        ];
        let (_tx, stop) = tokio::sync::watch::channel(false);
        let status = player.play("s", schedule, &dispatcher, 0, stop).await;
        assert_eq!(status, ProductScriptPlayStatus::Finished);
        // Both lines attempted, only the successful one emits Playing.
        assert_eq!(dispatcher.calls.lock().unwrap().len(), 2);
    }

    #[test]
    fn state_event_name_is_stable_for_frontend() {
        assert_eq!(PRODUCT_SCRIPT_STATE_EVENT, "product-script-state-changed");
        let state = ProductScriptPlayState {
            script_id: "s".into(),
            status: ProductScriptPlayStatus::Playing,
            current_line_id: Some("l".into()),
            current_index: 1,
            total: 2,
            started_at_ms: 0,
        };
        let value = serde_json::to_value(&state).unwrap();
        assert_eq!(value["scriptId"], "s");
        assert_eq!(value["currentLineId"], "l");
        assert_eq!(value["currentIndex"], 1);
    }

    #[test]
    fn action_wire_names_are_kebab_case() {
        assert_eq!(
            serde_json::to_value(ScriptLineAction::OnShelf).unwrap(),
            "on-shelf"
        );
        assert_eq!(
            serde_json::to_value(ScriptLineAction::CancelExplain).unwrap(),
            "cancel-explain"
        );
    }

    #[test]
    fn shop_commands_are_registered_in_actual_tauri_handler() {
        let source = include_str!("../lib.rs");
        let handler = source
            .split(".invoke_handler(tauri::generate_handler![")
            .nth(1)
            .unwrap()
            .split("])")
            .next()
            .unwrap();
        for command in [
            "shop_product_scripts_list",
            "shop_product_script_get",
            "shop_product_script_create",
            "shop_product_script_update",
            "shop_product_script_delete",
            "shop_product_script_add_line",
            "shop_product_script_update_line",
            "shop_product_script_delete_line",
            "shop_product_script_reorder_lines",
            "shop_product_script_play",
            "shop_product_script_stop",
        ] {
            assert!(
                handler
                    .lines()
                    .any(|line| line.trim() == format!("{command},")),
                "{command}"
            );
        }
    }

    // --- 真实 dispatcher 路由（离线：driver 无会话，动作全部失败） ---

    /// 四个动作都要真正委派到底层原语：driver 没有会话/助手页，因此每个方法
    /// 都必须返回错误（而不是静默成功）。这证明 dispatcher 不再只是占位。
    #[tokio::test]
    async fn driver_dispatcher_delegates_every_action_and_surfaces_failure() {
        let (_dir, driver) = fixture();
        let cancel = TaskCancel::new();
        let dispatcher =
            DriverShopActionDispatcher::new(Arc::clone(&driver), "missing-profile", cancel);
        for action in [
            ScriptLineAction::OnShelf,
            ScriptLineAction::OffShelf,
            ScriptLineAction::Explain,
            ScriptLineAction::CancelExplain,
        ] {
            let line = scheduled(action, "12345");
            assert!(
                dispatch_line_action(&dispatcher, &line).await.is_err(),
                "{action:?} must reach the real primitive and fail offline"
            );
        }
    }

    /// 取消令牌已置位时，讲解/取消讲解在取页前即被 `console_task_page`/门禁拒绝，
    /// 不会误判为成功。
    #[tokio::test]
    async fn driver_dispatcher_explain_honours_cancelled_token() {
        let (_dir, driver) = fixture();
        let cancel = TaskCancel::new();
        cancel.cancel();
        let dispatcher =
            DriverShopActionDispatcher::new(Arc::clone(&driver), "missing-profile", cancel);
        assert!(dispatcher.explain_goods("12345").await.is_err());
        assert!(dispatcher.cancel_explain_goods("12345").await.is_err());
    }

    /// 播放：无会话时排期照常建立、状态经广播发出，动作失败被跳过、最终 Finished。
    #[tokio::test]
    async fn play_runs_schedule_and_broadcasts_states_offline() {
        let (_dir, driver) = fixture();
        let script = driver
            .shop_product_script_create(CreateShopProductScriptInput {
                name: "t".into(),
                description: None,
            })
            .await
            .unwrap();
        for (action, goods) in [
            (ScriptLineAction::OnShelf, "1001"),
            (ScriptLineAction::Explain, "1002"),
        ] {
            driver
                .shop_product_script_add_line(AddShopProductScriptLineInput {
                    script_id: script.id.clone(),
                    action,
                    goods_id: goods.into(),
                    goods_name: None,
                    video_time_sec: 0.0,
                    lead_sec: 0.0,
                    content: String::new(),
                    sort_order: None,
                })
                .await
                .unwrap();
        }
        let mut rx = driver.script_player().subscribe();
        let playback = driver
            .play_shop_product_script("missing-profile", &script.id, None, Some(0))
            .await
            .unwrap();
        assert_eq!(playback.scheduled_count, 2);
        // 重复播放被拒绝。
        assert!(driver
            .play_shop_product_script("missing-profile", &script.id, None, Some(0))
            .await
            .is_err());
        // 收集广播直到 Finished。
        let mut saw_finished = false;
        for _ in 0..8 {
            match tokio::time::timeout(std::time::Duration::from_secs(5), rx.recv()).await {
                Ok(Ok(state)) => {
                    if state.status == ProductScriptPlayStatus::Finished {
                        saw_finished = true;
                        break;
                    }
                }
                _ => break,
            }
        }
        assert!(saw_finished, "playback must broadcast Finished");
    }

    /// 停止：播放中的脚本可被停止并返回 true；未播放的返回 false。
    #[tokio::test]
    async fn stop_reports_whether_a_playback_was_running() {
        let (_dir, driver) = fixture();
        assert!(!driver.stop_shop_product_script("nope"));
    }
}