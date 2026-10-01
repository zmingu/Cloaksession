# 中文化执行计划

## 前置门禁

- [x] 用户选择 A（完整应用界面/保留英文）和 1（默认简体中文）。
- [x] PRD 已收敛，研究和技术设计落盘。
- [ ] 用户在最终总结之后明确批准实施。
- [ ] 校验 manifest 并在批准后运行 task.py start；实现前加载 trellis-before-dev。

这是一个跨层功能，不拆成可单独验收的“中文菜单”和“中文表单”子任务；以下为同一交付内的有序批次。

## 执行顺序

### 1. 建立覆盖与回归基线

- 检查当前 git diff，不覆盖用户修改。
- 依据 research/localization-audit.md 枚举全部用户文案（正文、title、aria、placeholder、纯函数、原生弹窗、第三方控件），形成覆盖清单及原文保留例外。
- 读取现有 App 入口、设置加载、相关组件和测试再落代码；确认 emoji picker 的本地版本/API。
- 记录既有失败与无关问题，不趁迁移扩大范围。

### 2. 设置模型与兼容

- 修改 multizen-core AppSettings/default、settings-store RawSettings/load、TS types 及构造点/fixtures。
- language=zh-CN|en，默认 zh-CN；宽容加载但严格显式更新。保留现有 patch 契约及其他设置值。
- 加入缺文件、旧配置、未知值、错误类型、更新后 reopen、缓存与写失败测试；包含有效 chromix/browser 配置不丢失断言。
- 先运行 settings-store 定向测试和 Rust 检查。

### 3. 翻译基础与首屏

- 新增纯函数字典/插值层与 Provider；双字典键与参数一致性自动检查。
- 入口等待语言初始化后挂载完整 UI；失败回退中文，独立错误边界可翻译。
- 接入设置切换、保存期间状态、保存失败、document.lang，切换不卸载应用或丢失表单输入。

### 4. 全量组件迁移

- 导航/设置/列表/命令面板/公共对话框与提示。
- 新建编辑/指纹/Chromix/代理/扩展/引导。
- MCP/活动/更新/错误详情/第三方控件可配置文本。
- Intl 时间/地区名称及辅助功能文案；固定术语和中文字体 fallback。
- 所有状态及业务枚举保留原值；活动和已显示通知避免保存不可重译的应用自有句子。

### 5. 原生提示

- 在 commands/dialog.rs 和 commands/update.rs 等实际文案点使用已保存语言的静态选择。
- 不改变按钮行为、更新逻辑、链接或启动参数；不在设置锁持有期间打开阻塞对话框。
- 原生 OS 控件例外清单与可测试的自定义文案分别验证。

### 6. 全面回归与审阅

- Playwright fixtures 显式 language；原英文用例明确选择 en，新增中文默认用例，禁止仅批量替换期望掩盖缺陷。
- 测试切换/持久化、首屏语言、打开弹窗与未保存表单切换、失败回退、时间/地区格式化。
- Profile 指纹/代理/名称/备注/标签/Chromix 选项切换前后相等；IPC 枚举值和浏览器语言配置无新耦合。
- 两语言桌面和窄屏界面检查；用清单复核动态文本和 raw error 例外，不仅 grep ASCII。
- 执行 trellis-check；根据实际结果更新 specs 并遵循 finish 流程。提交和发布不在当前规划授权中。

## 验证命令与条件

从仓库根目录运行：

```powershell
cargo test -p settings-store --locked
cargo check --workspace --locked
cargo test --workspace --locked
npm --prefix crates/tauri-app/ui run build
```

从 `crates/tauri-app/ui` 运行：

```powershell
node --experimental-strip-types src/lib/chromixFingerprint.test.mjs
npx playwright test --config playwright.config.ts
```

新增纯翻译测试沿用项目现有 Node/Playwright 测试方式，并将实际命令写入交付记录。没有既有 lint 脚本，不虚构 lint 验证。

- 前提：已安装项目依赖、兼容 Node、Rust/Tauri 原生工具链与 Playwright 浏览器；缺失时报告阻塞，不擅自进行浏览器下载安装。
- Playwright mock Tauri IPC，不能代替 settings.json 实际落盘、原生文件过滤名称或更新提示验证。
- 原生手工验收使用隔离的测试数据：初次启动、升级旧设置、切换并重启、文件对话框、非破坏性更新提示。启动真实应用按 run 技能，禁止访问/覆盖用户真实 Profile 或执行安装更新。
- 本规划阶段没有运行上述产品测试；最终实现报告逐项标注通过/失败/未运行。

## 风险检查与回滚点

- 设置批次：重点检查异常 language 不触发整份配置归零、AppSettings 新构造点漏改。
- 字典批次：键和占位符一致性、React 外函数可用性、无 HTML 注入。
- UI 批次：重复 settings 请求与状态竞争、语言切换时表单状态、第三方控件遗漏。
- 原生批次：只本地化文本、不改权限/执行/更新行为。
- 所有批次可逐步回退代码；不删除用户设置、不改 Profile 数据库，不使用破坏性 git 重置。
