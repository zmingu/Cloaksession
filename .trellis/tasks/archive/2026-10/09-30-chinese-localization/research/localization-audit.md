# 中文化现状调研

日期：2026-09-30。来源：只读 Explore 调研；未修改业务代码。数量为估计，不是精确词条统计。

## 已确认现状

- Rust workspace + Tauri 2 + React 19 + TypeScript + Vite 6 + Tailwind 4；UI 当前没有 i18next/react-intl 等依赖或统一翻译机制。证据：`crates/tauri-app/ui/package.json:13-32`、`Cargo.toml:6-15`。
- 英文散布于导航、设置、引导、Profile 表单、指纹编辑、代理测试、扩展、MCP、活动、命令面板、确认框和 toast。估计约 25 个含用户文案文件，尚未精确盘点。证据：`crates/tauri-app/ui/src/components/screens/Settings.tsx:100-389`、`crates/tauri-app/ui/src/components/profile/FingerprintForm.tsx:139-317`、`crates/tauri-app/ui/src/components/onboarding/FirstRun.tsx:98-208`、`crates/tauri-app/ui/src/App.tsx:253-309`。
- 非显式文本也需处理：`crates/tauri-app/ui/index.html:2` 写死 lang=en；`crates/tauri-app/ui/src/components/atoms/Flag.tsx:115-117` 地区名称写死英文；`crates/tauri-app/ui/src/lib/relativeTime.ts:1-22` 时间文本英文；`crates/tauri-app/ui/src/main.tsx:24-36` 渲染错误边界英文。

## 应用设置接入点

- `crates/multizen-core/src/settings.rs:43-71` AppSettings 与 Default。
- `crates/settings-store/src/defaults.rs:20-75` RawSettings 可选字段、默认合并和缓存持久化；`crates/settings-store/tests/store.rs:6-67` 回退测试。
- `crates/tauri-app/src/commands/settings.rs:23-48` 以当前配置合并非 null patch，旧客户端不传新增字段不应清空。
- `crates/tauri-app/ui/src/types.ts:207-217` 前端设置类型；`crates/tauri-app/ui/src/lib/ipc.ts:132-143` IPC；`crates/tauri-app/ui/src/components/screens/Settings.tsx:36-66` patch 模式。
- 推荐应用显示语言走 settings.json，具体支持值与默认策略待产品确认。不能仅改 TS 类型，需同时兼容 Rust 默认值和旧配置。

## 必须保持的隔离

- `crates/multizen-core/src/profile.rs:89-108` locale/languages/accept_language/timezone/country 是每个 Profile 的浏览器参数。
- `crates/profile-manager/src/fingerprint.rs:20-24` 默认美区指纹不应因管理界面中文化改变。
- `crates/browser-launcher/src/args.rs:68-69,146-147` 指纹参数直接进入浏览器启动参数。
- `crates/cdp-driver/src/bootstrap.rs:59-79` 运行时语言覆盖使用 Profile 指纹；`crates/tauri-app/src/commands/fingerprint.rs:335-355` 调和以代理/指纹为依据。
- 应用 UI locale 必须独立于上述字段；不改用户名称、备注、标签、已有 Profile、SDK JSON keys 或 CLI 参数。字段显示标签可翻译，实际值不翻译。

## 错误和原生界面边界

- `crates/multizen-core/src/error.rs:4-31` 英文 Display；`crates/tauri-app/src/commands/settings.rs:20,29,33,46-47` 等命令返回字符串错误，当前不存在可依赖的统一错误码协议。
- 推荐首版中文操作级提示加保留原始诊断详情，不以易碎的英文子串匹配冒充错误码映射。若要求所有错误完全中文，则单列结构化错误协议改造。
- `crates/tauri-app/src/commands/dialog.rs:14-34` 文件选择器自定义 filter 可本地化，OS 自带按钮由系统控制。
- `crates/tauri-app/src/commands/update.rs:396-411` 应用自定义原生消息要纳入清单，不能假设所有文字都在 React。
- `crates/tauri-app/tauri.conf.json:15` 品牌标题 Cloaksession 保留。

## 技术候选与建议

- 统一字典 + React 响应式语言上下文；两个候选为类型安全轻量字典或成熟 react-i18next。最终按插值、复数、第三方控件和维护成本评估，不在范围未确认前锁死依赖。
- 处理首屏语言加载、设置保存失败、英文回退、键集合一致性、插值、Intl 时间/地区、document.lang 和辅助功能标签。
- 推荐以完整核心工作流为首版验收；内部先搭基础再迁移文案，而非发布只翻译壳层的半中文产品。
- emoji 选择器内置文字、外部字体和离线加载需复核；当前研究未建立完整第三方控件清单。

## 验证入口

`docs/ACCEPTANCE.md:18-24`、`README.md:187-198`、`crates/tauri-app/ui/playwright.config.ts:10-24`：

- `npm --prefix crates/tauri-app/ui run build`
- `cargo check --workspace --locked`
- `cargo test --workspace --locked`
- 在 UI 目录运行 `npx playwright test --config playwright.config.ts`（desktop/mobile 视口）。
- 按实际改动补 settings 迁移/非法值、语言切换/重启持久化、首屏、文案键、溢出与 Profile 不变回归。

## 尚需产品决定

1. 首版是应用界面、应用+文档、还是还包含启动的浏览器自身界面。
2. 是否保留英文切换，以及新安装/旧配置默认语言。

当前仅完成第一轮研究，不代表最终设计完成或获得实施许可。
