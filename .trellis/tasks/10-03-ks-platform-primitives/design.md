# design — ks-platform-primitives

## 技术设计

### 模块位置
新增 `crates/cdp-driver/src/platforms/kuaishou.rs`（或在 cdp-driver 内建 `platforms/` 子模块）。**不新建 crate**——三原语本质是 TaskPage 之上的薄封装，归 cdp-driver 避免反向依赖（browser-launcher/tauri-app 已依赖 cdp-driver）。

对外暴露：
```rust
pub mod platforms::kuaishou {
    pub fn connect(page: &mut TaskPage, cancel: &TaskCancel, timeout: Duration) -> TaskResult<bool>;
    pub async fn login(page: &mut TaskPage, cancel: &TaskCancel, timeout: Duration) -> TaskResult<()>;
    pub async fn ensure_auth(
        session: &BrowserSession,
        target_id: &str,
        desired_headless: bool,
        cancel: TaskCancel,
        on_phase: impl Fn(AuthPhase),
    ) -> TaskResult<EnsureAuthResult>;
}
```

### 三原语映射

**connect（原 `kuaishouConnect`）：**
- jieger: `page.goto(liveControlUrl)` → `Promise.race([waitForURL(loginPagePattern), waitForSelector(inLiveControlSelector)])` → `return !loginPagePattern.test(page.url())`
- Rust: `TaskPage::navigate(liveControlUrl)` → tokio::select!（cancel.cancelled() / deadline / `wait_for_selector(inLiveControlSelector, Visible, timeout)` / URL 轮询命中 loginPagePattern）
- URL 轮询：`TaskPage::evaluate("location.href")` 周期检查（poll_interval 500ms），命中 loginPagePattern 即「未登录」
- 返回 `!login_page_pattern_hit`

**login（原 `kuaishouLogin`）：**
- jieger: 若不在登录页则 goto(loginUrl) → `waitForSelector(loggedInSelector, timeout: 0)`
- Rust: `evaluate("location.href")` 判定，不在登录页则 `navigate(loginUrl)` → `wait_for_selector(loggedInSelector, Visible, login_timeout)`
- `timeout: 0` 在 Rust 侧改为有界（`KS_LOGIN_WAIT_TIMEOUT_MS=5min`），cancel 经 `TaskCancel` 传入

**ensure_auth（原 `ensureKuaishouAuth`）：**
- jieger 五步：closeSession → getOrCreateSession(headless) → connect 校验 → 失败切 headful 扫码 → 成功后切回 headless 重校验
- Rust: BrowserSession 的生命周期由 browser-launcher 管理（非本层职责）；本层只负责「在已绑定的 BrowserSession 上跑 connect/login 流程」。headless 切换是 browser-launcher 层能力——**本任务通过回调 `on_phase` 让上层（tauri-app driver）执行浏览器重启，不直接管 headless 切换**。
- EnsureAuthResult: `{ ok: bool, scanned: bool, error: Option<String> }`
- cancel 经 `TaskCancel::new()` 由调用方持有，drop 即取消

### 三套平台配置（Rust 形态）

```rust
pub struct PlatformConfig {
    pub login_url: &'static str,
    pub live_control_url: &'static str,
    pub store_home_url: &'static str,
    pub store_login_url: &'static str,
    pub verify: VerifyConfig,
}
pub struct VerifyConfig {
    pub login_page_pattern: regex::Regex,
    pub logged_in_selector: &'static str,
    pub in_live_control_selector: &'static str,
    pub store_home_pattern: regex::Regex,
    pub cookie_key: &'static str,
    pub local_storage_key: &'static str,
}
```

三套常量实例：
- `KUAISHOU_CONFIG`（小店中控，zs.kwaixiaodian.com）
- `KUAISHOU_SUB_ACCOUNT_CONFIG`（小号观众，kuaishou.com）——字段更少（loginUrl/liveRoomUrlTemplate/commentInputSelector/sendButtonSelector）
- `KUAISHOU_JINNIU_CONFIG`（金牛，niu.e.kuaishou.com/home?homeType=new）——含 `build_startup_url(account_id)` + selectors 启发式数组

选择器启发式数组用 `&'static [&'static str]`，tryLocators 顺序 try（复刻 jieger jinniu selectors 的 `accountDialog`/`masterItem`/`masterName`/`masterId`/`topbarAccountTrigger`）。

### 阶段事件广播

`AuthPhase` 枚举：`LaunchingBrowser | VerifyingSession | WaitingForLogin | RestoringHeadless`。经 `on_phase: impl Fn(AuthPhase)` 回调，上层（tauri-app driver）转 Tauri event `kuaishou-auth-phase`。**不直接 emit Tauri event**——保持 cdp-driver 不依赖 tauri-app。

### homeType=new 强制（金牛）
`build_startup_url(account_id)` 永远拼 `homeType=new`。这是架构记忆硬约束（快手命名反的：new=旧版），写进常量与单元测试断言。

### 数据流
```
tauri-app::commands::kuaishou_auth
  → browser-launcher: 获取/重启 BrowserSession（headless 切换在这层）
  → cdp_driver::platforms::kuaishou::ensure_auth(session, target_id, ...)
      → connect(loginControlUrl) | login(loginUrl)
      → on_phase 回调 → tauri-app emit "kuaishou-auth-phase"
  → 返回 EnsureAuthResult 给前端
```

### 兼容性
- 不改 TaskPage/BrowserSession 既有 API（只在其上加平台封装）
- 不改 business_accounts schema
- 不改 IPC 既有命令（新增 `kuaishou_connect`/`kuaishou_login`/`ensure_kuaishou_auth` 命令）

### 风险与取舍
- **选择器过期**：jieger 选择器是占位骨架（`selectors.ts` 注释明说需校准）。本任务只迁配置结构，选择器值在真号上由后续各业务 task 校准——design 不保证选择器有效。
- **headless 切换跨层**：ensure_auth 的 headless 重启需要 browser-launcher 协作，通过 on_phase 回调解耦，避免 cdp-driver 反向依赖 browser-launcher。
- **URL 轮询 vs waitForURL**：chromiumoxide 无 Playwright 的 `waitForURL`，用 `evaluate(location.href)` 周期轮询近似。poll_interval 500ms，与 TaskPage 的 selector 轮询一致。

### 验证策略
- 单元测试：三套配置的 URL pattern 正确性、`build_startup_url` 的 homeType=new 断言、`extract_jinniu_account_id_from_url` 边界。
- 离线 wire 测试：connect 的 race 逻辑用 `tests/common` 的 HTTP/WS peer mock（命中 loginPagePattern vs 命中 inLiveControlSelector 两条分支）。
- 真号扫码集成验收：单独确认，不自动跑。
