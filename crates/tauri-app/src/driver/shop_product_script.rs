//! jieger 商品话术库播放引擎：排期计算 + 动作派发 + 播放状态广播。
//!
//! shop-helper（`goods_on_shelf` / `goods_off_shelf`）与 goodsList
//!（`explain_goods` / `cancel_explain_goods`）不在此实现，只留 trait
//! 占位（[`ShopActionDispatcher`]），由后续接入或测试 mock 实现。
//! 商品知识扫描（供 auto-popup 调用）只留 [`ProductScriptSource`] 接口。
use super::{LauncherCmd, TauriBrowserDriver};
use multizen_core::{MultizenError, Result};
use profile_manager::{
    AddShopProductScriptLineInput, CreateShopProductScriptInput, ScriptLineAction,
    ShopProductScript, ShopProductScriptDetail, ShopProductScriptLine,
    UpdateShopProductScriptInput, UpdateShopProductScriptLineInput,
};
use serde::{Deserialize, Serialize};
use tokio::sync::oneshot;

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
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

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
        ] {
            assert!(
                handler
                    .lines()
                    .any(|line| line.trim() == format!("{command},")),
                "{command}"
            );
        }
    }
}
