# 勘察证据 — 小店账号建号向导

所有 file:line 均来自本次会话实际读取的代码。

## 1. 账号 / 环境关系
- `crates/multizen-core/src/business.rs:6` `BusinessAccountKind { KuaishouShop, KuaishouLive, KuaishouMate, KuaishouSub, Jinniu }`；`:51` `BusinessAccount { id, kind, display_name, platform_user_id, profile_id: Option<String>, created_at, updated_at }`。
- `crates/profile-manager/src/business_accounts.rs:13` 建表；`:23` 唯一索引 `(kind, platform_user_id) WHERE platform_user_id IS NOT NULL`；`:167` 同一 `profile_id` 同时只能一条；`:206` `business_accounts_unbind` 置 `profile_id=NULL`。
- `components/profile/BusinessAccountSection.tsx:187` 明示「仅手工登记…不代表已登录或免登录，不共享 Cookie」。
- **结论**：`business_accounts` 是「手工登记」，`profile_id` 可空、删除环境后保留。本次需求（账号即环境）**不复用**它作为账号来源。

## 2. 环境（Profile）创建
- `components/profile/NewProfileSheet.tsx:172` `profiles.create(input)`；`:78` 挂载即 `fingerprintApi.generate()` 生成随机指纹；`:254` 代理区（`ProxyTester`）；`:350` 指纹区 `FingerprintForm`（传 `proxyForForm`）。
- `ui/src/lib/ipc.ts:85` `profiles_create`；`:96` `profiles_launch`；`:100` `launchKuaishou` = `profiles_launch { id, entry: "kuaishou-shop" }`。

## 3. 指纹一致性（机制已存在）
- `crates/tauri-app/src/commands/fingerprint.rs:243` `fingerprint_generate(seed)` → `profile_manager::fingerprint::default_fingerprint`（随机但内部自洽）。
- 同文件 `:322` `fingerprint_reconcile(fingerprint, patch)`：可对齐 locale / timezone / **country**（`FingerprintReconcilePatch.country` 注释：用代理探测到的国家覆盖，保证指纹国家与出口 IP 一致）。
- 同文件 `:386` `fingerprint_locale_for_country(country)`：按出口国家反查 locale（含文化邻近回退）。
- `components/profile/FingerprintForm.tsx` 接 `proxy` prop，探测出口地理后 reconcile。
- **结论**：一致性 = 生成随机指纹后按所选代理出口地理 reconcile。无需新增机制。

## 4. 登录
- `crates/cdp-driver/src/platforms/kuaishou.rs:354` `AuthPhase { LaunchingBrowser, VerifyingSession, WaitingForLogin, RestoringHeadless }`；`:409` `EnsureAuthResult { ok, scanned, error }`；注释 `:403-406`：`scanned:true` 是「上层还原 headless」的信号，**本层从不自己切 headless**。
- `:496` `connect()` 导航 `live_control_url`(= `zs.kwaixiaodian.com/page/helper`) 竞速判定已登录；`:538` `login()` 导航 `login_url`(= `login.kwaixiaodian.com/?biz=zone&redirect_url=…/s.kwaixiaodian.com/zone/home`) 后 `wait_for_selector(logged_in_selector)` —— **二维码显示在真实浏览器窗口里，driver 不抓二维码**。
- `crates/tauri-app/src/commands/kuaishou_auth.rs:53/68/84` `kuaishou_connect` / `kuaishou_login` / `ensure_kuaishou_auth`；`:24` 事件 `kuaishou-auth-phase` `{ profileId, phase }`；`:9` 「Headless restore after `scanned: true` belongs to the caller, never to this layer」。
- **结论**：小店登录的二维码目前**只在真实窗口显示**。要做「二维码进向导」必须新增：定时 `screenshot` 当前页面 + 裁出二维码区域（或整图给用户扫）。

## 5. 无头 / 隐藏窗口 现状
- 仓库没有「把窗口移出屏幕」的现成能力。`chromix` 支持 `headless` 选项（`resources/chromix/index.js:244` `effectiveHeadless`；`bridge.mjs:65` 缺省 `headless=false`）。
- 已有真实无头先例：`crates/tauri-app/src/driver/account_init/chromix_entry_fixture.rs:121` 用 `--headless=new`。
- **决定**：采用「有头启动 + 窗口隐藏到屏幕外/最小化」，复用已验证的有头登录路径，规避无头风控风险。

## 6. 截图能力（可复用）
- `crates/cdp-driver/src/page_ops.rs:96` `screenshot()`（CDP `Page.captureScreenshot`，PNG，不抢前台）；`bound_page.rs:72` / `tools.rs:30` / `task_page.rs:235` 逐层暴露。
- `crates/tauri-app/src/driver.rs:978` driver 级 `screenshot(profile_id) -> base64 PNG`；`mcp_embed.rs:128` 已用于 MCP `screenshot` 工具。
- **结论**：向导内定时截图二维码 = 复用 driver `screenshot`（轮询）。

## 7. 身份读取（可复用）
- `crates/tauri-app/src/driver/identity/`（`extract.rs` / `avatar.rs`）从小店顶栏读 `platformUserId` / `nickname` / `avatarKey`。
- `ui/src/lib/KuaishouIdentityProvider.tsx` 全局唯一轮询（15s）；`kuaishouIdentity.detect(profileId)` 手动重检。
- `components/profile/KuaishouIdentity.tsx:27` `IdentityAvatar`（已 export，小店列表在用）。

## 8. 初始化（沿用自动补做）
- `crates/multizen-core/src/kuaishou_account.rs:217` `KuaishouInitStep { Subject, Slice }`；`:249` `KuaishouInitStepRecord`。
- `crates/tauri-app/src/driver/account_init/`：`Subject`→`s.kwaixiaodian.com/zone/shop/info/qualification`（OCR+人工确认，`kuaishou_subject_*`）；`Slice`→`s.kwaixiaodian.com/zone/short-video-b/slice`。
- `KuaishouIdentity.tsx:177` 文案：「检测到有效账号后，后台自动补做主体采集和切片权限关闭的未完成项」。
- **结论**：向导不驱动初始化，交由既有自动补做。

## 9. 前端接入点（本次已实现部分）
- `components/kuaishou/KuaishouAccountsPage.tsx`：`shop` → `<KuaishouShopAccounts onAddAccount={...} />`。
- `components/kuaishou/KuaishouShopAccounts.tsx`：密集表格（`components/table/DataTable.tsx`）+ 「添加账号」按钮。
- `App.tsx:469` 目前 `onAddAccount={() => setShowSheet(true)}` —— **本次要替换为打开小店建号向导**。

## 10. i18n 约束
- `ui/src/i18n/dictionaries.test.mjs`：en / zh-CN **键集与占位符集必须完全一致**；`nav.item.shortcutTitle` 被该测试引用，不可删。
