# design — sub-account

## 技术设计

### 模块位置
新增 `crates/tauri-app/src/driver/sub_account.rs`。小号是观众身份（kuaishou.com 主站），与小店中控身份（zs.kwaixiaodian.com）不同——用 `KUAISHOU_SUB_ACCOUNT_CONFIG`。

### 账号存储决策
**复用 `business_accounts`**，新增 kind `kuaishou_sub`（或 `sub_account`）。理由：business_accounts 已有 scope=kuaishou + profile_id FK + platform_user_id，完全适配小号。不新增账号表（架构记忆：不新增账号表）。
- kind = `kuaishou_sub`
- platform_user_id = 快手 UID（小号）
- profile_id = 浏览器环境 Profile ID
- display_name = 小号别名

### 小号 CRUD
复用 business_accounts 的 save/list/unbind API，kind 固定 `kuaishou_sub`。分组用 business_accounts 的 group 字段或独立 `sub_account_groups` 表（决策：用 Profile group，不新增表）。

### 浏览器会话
小号会话 = browser-launcher 的独立 Profile（per-account）。**headless 切换由 browser-launcher 管**，本层只调 connect/login/enter_room/send_danmaku。
- `enter_live_room(account_id, live_room_url)`：navigate + waitForLiveRoomReady + 随机延迟 3-8s 防风控 + RATE_LIMIT_RETRIES=5 重试。
- `send_danmaku(account_id, message)`：经 TaskPage 在观众页操作（liveRoomActions.ts 等价）。

### 批量登录
`batch_login(account_ids, cancel)`：
- 逐个或并发（决策：并发上限 3，防风控）调 `login_account(account_id, cancel)`。
- `login_account`：ensure_auth（ks-platform-primitives，用 KUAISHOU_SUB_ACCOUNT_CONFIG）。
- 后台轮询登录态：`sub_login_poll` 每 3s 检查，`KS_SUB_LOGIN_DETECT_MS=30s` 初次等头像，`KS_VERIFY_MS=20s`/`KS_LOGIN_MS=3min`。
- `browserCloseGrace=150ms` 宽限（避免短暂抖动误判）。

### 发弹幕（liveRoomActions 等价）
- `pick_input` / `pick_send_button`：多候选选择器按可见/可用/位置/placeholder 评分选最佳（复刻 jieger 评分逻辑）。
- `fill_comment_input`：兼容 input/textarea/contenteditable。
- `send_danmaku`：click 发送按钮，缺失回退 Enter；检查输入框是否清空，必要时回车补发。
- `detect_security_verification`：检测滑块/验证码/安全验证（关键词 + captcha DOM），命中抛 `SubAccountVerificationRequiredError` 等价。

### 互动历史
`record_interaction(account_id, result)` 落库。表决策：profile-manager 新增 `sub_account_interactions` 表（account_id/action/content/result/timestamp）。

### 运行时状态
```rust
pub enum LiveRoomStatus { Idle, Entering, Entered, Error }
pub struct SubAccountRuntime {
    live_room_status: LiveRoomStatus,
    live_room_url: Option<String>,
    last_enter_error: Option<String>,
    stats: SubAccountStats,  // total_sent/success/fail/last_error/last_send_time
}
```
`RwLock<HashMap<String, SubAccountRuntime>>` per-account。

### sendLocks 串行
per-account 串行发送（`tokio::sync::Mutex` per-account），防并发触发风控。

### 风险与取舍
- **并发登录防风控**：批量登录并发上限 3，且每个登录间隔随机。
- **选择器评分**：pick_input/pick_send_button 的评分逻辑复杂但必要，直接复刻。
- **安全验证检测**：jieger 9 个关键词 + 8 个 DOM 选择器，全量迁移。
- **contenteditable 兼容**：fillCommentInput 的 contenteditable 分支用 `dispatchEvent(InputEvent)`，Rust 侧 evaluate JS 复刻。

### 验证
- 单元测试：pick_input/pick_send_button 评分逻辑（mock DOM 句柄）、detect_security_verification 关键词匹配。
- 离线 wire 测试：send_danmaku 流程（mock TaskPage evaluate/click）。
- 真号小号登录/发弹幕验收单独确认。
