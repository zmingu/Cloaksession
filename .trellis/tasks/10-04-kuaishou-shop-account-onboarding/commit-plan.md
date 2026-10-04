# Phase 3.4 — 提交计划（起草，**未执行**）

> 节点：④ Phase 3.3 spec 更新 + Phase 3.4 起草提交计划
> 分支：main｜工作目录：F:\Cloaksession｜日期：2026-10-04
> **本文件只是计划**：本节点未执行任何 `git add / commit / push / checkout / reset`。

---

## 0. 提交风格（学习自 `git log --oneline -8`）

最近提交样本：

```
d6d2ff8 chore(task): archive 10-04-jieger-frontend-catchup
a019ece chore(task): archive 10-04-frontend-shop-popup
978e2da fix(task): 启动panic修复（setup钩子改用async_runtime::spawn）+ 前端补齐6任务规划产物
1b746f7 feat(ui): 合并B组监听与小号前端（business壳comments/sub两tab，15组tab齐全）
68d740a feat(ui): D组挂入business壳（pscript/helper/popup三tab+导航统一+GoodsKnowledge改名隔离）
```

约定（本计划沿用）：
- 前缀：`feat` / `fix` / `chore` / `docs`；作用域用括号，取**层/域**（`ui` / `launcher` / `kuaishou` / `task` / `spec`）。
- 主题行中英混排，中文为主；长度偏长（可到 ~60 字），信息密度高。
- 任务产物用 `chore(task): ...`；本任务尚未归档，故**先不写 archive**。

---

## 1. 脏文件全量清单（`git status --porcelain`，2026-10-04 实测）

### 1.1 已跟踪 · 修改（M）

