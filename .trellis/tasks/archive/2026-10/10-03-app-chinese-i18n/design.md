# App Chinese localization — 技术设计

## 状态与决策

本任务基于独立调研(2026-10-03)裁定决策;前序归档任务 09-30-chinese-localization 被判定失效,其规划仅作佐证参考,不直接继承。待最终审阅;不授权实现。

核心决策见 `prd.md` "决策" 节:完整应用界面 + 中英切换、默认简体中文不跟随系统、自建字典 + React Context 不引 i18next、AppSettings.language 与 Profile.locale 隔离、已有硬编码中文纳入字典、companion cs.js 列为例外。

## 架构

采用本地打包的类型安全中英字典与 React Context。仅两种语言、无 ICU 复数需求,字典 + 具名插值 + Intl 即可满足;避免新增依赖。若后续出现多语言 ICU/复数需求,可在保持键命名和调用边界的前提下替换内部实现。

新增 `crates/tauri-app/ui/src/i18n/`,含:

- 纯翻译函数 `t(key, params)` 与类型层(键联合类型,缺键编译期/CI 检查)
- `en.ts`、`zh-CN.ts` 双字典
- `LanguageProvider` / `useLanguage` / `useT` Context
- 键按领域语义命名(如 `nav.profiles`、`settings.language.label`、`errors.profileNotFound`),不以整句英文做 key
- 具名占位符 `{{name}}`;插值仅作文本节点,不 HTML 注入
- 数量句式显式提供变体,不拼接词序片段

## 语言设置契约

- `AppSettings.language` wire 值仅 `zh-CN` | `en`,与 Profile.locale 无关,无 system 模式。
- 规范化集中在模型/设置所有者:
  - `crates/multizen-core/src/settings.rs` AppSettings 默认 zh-CN
  - `crates/settings-store/src/defaults.rs` RawSettings 对语言字段宽容读取(缺失/null/未知字符串/错误类型仅回退语言,不清空其他有效设置)
  - `crates/tauri-app/src/commands/settings.rs` IPC 更新接受已知值,非法显式更新返回错误;旧客户端不传 language 保留原值;沿用现有非 null 顶层合并,不改 null/嵌套语义
- 同步 Rust 模型、RawSettings、TS AppSettings(`crates/tauri-app/ui/src/types.ts`)、IPC fixtures;盘点所有 AppSettings 构造点与测试工厂。
- 存储沿用 SettingsStore;非原子写入,本任务不提供新磁盘原子性;失败时不刷新缓存、不更新成功状态。

## 首屏与切换数据流

1. 入口(`main.tsx`)在渲染完整应用前读取 settings,用轻量启动占位避免已保存英文用户先看到整屏中文。初始 HTML lang 可为 zh-CN,设置加载后同步 document.lang。
2. 设置加载失败进入中文回退界面并提供操作说明,不永久卡死;错误边界使用不依赖 Provider 的纯翻译 fallback。
3. 设置页通过现有 IPC 保存 language,等服务端返回成功再更新全局语言;期间禁用重复提交;不重建 App、不卸载表单,保留未保存输入和当前导航。
4. 时间/地区格式化(`relativeTime.ts`、`Flag.tsx`)显式接受应用语言,经 Intl 处理;通知与活动说明用语义键 + 参数或稳定事件类型映射,避免已打开组件缓存旧语言。外部原始诊断不改写。
5. Provider 是界面语言状态唯一入口;不写 Profile、navigator 注入、浏览器 --lang、SDK locale 或代理调和流程。

## 文案覆盖边界

- 覆盖全部应用自有组件(导航/设置/列表/命令面板/公共对话框/toast/onboarding/新建编辑/指纹/Chromix/代理/扩展/MCP/活动/更新/错误详情)及运行时错误边界;检查 title/aria-label/placeholder/空状态/确认按钮/工具提示。
- 已硬编码中文迁移进字典统一管理:`BusinessAccountSection.tsx:13-19`、`KuaishouSubject.tsx:6-14`、`kuaishouSubject.ts:31`、`business_accounts.rs:9`、`local-ocr/src/lib.rs:33` 等;补 en 值。
- Profile 国家/语言显示名称可随 UI;选项实际值不变。指纹自命名(`fingerprint.rs:97-118,186-206`)是 Profile 数据,自命名列表不改,显示随 UI 取自命名。
- 品牌、JSON keys、命令、MCP 工具名/协议示例保留原样。
- 第三方控件(emoji picker)用其已支持本地化入口或本地数据(`@emoji-mart/data/i18n/zh.json`),不改 vendor;开发前确认本地已装版本 API。
- 活动面板翻译固定操作标签/状态,不翻译外部任意文本;扩展第三方标题/说明/发布说明保留原文。
- 统一术语:Profile→"浏览器配置",Fingerprint→"浏览器指纹",Locale→"区域语言";首次出现技术术语可带英文辅助,不改数据字段。
- **例外**:companion 扩展 `crates/tauri-app/resources/companion/cs.js:45,74,78` 注入网页的按钮("Add to Cloaksession" 等)保留英文。

## 后端及原生提示

- 不修改共享 Rust Error 的 Display(`multizen-core/src/error.rs`),不破坏 MCP/日志消费者。
- UI 捕获错误时用操作级翻译 + 原始详情;禁止英文字串映射。未知/外部错误保留原文。
- 原生文件对话框 filter 标签(`commands/dialog.rs`)、更新提示文本(`commands/update.rs`)从持久化应用语言读 Rust 侧小型静态字典;调用时读设置快照,释放锁后再开阻塞弹窗。OS 自带按钮/装饰由系统语言控制,列入例外清单。
- 跨前后端核心默认行为一致并测试。

## 兼容与风险

- 老设置文件自然加载为中文(含原英文用户),是明确选择。
- 不修改 autoUpdate 默认差异、事件 payload 已知问题、更新器命名/端口等无关问题。
- 中文字形用可靠系统字体 fallback,不依赖外部字体 CDN。
- 不刷新已打开的 OS 模态窗口,后续新弹窗用已保存语言;React 弹窗随 Context 更新。
- 词条迁移漏项、英语长度回归、第三方控件为主要 UI 风险;用文案清单、两语言测试、截图检查管理。

## 回滚

功能批次隔离:设置模型、翻译基础、组件迁移、原生提示、验证。回退代码不改 profiles.db、不删用户设置;新增 language 字段不要求数据库迁移。回滚前确认是否有用户更改,禁止硬重置或覆盖共享设置文件。