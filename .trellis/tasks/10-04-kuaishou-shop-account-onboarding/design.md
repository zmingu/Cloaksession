# Design — 小店账号建号向导

## 1. 边界（Boundaries）

| 层 | 职责 | 不做什么 |
|---|---|---|
| UI（`components/kuaishou/KuaishouAccountWizard.tsx`） | 向导状态机、表单、二维码展示、身份展示、警告、完成/取消 | 不直接发 CDP、不读文件 |
| UI 编排（`lib/kuaishouShopWizard.ts`） | 组合 IPC：创建环境 → 隐藏启动 → 截图轮询 → 身份读取 → 收尾 | 不含业务规则 |
| IPC（`crates/tauri-app/src/commands/*`） | 新增「隐藏启动」与「二维码截图」命令，转发 driver | 不写 DB |
| driver（`crates/tauri-app/src/driver/`） | 启动时隐藏窗口；对当前页面截图；登录等待 | 不决定 UI 文案 |
| cdp-driver（`crates/cdp-driver/`） | 复用 `screenshot`、`login`/`ensure_auth`、`AuthPhase` | 不改 headless 语义 |

## 2. 数据流（Data Flow）

```
用户点「添加账号」
  → Wizard step=form (name, proxy)
  → 提交: fingerprint_generate()  → fingerprint_reconcile(fp, {country/timezone/locale from proxy})   [有代理时]
  → profiles_create({ name, proxy, fingerprint, startUrl: 小店登录页 })
  → profiles_launch({ id, entry: "kuaishou-shop", hidden: true })     ← 新增 hidden 参数
  → step=waiting
  → loop: screenshot(id) → 裁二维码 → <img> 刷新；  kuaishou_identity_detect(id) → 有 platformUserId?
  → 读到身份 → 查重（对比其他环境 platformUserId）→ step=done（含警告如有）
  → 用户点「完成」→ 关向导；关闭隐藏窗口（profiles_close）
```

## 3. 契约（Contracts）

### 3.1 新增/扩展 IPC

- **扩展** `profiles_launch`（`crates/tauri-app/src/commands/profiles.rs`）：新增可选 `hidden: bool`（默认 false）。
  - 语义：以有头方式启动，但把窗口置于屏幕外 / 最小化；不改 startUrl、不改指纹。
  - 兼容：缺省 false = 现有行为，全部既有调用方不受影响。
- **新增** `kuaishou_login_qr(profile_id) -> Option<String>`（base64 PNG，或 `None` 表示未就绪）
  - 实现：driver `screenshot(profile_id)` 后，在 Rust 侧按固定区域裁剪二维码（区域常量集中在 `driver/` 一处，便于随页面改版调整）；裁剪失败返回整图。
- **复用** `profiles_create` / `profiles_close` / `kuaishou_identity_detect` / `kuaishou_identity_list` / `fingerprint_generate` / `fingerprint_reconcile` / `fingerprint_locale_for_country`。

### 3.2 隐藏启动的实现选择（风险 R-a）

优先级：
1. **窗口位置**：`chromix`/launch 参数把窗口放到屏幕外（如 `--window-position=-32000,-32000`），最小化风险最低、最接近「有头」。
2. 若不支持：`startMaximized=false` + 启动后最小化。
3. 兜底：`--headless=new`（已有先例 `account_init/chromix_entry_fixture.rs:121`），但需在实现中实测风控影响。

**不可见性以 AC3 的可观测结果为准**（向导内可见二维码、桌面无可见浏览器窗口）。

### 3.3 二维码裁剪

- 登录页 `login.kwaixiaodian.com/...` 的二维码位于页面中央；裁剪区域用相对比例（如居中正方形，宽 30%–70%、高 20%–70%）先做，实测再收紧。
- 若裁剪结果为空/纯色，回退整页截图。
- 前端 `<img src={`data:image/png;base64,${qr}`}>`，轮询间隔建议 1.5–2s（可调常量）。

## 4. 兼容与迁移

- `profiles_launch` 新增参数为**可选**，序列化缺省 false；Rust 端 `#[serde(default)]`，旧调用不受影响。
- 不新增数据库表 / 迁移；不写 `business_accounts`。
- i18n：新增 `kuaishou.wizard.*` 键，en / zh-CN **同时**新增且键集一致（`dictionaries.test.mjs` 强校验）。

## 5. 权衡（Trade-offs）

- **不引入账号实体**（D1）：最省事、与「账号即环境」一致；代价是删除环境即账号消失（用户已确认接受）。
- **隐藏窗口而非 headless**（D5）：复用已验证登录路径、风控风险低；代价是需新增「隐藏启动」参数与二维码截图轮询。
- **提示不阻断重复**（D6）：尊重运营多开意图；代价是可能长期存在重复账号，靠警告而非约束。

## 6. 运维 / 回滚

- 回滚点：向导入口（`App.tsx` 的 `onAddAccount`）可一键切回 `setShowSheet(true)`；新增命令与 driver 改动独立、可单独回退。
- 隐藏窗口若异常（窗口仍在屏幕内），退化为「可见有头」，功能不阻塞（不破坏 AC 之外的可用性）。
