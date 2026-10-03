# design — huibo-live

## 技术设计

### 模块位置
新增 `crates/tauri-app/src/driver/huibo_live.rs`。跟播/回播是录播带货开播流程，在中控页操作。

### 功能（51 行最小模块）
jieger 的 `huiboLive`：
1. `getHuiboVideoList(account_id)`：确保会话连接 → `fetchVideoList(page)` 取自传视频列表。
2. `startHuiboLive({ accountId, replayId })`：确保连接 → `startHuiboLiveFlow(page, replayId)` 执行慧播开播流程 → `getShopLiveState(accountId)` 返回直播状态。

### 依赖 liveControl
jieger `huiboLive` 调 `connect`/`getShopLiveState`（来自 `tasks/liveControl`）。Cloaksession：
- `connect` → ks-platform-primitives 的 `ensure_auth`。
- `getShopLiveState` → 本任务或 live-launch 提供（直播状态查询）。

### HuiboVideo
```rust
pub struct HuiboVideo {
    pub replay_id: String,
    pub title: String,
    pub duration: i64,
    pub thumbnail: Option<String>,
}
```
字段从 jieger `src/types/entities.ts` 的 HuiboVideo 提取。

### ShopLiveState
```rust
pub struct ShopLiveState {
    pub status: String,  // idle/live/error 等
    pub live_stream_id: Option<String>,
    // ... 从 liveControl 提取完整字段
}
```

### huiboActions
`fetchVideoList(page)` / `startHuiboLiveFlow(page, replay_id)` 在 `platforms/kuaishou/huiboActions.ts`。Rust 侧复刻为 TaskPage evaluate + click。

### 风险与取舍
- **最小模块**：51 行，细节少，但依赖 liveControl 的 getShopLiveState——若 ShopLiveState 已在 live-launch 提供，复用；否则本任务实现。
- **回播语义**：replayId 是录播视频 ID，startHuiboLiveFlow 在中控页选视频开播。

### 验证
- 单元测试：HuiboVideo/ShopLiveState 结构体解析。
- 离线 wire 测试：mock TaskPage evaluate 返回视频列表 DOM。
- 真号回播验收单独确认。
