# App Chinese localization

## Goal

让中文用户完整使用 Cloaksession 管理应用,同时保留英文切换能力。新增应用级国际化基础设施,覆盖前端 React UI 与 Rust 侧用户可见字符串,持久化语言偏好,并提供 locale 感知的格式化。

## Background

- Tauri 2 + React 19 + TS + Vite 6 + Tailwind 4;Rust 九 crate 工作区。当前无任何 i18n 库或统一翻译机制。证据:`crates/tauri-app/ui/package.json:13-33`、`Cargo.toml:4-16`。
- UI 文案硬编码且混合语言:大部分英文(导航/设置/onboarding/toast/指纹/扩展目录等,约 419 候选行/40 文件,量级数百条),快手/业务账号功能已有硬编码中文。证据:`crates/tauri-app/ui/src/components/screens/Sidebar.tsx:18-22`、`crates/tauri-app/ui/src/components/profile/BusinessAccountSection.tsx:13-19`。
- 后端 Rust 通过 `Result<…, String>` + `.map_err(|e| e.to_string())` 返回用户可见字符串,流入前端 toast。证据:`crates/multizen-core/src/error.rs:3-33`、`crates/tauri-app/src/commands/profiles.rs:28-99`、`crates/tauri-app/src/commands/archive.rs:201,370,392`、`crates/tauri-app/src/commands/business_accounts.rs:9`。
- 持久化:自定义 Rust `SettingsStore` 写 `settings.json` 到 `app_local_data_dir`(`crates/settings-store/src/defaults.rs:20-79`);前端 `localStorage` 用 `multizen.ui.*` 命名空间(`crates/tauri-app/ui/src/lib/persisted.ts:3-35`)。
- 邻接能力(非应用 UI 语言,不重复造):Emoji Mart 自带 22 语言 JSON 含 `zh.json` 但应用未传 `locale`;浏览器 profile 指纹 locale(20 预设含 zh-CN/zh-TW)影响被管理浏览器,与管理 UI 隔离;本地 OCR 固定 `zh-Hans-CN`。
- 前序规划(佐证):归档任务 `.trellis/tasks/archive/2026-10/09-30-chinese-localization/` 已完成 prd/design/implement/research 但未实施、无 commit,且被判定失效;其调研结论与本次独立调研一致,仅作佐证,决策由本任务重新裁定。

## 决策(本次任务基于独立调研裁定)

1. **范围**:覆盖应用自有完整界面(导航/配置列表/新建编辑/指纹/代理/扩展/设置/MCP/活动/命令面板/引导/通知/确认框/原生自定义提示/辅助功能/占位符/时间地区显示),保留 English/简体中文 切换。不翻译启动的浏览器自身界面、网页、指纹参数。
2. **默认**:新安装及旧配置未存语言字段时默认简体中文,不跟随系统;设置中可切换;保存成功后无需重启,后续启动保留;保存失败不假装成功。
3. **技术路线**:轻量自建类型安全字典 + React Context,不引入 i18next/react-intl;按领域语义键,非整句英文做 key;具名插值;无 HTML 注入。
4. **设置契约**:`AppSettings.language` wire 值仅 `zh-CN`|`en`,与 Profile.locale 无关,无 system 模式;RawSettings 宽容读取、严格显式更新;缺/null/未知/类型错仅回退语言,不清空其他有效设置。
5. **已有中文纳入字典**:快手/业务账号/OCR 等已硬编码中文字符串迁移进 zh-CN 字典并补 en 值,统一用 key 管理,避免散落。
6. **后端错误**:不修改共享 Rust Error 的 Display;UI 捕获时用操作级中文说明 + 原始技术详情;禁止英文字串猜测映射;未知/外部错误可保留原文。
7. **原生提示**:文件对话框 filter、更新提示等 Rust 侧自定义文案用小型静态字典按已保存语言选择;OS 自带按钮/装饰由系统语言控制,列入例外清单。
8. **companion 扩展例外**:`crates/tauri-app/resources/companion/cs.js` 注入网页的按钮文字保留英文,因其在被管理浏览器页面上下文运行,获取应用语言成本不匹配。
9. **术语统一**:Profile→"浏览器配置",Fingerprint→"浏览器指纹",Locale→"区域语言";品牌 Cloaksession、JSON keys、命令、MCP 工具名/协议示例保留原样。

## Acceptance Criteria

- AC1:研究、设计、执行计划及 spec/research 上下文均已落盘;实施前获得用户审阅批准。
- AC2:新配置与旧版无语言字段配置打开即为中文;手动改英文后所有当前应用自有界面随之更新,重启仍为英文。
- AC3:完整核心流程中无未登记的应用自有英文残留;品牌、缩写、协议值、用户数据、外部内容与技术详情属明确例外。主要弹窗、表单在现有桌面/窄屏视口下无遮挡或不可操作的截断。
- AC4:切换语言前后 Profile 指纹、代理、用户名称/备注/标签及 Chromix 原始选项不变;传给浏览器/IPC 的业务枚举值不变。
- AC5:设置缺字段、非法语言字符串、语言字段类型异常、保存失败、缓存读取与重新打开均有测试;失败显示清晰提示且内存选择不冒充已保存。
- AC6:时间与地区显示使用应用语言;document.lang 更新;错误含中文操作说明及可读取的原始诊断。系统控制的按钮不要求随应用语言变化。
- AC7:中英字典键与插值参数检查、UI 构建、settings-store 测试和相关 UI 回归通过;原生效果单独实测,未执行项明确标注。

## Out of Scope

- README、安装与使用文档的整篇翻译(工程规范记录不在此限)。
- 启动的浏览器自身界面、网页语言、指纹参数的中文化。
- 操作系统按钮、窗口装饰、第三方服务返回文本、用户内容和完整后端错误码协议重构。
- 翻译品牌名 Cloaksession、技术标识、MCP 工具协议/JSON 示例键、日志原始诊断、上游发布说明或 SDK 配置键。
- companion 扩展(cs.js)注入网页的按钮文字中文化。
- 无关功能修复、浏览器下载、安装包发布和 vendored Chromix 源码改造。