| # | 文件 | 归属 |
|---|---|---|
| 1 | `.trellis/spec/browser-launcher/backend/chromix.md` | 本节点 spec |
| 2 | `.trellis/spec/browser-launcher/backend/lifecycle.md` | 本节点 spec |
| 3 | `.trellis/spec/guides/index.md` | 本节点 spec |
| 4 | `.trellis/spec/mcp-server/backend/tools.md` | 本节点 spec |
| 5 | `.trellis/spec/profile-manager/backend/business-accounts.md` | 本节点 spec |
| 6 | `.trellis/spec/tauri-app/backend/index.md` | 本节点 spec |
| 7 | `.trellis/spec/tauri-app/backend/ipc.md` | 本节点 spec |
| 8 | `crates/browser-launcher/src/args.rs` | 本任务 |
| 9 | `crates/browser-launcher/src/driver.rs` | 本任务 |
| 10 | `crates/browser-launcher/tests/args.rs` | 本任务 |
| 11 | `crates/browser-launcher/tests/chromix.rs` | 本任务 |
| 12 | `crates/browser-launcher/tests/driver.rs` | 本任务 |
| 13 | `crates/mcp-server/src/driver.rs` | 本任务 |
| 14 | `crates/mcp-server/src/tools.rs` | 本任务 |
| 15 | `crates/mcp-server/tests/mock_driver.rs` | 本任务 |
| 16 | `crates/mcp-server/tests/tools.rs` | 本任务 |
| 17 | `crates/tauri-app/Cargo.toml` | ⚠️ 伪改动（见 §4.3） |
| 18 | `crates/tauri-app/src/commands/companion.rs` | 本任务 |
| 19 | `crates/tauri-app/src/commands/kuaishou_auth.rs` | 本任务 |
| 20 | `crates/tauri-app/src/commands/profiles.rs` | 本任务 |
| 21 | `crates/tauri-app/src/driver.rs` | 本任务 |
| 22 | `crates/tauri-app/src/driver/business_tests.rs` | 本任务 |
| 23 | `crates/tauri-app/src/driver/identity/tests.rs` | 本任务 |
| 24 | `crates/tauri-app/src/lib.rs` | 本任务 |
| 25 | `crates/tauri-app/src/mcp_embed.rs` | 本任务 |
| 26 | `crates/tauri-app/tauri.conf.json` | 品牌改名 |
| 27 | `crates/tauri-app/ui/index.html` | 品牌改名 |
| 28 | `crates/tauri-app/ui/package.json` | 品牌改名 |
| 29 | `crates/tauri-app/ui/src/App.tsx` | 本任务（含品牌注释） |
| 30 | `crates/tauri-app/ui/src/components/UpdateBanner.tsx` | 品牌改名 |
| 31 | `crates/tauri-app/ui/src/components/atoms/Button.tsx` | 品牌改名 |
| 32 | `crates/tauri-app/ui/src/components/atoms/Cube.tsx` | 品牌改名 |
| 33 | `crates/tauri-app/ui/src/components/business/BusinessSection.tsx` | 本任务 |
| 34 | `crates/tauri-app/ui/src/components/mcp/McpPanel.tsx` | 品牌改名 |
| 35 | `crates/tauri-app/ui/src/components/onboarding/ChromiumBootstrapModal.tsx` | 品牌改名 |
| 36 | `crates/tauri-app/ui/src/components/onboarding/FirstRun.tsx` | 品牌改名 |
| 37 | `crates/tauri-app/ui/src/components/palette/CommandPalette.tsx` | 本任务 |
| 38 | `crates/tauri-app/ui/src/components/profile/ExtensionsSection.tsx` | 品牌改名 |
| 39 | `crates/tauri-app/ui/src/components/profile/KuaishouIdentity.tsx` | 本任务 |
| 40 | `crates/tauri-app/ui/src/components/screens/ChromixSettingsEditor.tsx` | 品牌改名 |
| 41 | `crates/tauri-app/ui/src/components/screens/LeftRail.tsx` | 本任务 |
| 42 | `crates/tauri-app/ui/src/components/screens/Sidebar.tsx` | 本任务 |
| 43 | `crates/tauri-app/ui/src/i18n/en.ts` | 本任务（含品牌串） |
| 44 | `crates/tauri-app/ui/src/i18n/zh-CN.ts` | 本任务（含品牌串） |
| 45 | `crates/tauri-app/ui/src/lib/ipc.ts` | 本任务（含品牌串） |
| 46 | `crates/tauri-app/ui/src/lib/kuaishouAuth.ts` | 本任务 |
| 47 | `crates/tauri-app/ui/src/types.ts` | 品牌改名（注释） |
| 48 | `crates/tauri-app/ui/tests/businessAccountsMock.ts` | 本任务（选择器） |
| 49 | `crates/tauri-app/ui/tests/chromix-settings.spec.ts` | 本任务（选择器） |
| 50 | `crates/tauri-app/ui/tests/listenSubMock.ts` | 本任务（选择器） |
| 51 | `crates/tauri-app/ui/tests/wave4-verify.spec.ts` | 本任务（选择器） |

### 1.2 已跟踪 · 删除（D）

| 文件 | 归属 |
|---|---|
| `crates/tauri-app/ui/src/components/screens/TopBar.tsx` | 本任务（导航重构删除顶栏） |

### 1.3 未跟踪（??）

| 路径 | 归属 |
|---|---|
| `.snow/`（logs/、scan/，6 文件） | ⚠️ 工具运行目录（见 §4.1） |
| `crates/tauri-app/ui/.snow/`（logs/，1 文件） | ⚠️ 工具运行目录（见 §4.1） |
| `.trellis/tasks/10-04-kuaishou-shop-account-onboarding/`（12 文件） | 本任务产物 |
| `crates/tauri-app/ui/src/components/kuaishou/`（3 文件） | 本任务（新增） |
| `crates/tauri-app/ui/src/components/table/`（1 文件 DataTable.tsx） | 本任务（新增） |
| `crates/tauri-app/ui/tests/kuaishou-shop.spec.ts` | 本任务（新增） |
| `crates/tauri-app/ui/tests/kuaishou-shop-wizard.spec.ts` | 本任务（新增） |

---

