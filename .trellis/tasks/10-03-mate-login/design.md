# design — mate-login

## 技术设计

### 模块位置
新增 `crates/tauri-app/src/driver/mate_login.rs`（HTTP QR 登录是应用层 HTTP 客户端，不归 cdp-driver）。新 IPC 命令挂 `commands/mate_login.rs`。

### HTTP 客户端
- 用 `reqwest`（已是 workspace 依赖或新增）。
- 两个 UA：`Mozilla/4.0 (compatible; MSIE 9.0; Windows NT 6.1)`（start/scanResult/acceptResult）与 `kuaishou 5.105.2.3505`（receive）。
- Content-Type: `application/x-www-form-urlencoded; Charset=UTF8`。
- Referer 设为请求 URL 本身。

### 4 步状态机
```rust
pub enum MateLoginStage {
    Idle, Starting, AwaitingScan, AwaitingConfirm, Receiving, Success, Expired, Cancelled, Error,
}
pub struct MateLoginState {
    account_id: String,
    stage: MateLoginStage,
    qr_image_data_url: Option<String>,
    qr_login_token: Option<String>,
    qr_login_signature: Option<String>,
    expire_at: Option<i64>,
    error_message: Option<String>,
    user: Option<MateUser>,
    started_at: Option<i64>,
    finished_at: Option<i64>,
}
```

### 4 步流程
1. **start**: POST `qr.kuaishou.com/rest/q/user/login/qrcode/start` → `{result, qrLoginToken, qrLoginSignature, expireTime, imageData}`。imageData 直接是 data URL（base64 PNG），存到 state 广播前端展示 QR。
2. **scanResult**: long poll POST `id.kwaixiaodian.com/rest/c/infra/ks/qr/scanResult`（`POLL_TIMEOUT_MS=70s`），重试 `POLL_MAX_RETRIES=5` × `POLL_RETRY_DELAY_MS=1.5s`。result 表示扫码状态。
3. **acceptResult**: long poll POST `acceptResult`，同上超时/重试。result 表示手机确认。
4. **receive**: POST `qr.kuaishou.com/rest/q/user/login/qrcode/receive` → `{result, passToken, token, lmtoken, kuaishou.live.mate_st, kuaishou.live.mate.h5_st, user: {user_id, user_name, headurl}}`。UA 用 `kuaishou 5.105.2.3505`。SID `kuaishou.shop.b`。

### 取消
`tokio::select!` 包每个 HTTP 请求：`cancel.cancelled()` 中断 long poll。`TaskCancel` 复用 cdp-driver 的（一致性）。drop cancel handle 即取消。

### 账号管理
jieger mate 账号用 `Account.usage='mate'`。Cloaksession 用 `business_accounts` 的 kind——**新增 kind `mate`**（或在 design.md 论证用独立字段）。CRUD：`add_mate_account`/`remove_mate_account`/`rename_mate_account`/`list_mate_accounts`。

### 状态广播
`MateLoginState` 变化 → `app_handle.emit("mate-login-state-changed", state)`。QR 图片经 state 的 `qr_image_data_url` 传前端。

### 风险与取舍
- **long poll 超时**：70s + 5 重试 × 1.5s ≈ 77.5s 每轮，总登录窗口与 jieger `KS_LOGIN_MS=3min` 对齐——但 mate 是 HTTP 直连，不受浏览器扫码窗口限制，超时由 `expireAt` 决定。
- **result 码语义**：jieger 用 result 数字判定状态，需在 design.md 对照源码逐个 result 值映射（start/scanResult/acceptResult/receive 各自的 result 表）——实现时从 jieger 源码提取完整 result 表。
- **token 持久化**：receive 返回的 passToken/token/lmtoken 是否持久化？jieger 存 Account；Cloaksession 决策：**不持久化 token**（架构记忆：不搬旧登录态），每次 mate-login 重新走 QR 流程获取。

### 验证
- 单元测试：4 步状态机转换、result 码映射表、UA/header 正确性。
- 离线 HTTP mock 测试：mock 4 个端点，验证状态机走通 + cancel 中断 long poll。
- 真号扫码验收单独确认。
