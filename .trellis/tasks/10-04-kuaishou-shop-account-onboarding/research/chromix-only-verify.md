# Chromix-only 全量校验（node-3）

日期：2026-10-04　工作目录：`F:\Cloaksession`　平台：Windows / PowerShell
范围：删除 CloakBrowser/CFT 引擎（node-1）+ UI 删除引擎选择与 Fingerprint 栏（node-2）+ 向导改版（node-4）
状态：**工作区未提交**（`git status` 显示 40+ 改动/删除文件），全程**未执行任何 git 写操作**。

---

## 1. 必跑命令（退出码 + 关键统计）

| # | 命令 | 退出码 | 关键统计 |
|---|------|--------|----------|
| 1 | `cargo check --workspace --all-targets` | **0** | 仅 1 条预存在 warning：`verify_initialization_identity is never used`（`crates/cdp-driver/tests/..`）。无 error |
| 2 | `cargo test -p multizen-core -p settings-store -p browser-launcher -p cdp-driver -p mcp-server -p tauri-app` | **0** | 全部 `test result: ok`，**0 failed**。tauri-app lib 224 tests（222 passed / 2 ignored）；browser-launcher 40 passed；cdp-driver 64 passed / 9 ignored；mcp-server 45 passed；settings-store 19 passed；multizen-core 7 passed |
| 3 | `cargo test -p browser-launcher` | **0** | args 2 / business_guard 5 / chromix 8 / data_dir 5 / proxy_geo 5 / socks5_bridge 3 / version 5 / version_detect 7 —— 全 ok，含 hidden/args/chromix 回归 |
| 4 | `npx tsc -b --force`（ui） | **0** | 无输出 |
| 5 | `node --test src/i18n/dictionaries.test.mjs` | **0** | `pass 6 / fail 0` |
| 6 | `npm run build`（ui） | **0** | `built in 1.52s`，1680 modules；仅 chunk >500kB 体积提示（非错误） |
| 7 | `npx playwright test --project=desktop-chrome` | **1** | **83 passed / 4 failed**，全部为 `tests/kuaishou-subject.spec.ts`（预存在，见 §3） |

改后复跑（本轮修复后）：`npx tsc -b --force` → **0**；`npx playwright test tests/chromix-settings.spec.ts` → **6 passed**。

---

## 2. 额外核对（因删引擎）

### 核对 1 —— CloakBrowser/CFT 死代码路径
`grep -rn "Cloakbrowser|CloakBrowser|Engine::Cft|browser_engine|Cft" crates --include=*.rs` → **18 命中，全部无害**：

- `multizen-core/src/settings.rs:4`、`cdp-driver/src/bootstrap.rs:6`、`cdp-driver/src/safe_cdp.rs:3-4`、`browser-launcher/tests/args.rs:59`：均为**注释**说明"former Cft/CloakBrowser 路径已移除"。
- `settings-store/src/defaults.rs:64-69` + `tests/store.rs:63`：**容错迁移**——历史值 `"cloakbrowser"`/`"cft"` 或未知字符串一律归一为 `BrowserEngine::Chromix`（枚举现为单变体）。
- `settings.rs:64/81`、`tauri-app/src/lib.rs:205`、`tests/chromix.rs`：`browser_engine` 字段/默认值本身（保留以稳定 `settings.json` 字段形状）。
- **结论：Rust 源码中已无 `Cft`/`Cloakbrowser` 变体分支**（`BrowserEngine` 仅剩 `Chromix`，`match` 单臂）。
- `CLOAKBROWSER_*` 环境变量名仍存在于 Chromix SDK（`resources/chromix/vendor/chromix/index.js`、`bridge.mjs`）——这是 SDK 兼容别名，**允许保留**，非死代码。

### 核对 2 —— `--fingerprint-*` 注入已彻底移除
`grep -rn "--fingerprint" crates --include=*.rs` → 命中仅在 `settings-store/tests/chromix.rs`（测试里把 `--fingerprint` 当**不透明用户 args 透传**验证）与文档注释。
进一步限定 `crates/*/src/**.rs` 搜 `"--fingerprint` / `"--user-agent` / `"--test-type` → **0 命中**。
**结论：Rust 生产代码不再构建任何 `--fingerprint-*`/`--user-agent`/`--test-type` 参数**；`build_spawn_args` 现仅产出 `--user-data-dir` + `--remote-debugging-address/port`（`args.rs`）。

