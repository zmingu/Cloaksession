# Implement — 小店账号建号向导

> 依赖顺序：Rust 侧（隐藏启动 + 二维码命令）→ IPC 封装 → 向导 UI → 接入入口 → 测试。

## 步骤清单（有序）

### A. Rust：隐藏启动
- [ ] A1 `crates/tauri-app/src/commands/profiles.rs`：`profiles_launch` 增加可选 `hidden: bool`（`#[serde(default)]`）。
- [ ] A2 `crates/tauri-app/src/driver/`：把 `hidden` 传到启动参数；按 `design.md §3.2` 优先用窗口位置方案。
- [ ] A3 单测：`hidden=false` 与缺省行为一致；`hidden=true` 时启动参数含隐藏项（可用现有 fixture 断言启动参数）。

### B. Rust：二维码截图
- [ ] B1 新增 `kuaishou_login_qr(profile_id) -> Option<String>`（`commands/kuaishou_auth.rs` 或新文件）。
- [ ] B2 driver：`screenshot(profile_id)` 后裁剪二维码区域；区域常量集中一处；裁剪失败回退整图。
- [ ] B3 在 `lib.rs` 注册命令；单测覆盖「未运行 → None」「有图 → 返回 base64」。

### C. 前端 IPC 封装
- [ ] C1 `ui/src/lib/ipc.ts`：`profiles.launch` 支持 `hidden`；新增 `kuaishouLoginQr(profileId)`。
- [ ] C2 类型：`ui/src/types.ts` 补必要类型（如需要）。

### D. 向导 UI
- [ ] D1 新增 `ui/src/components/kuaishou/KuaishouAccountWizard.tsx`：
  - step `form`：名称（必填）+ 代理（复用 `ProxyTester` / `parseProxyString`）。
  - step `creating`：创建 + 隐藏启动。
  - step `waiting`：二维码 `<img>` + 轮询（1.5–2s）截图 + 轮询身份检测。
  - step `done`：昵称/头像/快手ID + 重复警告 + 「完成」。
  - 取消语义：未创建＝直接关；创建后＝提示「账号已创建」并关隐藏窗口。
- [ ] D2 指纹：创建前 `fingerprint_generate()` → 有代理时 `fingerprint_locale_for_country(country)` + `fingerprint_reconcile()`。
- [ ] D3 用 `Modal`（`components/atoms`）承载；样式对齐现有设计系统（Tailwind + 自研组件）。
- [ ] D4 轮询在 `done` / 卸载时清理（AC9）。

### E. 接入
- [ ] E1 `App.tsx`：`onAddAccount` 由 `setShowSheet(true)` 改为打开向导（新增 `wizardOpen` 状态）。
- [ ] E2 `KuaishouShopAccounts` 不变（仍传 `onAddAccount`）。

### F. i18n
- [ ] F1 `ui/src/i18n/en.ts` 与 `zh-CN.ts` **同时**新增 `kuaishou.wizard.*`（键集/占位符一致）。

### G. 测试与校验
- [ ] G1 Playwright：`tests/kuaishou-shop-wizard.spec.ts`（mock IPC）覆盖 AC1/AC2/AC3/AC4/AC5/AC8。
- [ ] G2 Rust 单测：A3 / B3。
- [ ] G3 校验命令（见下）。

## 校验命令

```powershell
# 前端（在 F:\Cloaksession\crates\tauri-app\ui）
npx tsc -b --force                     # 期望 EXIT 0
node --test src/i18n/dictionaries.test.mjs   # 期望 6 pass
npm run build                          # 期望成功
npx playwright test tests/kuaishou-shop-wizard.spec.ts --project=desktop-chrome

# Rust（在 F:\Cloaksession）
cargo test -p tauri-app                # 期望通过
cargo clippy -p tauri-app -- -D warnings
```

## 风险文件 / 回滚点

- `crates/tauri-app/src/commands/profiles.rs`（launch 参数）—— 影响面最广，务必保持缺省行为不变。
- `crates/tauri-app/src/driver.rs`（启动参数注入）—— 回归「可见启动」。
- `ui/src/App.tsx`（`onAddAccount`）—— 一键回滚到 `setShowSheet(true)`。
- 二维码裁剪区域常量 —— 页面改版即失效，集中一处便于修。

## 启动前复核

- [ ] `research/evidence.md` 的 file:line 仍有效（若代码已变动，先更新）。
- [ ] 确认隐藏启动方案（A2）在真实 `chromix` 上可行；不可行则按 `design.md §3.2` 降级。
- [ ] 确认无新增 DB 迁移、未写 `business_accounts`。
