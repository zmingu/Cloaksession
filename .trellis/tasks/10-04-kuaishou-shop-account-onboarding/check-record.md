# Phase 2.2 全量质量复核记录

> 节点：③ Phase 2.2 全量质量复核（full-scope，非仅最近改动）
> 分支：main｜工作目录：F:\Cloaksession｜日期：2026-10-04
> 复核范围：本任务全部工作区改动（`git status --porcelain` 全量，含上游 node-1/node-2 的改动）

---

## 一、逐项 spec 核对结论

| # | Spec 文件 | 结论 | 依据 / 备注 |
|---|---|---|---|
| 1 | `tauri-app/backend/index.md` | ✅ 通过 | 命令注册在 `lib.rs::run`（`kuaishou_login_qr` 已注册）；IPC 参数/序列化对齐；同时检查 Tauri IPC 与 MCP 消费方（`BrowserDriver::launch` 签名同步更新）；浏览器类测试如实标注为 mock、非 native E2E。 |
| 2 | `tauri-app/backend/ipc.md` | ⚠️ 发现并修复 | 违反「Keep wrappers in `ui/src/lib/ipc.ts`…旧调用不受影响」：`profiles.launch/launchKuaishou` 曾无条件发送 `hidden`。已改为缺省不发送（见问题 ①）。`kuaishou_login_qr` 薄适配、`Result<Option<String>, String>`、无 DB 写入，符合。 |
| 3 | `tauri-app/backend/i18n.md` | ✅ 通过 | `kuaishou.wizard.*` / `kuaishou.shop.*` 键在 en 与 zh-CN 同时新增；键集/占位符集一致（`dictionaries.test.mjs` 6 pass）；使用 `{{name}}`/`{{error}}` 命名占位符；未改 `MultizenError::Display`；未新增需翻译的 Rust 原生文案。 |
| 4 | `tauri-app/backend/kuaishou-identity.md` | ✅ 通过 | 向导复用 `kuaishou_identity_detect`（只读、不导航）；`Snapshot` 字段 camelCase、可空字段显式 null 未被改动；`detected` 语义未被曲解为「已登录」。 |
| 5 | `tauri-app/backend/kuaishou-initialization.md` | ✅ 通过 | 向导不驱动初始化（R5/D7），未触碰 `account_init` 的租约/槽位/暂存契约；未新增初始化命令。 |
| 6 | `cdp-driver/backend/sessions.md` | ✅ 通过 | 复用既有 `screenshot`（base64 PNG、无 data-URI 前缀，与本任务契约一致）；未改 headless 语义、未改 active-page 行为。 |
| 7 | `cdp-driver/backend/task-control.md` | ✅ 通过 | 未新增 TaskPage 用法；`kuaishou_login_qr` 走 registry session 的既有 `screenshot`，未绕开协作式租约契约。 |
| 8 | `browser-launcher/backend/lifecycle.md` | ⚠️ 发现并修复 | `build_spawn_args` 的 `hidden` 只追加 `--window-position`，**未加 `--headless`**（符合）；但 `launch_with_chromix` 的 hidden 分支曾整体覆盖 `launchOptions.args`，违反「保留 profile 自身 options」的既有契约。已修复（见问题 ②）。 |
| 9 | `browser-launcher/backend/business-isolation.md` | ✅ 通过 | 未改 `validate_business_directory` 门禁、未改 `effective_data_dir`、未新增 Node worker；`hidden` 仅影响窗口位置参数，不改变目录/隔离策略。 |
| 10 | `profile-manager/backend/business-accounts.md` | ✅ 通过 | `crates/profile-manager/` 零改动；未新增迁移；未写 `business_accounts`；唯一索引/事务边界未受影响（`git status --porcelain -- crates/profile-manager/` 为空）。 |
| 11 | `guides/cross-layer-thinking-guide.md` | ⚠️ 发现并修复 | Rust `launch(hidden)` ↔ IPC 封装 ↔ UI 向导跨层一致，但问题 ① 正是「契约漂移」型跨层缺陷，已修复并加回归。 |
| 12 | `guides/code-reuse-thinking-guide.md` | ✅ 通过 | 复用 `screenshot`/`kuaishou_identity_detect`/`fingerprint_generate`/`fingerprint_reconcile`/`fingerprint_locale_for_country`/`proxy_detect_geo`/`parseProxyString`/`ProxyTester`/`IdentityAvatar`/`Modal`/`DataTable`，未重复造轮子；未复制粘贴逻辑。 |

