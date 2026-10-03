# App Chinese localization — 执行计划

## 前置门禁

- [x] 决策已裁定:范围 A、默认简体中文、自建字典、companion 例外。
- [x] PRD 已收敛(本次收敛 pass)。
- [x] `design.md` 落盘。
- [ ] 用户在最终总结之后明确批准实施。
- [ ] 校验 manifest 并在批准后运行 `task.py start`;实现前加载 `trellis-before-dev`。

跨层功能,不拆子任务;以下为同一交付内有序批次。

## 执行顺序

### 1. 建立覆盖与回归基线

- 检查 git diff,不覆盖用户修改(当前 `crates/tauri-app/Cargo.toml` 有用户改动)。
- 依据 `research/localization-audit.md` 枚举全部用户文案(正文/title/aria/placeholder/纯函数/原生弹窗/第三方控件),形成覆盖清单及原文保留例外。
- 读取 App 入口、设置加载、相关组件和测试再落代码;确认 emoji picker 本地版本/API。
- 记录既有失败与无关问题,不趁迁移扩大范围。

### 2. 设置模型与兼容

- 修改 `multizen-core/src/settings.rs` AppSettings/default、`settings-store/src/defaults.rs` RawSettings/load、`ui/src/types.ts` 及构造点/fixtures。
- language=`zh-CN`|`en`,默认 zh-CN;宽容加载但严格显式更新。保留现有 patch 契约及其他设置值。
- 加测试:缺文件、旧配置、未知值、错误类型、更新后 reopen、缓存与写失败;含有效 chromix/browser 配置不丢失断言。
- 先跑 `cargo test -p settings-store --locked` 和 `cargo check --workspace --locked`。

### 3. 翻译基础与首屏

- 新增 `ui/src/i18n/` 纯函数字典/插值层 + Provider;双字典键与参数一致性自动检查。
- 入口等待语言初始化后挂载完整 UI;失败回退中文,独立错误边界可翻译。
- 接入设置切换、保存期间状态、保存失败、document.lang;切换不卸载应用或丢表单输入。

### 4. 全量组件迁移

- 导航/设置/列表/命令面板/公共对话框与提示。
- 新建编辑/指纹/Chromix/代理/扩展/引导。
- MCP/活动/更新/错误详情/第三方控件可配置文本。
- 已硬编码中文(快手/业务账号/OCR)迁移进字典。
- Intl 时间/地区名称及辅助功能文案;固定术语和中文字体 fallback。
- 所有状态及业务枚举保留原值;活动和已显示通知避免保存不可重译的应用自有句子。

### 5. 原生提示

- 在 `commands/dialog.rs` 和 `commands/update.rs` 等实际文案点用已保存语言的静态选择。
- 不改按钮行为/更新逻辑/链接/启动参数;不在设置锁持有期间开阻塞对话框。
- 原生 OS 控件例外清单与可测试的自定义文案分别验证。

### 6. 全面回归与审阅

- Playwright fixtures 显式 language;原英文用例选 en,新增中文默认用例;禁止批量替换期望掩盖缺陷。
- 测切换/持久化、首屏语言、打开弹窗与未保存表单切换、失败回退、时间/地区格式化。
- Profile 指纹/代理/名称/备注/标签/Chromix 选项切换前后相等;IPC 枚举值和浏览器语言配置无新耦合。
- 两语言桌面和窄屏界面检查;用清单复核动态文本和 raw error 例外,不仅 grep ASCII。
- 执行 `trellis-check`;按实际结果更新 specs 并遵循 finish 流程。提交/发布不在本规划授权。

## 验证命令

仓库根:

```powershell
cargo test -p settings-store --locked
cargo check --workspace --locked
cargo test --workspace --locked
npm --prefix crates/tauri-app/ui run build
```

`crates/tauri-app/ui`:

```powershell
node --experimental-strip-types src/lib/chromixFingerprint.test.mjs
npx playwright test --config playwright.config.ts
```

新增纯翻译测试沿用现有 Node/playwright 方式,实际命令写入交付记录。无既有 lint 脚本,不虚构 lint 验证。

## 前提与边界

- 需已装项目依赖、兼容 Node、Rust/Tauri 原生工具链与 Playwright 浏览器;缺失报告阻塞,不擅自下载。
- Playwright mock Tauri IPC,不能代替 settings.json 实际落盘、原生文件 filter 名或更新提示验证。
- 原生手工验收用隔离测试数据:初次启动、升级旧设置、切换并重启、文件对话框、非破坏性更新提示。启动真实应用按 `run` 技能,禁止访问/覆盖用户真实 Profile 或执行安装更新。
- 本阶段未运行上述产品测试;最终实现报告逐项标注通过/失败/未运行。

## 风险检查与回滚点

- 设置批次:异常 language 不触发整份配置归零、AppSettings 新构造点漏改。
- 字典批次:键和占位符一致性、React 外函数可用性、无 HTML 注入。
- UI 批次:重复 settings 请求与状态竞争、语言切换时表单状态、第三方控件遗漏。
- 原生批次:只本地化文本,不改权限/执行/更新行为。
- 所有批次可逐步回退代码;不删用户设置、不改 Profile 数据库,不用破坏性 git 重置。