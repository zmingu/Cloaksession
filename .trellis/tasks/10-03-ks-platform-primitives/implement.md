# implement — ks-platform-primitives

## 执行清单

### 1. 平台配置层
- [ ] 1.1 在 `crates/cdp-driver/src/platforms/mod.rs` 建 `pub mod kuaishou;`
- [ ] 1.2 `kuaishou.rs` 定义 `PlatformConfig`/`VerifyConfig`/`SubAccountPlatformConfig`/`JinniuPlatformConfig` 结构体 + 三套 `const` 实例（`KUAISHOU_CONFIG`/`KUAISHOU_SUB_ACCOUNT_CONFIG`/`KUAISHOU_JINNIU_CONFIG`）
- [ ] 1.3 `build_startup_url(account_id: Option<&str>)` 实现，断言 `homeType=new` 强制
- [ ] 1.4 `extract_jinniu_account_id_from_url(url: &str) -> Option<String>` 实现
- [ ] 1.5 单元测试：URL pattern 正确性、build_startup_url 的 homeType 断言、extract 边界（空/无参/有参/URL 无效）

### 2. connect 原语
- [ ] 2.1 `pub async fn connect(page: &mut TaskPage, cfg: &PlatformConfig, cancel: &TaskCancel, timeout: Duration) -> TaskResult<bool>`
- [ ] 2.2 `page.navigate(cfg.live_control_url, timeout)` goto 中控页
- [ ] 2.3 tokio::select! race：cancel.cancelled() / deadline / `wait_for_selector(in_live_control_selector, Visible, timeout)` / URL 轮询（`evaluate("location.href")` 命中 loginPagePattern）
- [ ] 2.4 返回 `!login_page_pattern_hit`
- [ ] 2.5 离线 wire 测试：用 `tests/common` peer mock 两条分支（命中 loginPagePattern 返回 false / 命中 inLiveControlSelector 返回 true）

### 3. login 原语
- [ ] 3.1 `pub async fn login(page: &mut TaskPage, cfg: &PlatformConfig, cancel: &TaskCancel, timeout: Duration) -> TaskResult<()>`
- [ ] 3.2 `evaluate("location.href")` 判定，不在登录页则 `navigate(cfg.login_url, timeout)`
- [ ] 3.3 `wait_for_selector(logged_in_selector, Visible, login_timeout)`（login_timeout = 5min）
- [ ] 3.4 cancel 经 select! 中断 wait_for_selector
- [ ] 3.5 离线 wire 测试：navigate 分支 + selector 命中分支

### 4. ensure_auth 原语
- [ ] 4.1 `pub async fn ensure_auth(session: &BrowserSession, target_id: &str, cfg: &PlatformConfig, desired_headless: bool, cancel: TaskCancel, on_phase: impl Fn(AuthPhase) + Send) -> TaskResult<EnsureAuthResult>`
- [ ] 4.2 `AuthPhase` 枚举 + `on_phase(LaunchingBrowser)`
- [ ] 4.3 调 `connect` 校验；成功返回 `{ ok: true, scanned: false }`
- [ ] 4.4 失败 → `on_phase(WaitingForLogin)` → 调 `login`；cancel 检查
- [ ] 4.5 成功后若 `desired_headless` → `on_phase(RestoringHeadless)` → 交回上层重启 headless（本层不直接重启，通过返回值 `scanned: true` 让上层知道需重启）
- [ ] 4.6 `EnsureAuthResult { ok, scanned, error: Option<String> }`

### 5. tauri-app 命令层
- [ ] 5.1 `crates/tauri-app/src/commands/kuaishou_auth.rs` 新建
- [ ] 5.2 `#[tauri::command] async fn kuaishou_connect(profile_id: String) -> Result<bool, String>`
- [ ] 5.3 `#[tauri::command] async fn kuaishou_login(profile_id: String) -> Result<(), String>`
- [ ] 5.4 `#[tauri::command] async fn ensure_kuaishou_auth(profile_id: String, desired_headless: bool) -> Result<EnsureAuthResult, String>`
- [ ] 5.5 `on_phase` 回调 → `app_handle.emit("kuaishou-auth-phase", phase)`
- [ ] 5.6 `commands/mod.rs` 注册三个命令到 `invoke_handler`
- [ ] 5.7 前端 `ui/src/lib/ipc.ts` 加三个调用 wrapper

### 6. 验证
- [ ] 6.1 `cargo test -p cdp-driver --locked`（含新单元测试 + wire 测试）
- [ ] 6.2 `cargo check --workspace --locked`
- [ ] 6.3 `cargo test --workspace --locked`（确认未破坏既有）
- [ ] 6.4 真号扫码登录集成验收（单独确认，不自动）

## 验证命令
```bash
cargo test -p cdp-driver --locked
cargo check --workspace --locked
cargo test --workspace --locked
```

## Rollback 点
- 1.x 完成后可独立 commit（配置层无副作用）。
- 2-4 完成后可独立 commit（原语层 + wire 测试）。
- 5 完成后 commit（IPC 层）。
- 6 验证通过后准备 archive；真号验收失败不回滚代码，标 task 未完成。

## 依赖前置
- 无（波 0，所有其他子任务依赖本任务）。

## Notes
- 选择器值用 jieger 的占位骨架，真号校准由各业务 task 负责。
- headless 切换不在本层，通过 `EnsureAuthResult.scanned` 让上层决定是否重启。