补充核对（`get_context.py --mode packages` 列出的全部 spec 层）：
- `multizen-core/backend/contracts.md`：`LaunchedProfile` / `ProxyConfig` / `FingerprintConfig` 未改线格式；`launch` 的 `hidden` 只是 `BrowserDriver` trait 方法参数，非 serde 模型。✅
- `mcp-server/backend/tools.md` + `index.md`：MCP 路径 `driver.launch(id, false)` 显式传 `false`，行为与旧「可见启动」完全一致；`mock_driver`/`tools.rs` 测试同步更新。✅
- `browser-launcher/backend/chromix.md`：hidden 只走 `launchOptions.args` 合并，未改 bridge `prepareOptions`、未注入 CloakBrowser 指纹/代理泄漏参数、未设 headless。✅
- `settings-store/backend/*`、`behavioral/backend/*`：本任务零改动。✅

---

## 二、发现的问题与修法

### 问题 ①（跨层契约回归，已修复）— `profiles.launch` 无条件发送 `hidden`

- **文件**：`crates/tauri-app/ui/src/lib/ipc.ts`（L95–107）
- **现象**：`launch(id, hidden = false)` / `launchKuaishou(id, hidden = false)` 把 `hidden` **总是**放进 payload，普通启动会发 `{ id, hidden: false }`（小店页的 Launch 按钮、App 的启动、`KuaishouIdentity` 的扫码按钮全部受影响）。
- **违反**：任务硬性要求「`profiles_launch` 的 hidden 必须是可选，**缺省行为不变**（不破坏既有调用方与 MCP）」；`ipc.md` 的「旧调用不受影响」。
- **证据**：`tests/shop-login.spec.ts` 3 例中 2 例失败（`expect(calls[0]).toEqual({ id })` 实收 `{ id, hidden: false }`）。用 `git stash push -- ipc.ts` 单独暂存该文件后复跑 → **3 passed**，证明为**本任务引入**（非预存在）。
- **修法**：缺省不发送 —— `hidden ? { id, hidden: true } : { id }`；`launchKuaishou` 同理（无 hidden 时退回 `{ id, entry }`）。
- **验证**：`shop-login.spec.ts` 3 passed；连同向导/小店用例共 5 passed。

### 问题 ②（Chromix 选项丢失，已修复）— hidden 覆盖 `launchOptions.args`

- **文件**：`crates/browser-launcher/src/driver.rs::launch_with_chromix`（L214–234）
- **现象**：hidden 分支用 `insert("args", ["--window-position=…"])` **整体替换** `launchOptions.args`，会丢弃 profile/SDK 已有的自定义 args。
- **违反**：`chromix.md`「Global/profile options are shallow-combined … direct launcher callers must provide the effective config」与 `lifecycle.md` 的参数契约（只追加，不覆盖）。
- **修法**：改为读取既有 `args` 数组并 `push` 位置参数（缺失时视为空数组）。
- **新增回归**：`crates/browser-launcher/tests/chromix.rs::hidden_launch_merges_window_position_into_existing_launch_args`（fake-SDK 断言：既保留 `--existing-flag`、追加 `--window-position=-32000,-32000`、保留 `slowMo`、且无任何 `--headless`）。
- **验证**：`cargo test -p browser-launcher --test chromix` 8 passed。

### 问题 ③（AC9 定时器泄漏，已修复）— 关闭向导后 QR 轮询不停

- **文件**：`crates/tauri-app/ui/src/components/kuaishou/KuaishouAccountWizard.tsx`（轮询 `useEffect`）
- **现象**：`Modal` 在 `open=false` 时返回 `null`，但向导组件**保持挂载**；轮询 effect 的依赖是 `[step, profileId]`，关闭（cancel）后 `step` 仍是 `waiting`，故 cleanup **不会执行**，定时器继续截图 + 轮询身份 —— 违反 AC9「向导关闭后停止（不泄漏定时器/不持续截图）」。
- **修法**：轮询 effect 加入 `open` 门禁（`if (!open || step !== "waiting" || !profileId) return;`，依赖 `[open, step, profileId]`），关闭即拆定时器。
- **新增回归**：`tests/kuaishou-shop-wizard.spec.ts` 增加「cancelling the wait stops the QR/identity polling」用例（关闭后等待 4.5s，断言 `kuaishou_login_qr` / `kuaishou_identity_detect` 调用数不再增长）；并给 mock 增加 `neverDetect` 开关。
- **验证**：临时回退修法（去掉 `open` 门禁）→ 该用例 **1 failed**（证明能捕获回归）；恢复修法 → **2 passed**。

