# implement — mate-login

## 执行清单

### 1. HTTP 客户端与状态结构
- [ ] 1.1 `crates/tauri-app/src/driver/mate_login.rs` 建 MateLoginStage/MateLoginState/MateUser 结构体
- [ ] 1.2 `post_form<T>(url, body, cancel, ua)` reqwest 异步 POST，`Content-Type: application/x-www-form-urlencoded; Charset=UTF8`，Referer=url
- [ ] 1.3 两个 UA 常量：`UA_BROWSER`/`UA_RECEIVE`
- [ ] 1.4 超时常量：`POLL_TIMEOUT_MS=70s`/`POLL_RETRY_DELAY_MS=1.5s`/`POLL_MAX_RETRIES=5`/`ONE_SHOT_TIMEOUT_MS=20s`
- [ ] 1.5 4 个 URL 常量 + SID `kuaishou.shop.b`

### 2. 4 步状态机
- [ ] 2.1 `step_start(account_id, cancel)` → POST start，解析 QrStartResp，存 token/signature/imageData/expireAt，广播 state
- [ ] 2.2 `step_scan_result(account_id, cancel)` → long poll scanResult（select! cancel + 70s 超时 + 5 重试），解析 ScanResultResp
- [ ] 2.3 `step_accept_result(account_id, cancel)` → long poll acceptResult，解析 AcceptResultResp
- [ ] 2.4 `step_receive(account_id, cancel)` → POST receive（UA_RECEIVE），解析 ReceiveResp，提取 user 信息
- [ ] 2.5 `start_login(account_id, cancel)` 串联 4 步，每步更新 stage + 广播
- [ ] 2.6 `cancel(account_id)` 取消进行中登录（TaskCancel）
- [ ] 2.7 从 jieger 源码提取完整 result 码映射表（start/scanResult/acceptResult/receive 各 result 值含义）

### 3. 账号管理
- [ ] 3.1 business_accounts 新增 kind `mate`（或论证用独立字段）
- [ ] 3.2 `add_mate_account(name)`/`remove_mate_account(id)`/`rename_mate_account(id, name)`/`list_mate_accounts()`
- [ ] 3.3 remove 时先 cancel 进行中登录

### 4. IPC 命令层
- [ ] 4.1 `crates/tauri-app/src/commands/mate_login.rs` 新建
- [ ] 4.2 `#[tauri::command] async fn start_mate_login(account_id) -> Result<(), String>`
- [ ] 4.3 `#[tauri::command] async fn cancel_mate_login(account_id) -> Result<(), String>`
- [ ] 4.4 `#[tauri::command] async fn get_mate_login_state(account_id) -> Result<MateLoginState, String>`
- [ ] 4.5 `#[tauri::command] async fn list_mate_accounts() -> Result<Vec<Account>, String>` + add/remove/rename
- [ ] 4.6 状态变化 `app_handle.emit("mate-login-state-changed", state)`
- [ ] 4.7 `commands/mod.rs` 注册 + 前端 ipc.ts wrapper

### 5. 验证
- [ ] 5.1 `cargo test -p tauri-app --locked`（mock HTTP 4 端点，状态机走通 + cancel 中断）
- [ ] 5.2 `cargo check --workspace --locked`
- [ ] 5.3 真号 QR 扫码验收（单独确认）

## 验证命令
```bash
cargo test -p tauri-app --locked
cargo check --workspace --locked
```

## Rollback 点
- 1-2 HTTP+状态机层可独立 commit
- 3 账号管理层 commit
- 4 IPC 层 commit
- 5 验证后准备 archive

## 依赖前置
- `ks-platform-primitives`（与浏览器登录态协作，但 mate 本身是 HTTP 直连不依赖浏览器页面）
- 协作：`live-launch`（推流用 mate token）

## Notes
- token 不持久化（架构记忆：不搬旧登录态），每次重新走 QR。
- result 码表实现时从 jieger 源码完整提取，不要臆测。