## 2. 分类：本任务 vs 未识别

| 类别 | 内容 | 是否本任务 |
|---|---|---|
| **A. 快手小店建号（本任务）** | `hidden` 离屏启动链路（browser-launcher / mcp-server / tauri-app driver + commands + IPC）、`kuaishou_login_qr`、快手侧栏二级菜单、小店账号列表、建号向导、i18n `kuaishou.*`、新增组件与测试、导航重构后的 Playwright 选择器迁移 | ✅ 是 |
| **B. 品牌更名 Cloaksession → JiegeGo** | 产品名/窗口标题/包名、UI 文案与注释里的品牌串、i18n 品牌串 | ⚠️ 非本任务目标，但**同一工作区并行完成**（见 §4.2） |
| **C. spec 沉淀（本节点）** | `.trellis/spec/**` 7 个文件 | ✅ 是（本节点产出） |
| **D. 任务产物** | `.trellis/tasks/10-04-.../`（prd/design/implement/check/research 等） | ✅ 是 |
| **E. 工具运行目录（未识别）** | `.snow/`、`crates/tauri-app/ui/.snow/` | ❌ 否（见 §4.1） |
| **F. 伪改动（未识别）** | `crates/tauri-app/Cargo.toml`（`git status` 报 M，`git diff` 无内容差） | ❌ 否（见 §4.3） |

---

## 3. 建议的提交分组（一个连贯改动单元 = 一个提交）

> 顺序建议：C1 → C2 → C3 → C4 → C5。
> C2 是 C3 的**前置**（向导调用 `profiles_launch { hidden }` 与 `kuaishou_login_qr`，UI 包装在 `ipc.ts`/`kuaishouAuth.ts`），故 **C2 必须先于 C3**，否则 C3 提交点无法编译/自洽。

### C1 — 品牌更名

```
chore(ui): 品牌更名 Cloaksession → JiegeGo（产品名/窗口标题/包名/文案）
```

文件（13）：

```
crates/tauri-app/tauri.conf.json
crates/tauri-app/ui/index.html
crates/tauri-app/ui/package.json
crates/tauri-app/ui/src/components/UpdateBanner.tsx
crates/tauri-app/ui/src/components/atoms/Button.tsx
crates/tauri-app/ui/src/components/atoms/Cube.tsx
crates/tauri-app/ui/src/components/mcp/McpPanel.tsx
crates/tauri-app/ui/src/components/onboarding/ChromiumBootstrapModal.tsx
crates/tauri-app/ui/src/components/onboarding/FirstRun.tsx
crates/tauri-app/ui/src/components/profile/ExtensionsSection.tsx
crates/tauri-app/ui/src/components/screens/ChromixSettingsEditor.tsx
crates/tauri-app/ui/src/components/screens/TopBar.tsx   # 删除
crates/tauri-app/ui/src/types.ts                        # 仅注释品牌串
```

> ⚠️ **混合文件**：`en.ts` / `zh-CN.ts` / `ipc.ts` / `App.tsx` 里**同时**含品牌串与本任务功能改动，本计划把它们归入功能提交（C2/C3），品牌串随行；若要求严格分离，用 `git add -p` 按 hunk 拆分。

### C2 — hidden 离屏启动 + 二维码截图命令（机制层）

```
feat(launcher): hidden 离屏启动（仅 --window-position，不用 --headless）+ kuaishou_login_qr 二维码截图命令
```

文件（19）：

```
crates/browser-launcher/src/args.rs
crates/browser-launcher/src/driver.rs
crates/browser-launcher/tests/args.rs
crates/browser-launcher/tests/chromix.rs
crates/browser-launcher/tests/driver.rs
crates/mcp-server/src/driver.rs
crates/mcp-server/src/tools.rs
crates/mcp-server/tests/mock_driver.rs
crates/mcp-server/tests/tools.rs
crates/tauri-app/src/driver.rs
crates/tauri-app/src/driver/business_tests.rs
crates/tauri-app/src/driver/identity/tests.rs
crates/tauri-app/src/lib.rs
crates/tauri-app/src/mcp_embed.rs
crates/tauri-app/src/commands/companion.rs
crates/tauri-app/src/commands/profiles.rs
crates/tauri-app/src/commands/kuaishou_auth.rs
crates/tauri-app/ui/src/lib/ipc.ts
crates/tauri-app/ui/src/lib/kuaishouAuth.ts
```