### 非问题（确认无越界）
- 未新增 DB 迁移、未写 `business_accounts`：`git status --porcelain -- crates/profile-manager/` 为空，全量 diff 中无 `INSERT INTO business_*` / `migrate` 变更。
- `build_spawn_args` 的 hidden 分支**只加** `--window-position`，**无 `--headless`**（既有回归 + 新增断言双重覆盖）。

---

## 三、校验命令与结果（全部真实执行）

### 前端（cwd = `crates/tauri-app/ui`）
| 命令 | 结果 |
|---|---|
| `npx tsc -b --force` | **EXIT 0**，无错误 |
| `node --test src/i18n/dictionaries.test.mjs` | **EXIT 0**，**6 pass / 0 fail** |
| `npm run build` | **EXIT 0**，`✓ built in 1.46s` |
| `npx playwright test tests/kuaishou-shop.spec.ts tests/kuaishou-shop-wizard.spec.ts --project=desktop-chrome` | **EXIT 0**，**3 passed**（含新增 AC9 取消用例） |
| 回归：`wave4-verify / chromix-settings / d-shop-popup / listen-sub / business-accounts / modal-focus / kuaishou-identity / kuaishou-lifecycle / shop-login` | **EXIT 0**，**50 passed** |

### Rust（cwd = `F:\Cloaksession`）
| 命令 | 结果 |
|---|---|
| `cargo check --workspace --all-targets` | **EXIT 0**（仅 1 条既有 dead_code 警告，非本任务） |
| `cargo test -p tauri-app` | **EXIT 0**，222 passed / 0 failed（+4 registry） |
| `cargo test -p browser-launcher` | **EXIT 0**，7 passed；`--test chromix` 8 passed（含新增用例） |
| `cargo test -p mcp-server -- --test-threads=1` | **EXIT 0**，全通过 |

---

## 四、遗留项 / 未验证风险

1. **真机未验收（阻塞性风险的降级项，非本次可解）**：
   - `--window-position=-32000,-32000` 在真实 CloakBrowser / Chromix 上是否真正「不可见」且仍能正常渲染二维码，**未在真机验证**（AC3 的最终可观测结论需真机）。
   - 二维码「裁剪」未实现：`kuaishou_login_qr` 直接返回**整页截图**（design.md §3.3 的「先整页回退」路径）。功能可用（用户可自扫整页），但体验未达 design 的最优形态；页面改版不影响。
2. **`kuaishou-subject.spec.ts` 4 例仍失败，确认为预存在、与本任务无关**：
   - 失败点 `getByRole("textbox", { name: "搜索环境与账号档案" })`（zh 文案），而这些 spec 的 mock 默认 `language: "en"`（`tauriMock.ts:6`），属既有 i18n/mock 语言不匹配；node-1 基线亦未跑该 spec。建议后续单独开节点处理。
3. 未执行 `git add` / `commit`（遵守约束）。

---

## 五、可供 Phase 3.3 参考的新知识线索

- **「可选参数」的 UI 封装陷阱**：给 IPC 封装加带默认值的可选参数时，若把默认值放进 payload（`{ hidden }`），会让既有调用方在 wire 上出现新字段 —— 契约「可选」应体现在**是否发送**，而非发送 `false`。既有测试（`shop-login.spec.ts`）会以 `toEqual({id})` 捕获此漂移。
- **`Modal open=false` 不等于卸载**：项目内所有 `useEffect` 轮询若依赖组件挂载生命周期，关闭弹窗时不会触发 cleanup；凡「弹窗内定时器」必须把 `open` 纳入 effect 依赖/门禁（AC9 类需求）。
- **Chromix `launchOptions.args` 合并**：`launch_with_chromix` 需要注入参数时必须 **append**，不能 `insert` 覆盖；bridge `prepareOptions` 也依赖各层 args 数组存在。