### 核对 3 —— hidden 在 Chromix 路径仍生效
`crates/browser-launcher/src/driver.rs:53-93` `launch_with_chromix(..., hidden: bool)`：`hidden=true` 时把 `--window-position=-32000,-32000` **append** 进 `launchOptions.args`（保留 profile 自身 args，不覆盖），构造 `hidden_config` 后走 SDK。
测试覆盖：`tests/chromix.rs:392` `hidden_launch_merges_window_position_into_existing_launch_args`（8 passed 之一）；`tests/args.rs:90` `chromix_args_ignore_hidden_and_fingerprint_inputs` 锁定原生 args 不受 hidden/fingerprint 影响。
**结论：hidden 仍只靠 `--window-position` 离屏（不用 `--headless`），行为完好。**

### 核对 4 —— UI 不再出现 CloakBrowser 指纹栏/引擎选择器
- `Settings.tsx`：引擎选择器（`engineOptions` + `aria-pressed` 按钮 + `settings.engine.*` 文案）**已删除**；改为直接渲染 `ChromixSettingsEditor`，二进制行固定用 `settings.binary.descChromix`。`settings.engine.*` / `settings.binary.descDefault` i18n 键在 en/zh-CN 均已删除。
- 全仓 `browserEngine` 仅剩：`src/types.ts`（类型定义 `"cft"|"cloakbrowser"|"chromix"` + `AppSettings` 字段，保留字段形状）+ `tests/tauriMock.ts`（fixture 存 legacy 值以证明 UI 引擎无关）+ `tests/chromix-settings.spec.ts:190` 用例断言"UI 不再 patch `browserEngine`"。**无任何 UI 控件读写 `browserEngine`。**
- `profile/FingerprintForm.tsx` **已删除**（文件不存在）；`FingerprintPanel` / `wizard-fingerprint` testid 全仓 **0 命中**。
- 向导 `KuaishouAccountWizard.tsx`：4 分区（主页/代理/扩展/指纹），指纹分区用 `ChromixProfileOptions`（Chromix 语义），创建时**不传 `fingerprint`**。

---

## 3. 预存在失败清单（与本轮改动无关）

`tests/kuaishou-subject.spec.ts` 4 个失败（77 / 100 / 123 / 147 行）：
- 现象：`getByRole("textbox", { name: "搜索环境与账号档案" })` 超时（中文 aria），而 mock 默认 `language: "en"`（`tauriMock.ts:defaultSettings`）。
- 证据（**预存在**）：`git diff HEAD -- tests/kuaishou-subject.spec.ts tests/kuaishouIdentityMock.ts tests/businessAccountsMock.ts` → **空**（这三个文件本轮未改动）；该 spec 由更早提交 `87874c0` 引入，且 `tauriMock` 在 HEAD 已是 `language: "en"`。属 zh aria vs en mock 的既有错配，**非本轮引入**。
- 复跑 `tests/chromix-settings.spec.ts`、`tests/kuaishou-shop-wizard.spec.ts`、`tests/kuaishou-shop.spec.ts` → 全绿（10 passed / 6 passed），确认本轮相关用例无回归。

---

## 4. 发现并修复的小问题

**1 处已修复**：`crates/tauri-app/ui/src/components/screens/ChromixSettingsEditor.tsx:131-132`
- 问题：渲染文案硬编码残留 `"...Switching engines does not erase this configuration."`（引擎已唯一，文案过时）；同轮 i18n 键 `chromix.editor.intro` 已删掉该句，导致"键已改、界面仍显示旧句"的不一致。
- 修复：删除 `Switching engines does not erase this configuration.` 一句。
- 验证：`npx tsc -b --force` → 0；`chromix-settings.spec.ts` → 6 passed（无测试断言该句，无副作用）。

**未改动（如实记录，非小问题）**：`ChromixSettingsEditor.tsx` 与 `ChromixFingerprintForm.tsx` 内仍有大量硬编码英文（node hint、`Global fingerprint parameters`、raw-args 等），且已有对应 i18n 键（`chromix.editor.*`）却未接线到 `useT()`。属 node-2/node-4 已注明的既有技术债，涉及大范围重构，**不擅自大改**，留待判断。

---

## 5. 遗留风险

1. `kuaishou-subject.spec.ts` 4 个预存在失败未处理（另一条线：spec 用中文 aria、mock 默认 en）。
2. `types.ts` 的 `BrowserEngine` 仍保留 `"cft"|"cloakbrowser"` 字面量（Rust 侧枚举已单变体）；仅类型/字段形状冗余，无运行期影响。
3. `ChromixSettingsEditor` / `ChromixFingerprintForm` 硬编码英文（见 §4），i18n 键闲置。
4. 工作区全部改动**未提交**（本轮遵守约束不执行 git 写操作），提交由后续节点决定。