要点：`BrowserDriver::launch` 新增 `hidden: bool`（MCP/嵌入路径传 `false` 保持行为不变）；`build_spawn_args` 只追加 `--window-position`；Chromix 走 `launchOptions.args` **append**；`kuaishou_login_qr -> Option<String>`；`ipc.ts` 的 `hidden` 缺省**不发送**。

### C3 — 快手板块 UI + 建号向导（功能层）

```
feat(ui): 快手板块（侧栏二级菜单 + 小店账号列表）与小店建号向导
```

文件（18）：

```
crates/tauri-app/ui/src/App.tsx
crates/tauri-app/ui/src/components/screens/Sidebar.tsx
crates/tauri-app/ui/src/components/screens/LeftRail.tsx
crates/tauri-app/ui/src/components/palette/CommandPalette.tsx
crates/tauri-app/ui/src/components/business/BusinessSection.tsx
crates/tauri-app/ui/src/components/profile/KuaishouIdentity.tsx
crates/tauri-app/ui/src/components/kuaishou/KuaishouAccountsPage.tsx   # 新增
crates/tauri-app/ui/src/components/kuaishou/KuaishouShopAccounts.tsx   # 新增
crates/tauri-app/ui/src/components/kuaishou/KuaishouAccountWizard.tsx  # 新增
crates/tauri-app/ui/src/components/table/DataTable.tsx                 # 新增
crates/tauri-app/ui/src/i18n/en.ts
crates/tauri-app/ui/src/i18n/zh-CN.ts
crates/tauri-app/ui/tests/kuaishou-shop.spec.ts                        # 新增
crates/tauri-app/ui/tests/kuaishou-shop-wizard.spec.ts                 # 新增
crates/tauri-app/ui/tests/businessAccountsMock.ts
crates/tauri-app/ui/tests/listenSubMock.ts
crates/tauri-app/ui/tests/chromix-settings.spec.ts
crates/tauri-app/ui/tests/wave4-verify.spec.ts
```

要点：`TopBar` 删除后的导航重构（侧栏二级菜单 + 去掉快捷键后缀）；4 个既有 spec 的选择器从 `getByTitle("Profiles · ⌘1")` 迁移到 `getByRole("button", { name: "Profiles", exact: true })`，属同一连贯改动。

### C4 — spec 沉淀（本节点产出）

```
docs(spec): 沉淀 hidden 离屏启动、IPC 可选参数、弹窗定时器、账号即环境边界
```

文件（7）：

```
.trellis/spec/browser-launcher/backend/lifecycle.md
.trellis/spec/browser-launcher/backend/chromix.md
.trellis/spec/tauri-app/backend/ipc.md
.trellis/spec/tauri-app/backend/index.md
.trellis/spec/profile-manager/backend/business-accounts.md
.trellis/spec/mcp-server/backend/tools.md
.trellis/spec/guides/index.md
```

### C5 — 任务产物

```
chore(task): 快手小店建号向导任务产物（PRD/设计/实现/验收/复核记录）
```

文件（`.trellis/tasks/10-04-kuaishou-shop-account-onboarding/` 全量 12）：

```
task.json  prd.md  design.md  implement.md  implement.jsonl
check.jsonl  check-record.md  commit-plan.md
research/baseline-validation.md  research/evidence.md  research/manual-acceptance.md
```

> 注：仓库约定任务完成后单独 `chore(task): archive ...`（见历史提交）。本任务 `status=in_progress`，**归档提交留待 Phase 4**，此处只提交产物。

---

## 4. 未识别文件与纳入建议（**需用户确认**）

### 4.1 `.snow/`、`crates/tauri-app/ui/.snow/` —— 建议**不纳入**，并补 `.gitignore`

- 内容：`.snow/logs/*.log`、`.snow/scan/*.txt`、`crates/tauri-app/ui/.snow/logs/*.log` —— 是 Snow App 的**工具运行目录**（终端日志/扫描输出），与产品代码无关，且内容随运行变化。
- 现状：`git check-ignore` 对二者均返回未忽略；`git ls-files` 显示历史上**从未跟踪**任何 `.snow/` 文件。
- **建议**：在根 `.gitignore` 增加一行 `.snow/`（无斜杠前缀 → 匹配任意深度的 `.snow/` 目录，可同时覆盖两处）。
- ⚠️ **本节点未改 `.gitignore`**，请用户定夺后由 node-5 或用户执行。
- 建议命令（仅示例，未执行）：在 `.gitignore` 末尾追加 `.snow/`，随后 `.snow/` 会自动从 `git status` 消失，无需 `git rm --cached`（因从未跟踪）。

### 4.2 品牌更名 Cloaksession → JiegeGo —— 需用户确认是否本批次提交

- 这是与本任务**并行**出现的另一条改动线（产品更名），证据：`tauri.conf.json` productName/title、`index.html` `<title>`、`package.json` name、十余处 UI 文案/注释、i18n 品牌串。
- 它**不属于**「小店建号向导」需求本身；但同一工作区里已与功能改动交织（4 个混合文件）。
- **建议**：作为**独立提交 C1** 提交（已在上方分组）。若用户希望改名走单独分支/单独评审，请先告知，本计划可把 C1 拆出。
- 待用户确认项：`identifier` 仍为 `com.cloaksession.browser`（未改）——是否有意保留？本计划**不动**它。

### 4.3 `crates/tauri-app/Cargo.toml` —— 伪改动，建议提交时排除

- `git status --porcelain` 报 ` M`，但 `git diff` / `git diff --numstat` / `git diff --ignore-all-space` **均为空**（无内容差）；`git diff` 仅打印 “LF will be replaced by CRLF” 警告。
- 结论：**行尾（CRLF/LF）stat 缓存差异**，非真实改动。
- **建议**：不要把它写进任何提交消息的文件清单；`git add` 该文件不会产生内容变更（可安全忽略）。若想让 `git status` 变干净，可（**需用户确认**）`git add --renormalize crates/tauri-app/Cargo.toml` 或 `git checkout -- crates/tauri-app/Cargo.toml`。**本节点未执行**。

---

## 5. 提交前应跑的校验（供 node-5 复核）

前端（cwd = `crates/tauri-app/ui`）：

```powershell
npx tsc -b --force
node --test src/i18n/dictionaries.test.mjs
npm run build
npx playwright test kuaishou-shop.spec.ts kuaishou-shop-wizard.spec.ts shop-login.spec.ts --project=desktop-chrome
```

Rust（cwd = `F:\Cloaksession`）：

```powershell
cargo check --workspace --all-targets
cargo test -p tauri-app
cargo test -p browser-launcher
cargo test -p mcp-server -- --test-threads=1
```

（以上为 Phase 2.2 已通过的同一组命令；node-5 提交前至少重跑与本次分组直接相关的用例。）

---

## 6. 明确不做 / 风险提示

- 本节点**未执行** `git add / commit / push / checkout / reset`。
- 本节点**未改** `.gitignore`（§4.1 仅建议）。
- 本节点**未改**产品代码（仅 `.trellis/spec/**` 与本任务目录 md）。
- 未弱化任何已通过验收：spec 更新为**追加**，未删除既有契约；提交计划不改变任何代码。
- 遗留（沿用 Phase 2.2 结论）：`--window-position` 真机不可见性未验收；二维码裁剪未实现（返回整页，设计文档的回退路径）；`kuaishou-subject.spec.ts` 4 例预存在失败与本任务无关。
