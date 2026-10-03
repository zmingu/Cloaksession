# 应用中文 i18n — 用户可见字符串清单（string-inventory）

日期：2026-10-03。任务：`.trellis/tasks/10-03-app-chinese-i18n` BATCH 1（覆盖基线）。
只读盘点：未修改任何产品代码（`crates/tauri-app/Cargo.toml` 未动）。

## 方法

- 前端：`crates/tauri-app/ui/src/` 全量 `.tsx/.ts`（排除 `src/i18n/` 空目录与 `*.test.mjs`），三遍提取：
  1. 同行 JSX 文本节点、可见属性（title / aria-* / placeholder / alt / label / description / confirmLabel / subtitle 等）；
  2. 含空格或 CJK 的字符串字面量（含 toast / Error / validation 模板）；
  3. 短标签（`label: "Profiles"`、`"Launch"` 等无空格单词）；
  4. 全文件多行 JSX 文本（补回跨行句子，如 EmptyState 描述、McpPanel 段落）。
- Rust：`crates/tauri-app/src/commands/`、`crates/multizen-core/src/error.rs`、`crates/local-ocr/src/lib.rs`、`business_accounts.rs` 全量引号串 audit，区分渲染文案 / 日志 / 线协议。
- 噪声（className、style、SVG path、注释、类型字面量）已剔除；零字符串文件在 §T 列出以证覆盖。

## 覆盖统计

- 前端 60 个源文件：53 个含用户可见字符串（下文逐条），7 个经核验零字符串（§T）。
- Rust：16 个命令文件 + `error.rs` + `local-ocr/lib.rs`：9 个含用户可见字符串，其余为纯 `e.to_string()` 透传或零字面量（§T）。
- 条目约 600 行（含 file:line）。行号以本 commit 树为准。

## Key 命名约定（按 design.md）

- 领域前缀语义键，不以整句英文做 key：`nav.profiles`、`settings.mcp.title`、`errors.profileNotFound`。
- 插值统一具名 `{{name}}`（源码中 `${x}` / `{x}` / `%s` 详见备注列）。
- 动态用户数据（profile 名、昵称、ID、路径、原始错误详情）不进字典，备注标 `user-data` / `external-raw`。
- 中文术语按 PRD：Profile→“浏览器配置”，Fingerprint→“浏览器指纹”，Locale→“区域语言”。下文“建议中文”列仅为实施参考，dictionaries 落定时再定稿。

## 分类图例

| 分类 | 含义 |
|---|---|
| `translate` | 应用自有文案，进 `zh-CN` / `en` 字典 |
| `keep-original` | 中英文 UI 均保留原文（SDK 键值、单位、键盘名、OS 名、代码样例、纯数字格式） |
| `brand` | 品牌/产品名（Cloaksession、CloakBrowser、Chromix 等），永不翻译 |
| `protocol` | 线协议/配置键/枚举透传值/MCP 工具名/URL/IPC 名，永不翻译 |
| `user-data` | 用户输入与数据（名称、标签、备注、昵称、ID、路径），不进字典 |
| `external` | 第三方原文（扩展商店标题/描述、Emoji Mart 内部文案）与后端/OS 原始诊断（按设计以原文展示） |

---

## A. 共通按钮 / 对话框（`common.*`）

| 位置 | 当前字符串 | 建议 key | 分类 | 备注 |
|---|---|---|---|---|
| `ui/src/components/atoms/Modal.tsx:198` | `Close`（aria-label，✕ 按钮） | `common.close.aria` | translate | 建议中文“关闭” |
| `ui/src/components/atoms/Modal.tsx:231` | `Discard your changes?`（ConfirmHost 默认标题；App.tsx:431 另有实例） | `common.discardChanges.title` | translate | 建议“放弃更改？”；实施时与 profile.create 共用或保留两 key |
| `ui/src/components/atoms/Modal.tsx:232` | `Discard`（默认确认按钮） | `common.discard` | translate | 建议“放弃” |
| `ui/src/components/atoms/Modal.tsx:293` | `Cancel`（默认取消按钮） | `common.cancel` | translate | 建议“取消” |
| `ui/src/components/atoms/Modal.tsx:310` | `Confirm`（Confirm 组件确认按钮） | `common.confirm` | translate | 建议“确认” |
| `ui/src/components/screens/Confirm.tsx:70` | `Confirm`（同上另一实例） | `common.confirm`（复用） | translate | 同一 key |
| `ui/src/components/screens/Confirm.tsx:161` | `Submit`（异步提交中按钮） | `common.submit` | translate | 建议“提交” |
| `ui/src/components/screens/Confirm.tsx:138` | 输入框 placeholder（调用方传入，动态） | `common.input.placeholder`（调用方各自 key） | user-data | 输入内容为 user-data |
| `ui/src/App.tsx:433` | `Discard`（新建页未保存确认） | `common.discard`（复用） | translate | — |
| `ui/src/components/mcp/McpPanel.tsx:342` | `Copy`（复制按钮 title） | `common.copy` | translate | 建议“复制” |
| `ui/src/components/mcp/McpPanel.tsx:399` | `Copied`（复制成功态） | `common.copied` | translate | 建议“已复制” |
| `ui/src/components/screens/Settings.tsx:137` | `Copy URL`（aria-label） | `common.copyUrl.aria` | translate | 建议“复制 URL” |
| `ui/src/components/screens/Settings.tsx:176` | `Copy token`（aria-label） | `common.copyToken.aria` | translate | 建议“复制令牌” |
| `ui/src/components/screens/Settings.tsx:168` | `Reveal token` / `Hide token`（aria-label 二态） | `settings.mcp.revealToken.aria` / `settings.mcp.hideToken.aria` | translate | 建议“显示令牌 / 隐藏令牌” |
| `ui/src/components/profile/ExtensionsSection.tsx:347` | `Remove`（aria-label，移除扩展） | `common.remove.aria` | translate | 建议“移除” |
| `ui/src/components/onboarding/FirstRun.tsx:129` | `Continue` | `common.continue` | translate | 建议“继续” |
| `ui/src/components/onboarding/FirstRun.tsx:157` | `Enable`（遥测 opt-in） | `common.enable` | translate | 建议“启用” |
| `ui/src/components/onboarding/FirstRun.tsx:164` | `Not now` | `common.notNow` | translate | 建议“暂不” |
| `ui/src/components/UpdateBanner.tsx:64` | `Later` | `common.later` | translate | 建议“稍后” |
| `ui/src/components/UpdateBanner.tsx:81` | `Dismiss`（aria-label，✕） | `common.dismiss.aria` | translate | 建议“忽略” |
| `ui/src/components/atoms/Modal.tsx:97` | `Escape`（快捷键提示） | — | keep-original | 键盘键名，中英保留 |
| `ui/src/components/screens/Confirm.tsx:18,140` | `Escape` / `Enter`（快捷键提示） | — | keep-original | 同上 |
| `ui/src/components/palette/CommandPalette.tsx:126,130,133,136` | `Escape` / `ArrowDown` / `ArrowUp` / `Enter` | — | keep-original | 键盘键名 |
| `ui/src/components/palette/CommandPalette.tsx:188` | `esc`（Kbd） | — | keep-original | 键盘键名 |
| `ui/src/components/screens/Confirm.tsx:54` | `esc`（Kbd） | — | keep-original | 同上 |
| `ui/src/components/profile/EmojiField.tsx:48` | `Escape`（关闭 picker） | — | keep-original | 键盘键名 |
| `ui/src/components/onboarding/FirstRun.tsx:192` | `Enter`（回车提交提示） | — | keep-original | 键盘键名 |
| `ui/src/components/profile/NewProfileSheet.tsx:181` | `Enter`（同上） | — | keep-original | 键盘键名 |
| `ui/src/components/screens/LeftRail.tsx:43` | `${it.label} · ⌘${it.kbd}`（title 模板） | `nav.item.shortcutTitle`，`{{label}} · ⌘{{kbd}}` | translate | 模板翻译，label/kbd 为变量 |
| `ui/src/components/screens/Sidebar.tsx:66` | 同上（Sidebar 实例） | `nav.item.shortcutTitle`（复用） | translate | — |
| `ui/src/components/screens/LeftRail.tsx:61` | `Command palette · ⌘K`（title） | `nav.commandPalette.title` | translate | 建议“命令面板 · ⌘K” |
| `ui/src/components/screens/Sidebar.tsx:118` | 同上（Sidebar 实例） | `nav.commandPalette.title`（复用） | translate | — |
| `ui/src/components/screens/TopBar.tsx:53` | `⌘ K`（Kbd） | — | keep-original | 快捷键符号 |
| `ui/src/components/profile/Constellation.tsx:208` | `⌘ N`（Kbd） | — | keep-original | 快捷键符号 |
| `ui/src/components/palette/CommandPalette.tsx:66,246` | `⌘ N`（Kbd） | — | keep-original | 快捷键符号 |
| `ui/src/components/profile/EmptyState.tsx:48` | `⌘ N`（Kbd） | — | keep-original | 快捷键符号 |
| `ui/src/components/activity/ActivityDrawer.tsx:52` | `⌘ ⇧ A`（Kbd） | — | keep-original | 快捷键符号 |

## B. 导航 / 侧栏 / 顶栏（`nav.*`）

| 位置 | 当前字符串 | 建议 key | 分类 | 备注 |
|---|---|---|---|---|
| `ui/src/components/screens/Sidebar.tsx:19` | `Profiles`（NAV label） | `nav.profiles` | translate | 建议“浏览器配置”；LeftRail:15、palette:55/80 复用 |
| `ui/src/components/screens/Sidebar.tsx:20` | `MCP`（NAV label） | `nav.mcp` | keep-original | 缩写，中英保留；LeftRail:16、palette:88 复用 |
| `ui/src/components/screens/Sidebar.tsx:21` | `Settings`（NAV label） | `nav.settings` | translate | 建议“设置”；LeftRail:17、palette:96、TopBar:72、McpPanel:194/260（“in Settings”内联引用）复用 |
| `ui/src/components/screens/Sidebar.tsx:89` | `All`（分组过滤） | `group.filter.all` | translate | 建议“全部”；Constellation:215 复用 |
| `ui/src/components/screens/Sidebar.tsx:105` | `Ungrouped`（分组过滤；App.tsx:113 同值） | `group.filter.ungrouped` | translate | 建议“未分组”；Constellation:232 复用 |
| `ui/src/components/screens/Sidebar.tsx:169` | `Delete group "${label}"`（title 模板） | `group.delete.title`，`{{label}}` | translate | 建议“删除分组“{{label}}”” |
| `ui/src/components/screens/Sidebar.tsx:172` | `Delete group ${label}`（aria-label 模板） | `group.delete.aria`，`{{label}}` | translate | 同上 |
| `ui/src/App.tsx:286` | `Delete group failed: ${msg}`（toast） | `errors.deleteGroupFailed`，`{{msg}}` | translate | msg 为 external-raw |
| `ui/src/components/screens/TopBar.tsx:32` | `Cloaksession`（标题） | — | brand | — |
| `ui/src/components/screens/TopBar.tsx:50` | `Search profiles, tags, urls…`（顶栏搜索占位） | `nav.topbar.searchPlaceholder` | translate | 建议“搜索配置、标签、网址…” |
| `ui/src/components/screens/TopBar.tsx:60` | `running`（计数后缀） | `nav.topbar.running` | translate | 建议“运行中” |
| `ui/src/components/screens/TopBar.tsx:63` | `total`（计数后缀） | `nav.topbar.total` | translate | 建议“共计” |
| `ui/src/components/screens/TopBar.tsx:66` | `` `· :${new URL(mcpUrl).port}` ``（MCP 端口后缀；`MCP … off` 整体见下） | — | protocol | 端口号为数据 |
| `ui/src/components/screens/TopBar.tsx:66`（Pill） | `MCP · :{port}` / `MCP off`（字面量 `off`） | `nav.topbar.mcpOff`（仅 off 部分） | translate | 建议“未启动”；端口部分 keep |
| `ui/src/components/screens/Sidebar.tsx:8` | `Constellation.GroupFilter`（注释） | —（注释） | — | 非渲染文案，仅记录 |

## C. App 壳 / 错误边界 / 成就 toast（`app.*`）

| 位置 | 当前字符串 | 建议 key | 分类 | 备注 |
|---|---|---|---|---|
| `ui/src/main.tsx:28` | `Renderer error`（错误边界标题） | `app.rendererError.title` | translate | 建议“渲染错误” |
| `ui/src/main.tsx:24-36` | 错误边界详情（`error.message` 堆栈原文） | — | external | 原始诊断保留原文 |
| `ui/src/main.tsx:7` | `#root not found`（启动 throw，dev 向） | — | keep-original | 开发者向，不进字典 |
| `ui/src/App.tsx:411-413` | `Preload bridge missing — <code>window.multizen</code> is undefined. Open DevTools for details.` | `app.bridgeMissing` | translate | `window.multizen` 为 protocol 不译；建议“预加载桥缺失 … 打开 DevTools 查看详情” |
| `ui/src/App.tsx:298` | `Launch failed: ${msg}` | `errors.launchFailed`，`{{msg}}` | translate | 建议“启动失败：{{msg}}” |
| `ui/src/App.tsx:335` | `Exported to ${file}` | `archive.exportedTo`，`{{file}}` | translate | file 为 user-data（路径） |
| `ui/src/App.tsx:337` | `Export failed: ${result.reason}` | `errors.exportFailed`，`{{reason}}` | translate | reason 透传后端（§R），原文展示 |
| `ui/src/App.tsx:319` | `Import failed: ${result.reason}` | `errors.importFailed`，`{{reason}}` | translate | 同上 |
| `ui/src/App.tsx:329` | `Passphrase must be at least 8 characters`（前端校验；后端 archive.rs:201 同文） | `archive.passphraseMinLength` | translate | 建议“口令至少 8 个字符”；前后端同文需一致 |
| `ui/src/App.tsx:77` | `` Added "${e.extension.name}" — profile is reopening… `` | `extensions.installedReopening`，`{{name}}` | translate | name 为 user-data（扩展名，external 原文） |
| `ui/src/App.tsx:78` | `Extension install failed: ${e.error}` | `errors.extensionInstallFailed`，`{{error}}` | translate | error 为 external-raw |
| `ui/src/App.tsx:69` / `lib/ipc.ts:440` / `types.ts:304` | `Add to Cloaksession`（companion 信号注释/匹配串） | — | protocol | companion 例外（§S）：按钮在网页上下文，保持英文 |
| `ui/src/App.tsx:421` | `New profile`（新建 sheet 标题） | `profile.new.title` | translate | 建议“新建浏览器配置”；Constellation:206、EmptyState:45 复用 |
| `ui/src/App.tsx:422` | `Cookies, login state, and fingerprint live in this profile only.`（副标题） | `profile.new.subtitle` | translate | 建议“Cookie、登录态与指纹仅属于此配置。” |
| `ui/src/App.tsx:431-432` | `Discard your changes?` / `You haven't created the profile yet. Closing will lose what you've entered.` | `profile.create.discardTitle` / `.discardBody` | translate | 通用 key 备选（见 §A）；此处为新建页实例 |
| `ui/src/App.tsx:491` | `` Edit ${editingProfile.name} `` / `Edit profile` | `profile.edit.title`，`{{name}}` / `profile.edit.titleFallback` | translate | 建议“编辑 {{name}} / 编辑浏览器配置”；Row:365、Tile:445 菜单项复用 titleFallback |
| `ui/src/App.tsx:492` | `Profile changes autosave. Business account registration saves separately.` | `profile.edit.subtitle` | translate | 建议“配置更改自动保存；业务账号登记需单独保存。” |
| `ui/src/App.tsx:375`（ProfileEditSheet） | `{status.message}`（保存状态动态串） | — | external | 动态状态，见 §D |

## D. 新建 / 编辑 Profile（`profile.*`）

Sheet 分区 tab（`ui/src/components/profile/profileSheetKit.tsx:13-18`）：

| 位置 | 当前字符串 | 建议 key | 分类 | 备注 |
|---|---|---|---|---|
| profileSheetKit.tsx:13 | `General` | `profile.tabs.general` | translate | 建议“通用”；NewProfileSheet:56 / ProfileEditSheet:114 默认值复用 |
| profileSheetKit.tsx:14 | `Browser` | `profile.tabs.browser` | translate | 建议“浏览器”；BrowserSection:6 复用 |
| profileSheetKit.tsx:15 | `Proxy` | `profile.tabs.proxy` | translate | 建议“代理” |
| profileSheetKit.tsx:16 | `Extensions` | `profile.tabs.extensions` | translate | 建议“扩展” |
| profileSheetKit.tsx:17 | `Fingerprint` | `profile.tabs.fingerprint` | translate | 建议“浏览器指纹” |
| profileSheetKit.tsx:18 | `Chromix fingerprint` | `profile.tabs.chromixFingerprint` | translate | Chromix 为 brand；建议“Chromix 指纹参数” |

通用字段（NewProfileSheet / ProfileEditSheet 共享措辞，key 复用）：

| 位置 | 当前字符串 | 建议 key | 分类 | 备注 |
|---|---|---|---|---|
| NewProfileSheet.tsx:138 / ProfileEditSheet.tsx:72 | `Name is required`（校验） | `profile.validation.nameRequired` | translate | 建议“名称为必填项” |
| NewProfileSheet.tsx:112 / ProfileEditSheet.tsx:73 | `Proxy host is required`（校验） | `profile.validation.proxyHostRequired` | translate | 建议“代理主机为必填项” |
| NewProfileSheet.tsx:143 | `Fingerprint preset is still loading` | `profile.new.fingerprintLoading` | translate | 建议“指纹预设加载中” |
| NewProfileSheet.tsx:201 / ProfileEditSheet.tsx:223 | `Icon`（Field label） | `profile.field.icon` | translate | 建议“图标” |
| NewProfileSheet.tsx:205 / ProfileEditSheet.tsx:233 | `Name`（Field label） | `profile.field.name` | translate | 建议“名称”；ProfileTable:24 复用 |
| NewProfileSheet.tsx:213 | `e.g. acme — sales · west`（placeholder 示例） | `profile.field.namePlaceholder` | translate | 示例文本，建议中文示例 |
| NewProfileSheet.tsx:216 / ProfileEditSheet.tsx:236 | `Tags` | `profile.field.tags` | translate | 建议“标签”；ProfileTable:26 复用 |
| NewProfileSheet.tsx:220 | `comma-separated (optional)`（placeholder） | `profile.field.tagsPlaceholder` | translate | ProfileEditSheet:240 `comma-separated` 同 key |
| NewProfileSheet.tsx:225 / ProfileEditSheet.tsx:245 | `Group` | `profile.field.group` | translate | 建议“分组” |
| NewProfileSheet.tsx:229 / ProfileEditSheet.tsx:249 | `optional — e.g. sales`（placeholder） | `profile.field.groupPlaceholder` | translate | 建议中文示例 |
| NewProfileSheet.tsx:232 / ProfileEditSheet.tsx:252 | `Notes` | `profile.field.notes` | translate | 建议“备注” |
| NewProfileSheet.tsx:236 | `Optional — what this profile is for`（placeholder） | `profile.field.notesPlaceholder` | translate | 建议“选填——此配置的用途” |
| NewProfileSheet.tsx:211 | `…required`（字段必填后缀片段；多行 JSX 的一部分） | `profile.field.requiredSuffix` | translate | 建议“（必填）”；与 label 组合渲染 |
| NewProfileSheet.tsx:260 / ProfileEditSheet.tsx:272 | `Use proxy`（复选框） | `profile.proxy.useProxy` | translate | 建议“使用代理” |
| NewProfileSheet.tsx:266 / ProfileEditSheet.tsx:278 | `Type`（代理类型） | `profile.proxy.type` | translate | 建议“类型” |
| NewProfileSheet.tsx:275-276 / ProfileEditSheet.tsx:285-286 | `HTTP` / `SOCKS5`（下拉选项） | — | protocol | 协议名，中英保留 |
| NewProfileSheet.tsx:279 / ProfileEditSheet.tsx:289 | `Host` | `profile.proxy.host` | translate | 建议“主机” |
| NewProfileSheet.tsx:299 / ProfileEditSheet.tsx:306 | `host or host:port:user:pass`（placeholder 格式说明） | `profile.proxy.hostPlaceholder` | keep-original | 格式样例，保留原文更准确 |
| NewProfileSheet.tsx:303 / ProfileEditSheet.tsx:310 | `Port` | `profile.proxy.port` | translate | 建议“端口” |
| NewProfileSheet.tsx:307 / ProfileEditSheet.tsx:314 | `8080`（placeholder） | — | keep-original | 端口示例数字 |
| NewProfileSheet.tsx:313 / ProfileEditSheet.tsx:320 | `Username` | `profile.proxy.username` | translate | 建议“用户名” |
| NewProfileSheet.tsx:320 / ProfileEditSheet.tsx:327 | `Password` | `profile.proxy.password` | translate | 建议“密码” |
| NewProfileSheet.tsx:356 | `Loading preset…` | `profile.new.presetLoading` | translate | 建议“预设加载中…” |
| NewProfileSheet.tsx:391 | `Create & launch`（创建按钮；`…` 为省略号片段） | `profile.new.createAndLaunch` | translate | 建议“创建并启动” |
| ProfileEditSheet.tsx:375 | `{status.message}`：`All changes saved`（:381） | `profile.edit.allSaved` | translate | 建议“所有更改已保存”；其余动态 message 为校验/错误原文 |
| `ui/src/components/profile/BrowserSection.tsx:28` | `Start page` | `profile.browser.startPage` | translate | 建议“起始页” |
| `ui/src/components/profile/BrowserSection.tsx:33` | `DEFAULT_START_URL` 占位（`about:blank`，见 :41） | — | protocol | URL 值透传浏览器 |
| `ui/src/components/profile/BrowserSection.tsx:39` | `Opens on a profile's first launch (later launches restore your tabs). Leave the default or set your own — any http(s) URL, or`（多行段落，after `or` 接 about:blank 选项） | `profile.browser.startPageHint` | translate | `http(s)`/`about:blank` 为 protocol 不译 |

## E. 代理连通性（`proxy.*`）

| 位置 | 当前字符串 | 建议 key | 分类 | 备注 |
|---|---|---|---|---|
| `ui/src/components/profile/ProxyTester.tsx:61` | `Test proxy`（按钮） | `proxy.test` | translate | 建议“测试代理” |
| `ui/src/components/profile/ProxyTester.tsx:84` | `` `${result.city}, …` ``（`city, countryName (ip)` 模板片段） | — | user-data | Geo 结果为数据；标点格式保留 |
| `ui/src/components/profile/ProxyTester.tsx:90,94` | `IP` / `TZ`（结果标签，`&nbsp;` 分隔） | — | keep-original | 通用缩写，中英保留 |
| `ui/src/components/profile/ProxyTester.tsx` 结果/错误 | 动态 `error` 串 | — | external | 后端透传原文（`e.to_string()`） |
| `ui/src/components/profile/ProfileRow.tsx:250` | `direct`（无代理 pill/直连标志） | `proxy.direct` | translate | 建议“直连” |
| `ui/src/components/profile/ProfileTile.tsx:248` | `no proxy` | `proxy.none` | translate | 建议“无代理” |
| `ui/src/components/profile/ProfileTile.tsx:259` | `Direct — no proxy` | `proxy.directNone` | translate | 建议“直连——无代理” |
| `ui/src/components/profile/ProfileTile.tsx:260` | `host DNS`（直连说明片段） | `proxy.hostDns` | keep-original | 技术术语保留 |
| `ui/src/components/profile/ProfileTile.tsx:269` | `Connected`（代理可用） | `proxy.connected` | translate | 建议“已连通” |
| `ui/src/components/profile/ProfileTile.tsx:271` / ProfileRow（同模板） | `Proxy unreachable` | `proxy.unreachable` | translate | 建议“代理不可达” |
| `ui/src/components/profile/ProfileRow.tsx:264` / ProfileTile.tsx:274 | `` `${health.error} — click to retry` `` | `proxy.healthRetry`，`{{error}}` | translate | error 为 external-raw |
| `ui/src/components/profile/ProfileRow.tsx:266` / ProfileTile.tsx:276 | `` `${label}${cc ? … : ""} — click to re-check` `` | `proxy.healthRecheck`，`{{label}}` | translate | label/cc 为数据 |
| `ui/src/components/profile/ProfileRow.tsx:267` / ProfileTile.tsx:277 | `checking proxy…` / `Checking proxy…`（Row/Tile 大小写各一） | `proxy.checking` | translate | 统一一 key，建議“正在检测代理…” |
| `ui/src/components/profile/FingerprintForm.tsx:156-168`（按钮文本） | `Regenerate fingerprint`（见 §F）/ `{detecting ? "Detecting…" : "Match proxy"}` | `fingerprint.regenerate` / `fingerprint.detecting` / `fingerprint.matchProxy` | translate | “Detecting…”建议“检测中…”；注：源码注释（:21,29）中的 "Detect from proxy" 仅为注释措辞，实际渲染按钮为 Match proxy |
| `ui/src/components/profile/FingerprintForm.tsx:168` | `Match proxy`（按钮；`Detecting…` 见上行） | `fingerprint.matchProxy` | translate | 建议“匹配代理” |
| `ui/src/components/profile/FingerprintForm.tsx:205` | `Match proxy did nothing`（toast） | `fingerprint.matchProxyNoop` | translate | 建议“匹配代理未产生变化” |
| `ui/src/components/profile/FingerprintForm.tsx:165` | `Probe the proxy and align locale/timezone with its geo`（title） | `fingerprint.matchProxy.title` | translate | 建议“探测代理并按其地理对齐区域语言/时区” |
| `ui/src/components/profile/FingerprintForm.tsx:93` | `` `Proxy is in ${geo.countryName} (${geo.country}) — no locale preset matches. Pick a locale manually below.` `` | `fingerprint.noLocaleMatch`，`{{countryName}} {{country}}` | translate | 地名/代码为数据 |
| `ui/src/components/profile/FingerprintForm.tsx:100` | `` `Locale ${localeId} not in catalog. Pick one manually.` `` | `fingerprint.localeNotInCatalog`，`{{localeId}}` | translate | localeId 为 protocol |
| `ui/src/components/profile/FingerprintForm.tsx:601` | `{detectError}`（探测错误动态） | — | external | 后端/探测原文 |
| `ui/src/components/profile/FingerprintForm.tsx:612-619` | `Locale country is {country} but proxy IP is in {country} — anti-bot systems flag this. Pick a matching locale or proxy.`（多段 JSX，大写 strong 为数据） | `fingerprint.proxyMismatch`，`{{profileCountry}} {{proxyCountry}}` | translate | 建议“区域语言国家为…，但代理 IP 位于…，反机器人系统会标记。请匹配区域语言或代理。” |
| `ui/src/components/profile/FingerprintForm.tsx:629-636` | `Coherent: proxy resolves to {city}, {countryName} ({ip}) — matches locale.` | `fingerprint.proxyCoherent`，`{{city}} {{countryName}} {{ip}}` | translate | 建议“一致：代理解析到…，与区域语言匹配。” |

## F. 指纹表单 FingerprintForm（`fingerprint.*`，CloakBrowser/CFT 引擎）

| 位置 | 当前字符串 | 建议 key | 分类 | 备注 |
|---|---|---|---|---|
| FingerprintForm.tsx:150 | `Generate a new coherent device + locale + screen + UA`（title） | `fingerprint.regenerate.title` | translate | 按钮文本见 :152 |
| FingerprintForm.tsx:152 | `Regenerate fingerprint`（按钮文本；再生整套指纹） | `fingerprint.regenerate` | translate | 建议“重新生成指纹” |
| FingerprintForm.tsx:173 | `Device` | `fingerprint.field.device` | translate | 建议“设备” |
| FingerprintForm.tsx:181 | `Locale` | `fingerprint.field.locale` | translate | 建议“区域语言”（PRD 术语） |
| FingerprintForm.tsx:188 | `` `${fingerprint.locale} (custom)` ``（自定义选项） | `fingerprint.localeCustom`，`{{locale}}` | translate | 建议“{{locale}}（自定义）” |
| FingerprintForm.tsx:195 | `Timezone` | `fingerprint.field.timezone` | translate | 建议“时区” |
| FingerprintForm.tsx:214 | `` `${fingerprint.timezone} (proxy)` ``（代理检出选项） | `fingerprint.timezoneProxy`，`{{timezone}}` | translate | 建议“{{timezone}}（代理）” |
| FingerprintForm.tsx:222 | `Screen size` | `fingerprint.field.screen` | translate | 建议“屏幕尺寸” |
| FingerprintForm.tsx:242 | `` `${w} × ${h} (custom)` ``（自定义选项） | `fingerprint.screenCustom` | translate | 建议“{{w}} × {{h}}（自定义）” |
| FingerprintForm.tsx:258 | `title={fingerprint.userAgent}`（UA 全文悬停） | — | user-data | UA 为数据 |
| FingerprintForm.tsx:260 | `UA · ` + `{shortUA(…)}`（UA 缩写预览） | `fingerprint.uaPrefix`（仅 `UA · ` 前缀） | keep-original | UA 令牌不译；`shortUA` 输出为数据 |
| FingerprintForm.tsx:268 | `User-Agent` | `fingerprint.field.userAgent` | keep-original | HTTP 头名，中英保留 |
| FingerprintForm.tsx:278 | `Platform` | `fingerprint.field.platform` | translate | 建议“平台” |
| FingerprintForm.tsx:287 | `Country` | `fingerprint.field.country` | translate | 建议“国家/地区” |
| FingerprintForm.tsx:298 | `Languages` + desc `Comma-separated navigator.languages values.` | `fingerprint.field.languages` / `.languagesDesc` | translate | `navigator.languages` 为 protocol；建议“语言 / 逗号分隔的 navigator.languages 值。” |
| FingerprintForm.tsx:312 | `Accept-Language` | `fingerprint.field.acceptLanguage` | keep-original | HTTP 头名 |
| FingerprintForm.tsx:322 | `DPR` | `fingerprint.field.dpr` | keep-original | 缩写（device pixel ratio） |
| FingerprintForm.tsx:336 | `Hardware concurrency` | `fingerprint.field.hwConcurrency` | translate | 建议“硬件并发数” |
| FingerprintForm.tsx:352 | `Device memory (GB)` | `fingerprint.field.deviceMemory` | translate | GB 为 keep-original 单位 |
| FingerprintForm.tsx:369 | `Available screen size` + desc `Clear both values to omit availScreen.` | `fingerprint.field.availScreen` / `.availScreenDesc` | translate | `availScreen` 为 protocol；建议“可用屏幕尺寸 / 清空两值以省略 availScreen。” |
| FingerprintForm.tsx:375,394 | `width` / `height`（placeholder） | `fingerprint.availWidth.placeholder` / `.availHeight.placeholder` | translate | 建议“宽 / 高” |
| FingerprintForm.tsx:410 | `Clear`（清空可用尺寸） | `common.clear` | translate | 建议“清空” |
| FingerprintForm.tsx:414 | `WebGL vendor` | `fingerprint.field.webglVendor` | keep-original | API 术语，中英保留（或译“WebGL 供应商”——实施定稿；此处先 keep） |
| FingerprintForm.tsx:423 | `WebGL renderer` | `fingerprint.field.webglRenderer` | keep-original | 同上 |
| FingerprintForm.tsx:432 | `Client hints` + desc `All values are persisted verbatim and used by the selected browser engine where supported.` | `fingerprint.field.clientHints` / `.clientHintsDesc` | translate | Client Hints 为品牌式术语，保留英文词；建议“Client Hints / 所有值按原文持久化，并由所选浏览器引擎在支持时使用。” |
| FingerprintForm.tsx:438 | `aria-label={key}`（client-hints 行键名） | — | protocol | 键名为协议 |
| FingerprintForm.tsx:448 | `Storage quota (MB)` + desc `Pins navigator.storage.estimate(). Empty = engine default.` | `fingerprint.field.storageQuota` / `.storageQuotaDesc` | translate | API 名保留；建议“存储配额 (MB) / 固定 navigator.storage.estimate()。留空 = 引擎默认。” |
| FingerprintForm.tsx:471 | `0 (default)`（placeholder） | `fingerprint.storageQuota.placeholder` | translate | 建议“0（默认）” |
| FingerprintForm.tsx:477 | `Font directory` + desc `Folder CloakBrowser loads fonts from (--fingerprint-fonts-dir). Leave empty for the OS default (C:\Windows\Fonts on Windows).` | `fingerprint.field.fontsDir` / `.fontsDirDesc` | translate | CloakBrowser 为 brand；flag/路径为 protocol |
| FingerprintForm.tsx:490 | `C:\Windows\Fonts (default)`（placeholder） | — | keep-original | OS 路径 |
| FingerprintForm.tsx:507 | `Browse for a font folder`（title） | `fingerprint.browseFonts.title` | translate | 建议“浏览字体文件夹” |
| FingerprintForm.tsx:510 | `Browse…`（按钮） | `common.browse` | translate | 建议“浏览…” |
| FingerprintForm.tsx:521 | `Reset to engine default`（title） | `fingerprint.resetEngineDefault.title` | translate | 建议“重置为引擎默认” |
| FingerprintForm.tsx:534 | `Canvas noise seed` + desc `Rotates CloakBrowser canvas/audio/WebGL noise. Same seed = same noise. Empty = stable per-profile default.` | `fingerprint.field.noiseSeed` / `.noiseSeedDesc` | translate | 品牌保留；建议“Canvas 噪声种子 / 轮换 CloakBrowser 的 canvas/音频/WebGL 噪声。相同种子 = 相同噪声。留空 = 稳定的按配置默认值。” |
| FingerprintForm.tsx:547 | `(profile default)`（placeholder） | `fingerprint.noiseSeed.placeholder` | translate | 建议“（配置默认值）” |
| FingerprintForm.tsx:567 | `Generate a random seed to rotate canvas/audio/WebGL noise`（title） | `fingerprint.randomizeSeed.title` | translate | 建议“生成随机种子以轮换噪声” |
| FingerprintForm.tsx:570 | `Randomize`（按钮） | `fingerprint.randomizeSeed` | translate | 建议“随机” |
| FingerprintForm.tsx:583 | `Reset to per-profile default`（title） | `fingerprint.resetProfileDefault.title` | translate | 建议“重置为按配置默认值” |
| FingerprintForm.tsx:719 | `` `Chrome ${m[1]} / ${platform}` ``（UA 预览行） | — | protocol | 引擎令牌拼接，由数据构成，不进字典 |
| FingerprintForm.tsx:129 | `SG`（注释内国别示例；渲染侧为动态国别码） | — | protocol | ISO 国别码 |
| `commands/fingerprint.rs:52-54` | 20 个 locale id（`en-US` … `vi-VN`） | — | protocol | 透传浏览器的 BCP-47 值 |
| `commands/fingerprint.rs:99-117` | 设备显示名（`MacBook Pro 14" (M3)` … `Linux Desktop (NVIDIA)`） | `fingerprint.deviceName.*`（按 family id 分 key，值建议保留英文） | keep-original | 硬件专有名词，中英 UI 均保留；`_ => "Unknown device"`（:118）除外→ `fingerprint.unknownDevice`（translate，建议“未知设备”） |
| `commands/fingerprint.rs:135,139-154` | 屏幕选项 label（`1512 × 982 (native)` / `1920 × 1080` / `2560 × 1440`） | `fingerprint.screenNative`（仅 `(native)` 限定词） | translate | 尺寸数字保留；建议“（原生）” |
| `commands/fingerprint.rs:186-206` | locale 自命名（`English (United States)` / `中文 (简体, 中国大陆)` / `日本語 (日本)` … 20 条） | — | keep-original | 按设计为自命名显示（Profile 数据），不改；选项实际值（locale id）不变 |
| `commands/fingerprint.rs:29-47` | family id / locale id / tzid 表 | — | protocol | 透传值 |

## G. Chromix 指纹（`chromix.*`，`lib/chromixFingerprint.ts` + `ChromixFingerprintForm.tsx` + `ChromixProfileOptions.tsx`）

说明：section label/description、字段 label/description、校验提示均为渲染文案（translate）；choices 值、单位、SDK 键、flag 拼写为 protocol（keep-original）。条目多，为清单完整逐条列出；实施时可按 `CHROMIX_FINGERPRINT_GROUPS` / field id 批量建 key。

### G1. 分组（`lib/chromixFingerprint.ts:33-45`）

| 位置 | 当前字符串 | 建议 key | 分类 | 备注 |
|---|---|---|---|---|
| :33 | `Identity & browser` / `One immutable launch persona is shared by renderers and workers. An unset field writes no override.` | `chromix.groups.identity` / `.identityDesc` | translate | — |
| :34 | `CPU & memory` / `Presentation settings do not allocate CPU cores or change V8 heap limits.` | `chromix.groups.hardware` / `.hardwareDesc` | translate | V8 为 brand 保留 |
| :35 | `Screen, work area & viewport` / `Screen/work-area values are DIP. …` | `chromix.groups.display` / `.displayDesc` | translate | DIP/SDK 为 protocol |
| :36 | `GPU & rendering backend` / `Native is the ordinary launch default. …` | `chromix.groups.gpu` / `.gpuDesc` | translate | WebGL/WebGPU 为 protocol |
| :37 | `Fonts` / `Use actual installed fonts. A family name is not a font file or a DirectWrite rasterizer.` | `chromix.groups.fonts` / `.fontsDesc` | translate | DirectWrite 为 protocol |
| :38 | `Locale & timezone` / `Explicit regional settings survive fingerprint=off. …` | `chromix.groups.regional` / `.regionalDesc` | translate | `fingerprint=off` 为 protocol |
| :39 | `Storage quota` / `Quota is a launch-local backend policy, not a disk reservation. Zero is a valid quota.` | `chromix.groups.storage` / `.storageDesc` | translate | — |
| :40 | `WebRTC & GeoIP` / `IP overrides change local ICE/SDP/stats presentation, not sockets, STUN success or routing. …` | `chromix.groups.network` / `.networkDesc` | translate | ICE/SDP/STUN 为 protocol |
| :41 | `Audio, codecs & clocks` / `Codec restrictions never add support. …` | `chromix.groups.media` / `.mediaDesc` | translate | — |
| :42 | `CSS & input preferences` / `These configure effective WebPreferences. …` | `chromix.groups.preferences` / `.preferencesDesc` | translate | WebPreferences 为 protocol |
| :43 | `Noise, cookies & author shadow roots` / `Noise controls existing perturbation paths, …` | `chromix.groups.privacy` / `.privacyDesc` | translate | FakeShadowRoot 为 protocol |
| :44 | `SDK launch behavior` / `Only explicit selections are written. …` | `chromix.groups.sdk` / `.sdkDesc` | translate | — |
| :45 | `Advanced / high-risk engine controls` / `Explicit SDK-documented opt-ins. …` | `chromix.groups.advanced` / `.advancedDesc` | translate | Canvas Bridge 为 protocol |

### G2. 字段 label/description（`:65-126`，key 模式 `chromix.fields.<id>` / `.<id>Desc`）

| 位置（label） | 当前 label | 分类 | description 摘要（全文见源码行） |
|---|---|---|---|
| :65 | `Fingerprint seed / off` | translate | `Nonzero decimal uint64, or off / false / 0 / disable / disabled. …`（translate；uint64/flag 拼写为 protocol） |
| :66 | `Platform`（注意与 F. `Platform` 同词不同 key） | translate | `windows / Win32, macos / MacIntel, linux / Linux x86_64. …`（translate；choices 值见 G4） |
| :67 | `Browser brand` | translate | `Changes UA and Client Hints branding, …`（translate；choices `Chrome/Edge/Opera/Vivaldi` 见 G4） |
| :68 | `Brand version` | translate | `Numeric version with up to four components. …` |
| :69 | `Platform version` | translate | `Numeric Client Hints version … Does not change the host OS.` |
| :70 | `Chromium engine version` | translate | `Explicit engine version takes precedence over Chrome's derived brand version; …`（Chromium/Chrome 为 brand） |
| :71 | `User-Agent` | keep-original | label 为头名；desc `SDK/Playwright context User-Agent override. …`（translate；Playwright 为 brand） |
| :72 | `Hardware concurrency` | translate | `Default value 8 is supplied by the native implementation. …` |
| :73 | `Device memory` | translate | `Positive decimal up to 32, rounded to a supported bucket: 0.25, … 8 GB.`（GB/units 见 G4） |
| :74 | `Screen width` | translate | `Seeded default: 1920 on Windows/Linux, 1440 on macOS. …` |
| :75 | `Screen height` | translate | `Seeded default: 1080 on Windows/Linux, 900 on macOS. …` |
| :76 | `Taskbar / dock height` | translate | `Seeded default: Windows 48, macOS 95, Linux 0. …` |
| :77 | `Available screen width` | translate | `Work-area width; cannot exceed screen width. …` |
| :78 | `Available screen height` | translate | `Work-area height; cannot exceed screen height. …` |
| :79 | `Device pixel ratio` | translate | `SDK synthetic geometry accepts 0.25–8. …` |
| :80 | `Raw viewport width` | translate | `Supply both raw viewport dimensions. …` |
| :81 | `Raw viewport height` | translate | `Supply both raw viewport dimensions. SDK viewport:null removes inherited …`（`viewport:null` 为 protocol） |
| :82 | `Outer window width` | translate | `Native window bounds. Must agree with an explicit --window-size.` |
| :83 | `Outer window height` | translate | `Native window bounds. SDK synthetic geometry requires more than its 85 DIP UI strip.` |
| :84 | `Window X` | translate | `Signed window coordinate. Explicit native --window-position takes precedence.` |
| :85 | `Window Y` | translate | 同上 |
| :86 | `GPU backend policy` | translate | `Native shares one Canvas/WebGL/WebGPU policy and suppresses legacy noise, Bridge and capability/identity overrides. …` |
| :87 | `WebGL unmasked vendor` | keep-original（label） | `Presentation only in compatibility mode on nonsuppressed contexts. …`（translate） |
| :88 | `WebGL unmasked renderer` | keep-original（label） | `Same compatibility/context guards as vendor. …`（translate） |
| :89 | `Geolocation presentation` | translate | `Native normalizer alias retained for compatibility with the public fingerprint switch family. …` |
| :90 | `Disable GPU fingerprint` | translate | `Native normalizer compatibility switch. … preserved as an explicit raw control.` |
| :91 | `Windows font metrics` | translate | `Requires Linux → Windows persona and actual matching Windows font files. …` |
| :92 | `Font policy` | translate | `Restricted checks resolved fonts, fallback and Local Font Access. …` |
| :93 | `Installed font families` | translate | `Comma-separated 1–256 actual installed family names; maximum 4096 UTF-8 bytes. …` |
| :94 | `Font directory`（注意与 F. 同词不同 key） | translate | `SDK parses real font files for a default whitelist before restricted-policy validation. …` |
| :95 | `Timezone flag` | translate | `IANA timezone. Explicit SDK timezone overwrites the public flag, while an explicit --uxr-timezone still takes precedence.` |
| :96 | `Locale flag / raw languages` | translate | `Language tag and normalized Accept-Language. The raw --uxr-languages field accepts a comma-separated language list and wins over this alias.` |
| :97 | `SDK timezone` | translate | `High-level timezone routed to the native flag, not context emulation. …` |
| :98 | `SDK locale` | translate | `High-level language tag routed to --lang and --fingerprint-locale. …` |
| :99 | `Storage quota` | translate | `Integer MiB (1024² bytes), 0–8796093022207. Seeded default: 102400 MiB. …`（MiB 见 G4） |
| :100 | `WebRTC presentation IP` | translate | `Literal IPv4/IPv6 or auto. SDK auto resolves once through the effective proxy; …` |
| :101 | `Native WebRTC IP policy` | translate | `With a proxy the SDK defaults to disable_non_proxied_udp unless an explicit native policy wins. …` |
| :102 | `Resolve GeoIP` | translate | `Resolve timezone, locale and WebRTC exit IP through the effective proxy. …` |
| :103 | `Audio render policy` | translate | `Native by default. Isolated processes the actual output bus using a seed-dependent 2^-20 sample grid; …` |
| :104 | `Audio isolation seed` | translate | `Nonzero decimal uint64. Isolated can fall back to the fingerprint seed. noise=false or fingerprint=off disables the processing.` |
| :105 | `Timer resolution` | translate | `Decimal integer 0–1000 milliseconds, not microseconds. …` |
| :106 | `` `${codec.toUpperCase()} codec policy` ``（h264/vp8/vp9/av1/hevc×5） | translate（模板，`{{codec}}`） | `Native or disabled; an explicit empty value also disables this family. …`（translate） |
| :107 | `Synthetic Windows voice table` | translate | `Only affects the Windows synthetic fixture. … does not install SAPI or synthesize voices.` |
| :108 | `Maximum touch points` | translate | `Integer 0–16. fine + positive count supports mixed input; none + positive count or coarse + zero is rejected.`（fine/none/coarse 为 protocol） |
| :109 | `Pointer` | translate | `none + hover is rejected. This describes effective settings, not physical input hardware.` |
| :110 | （validation 文案，非 label）`Use a combination consistent with pointer and touch count.` | translate | `chromix.validation.pointerCombo` |
| :111 | `Color scheme` | translate | `Effective CSS query and styling preference. Playwright context colorScheme is a separate emulation setting.` |
| :112 | `Preferred contrast` | translate | `Effective CSS and styling preference.` |
| :113 | `Forced colors` | translate | `Controls actual author-style color replacement as well as queries.` |
| :114 | （boolean 行 desc，无独立 label 行）`Independent boolean applied to effective WebPreferences. False is an explicit override, not an unset field.` | translate | `chromix.fields.<id>Desc`（对应 field 见源码 :114） |
| :115 | （desc）`Public value must be native; a query string cannot supply an HDR backend.` | translate | 同上（:115） |
| :116 | `Keyboard layout` | translate | `Public value must be native. us / en-US require explicit synthetic tests and do not install a physical layout.`（`us/en-US` 为 protocol） |
| :117 | `SDK colorScheme` | translate | `Playwright context colorScheme override: light, dark or no-preference. …`（`light/dark/no-preference` 为 protocol） |
| :118 | `Fingerprint perturbation` | translate | `false keeps identity seeds but disables existing perturbations, …` |
| :119 | `Allow third-party cookies` | translate | `Launch-only opt-in, default off. Does not change persisted preferences, SameSite/Secure requirements or site-specific blocks.` |
| :120 | （FakeShadowRoot 行 desc）`Exposes closed author roots through element.shadowRoot, not UA-internal roots. …` | translate | `element.shadowRoot` 为 protocol |
| :121 | `SDK stealth defaults` | translate | `false skips default stealth arguments and persistent seed I/O. …` |
| :122 | （launchMode 行 desc）`SDK launch mode. launchOptions.headless takes precedence if present; no related options are cleared.` | translate | `launchOptions.headless` 为 protocol |
| :123 | `Start maximized` | translate | `Explicit SDK window behavior. Existing --start-maximized, --window-size or --window-position retain their own precedence.` |
| :124 | `DevTools Runtime suppression` | translate | `SDK-documented presence switch. Can break console/binding-based automation. …` |
| :125 | `Canvas Bridge endpoint` | translate | `Forwards Canvas/WebGL operations to the configured endpoint. Requires a compatible backend and has sandbox/security implications.` |
| :126 | `Unsafe Canvas Bridge opt-in` | translate | `SDK-documented presence switch. Bridge renderer processes lose their sandbox. Enable only when you accept that boundary.` |

### G3. 校验 / 冲突提示（`:139-310`，key 模式 `chromix.validation.*`）

| 位置 | 当前字符串 | 分类 | 备注 |
|---|---|---|---|
| :139 | `options.args must be an array of strings. It is preserved; fix it in SDK JSON before editing flags.` | translate | `options.args`/SDK JSON 为 protocol |
| :234 | `Unknown field parameter.` | translate | — |
| :241 | `` `Line ${index + 1}: use one --flag or --flag=value per line, without shell quotes around the argument.` `` | translate（`{{line}}`） | flag 写法为 protocol |
| :254 | `` `Use a nonzero decimal uint64 (1–${UINT64_MAX}), a bare flag, or off.` `` | translate | `bare flag`/`off` 为 protocol 拼写 |
| :257 | `Use a public boolean spelling: true/1/on/enable/enabled or false/0/off/disable/disabled.` | translate | 拼写表为 protocol |
| :259 | `This documented opt-in uses a bare presence switch; remove it to leave it unset.` | translate | — |
| :262-267 | `` `Use a decimal ${min===0 ? "greater than 0" : …} and at most ${max}.` ``（多行模板） | translate | min/max 为数据 |
| :270 | `Use a numeric version with at most four uint32 components.` | translate | — |
| :271 | `Brand major version must be a positive int32.` | translate | — |
| :277 | `` `Public values: ${choices…}, …` ``（含 `(explicit empty = disabled)` 空选项名） | translate | choices 值本身为 protocol |
| :288 | `` `${key}.args exists. The SDK can select this argument array instead of options.args. …` `` | translate | — |
| :290 | `devicePool is measured-device mode and rejects ordinary field/launch/context overrides. …` | translate | devicePool 为 protocol |
| :291 | `SDK viewport is explicitly configured, independently of raw screen/viewport flags. …` | translate | `viewport:null` 等为 protocol |
| :297 | `` `${raw.name} takes precedence over ${field.flag}; both are retained.` `` + 条件后缀 ` The SDK rejects conflicting geometry aliases in synthetic mode.` | translate | flag 名为 protocol |
| :300 | `` `SDK ${name} overwrites --fingerprint-${name} at launch; a raw --uxr-… still wins. …` `` | translate | flag 名为 protocol |
| :304 | `fingerprint=off disables persona overrides at launch; explicit regional settings, cookie and FakeShadowRoot opt-ins remain independent. …` | translate | — |
| :306 | `The SDK rejects this pointer / hover / max-touch-points combination. …` | translate | — |
| :307 | `Restricted font policy requires 1–256 installed font families or a fontsDir containing parseable real fonts.` | translate | — |
| :309 | `Supply both --uxr-viewport-width and --uxr-viewport-height; the SDK rejects incomplete synthetic viewport pairs.` | translate | — |
| :310 | `Explicit WebGL vendor/renderer presentation requires compatibility GPU mode. …` | translate | — |

### G4. 选项值 / 单位（keep-original，不进字典，实施时原样保留）

- `:62,76-78` 单位 `DIP`；`:73` `GB`；`:99` `MiB`；`:105` `ms`；`:72` `cores`；`:79` `ratio`。
- `:66` choices `windows / Win32 / macos / MacIntel / linux / Linux x86_64`；`:67` `Chrome / Edge / Opera / Vivaldi`；`:86` `native / compatibility`；`:92` `native / restricted`；`:103` `native / isolated`；`:109` `fine / coarse / none`；`:106` `native / disabled / ""`；`:117` `light / dark / no-preference`。
- `:274` 别名匹配小写 `google chrome / microsoft edge`（匹配逻辑用）；`:104` `noise=false`；`:97-98,101` flag 名 `--uxr-*`、`--lang`、`--fingerprint-locale`。
- `:62` 起 `dip = { unit: "DIP", … }` 等代码常量（非渲染）。

### G5. Chromix 表单组件文案

| 位置 | 当前字符串 | 建议 key | 分类 | 备注 |
|---|---|---|---|---|
| ChromixFingerprintForm.tsx:39,41 | `Chromix fingerprint parameters`（aria-label + 标题） | `chromix.form.title` | translate | Chromix 为 brand；建议“Chromix 指纹参数” |
| ChromixFingerprintForm.tsx:42 | `Every field starts unset. Changes update SDK options for the outer Save action; they do not launch a browser. Numeric flags remain exact strings, including uint64 seeds. Unknown arguments and SDK properties are preserved.` | `chromix.form.intro` | translate | — |
| ChromixFingerprintForm.tsx:46 | `This is the public source contract, not proof that an installed binary supports it. Use a matching rebuilt browser and SDK. Raw aliases retain SDK precedence; editing one parameter never clears its other aliases or unrelated settings.` | `chromix.form.contractNote` | translate | — |
| ChromixFingerprintForm.tsx:51-53 | `Public flags ↗` / `Node SDK ↗` / `Backend policy ↗`（外链） | `chromix.form.docLinks.*` | translate | `↗` 保留；链接 URL 为 protocol |
| ChromixFingerprintForm.tsx:58-59 | `Find a parameter` / placeholder `Name, --flag, SDK property or description` | `chromix.form.search` / `.searchPlaceholder` | translate | flag 示例为 protocol |
| ChromixFingerprintForm.tsx:63 | `` `Configuration notes (${notices.length})` ``（summary） | `chromix.form.notesCount`，`{{n}}` | translate | notices 内容为动态校验串（G3） |
| ChromixFingerprintForm.tsx:76 | `` `{configured} set · {groupFields.length} fields` `` | `chromix.form.configuredCount` | translate | 数字为数据 |
| ChromixFingerprintForm.tsx:85 | `No matching parameters.`（空搜索） | `chromix.form.noMatch` | translate | 建议“没有匹配的参数。” |
| ChromixFingerprintForm.tsx:87-91 | `Additional SDK objects (including viewport, contextOptions and launchOptions) remain available in the outer SDK JSON editor. Warnings do not normalize or delete stored values. The matching SDK performs final launch validation.` | `chromix.form.footerNote` | translate | 对象名为 protocol |
| ChromixFingerprintForm.tsx:112 | `Unset`（清除单参按钮）+ title `Remove only the selected parameter` | `chromix.field.unset` / `.unsetTitle` | translate | 建议“未设置 / 仅移除所选参数” |
| ChromixFingerprintForm.tsx:117 | `` `${field.label} parameter to edit` ``（aria-label 模板） | `chromix.field.editAria`，`{{label}}` | translate | label 为已翻译字段名 |
| ChromixFingerprintForm.tsx:124 | ` · public` / ` · raw / higher priority`（来源后缀） | `chromix.field.srcPublic` / `.srcRaw` | translate | 建议“ · 公开 / · 原始（更高优先级）” |
| ChromixFingerprintForm.tsx:138 | `Not set (SDK / engine default)`（下拉选项） | `chromix.field.notSet` | translate | 建议“未设置（SDK / 引擎默认）” |
| ChromixFingerprintForm.tsx:139 | `null (stored)`（有 sdkKey 时）/ `Bare flag (present, no value)`（无 sdkKey 时） | `chromix.field.nullStored` / `.bareFlag` | translate | 建议“null（已存储）/ 裸 flag（存在，无值）” |
| ChromixFingerprintForm.tsx:140 | `Explicit empty (=) — disabled` | `chromix.field.explicitEmptyDisabled` | translate | 建议“显式空（=）——已禁用” |
| ChromixFingerprintForm.tsx:141 | `` `{stored} (stored, preserved)` `` 前缀选项 + `Explicit empty (=)` | `chromix.field.storedValue` / `.explicitEmpty` | translate | stored 值为 protocol 数据 |
| ChromixFingerprintForm.tsx:153 | placeholder 三态：`Bare flag (no value)` / `Explicit empty value (=)` / `Not set` | `chromix.field.phBare` / `.phEmpty` / `.phUnset` | translate | — |
| ChromixFingerprintForm.tsx:161-162 | `Bare flag / generate seed` / `Set off`（seed 快捷按钮） | `chromix.field.bareOrSeed` / `.setOff` | translate | — |
| ChromixFingerprintForm.tsx:169 | `Editing the raw alias. Public aliases are preserved and can be selected separately.` | `chromix.field.rawAliasNote` | translate | — |
| ChromixFingerprintForm.tsx:185 | `An argument contains a line break. Use SDK JSON to preserve that value exactly.` | `chromix.validation.lineBreak` | translate | — |
| ChromixFingerprintForm.tsx:201 | `Raw options.args — one flag per line` | `chromix.rawArgs.title` | translate | — |
| ChromixFingerprintForm.tsx:202 | `Complete options.args array, including unknown flags and duplicates. Use --key=value (spaces inside the value are literal); no shell splitting or automatic alias cleanup. Apply updates options only; the outer Save still persists it.` | `chromix.rawArgs.desc` | translate | — |
| ChromixFingerprintForm.tsx:206-210 | `Browser arguments` / `${controlClass} resize-y`（样式，剔除）/ `Apply raw args` / `Discard raw draft / reload current args` | `chromix.rawArgs.browserArgs` / `.apply` / `.discard` | translate | 建议“浏览器参数 / 应用原始参数 / 丢弃草稿并重载当前参数” |
| ChromixProfileOptions.tsx:22 | `SDK options must be a JSON object.`（校验） | `chromix.profileOptions.jsonObject` | translate | — |
| ChromixProfileOptions.tsx:33 | `These settings apply when Chromix is selected in Settings → Browser engine. Each profile stores its own SDK options. Profile keys override global SDK defaults; arrays and nested objects replace the matching global value. Empty options inherit global defaults. The other engines keep using the original Fingerprint section.` | `chromix.profileOptions.intro` | translate | `Settings → Browser engine` 内导航引用，随 nav/settings key 翻译 |
| ChromixProfileOptions.tsx:41 | `Complete profile SDK options (JSON)`（标题） | `chromix.profileOptions.title` | translate | — |
| ChromixProfileOptions.tsx:42 | `Edit any JSON-serializable SDK option, including args, contextOptions, launchOptions, viewport and humanConfig. Apply JSON to update the form before saving. The CDP sidecar rejects the separate measured-device devicePool mode.` | `chromix.profileOptions.desc` | translate | 键名/devicePool 为 protocol |
| ChromixProfileOptions.tsx:48 | `Profile Chromix SDK options`（aria-label） | `chromix.profileOptions.aria` | translate | — |
| ChromixProfileOptions.tsx:56 | `Apply profile JSON`（按钮） | `chromix.profileOptions.apply` | translate | 建议“应用配置 JSON” |

### G6. Chromix 全局设置编辑器（`screens/ChromixSettingsEditor.tsx`，`chromix.editor.*`）

| 位置 | 当前字符串 | 建议 key | 分类 | 备注 |
|---|---|---|---|---|
| :27 | `Custom user agent — replace for your browser`（SDK options 示例 JSON 内的 userAgent 示例值，非 label） | — | keep-original | 示例载荷值，用户替换；随示例 JSON 保留原文 |
| :57,60 | 校验模板：`` `${label}: invalid JSON. ${error…}` `` / `` `${label} must be a top-level JSON object, not an array, null, or primitive.` `` | `chromix.editor.invalidJson` / `.mustBeObject`，`{{label}}` | translate | error 为 external-raw |
| :78,94 | `SDK options`（小节 label ×2） | `chromix.editor.sdkOptions` | translate | 建议“SDK 选项” |
| :92,149 | `Enter a Node.js executable path or node.` / `Default: node (from PATH). Use an absolute executable path if the desktop app cannot find Node.js.`（Node 说明） | `chromix.editor.nodeHint` / `.nodeDefault` | translate | `node`/PATH 为 protocol |
| :99 | `Environment`（小节 label） | `chromix.editor.environment` | translate | 建议“环境变量” |
| :101 | `Environment values must all be strings (including numbers and booleans, written in quotes).` | `chromix.editor.envHint` | translate | — |
| :122 | `` `Could not save Chromix settings: ${error…}` ``（toast） | `errors.chromixSaveFailed` | translate | — |
| :129-130 | aria `Chromix configuration` + 导语 `Global settings for all Chromix profiles. Save explicitly, then restart the app to apply changes to browser startup. Switching engines does not erase this configuration.` | `chromix.editor.title` / `.intro` | translate | — |
| :136 | `Node.js executable`（label） | `chromix.editor.nodeLabel` | translate | 建议“Node.js 可执行文件” |
| :142 | placeholder `node` | — | protocol | 可执行文件名 |
| :155,166,186 | 小节标题：`Global fingerprint parameters` / `SDK options JSON` / `Environment JSON` | `chromix.editor.sections.*`（3 key） | translate | JSON 键名保留 |
| :162 | `Correct SDK options JSON to edit the structured fingerprint fields. Your draft is preserved.`（JSON 非法时提示） | `chromix.editor.fixJsonHint` | translate | — |
| :207,210 | 保存按钮/状态：`Saving…` / `Save Chromix settings` / `Saved. Restart the app to apply.` / `Unsaved changes — save before leaving this page.` / `No unsaved changes.` | `chromix.editor.save.*`（5 key） | translate | — |
| :216,221,227 | 边界说明小节：`Automation boundary:` / `Measured devices:` / `Host controls:` | `chromix.editor.boundaryLabels.*`（3 key） | translate | — |
| :216 | `SDK humanize only affects SDK Playwright page objects. Existing MCP/CDP actions still use Cloaksession's current driver, not the SDK's humanized mouse or keyboard methods.` | `chromix.editor.humanizeNote` | translate | Playwright/MCP/CDP 为 brand/protocol |
| :221 | `devicePool requires the matching Python SDK and matching browser binary through a separate measured-device entry point. Cloaksession's CDP sidecar rejects devicePool and measured mode because that entry point excludes host CDP overrides. The example below is for direct SDK use only.` | `chromix.editor.devicePoolNote` | translate | devicePool/CDP 为 protocol |
| :227 | `Cloaksession supplies profile storage by default and reserves debugging arguments, including --remote-debugging-port, --remote-debugging-address and --remote-debugging-pipe. Do not override them in args, launchOptions or contextOptions.` | `chromix.editor.reservedArgsNote` | translate | flag/键名为 protocol |
| :231 | `Official Chromix Node SDK README ↗`（外链） | `chromix.editor.sdkReadme` | translate | 链接 URL 为 protocol |
| :236-237 | `Complete SDK JSON examples` + 说明 `Reference examples, not defaults or a field whitelist. Replace paths, credentials and persona values before use. Omit userDataDir to keep Cloaksession's per-profile storage; an explicit path overrides it and must not be shared by concurrent profiles. Availability depends on the installed SDK and matching native binary; consult the official README above.` | `chromix.editor.examplesTitle` / `.examplesNote` | translate | 键名（userDataDir）为 protocol |
| :243-245 | 三个示例标题：`SDK options example` / `Environment example` / `devicePool options example (separate measured-device mode)` | `chromix.editor.exampleTitles.*`（3 key） | translate | 示例 JSON 载荷本身为 protocol，不进字典 |
| :21 | 默认值 `America/New_York`（tz 默认） | — | protocol | IANA tzid 数据 |

## H. 扩展（`extensions.*` + 第三方目录 `catalog.*`）

| 位置 | 当前字符串 | 建议 key | 分类 | 备注 |
|---|---|---|---|---|
| ExtensionsSection.tsx:18 | `Attach existing`（分区标题）+ 代码样例 `profiles.create({ extensions })` | `extensions.attachExisting` / —（样例） | translate / protocol | 样例为 MCP/JSON 协议示例，保留（PRD 出范围） |
| ExtensionsSection.tsx:60,143 | `Add to Cloaksession`（说明文 + companion 按钮原文引用：`Or open the Chrome Web Store inside this profile and click <b>Add to Cloaksession</b>.`） | — | protocol | companion 例外（§S），保持英文 |
| ExtensionsSection.tsx:268 | `Hide your extensions` / `Attach from your profiles`（折叠开关二态） | `extensions.hideMine` / `.attachFromProfiles` | translate | 建议“隐藏我的扩展 / 从我的配置中附加” |
| ExtensionsSection.tsx:278 | `No other extensions in your library yet. Add one above and it becomes attachable to future profiles.` | `extensions.libraryEmpty` | translate | — |
| ExtensionsSection.tsx:303 | `Extensions load on first launch. Login/state is not copied — attached extensions start fresh in this profile.` | `extensions.freshNote` | translate | — |
| ExtensionsSection.tsx:378 | `Chrome Web Store URL or extension ID`（placeholder） | `extensions.urlOrIdPlaceholder` | translate | Chrome Web Store 为 brand 保留 |
| ExtensionsSection.tsx:399 | `Add .crx / .zip…`（文件选择按钮） | `extensions.addFile` | translate | `.crx/.zip` 为 protocol |
| ExtensionsSection.tsx:407 | `Add folder…`（文件夹选择按钮） | `extensions.addFolder` | translate | — |
| ExtensionsSection.tsx:429 | `Hide catalog` / `Browse catalog`（目录开关二态） | `extensions.hideCatalog` / `.browseCatalog` | translate | — |
| ExtensionCatalog.tsx / `data/extensionCatalog.ts:2` | 注释中的 “Discover”（`In-app "Discover" picker…`） | —（注释） | — | 非渲染文案；目录入口为 ExtensionsSection 的 Browse/Hide 开关；无 `catalog.discover` key |
| ExtensionCatalog.tsx:21 | `Added`（已添加徽标） | `catalog.added` | translate | 建议“已添加” |
| ExtensionCatalog.tsx:41 | `The extension catalog is empty in this build. Add extensions by Chrome Web Store URL or ID above.` | `catalog.empty` | translate | — |
| ExtensionCatalog.tsx:57 | `Search the catalog…`（placeholder） | `catalog.searchPlaceholder` | translate | 建议“搜索目录…” |
| ExtensionCatalog.tsx:63 | `No matches.`（空搜索） | `catalog.noMatch` | translate | CommandPalette:245 `No matches. Press … to create a new profile.` 另见 §N |
| `data/extensionCatalog.ts:28-34` | 分类：`Ad blocking` / `Privacy & anti-track` / `Productivity & tabs` / `Passwords & wallets` / `Developer tools` / `Utilities` / `Social & shopping` | `catalog.categories.*`（7 key） | translate | 应用自有分类体系；建议“广告拦截 / 隐私与反追踪 / 效率与标签页 / 密码与钱包 / 开发者工具 / 实用工具 / 社交与购物” |
| `data/extensionCatalog.ts:44-86` | 全部第三方条目名与描述（uBlock Origin Lite … Keepa，约 25 条，见源码） | — | external | 第三方标题/说明/发布说明保留原文（设计已定）；不进字典 |
| `data/extensionCatalogIcons.ts` | base64 图标表（键为 32 位扩展 ID） | — | protocol | 纯数据，无文案 |
| `ui/src/components/profile/ExtIcon.tsx:59` | `alt=""`（装饰图标） | — | keep-original | 装饰性空 alt，保留 |
| `ui/src/components/atoms/Cube.tsx:22` | `alt="Cloaksession"`（logo） | — | brand | — |

## I. MCP 面板（`mcp.*`）

| 位置 | 当前字符串 | 建议 key | 分类 | 备注 |
|---|---|---|---|---|
| McpPanel.tsx:19,179 | `Connect an agent`（标题） | `mcp.connectAgent` | translate | 建议“连接 AI 智能体” |
| McpPanel.tsx:31 | `MCP`（tab/标题缩写） | — | keep-original | 同 nav.mcp |
| McpPanel.tsx:34 | `Drive your profiles from an AI agent. Point any MCP client (Claude Desktop, Cursor, Cline, …) at Cloaksession, then launch a profile and let the agent open tabs, click, type, and read pages through it — every tool call streams into the feed below.` | `mcp.hero` | translate | 客户端/品牌名保留 |
| McpPanel.tsx:44 | `Live tool calls`（feed 标题） | `mcp.liveCalls` | translate | 建议“实时工具调用” |
| McpPanel.tsx:58 | `No agent calls yet`（feed 空态） | `mcp.feedEmpty` | translate | ActivityDrawer:64 另有一版 `No MCP calls yet. Connect an agent (see the MCP tab) and its tool calls stream here.` → `mcp.feedEmptyLong`（translate，“MCP tab”引用 nav key） |
| McpPanel.tsx:59 | `Connect a client above, launch a profile, and every MCP tool call the agent makes shows up here — with sanitized arguments, outcome, and duration.` | `mcp.feedHint` | translate | — |
| McpPanel.tsx:102,110,120,145-147 | `Bearer ${bearer}` / `Authorization: Bearer …`（header 样例） | — | protocol | HTTP 鉴权方案，保留 |
| McpPanel.tsx:110 | `[mcp_servers.multizen]\nurl = "${httpUrl}"\nhttp_headers = { Authorization = "Bearer ${bearer}" }` 等三段客户端配置样例（:145-147） | — | protocol | TOML/JSON 协议样例，键与结构保留（PRD 出范围） |
| McpPanel.tsx:135-150 | 粘贴给 LLM 的英文引导全文（含 8 个 lit：`I'm using the Cloaksession browser…` … `…prefixed "multizen." — I'll launch a profile…`） | `mcp.llmPrompt.*`（按段分 key，`{{httpUrl}}`/`{{sseUrl}}`/`{{bearer}}` 为变量） | translate | 正文译中；`multizen.` 工具前缀、URL、header 名、客户端名保留；`multizen` 服务器名保留 |
| McpPanel.tsx:183,185 | `server off` / `listening`（状态 pill；`194` 处 `Auto-start MCP HTTP transport` 为开关 label，见下） | `mcp.serverOff` / `.listening` | translate | 建议“服务未启动 / 监听中” |
| McpPanel.tsx:193 | `Auto-start MCP HTTP transport`（Settings 内开关；Settings.tsx:190 完整版 `Auto-start MCP HTTP transport on app launch`） | `settings.mcp.autostart` | translate | 以 Settings 完整版为准，McpPanel 内联引用 |
| McpPanel.tsx:194-260 | `The MCP server is disabled. Turn on {Auto-start…} in {Settings} to get a connection endpoint.`（off 态整段） | `mcp.disabledHint` | translate | 内嵌 settings key 引用 |
| McpPanel.tsx:200 | `Endpoint (Streamable HTTP)`（小节标题） | `mcp.endpoint` | translate | 传输名保留；建议“端点（Streamable HTTP）” |
| McpPanel.tsx:205-207 | `Legacy HTTP+SSE endpoint (older clients): {sseUrl}` | `mcp.legacyEndpoint`，`{{url}}` | translate | 建议“旧版 HTTP+SSE 端点（老客户端）：{{url}}” |
| McpPanel.tsx:210 | `Auth token`（小节标题） | `mcp.authToken` | translate | 建议“鉴权令牌” |
| McpPanel.tsx:214 | `Sent as {Authorization: Bearer …} — the configs below already include it. Keep it secret; anyone with it can drive your profiles.` | `mcp.tokenHint` | translate | header 样例保留 |
| McpPanel.tsx:215 | `Authorization: Bearer …`（行内 code） | — | protocol | — |
| McpPanel.tsx:222-224 | 三个 Step：`Copy the config for your client below into its MCP config.` / `Reload / restart the client so it connects.` / `Launch a profile here, and let the agent drive it.` | `mcp.steps.*`（3 key） | translate | — |
| McpPanel.tsx:230,244,250 | 配置分组标题：`Codex CLI — ~/.codex/config.toml` / `JSON URL clients — Cursor, Cline, Continue` / `Stdio clients — Claude Desktop` | `mcp.configGroups.*`（3 key） | translate | 路径/客户端名保留 |
| McpPanel.tsx:237,257 | 行内 `url`（code） | — | protocol | 配置键 |
| McpPanel.tsx:239 | `Stdio clients`（行内引用） | `mcp.stdioClients` | translate | 建议“Stdio 客户端” |
| McpPanel.tsx:234-236 | `The {url} field needs a Codex build with streamable-HTTP MCP support. On older Codex (stdio only), use the {Stdio clients} config below instead.` | `mcp.codexNote` | translate | 键名/产品名保留 |
| McpPanel.tsx:259 | `mcp-remote`（code）+ `(needs Node). Change the port in {Settings}.` | `mcp.stdioNote` | translate | 工具名/Settings 引用保留 |
| McpPanel.tsx:362,399 | `Copy for LLM`（按钮二态其一） | `mcp.copyForLlm` | translate | 建议“复制给 LLM” |
| McpPanel.tsx:383 | title `Copy a prompt you can paste into Claude Code / Cursor to set up the connection` | `mcp.copyForLlm.title` | translate | 品牌名保留 |
| McpPanel.tsx:429 | `multizen.`（工具前缀说明行内） | — | protocol | MCP 工具命名空间 |
| McpPanel.tsx:434 | `` ` · ${event.durationMs}ms` ``（时长后缀） | — | keep-original | 数字格式 |
| McpPanel.tsx:452-454 | `ok` / `live` / `error`（状态 pill；与 ActivityDrawer:140-142 同词） | `activity.status.*`（复用，见 §J） | translate | 建议“成功 / 进行中 / 失败”；实施时统一 |
| McpPanel.tsx:19（`Connect an agent`按钮 title？） | 以表格首行为准 | — | — | — |

## J. 活动抽屉（`activity.*`）

| 位置 | 当前字符串 | 建议 key | 分类 | 备注 |
|---|---|---|---|---|
| ActivityDrawer.tsx:44 | `MCP activity`（标题） | `activity.title` | translate | 建议“MCP 活动” |
| ActivityDrawer.tsx:46,48 | `` `{liveCount} live` `` / `` `{events.length} calls` ``（pill 计数） | `activity.liveCount` / `.callsCount`，`{{n}}` | translate | 建议“{{n}} 进行中 / {{n}} 次调用” |
| ActivityDrawer.tsx:50 | `last 5 min`（时间窗） | `activity.last5min` | translate | 建议“近 5 分钟” |
| ActivityDrawer.tsx:81-85 | 表头：`Time` / `Tool` / `Summary` / `Dur` / `Status` | `activity.cols.*`（5 key） | translate | 建议“时间 / 工具 / 摘要 / 时长 / 状态” |
| ActivityDrawer.tsx:106 | `` `${k}=${…}` ``（参数渲染 `k=v`） | — | protocol | 参数原样展示 |
| ActivityDrawer.tsx:124 | `multizen.`（工具名前缀截断显示） | — | protocol | 命名空间 |
| ActivityDrawer.tsx:140-142 | `ok` / `live` / `error`（状态 pill） | `activity.status.ok` / `.live` / `.error` | translate | ProfileRow:231/ProfileTile:342 的 `error`、ProfileRow:234/Tile:343 的 `idle`、Row:225/Tile:341 的 `running`、Tile:340/Constellation:25 的 `ai-driven` 统一到 `activity.status.*`＋`profile.state.*`（建议：成功/进行中/失败/空闲/运行中/AI 驱动） |
| ActivityDrawer.tsx 未命名 | 活动事件摘要/参数原文（外部工具回吐） | — | external | 外部任意文本不翻译（设计已定） |

## K. 设置页（`settings.*`）

| 位置 | 当前字符串 | 建议 key | 分类 | 备注 |
|---|---|---|---|---|
| Settings.tsx:66 | `` `Could not save settings: ${String(error)}` ``（toast） | `errors.settingsSaveFailed`，`{{error}}` | translate | error 为 external-raw；保存失败不冒充成功（PRD AC5） |
| Settings.tsx:100,102 | `Settings`（aria-label + 页标题） | `nav.settings`（复用） | translate | — |
| Settings.tsx:103 | `MCP server, browser startup, archives and build info. Configuration is stored locally. Chromix SDK downloads and optional GeoIP lookups follow your SDK settings.`（页导语） | `settings.intro` | translate | — |
| Settings.tsx:112-113 | `MCP server` + desc `Local HTTP transport that Cursor / Claude Desktop / Cline / any MCP client connects to. Requires the auth token below. The MCP tab has ready-to-paste client configs.` | `settings.mcp.title` / `.desc` | translate | 客户端名保留 |
| Settings.tsx:117 | `` `running on :${settings.mcpHttpPort}` ``（运行态后缀） | `settings.mcp.runningOn`，`{{port}}` | translate | 建议“运行于 :{{port}}” |
| Settings.tsx:149 | `Authorization: Bearer <token>`（占位样例行） | — | protocol | header 样例 |
| Settings.tsx:190 | `Auto-start MCP HTTP transport on app launch`（开关） | `settings.mcp.autostart` | translate | 建议“随应用启动自动启动 MCP HTTP 传输” |
| Settings.tsx:197-198 | `Browser engine` + desc `Global startup setting. Restart the app to apply; switching engines keeps each engine's saved configuration.` | `settings.engine.title` / `.desc` | translate | 建议“浏览器引擎 / 全局启动设置，重启应用生效；切换引擎保留各自已存配置。” |
| Settings.tsx:204-219 | 引擎选项卡（CloakBrowser / Chromix / Chrome for Testing，`selected` pill :219） | `settings.engine.options.*` + `settings.engine.selected` | translate（选项描述）/ brand（引擎名） | 引擎名永不翻译；`selected` 建议“已选”；选项描述见 :405-415 |
| Settings.tsx:233-234 | `Chromix SDK configuration` + desc `Full SDK JSON, Node.js runtime and environment overrides. Changes apply after the next app restart.` | `settings.chromix.title` / `.desc` | translate | — |
| Settings.tsx:249-252 | `Browser binary` + desc 二态（chromix 引擎版 / 其他引擎版，`Row desc={cond ? … : …}` 条件段落，原文以源码为准） | `settings.binary.title` / `.descChromix` / `.descDefault` | translate | 路径/下载器名为 protocol |
| Settings.tsx:256,260 | aria `Browser binary path` + placeholder 二态 `Default (Chromix SDK resolution)` / `Default (auto-download)` | `settings.binary.pathAria` / `.phChromix` / `.phDefault` | translate | — |
| Settings.tsx:292-293 | 复选框二态：`Skip Chromix SDK auto-download (use a local or SDK-cached binary)` / `Skip auto-download (use cached binary or the custom path above)` | `settings.binary.skipDownload` / `.skipDownloadLegacy` | translate | — |
| Settings.tsx:297-298 | 配套说明二态（离线启动长段落） | `settings.binary.offlineNote` / `.offlineNoteLegacy` | translate | `CLOAKBROWSER_BINARY_PATH`、SDK 缓存等 protocol 保留 |
| Settings.tsx:304-305 | `Archives` + desc `.mzar files are encrypted bundles of profiles — cookies, login state, fingerprints, notes — protected with a passphrase you set at export time.` | `settings.archives.title` / `.desc` | translate | `.mzar` 为 protocol |
| Settings.tsx:312 | `Import .mzar archive`（按钮；onImport 回调） | `archive.importAction` | translate | 与 palette:73 同 key |
| Settings.tsx:320 | `Read archive format docs`（外链按钮，URL 为 protocol） | `settings.archives.readDocs` | translate | 建议“阅读归档格式文档” |
| Settings.tsx:328,331-332 | `Updates` + desc 二态（macOS :331 / 其他 :332） | `settings.updates.title` / `.descMacos` / `.descDefault` | translate | macOS 为 keep-original（OS 名） |
| Settings.tsx:345 | `Check for updates`（按钮） | `update.check` | translate | 建议“检查更新” |
| Settings.tsx:347 | `{updateLabel(updateStatus)}`（状态行：`You're on the latest version` :424 / `v${version} available` :426 / `Downloading v${version}… ${percent}%` :428 / `v${version} ready — restart to update` :430 / `Check failed: ${message}` :432） | `update.status.*`（5 key） | translate | 版本号/百分比为数据；message 为 external-raw |
| Settings.tsx:350 | `Last checked: {relativeTime(…) | "never"}`（:45 `last checked` label、`never` 见 §Q） | `update.lastChecked` | translate | 时间格式化走应用语言（AC6） |
| Settings.tsx:359 | `Automatically check for updates`（复选框） | `settings.updates.autoCheck` | translate | 建议“自动检查更新” |
| Settings.tsx:366-367 | `Anonymous usage` + desc `Off by default. Help gauge how many people run Cloaksession.` | `settings.telemetry.title` / `.desc` | translate | — |
| Settings.tsx:375,378,380-383 | `Send an anonymous daily heartbeat`（开关）+ `When on, sends once a day: app version, OS family, and a random single-use token — <b>no</b> account, <b>no</b> persistent ID, and your IP is …` + `<code>MULTIZEN_NO_TELEMETRY=1</code> to force it off.` | `settings.telemetry.heartbeat` / `.heartbeatDesc` | translate（env var 除外） | `MULTIZEN_NO_TELEMETRY=1` 为 protocol；`no/account` 为强调片段，随整句翻译 |
| Settings.tsx:387 | `About`（Row title） | `settings.about.title` | translate | 建议“关于” |
| Settings.tsx:404-415 | 引擎介绍：`CloakBrowser`（:404）+ `Source-patched Chromium from CloakHQ releases. Primary runtime.`（:405）/ `Chromix`（:409）+ `Chromix Node SDK with full JSON options and environment configuration.`（:410）/ `Chrome for Testing`（:414）+ `Compatibility fallback using Google's official automation build.`（:415） | `settings.engine.cards.*` | brand（引擎名）/ translate（描述） | 引擎名永不翻译 |
| Settings.tsx:250（desc 字段值） | `chromix`（Row desc 传入的引擎 id？） | — | protocol | 引擎 id 透传 |

## L. 更新横幅与原生更新提示（`update.*`）

| 位置 | 当前字符串 | 建议 key | 分类 | 备注 |
|---|---|---|---|---|
| UpdateBanner.tsx:46 | `` `Downloading Cloaksession {version}…` ``（下载中横幅；另有 :9 `Restart to update` 主按钮） | `update.downloading`，`{{version}}` | translate | Cloaksession 为 brand |
| UpdateBanner.tsx:9 | `Restart to update` | `update.restart` | translate | 建议“重启以更新” |
| UpdateBanner.tsx:10,72-79 | `Download`（macOS 可用态主按钮）+ `Cloaksession {version} is available.` | `update.download` / `update.availableBanner` | translate | 后者建议“Cloaksession {{version}} 可用。” |
| UpdateBanner.tsx:55-64 | `Cloaksession {version} is ready to install.` + `Restart now` | `update.readyBanner` / `update.restartNow` | translate | 建议“… 已就绪，可安装。/ 立即重启” |
| `commands/update.rs:397-398` | 原生对话框：title `Download update` + message `` `Download Cloaksession {version} from:\n{url}` `` | `update.nativeDownloadTitle` / `.nativeDownloadBody` | translate | Rust 侧小字典（设计）；URL 为 protocol；Cloaksession 为 brand |
| `commands/update.rs:267` | `Download client build failed: {e}`（StatusEvent message） | `update.statusFailed` | translate | e 为 external-raw |
| `commands/update.rs:277,285` | `Download failed: {e}` / `Download HTTP {status}` | `update.downloadFailed` / `.downloadHttp` | translate | 同上 |
| `commands/update.rs:301,313,328` | `Cannot create temp file: {e}` / `Write failed: {e}` / `Download stream error: {e}` | `update.ioFailed.*` | translate | 同上 |
| `commands/update.rs:358,369` | `No downloaded installer to install` / `Installer not found: {path}` | `update.noInstaller` / `.installerMissing` | translate | path 为 user-data |
| `commands/update.rs:176,183,186,199` | `HTTP client: {e}` / `GitHub API request failed: {e}` / `GitHub API returned HTTP {status}` / `Failed to parse GitHub release: {e}` | — | external | 底层技术诊断，原文展示 |
| `commands/update.rs:173,260` | `Cloaksession/{version} (update-checker)` 等 UA | — | protocol | HTTP UA |
| `commands/update.rs:296` | `multizen-{version}-setup.exe`（安装包文件名模板） | — | protocol | 文件名 |
| `commands/update.rs:19-20` | `xiaozhou26` / `Cloaksession`（GitHub owner/repo） | — | protocol | URL 组件 |

## M. 引导 / 首屏（`onboarding.*`）

| 位置 | 当前字符串 | 建议 key | 分类 | 备注 |
|---|---|---|---|---|
| FirstRun.tsx:9,194 | `My first profile`（DEFAULT_NAME，预填 + placeholder） | `onboarding.defaultProfileName` | translate | 新用户首个配置的默认名（user-data 默认值，进字典以便中英一致） |
| FirstRun.tsx:104-109 | `A library of isolated browsers.` / `Cookies, login, fingerprint, and proxy isolated per profile. Drive them yourself or via any MCP agent.` | `onboarding.heroTitle` / `.heroSubtitle` | translate | — |
| FirstRun.tsx:113-121 | 平台 chips：`macOS` / `Windows` / `Linux` / `Chromium` / `MCP HTTP` | — | keep-original | OS/引擎/协议名 |
| FirstRun.tsx:140-146 | `Help improve Cloaksession?` / `Optionally send an anonymous daily heartbeat: just app version and OS family. No account, no persistent ID, and your IP is never stored. No profiles, proxies, or browsing — ever. Off unless you enable it; change it anytime in Settings.` | `onboarding.telemetryTitle` / `.telemetryBody` | translate | Settings 引用随 nav key |
| FirstRun.tsx:206 | `Creating…` / `Create profile`（提交按钮二态） | `onboarding.creating` / `.create` | translate | 建议“创建中… / 创建配置” |
| ChromiumBootstrapModal.tsx:65 | `Setting up Cloaksession`（标题） | `onboarding.bootstrap.title` | translate | brand 保留 |
| ChromiumBootstrapModal.tsx:67-75 | 状态机 label：`Looking up the browser runtime` / `Downloading browser runtime` / `Verifying download` / `Installing browser runtime` / `Setup failed` | `onboarding.bootstrap.status.*`（5 key） | translate | — |
| ChromiumBootstrapModal.tsx:84 | `First-run download. About 150-550 MB.`（说明） | `onboarding.bootstrap.firstRunNote` | translate | 数字保留 |
| ChromiumBootstrapModal.tsx:86-94 | 进度行：`Resolving the latest compatible build` / `Runtime ${version}` / `Checking integrity (SHA-256 + macOS code signature)` / `Unpacking runtime ${version}…` / `We could not download the Chromium binary.` | `onboarding.bootstrap.progress.*` | translate | SHA-256/macOS/Chromium 为 protocol/brand/keep |
| ChromiumBootstrapModal.tsx:104-160 | 子状态行：`Preparing download…` / `Resolving latest stable version…` / `Unpacking and verifying signature…` / `Verifying SHA-256 checksum…` | `onboarding.bootstrap.substatus.*` | translate | — |
| ChromiumBootstrapModal.tsx:151,206 | `` `{pct}%` `` / `` `${MB} MB` ``（数字格式） | — | keep-original | 纯数字格式 |
| ChromiumBootstrapModal.tsx:195 | `Retry download`（按钮） | `onboarding.bootstrap.retry` | translate | 建议“重试下载” |

## N. 命令面板（`palette.*`）

| 位置 | 当前字符串 | 建议 key | 分类 | 备注 |
|---|---|---|---|---|
| CommandPalette.tsx:53 | `` `{p.isRunning ? "Open" : "Launch"} · {p.name}` ``（标题模板） | `palette.openItem` / `.launchItem`，`{{name}}` | translate | name 为 user-data；ProfileRow:188 `Launch`、ProfileTile:225 `Launch`/`Launching…`（`palette.launching`）、Row:198/Tile:236 `Stop`（`palette.stop`）、Row:175/Tile:55 `Terminating…`（`palette.terminating`）统一到 palette/profile key |
| CommandPalette.tsx:54 | `` `${p.id.slice(0,12)} · ${p.tags.join(", ") || "no tags"}` ``（副标题；`no tags`） | `palette.noTags` | translate | id/tags 为 user-data；建议“无标签” |
| CommandPalette.tsx:65 | `Create new profile · "`（创建行前缀） | `palette.createNew` | translate | 后接用户输入（user-data） |
| CommandPalette.tsx:67,74 | `Actions`（分组头；:74 第二组同词） | `palette.actions` | translate | 建议“操作” |
| CommandPalette.tsx:55 | `Profiles`（分组头） | `nav.profiles`（复用） | translate | — |
| CommandPalette.tsx:73 | `Import .mzar archive` | `archive.importAction`（复用） | translate | — |
| CommandPalette.tsx:80,88,96 | `Go to Profiles` / `Go to MCP` / `Settings`（导航项；后两者文本为 `Go to MCP`/`Settings`？:96 为 `Settings`） | `palette.goToProfiles` / `.goToMcp` / nav.settings（复用） | translate | 建议“前往浏览器配置 / 前往 MCP / 设置” |
| CommandPalette.tsx:82,90,98 | `Navigate`（hint 三处） | `palette.navigate` | translate | 建议“导航” |
| CommandPalette.tsx:185 | `Search profiles, tags, actions…`（placeholder） | `palette.searchPlaceholder` | translate | 建议“搜索配置、标签、操作…” |
| CommandPalette.tsx:245-246 | `No matches. Press … to create a new profile.`（多段：`No matches. Press` + kbd + `to create a new profile.`） | `palette.noMatchPrefix` / `.noMatchSuffix` | translate | kbd 为 keep-original |

## O. 归档导入 / 导出（`archive.*`）

| 位置 | 当前字符串 | 建议 key | 分类 | 备注 |
|---|---|---|---|---|
| App.tsx:528-529 | `Import profile archive` / `Choose a .mzar file. Provide the passphrase used at export time.` | `archive.importTitle` / `.importDesc` | translate | — |
| App.tsx:530,532 | `Passphrase`（label + placeholder） | `archive.passphrase` / `.passphrasePlaceholder` | translate | 建议“口令” |
| App.tsx:533 | `Choose file & import`（确认按钮） | `archive.chooseAndImport` | translate | 建议“选择文件并导入” |
| App.tsx:540-541 | `Export profile archive` / `Choose a passphrase to encrypt the archive. You'll need it to import this profile elsewhere. Minimum 8 characters.` | `archive.exportTitle` / `.exportDesc` | translate | — |
| App.tsx:542 | `New passphrase`（label） | `archive.newPassphrase` | translate | 建议“新口令” |
| App.tsx:544 | `At least 8 characters`（placeholder） | `archive.passphrasePlaceholderMin` | translate | — |
| App.tsx:545 | `Encrypt & save…`（确认按钮） | `archive.encryptAndSave` | translate | 建议“加密并保存…” |
| App.tsx:557-559 | `Delete this profile?` / `Cookies, login state, and on-disk data will be erased permanently. This cannot be undone.` / `Yes, delete` | `profile.deleteTitle` / `.deleteBody` / `.deleteConfirm` | translate | Row:372 `Delete profile`、Tile:452 `Delete profile` 菜单项 → `profile.deleteMenu`（translate，建议“删除配置”）；Row:368/Tile:448 `Export archive…` → `archive.exportMenu` |
| App.tsx:89 | `Export`（顶栏/菜单导出动作） | `archive.exportAction` | translate | 建议“导出” |
| App.tsx:365,368,372 / Tile:445,448,452 | 菜单项：`Edit profile` / `Export archive…` / `Delete profile` | 见上（复用） | translate | — |
| App.tsx:260？（NewProfileSheet 无；此处无） | — | — | — | — |
| `commands/archive.rs:201` | `Passphrase must be at least 8 characters`（后端 reason） | `archive.passphraseMinLength`（与前端同 key） | translate | 前后端同文，字典统一 |
| `commands/archive.rs:207` | `not_found`（ResultExport reason 码） | — | protocol | 机器码，前端映射展示 |
| `commands/archive.rs:217` | 默认文件名 `profile`（`{default_name}.mzar`） | — | keep-original | 默认文件名，中英保留（备注：实施时可议） |
| `commands/archive.rs:221-222,354` | 原生对话框 filter `Cloaksession archive`（`&["mzar"]`）+ 默认文件名 | `archive.nativeFilter` | translate | Rust 侧小字典；扩展名 `mzar` 为 protocol |
| `commands/archive.rs:260` | 魔数 `MZAR` | — | protocol | 文件格式魔数 |
| `commands/archive.rs:367,370,374` | `File too small` / `Not a Cloaksession archive` / `` `Unsupported archive version {version}` ``（ResultImport reason） | `archive.importReason.*`（3 key） | translate | 经 `errors.importFailed` 模板展示 |
| `commands/archive.rs:392,396,400` | `Wrong passphrase or corrupted archive.` / `Corrupted archive (too small)` / `Corrupted archive (manifest truncated)` | `archive.importReason.*` | translate | 同上 |
| `commands/archive.rs:404,410,415,431,446` | `Corrupted archive (manifest JSON): {e}` / `(file length truncated)` / `(file content truncated)` / `Checksum mismatch: {path}`（×2） | `archive.importReason.*`（操作级）/ path 为 user-data，`{e}` 为 external-raw | translate（模板） | 技术细节部分保留原文（设计：操作级中文 + 原始详情） |
| `commands/archive.rs:105-184,266-307,363-496` | `readdir {dir}: {e}` / `strip_prefix: {e}` / `scrypt: {e}` / `aes key: {e}` / `encrypt: {e}` 等 20+ 底层 `format!` | — | external | 底层诊断，原文展示，不进字典 |

## P. 列表 / 表格 / 卡片（`profile.list.*`）

| 位置 | 当前字符串 | 建议 key | 分类 | 备注 |
|---|---|---|---|---|
| ProfileTable.tsx:24-28 | 表头：`Name` / `Status` / `Tags` / `Last opened` / `Proxy` | `profile.table.*`（5 key） | translate | 建议“名称 / 状态 / 标签 / 上次打开 / 代理” |
| ProfileTile.tsx:199 | `never opened`（未打开过） | `profile.list.neverOpened` | translate | 建议“从未打开” |
| ProfileTile.tsx:312 | `proxy`（卡片 proxy 行 label 片段） | `profile.list.proxyLabel` | translate | 建议“代理” |
| ProfileTile.tsx:340-343 | 状态 pill：`ai-driven` / `running` / `error` / `idle` | 见 §J（统一） | translate | Tile:217 `title={profile.lastTool ?? "ai-driven"}` 中 `ai-driven` 为 fallback 显示，`lastTool` 为 protocol（工具名） |
| ProfileTile.tsx:358 | `multizen.`（AI 驱动标注） | — | protocol | 工具前缀 |
| ProfileRow.tsx:336 / Tile:423 | `More actions`（aria-label，⋯ 菜单） | `profile.list.moreActions` | translate | 建议“更多操作” |
| Constellation.tsx:23-27 | 筛选 chips：`All` / `Running` / `AI-driven` / `Errors` / `Idle` | `profile.filter.*`（5 key） | translate | 与 pills 措辞统一 |
| Constellation.tsx:155 | `All profiles`（标题） | `profile.list.allProfiles` | translate | 建议“全部浏览器配置” |
| Constellation.tsx:158 | `` ` · ${aiCount} driven by Claude` ``（AI 计数后缀） | `profile.list.aiCount`，`{{n}}` | translate | Claude 为 brand |
| Constellation.tsx:180,196 | title `Grid view` / `List view` | `profile.list.gridView` / `.listView` | translate | 建议“网格视图 / 列表视图” |
| Constellation.tsx:206 | `New profile`（新建按钮） | `profile.new.title`（复用） | translate | — |
| Constellation.tsx:301 | aria `搜索环境与账号档案` + placeholder `搜索环境、姓名、快手ID或身份证号` | `profile.list.searchAria` / `.searchPlaceholder` | translate | 已中文，进字典并补 en |
| Constellation.tsx:312 | `No profiles match the current filter.` | `profile.list.noFilterMatch` | translate | 建议“当前筛选下没有匹配的配置。” |
| EmptyState.tsx:36-37,45 | `No profiles yet` / `Each profile is its own browser — cookies, login, fingerprint, proxy. Create one to get started.` / `New profile`（按钮） | `profile.empty.title` / `.body` / `profile.new.title`（复用） | translate | — |
| ProfileRow.tsx:188 | `…`（省略号按钮片段）+ `Launch` | `palette.launch`（复用） | translate | — |
| ProfileTile.tsx:225 | `Launching…` / `Launch`（按钮二态） | `palette.launching` / `.launch` | translate | 建议“启动中… / 启动” |

## Q. 业务账号 / 快手身份 / 主体档案 / OCR（`account.*` / `identity.*` / `subject.*` / `ocr.*`）

说明：本域当前多为硬编码中文，按 PRD 决策纳入字典并补 en（下文 key 均需中英两值）。

### Q1. 业务账号登记（`BusinessAccountSection.tsx`，`account.*`）

| 位置 | 当前字符串 | 建议 key | 分类 | 备注 |
|---|---|---|---|---|
| :14-18 | 类型选项：`快手小店` / `快手直播` / `快手直播伴侣` / `快手子账号` / `磁力金牛` | `account.scope.*`（5 key） | translate | 平台名保留原文（快手/磁力金牛为品牌），en 补译 |
| :94 | `` `无法加载账号登记：${cause…}` ``（toast） | `errors.accountLoadFailed`，`{{cause}}` | translate | cause 为 external-raw |
| :165-166 | toast：`账号登记已保存；这不代表已登录平台。` / `已解绑。登记记录已保留，未清除 Cookie；环境专用用途仍保留。` | `account.savedToast` / `.unboundToast` | translate | — |
| :171-172 | toast：`操作已完成，但最新状态读取失败，请重新加载，不要重复提交：${detail}` / `操作失败：${detail}` | `account.staleToast` / `.failedToast`，`{{detail}}` | translate | — |
| :183,185 | aria + 标题 `业务账号登记` | `account.sectionTitle` | translate | — |
| :186 | `账号信息未验证，仅手工登记类型、别名与平台用户 ID，不代表已登录或免登录，不共享 Cookie。 每个 Profile 同时只能关联一个登记记录，不会自动覆盖、移动或合并账号。`（须知段落） | `account.disclaimer` | translate | — |
| :190 | `本分区需单独保存；切换分区保留草稿，关闭编辑页会丢弃未保存的登记修改。` | `account.saveNote` | translate | — |
| :197-198 | 按钮 `重新加载 / 重试`（title `重新加载状态`）+ 提示 `重新加载会丢弃未保存的登记修改。` | `account.reload` / `.reloadTitle` / `.reloadHint` | translate | — |
| :205,207,209 | 金牛专用三段提示（磁力金牛专用环境 / 已用于快手业务 / 必须独立 Profile） | `account.jinniu.*`（3 key） | translate | — |
| :211 | `绑定、修改和解绑前必须停用 Profile；运行状态仅供提示，最终由后台校验。` | `account.stoppedRequired` | translate | — |
| :220,228-229 | `登记方式` / `新建登记并绑定` / `绑定已有未关联记录` | `account.mode.*`（3 key） | translate | — |
| :233,241,248 | `已有未关联记录` / `请选择未关联记录` / `包含删除环境后保留的登记；已关联其他 Profile 的记录不会出现在这里。` | `account.existing.*`（3 key） | translate | — |
| :244,255 | `（需要独立环境）`（选项后缀 ×2） | `account.needsIsolatedEnv` | translate | — |
| :252,258,260,262,266 | `账号类型` / `账号类型创建后不可修改；…` / `保存后此 Profile 将标记为磁力金牛专用…` / `账号别名（必填）` / `平台ID（可选，手填不代表已登录）` | `account.field.*` | translate | — |
| :270-271 | 提交按钮三态：`保存登记修改` / `绑定已有登记` / `创建并绑定登记` + `解绑登记`（:271） | `account.save.*`（4 key） | translate | — |
| :275,278-279 | `确认解绑登记`（aria）/ `确认解绑` / `取消`（后者复用 common.cancel） | `account.unbindConfirm.aria` / `.confirm` | translate | — |

### Q2. 快手身份（`KuaishouIdentity.tsx` + `KuaishouIdentityProvider.tsx` + `lib/kuaishouIdentity.ts`，`identity.*`）

| 位置 | 当前字符串 | 建议 key | 分类 | 备注 |
|---|---|---|---|---|
| KuaishouIdentity.tsx:18,21-22 | alt `快手平台头像` / aria `平台头像不可用` / fallback 字 `快` | `identity.avatarAlt` / `.avatarUnavailable` / `.avatarFallback` | translate | 平台名保留 |
| KuaishouIdentity.tsx:34 | title 二态：`浏览器已关闭或正在关闭；运行状态最终由后台判定` / `请后台重新检测当前小店页面身份` | `identity.statusTitle.*` | translate | — |
| KuaishouIdentity.tsx:36 | `检测中…` / `重新检测` | `identity.detecting` / `.redetect` | translate | — |
| KuaishouIdentity.tsx:52,54 | `已打开小店登录页，请在浏览器中扫码；此操作不代表已登录。` / `` `小店扫码入口未完成：${…}` `` | `identity.scanOpened` / `identity.scanFailed` | translate | — |
| KuaishouIdentity.tsx:62-63 | title `启动此环境并新开小店扫码页；不改起始页或登录数据，金牛环境不可用` + 按钮二态 `正在打开小店…` / `快手小店扫码` | `identity.openShop.*` | translate | — |
| KuaishouIdentity.tsx:78,113 | ` · 上次识别` / `上次识别` / `本轮识别` | `identity.lastDetected` / `.thisRound` | translate | — |
| KuaishouIdentity.tsx:80 | `` `${nickname} · ${platformUserId}` ``（身份行） | — | user-data | 昵称/ID 为数据 |
| KuaishouIdentity.tsx:87,117,149 | `快手详情`（按钮/标题 ×3） | `identity.details` | translate | — |
| KuaishouIdentity.tsx:101-102 | `刷新检测状态` / `读取 Rust 后台结果 · 不代表已登录或初始化完成` | `identity.refresh` / `.refreshNote` | translate | Rust 为 brand 保留 |
| KuaishouIdentity.tsx:114-116,168 | 冲突/展示三段：`登记冲突：手填平台ID与识别结果不同；…` / `登记冲突：后台报告身份登记冲突，…` / `识别结果仅展示，不会填入或保存到人工登记。` + 168 长段 | `identity.conflict.*` | translate | — |
| KuaishouIdentity.tsx:144,146 | `快手ID已复制` / `复制失败，请手动选择快手ID复制` | `identity.copied` / `.copyFailed` | translate | — |
| KuaishouIdentity.tsx:150,156 | aria `快手身份检测详情` / ` · 以下为上次识别信息` | `identity.detailsAria` / `.lastInfoSuffix` | translate | — |
| KuaishouIdentity.tsx:161-164 | 明细行：`快手ID` / `检测时间` / `最后识别时间` / `检测来源` + 值 `Rust 后台 · 小店页面身份检测（本地结果）` | `identity.rows.*` | translate | — |
| KuaishouIdentity.tsx:170-178 | `复制快手ID` / `刷新检测状态` / 说明三段（176-178：仅展示声明/自动补做声明/分开保存声明） | `identity.actionsNotes.*` | translate | — |
| Provider:100,138 | `` `检测状态未知，读取失败：${errorText(cause)}` `` / `` `检测状态未知，重新检测失败，可重试：${errorText(cause)}` `` | `identity.unknownRead` / `.unknownRedetect` | translate | cause 为 external-raw |
| Provider:175 | `KuaishouIdentityProvider is required`（无 Provider 的 dev throw） | — | keep-original | 开发者向 |
| Provider:191-196 | 状态 label：`检测状态未知 / 读取失败` / `等待本次运行重新检测` / `未检测` / `本轮已识别` / `本轮未检测到` / `登记冲突` / `已关闭` / `已跳过` / `检测失败` / `检测中…` | `identity.statusLabels.*`（10 key） | translate | — |
| `lib/kuaishouIdentity.ts:17,22,30,35` | 校验 throw：`身份检测结果格式无效` / `身份检测结果字段无效` / `身份检测列表格式无效` / `身份检测结果与请求的 Profile 不符` | `errors.identityInvalid.*`（4 key） | translate | 经 errorText 进 toast |

### Q3. 主体采集 / 档案（`KuaishouSubject.tsx` + `SubjectArchives.tsx` + `lib/kuaishouSubject.ts`，`subject.*`）

| 位置 | 当前字符串 | 建议 key | 分类 | 备注 |
|---|---|---|---|---|
| KuaishouSubject.tsx:6-7 | 步骤态：`待执行` / `执行中` / `已完成` / `未完成`；错误码 label：`上次中断，需重新验证` / `账号或会话已改变` / `执行超时` / `页面结构不支持` / `浏览器页面已崩溃并重建` / `照片不可用` / `中文 OCR 不可用` / `识别失败` / `自动校验未通过` / `平台保存尚未验证` | `subject.step.*`（4）+ `subject.errorLabels.*`（10，对应 `KuaishouInitErrorCode` 10 码，码本身为 protocol） | translate | OCR 为 brand 保留 |
| KuaishouSubject.tsx:9,13-14 | aria `持久初始化状态` / `主体采集` / `切片权限` / `尚未初始化` / `需重试` + 模板 ` · ${label}` / ` · 下次重试 ${time}` | `subject.*` | translate | 时间为数据 |
| KuaishouSubject.tsx:17,30 | `身份识别不等于初始化完成；采集完成不等于人工已核对。` / `初始化状态读取失败，请在详情刷新` | `subject.noteDetectVsInit` / `subject.readFailed` | translate | — |
| KuaishouSubject.tsx:52 | alt 三态：`照片读取失败，请刷新档案` / `照片加载中…` / `` `主体证件照片 ${index+1}` `` | `subject.photo.*` | translate | — |
| KuaishouSubject.tsx:60-62 | 执行说明长段 + `执行 / 补做初始化` + `需运行中的环境与新鲜身份；重新检测仅为只读检测。` | `subject.run.*` | translate | — |
| KuaishouSubject.tsx:92-93,106 | toast：`档案读取失败，请刷新重试。` / `本地中文 OCR 状态不可用，请检查系统语言资源；不会自动下载或上传。` / `补做请求已提交，请刷新查看持久状态；不代表执行完成。` | `subject.toast.*` | translate | — |
| KuaishouSubject.tsx:118,124-125 | `修正已保存并重新校验，请对照照片后确认。` / `重新识别已完成，仍需人工核对。` / `已核对` / `版本冲突：…` / `操作未完成，未强制确认。…` / `操作未完成，刷新也失败；…` | `subject.save.*` | translate | — |
| KuaishouSubject.tsx:130-131,133-134 | aria `主体档案` / 标题 `主体档案与账号初始化` / `刷新档案` / `处理中…` | `subject.archTitle` 等 | translate | — |
| KuaishouSubject.tsx:138,140 | `无关联环境 / 环境已删除` / `历史关联环境：${names}` / 来源三项 `主体信息明文` / `达人主体明文` / `本地 OCR` | `subject.source.*` | translate | names 为 user-data |
| KuaishouSubject.tsx:144,146-147 | `尚无可用照片，不能确认。` / `姓名`（aria `主体姓名`）/ `身份证号`（aria `主体身份证号`） | `subject.form.*` | translate | 输入值为 user-data |
| KuaishouSubject.tsx:149,152 | aria `自动校验六项` + 说明 `自动校验不是法律身份真实性证明。…` | `subject.validation.*` | translate | 校验项 label 见下 |
| KuaishouSubject.tsx:154-157 | `保存修正并校验` / `确认已核对` / `重新识别（本地 OCR）` / `复制主体资料` + toast `主体资料已复制[，包含未核对标记]` / `复制失败，请手动选择复制。` | `subject.actions.*` | translate | 复制模板含 `${reviewStatus…}\n快手ID…`（:50），字段名为数据 |
| KuaishouSubject.tsx:159 | `暂无主体档案；身份已识别不代表资料已采集。` | `subject.noArchive` | translate | — |
| `lib/kuaishouSubject.ts:31` | 校验项：`姓名` / `18位结构` / `校验码` / `出生日期` / `页面可见姓名` / `页面可见证件号` | `subject.checks.*`（6 key） | translate | — |
| `lib/kuaishouSubject.ts:37,42,55,62,77` | throw/校验：`主体档案响应无效，请刷新` / `初始化状态响应无效` / `档案列表响应无效` / `档案详情响应无效` / `OCR 状态不可用`（+ :70 `照片不可用`） | `errors.subjectInvalid.*` | translate | — |
| `lib/kuaishouSubject.ts:50` | `已核对` / `未核对`（`reviewStatus` 显示；SubjectArchives:39 `待核对（未核对）`同义） | `subject.reviewed` / `.unreviewed`（统一） | translate | — |
| SubjectArchives.tsx:28 | aria `独立账号档案`（标题） | `subject.archivesTitle` | translate | — |
| SubjectArchives.tsx:31,33-36 | `刷新档案列表` / `按姓名、快手ID、身份证号搜索；删除环境仍保留档案。环境筛选不影响档案结果。` / `正在查询档案…` / `档案查询失败，请刷新重试。` / `没有匹配的账号档案。` | `subject.archives.*` | translate | — |
| SubjectArchives.tsx:38-41 | 行模板：`{realName} · 快手ID：{id} · {nickname ?? "未读出昵称"}` / `{maskedIdCard} · {已核对/待核对}` / `{无关联环境 / 环境已删除 / 历史关联环境：…}` / `查看账号档案` / 分页 `上一页档案` / `下一页档案` / `{offset+1}–{…} / {total}` / title `账号档案` | `subject.archiveRow.*` | translate（模板）/ user-data（字段值） | 姓名/ID/身份证号为数据 |
| SubjectArchives.tsx:48 | title `账号档案` | `subject.archiveTitle` | translate | — |

### Q4. OCR / Rust 中文错误（`ocr.*`）

| 位置 | 当前字符串 | 建议 key | 分类 | 备注 |
|---|---|---|---|---|
| `local-ocr/src/lib.rs:31-46` | `OcrError` 8 条 Display：`本平台不支持 Windows 本地 OCR` / `本机中文 OCR 语言资源不可用；不会自动安装或转云端` / `本地 OCR 运行时不可用` / `图片为空或无法完整解码` / `仅支持静态 PNG、JPEG、WebP 图片` / `不接受动画图片` / `图片字节数超限` / `图片尺寸、像素数或解码内存超限`（+ 后续变体，见源码全文） | `ocr.errors.*`（按变体分 key） | translate | 已中文，补 en；按设计 Display 不改，后续实现 possible 方案：UI 层操作级映射或后端小字典（implement 阶段定，清单仅登记） |
| `local-ocr/src/lib.rs:25` | `OCR_LANGUAGE = "zh-Hans-CN"` | — | protocol | 固定识别语言 |
| `commands/kuaishou_init.rs:28` | `系统中文OCR可用；仅本地识别`（`OcrAvailability.message`） | `ocr.availableLocal` | translate | — |
| `commands/profiles.rs:97,99` | `` `浏览器已启动，小店扫码页未打开，可重试：{e}` `` / `` `小店扫码页已打开但未激活，请切换标签页：{e}` `` | `identity.shopNotOpened` / `.shopNotActive` | translate | e 为 external-raw |
| `commands/profiles.rs:76` | `KUAISHOU_LOGIN_URL`（快手登录 URL 常量） | — | protocol | URL |
| `commands/business_accounts.rs:9` | `` `业务账号操作失败：{error}。请核对输入及Profile状态、刷新列表后重试；数据库或线程错误请重启应用后重试。` `` | `errors.businessOpFailed` | translate | 已中文，补 en；error 为 external-raw |
| `commands/business_accounts.rs:18` | `launcher thread closed`（内部 Mcp 错误透传） | — | protocol | 内部错误码式串 |

## R. 时间 / 地区显示（`time.*`，AC6）

| 位置 | 当前字符串 | 建议 key | 分类 | 备注 |
|---|---|---|---|---|
| `lib/relativeTime.ts:4,6` | `never`（空/非法时间 ×2） | `time.never` | translate | 建议“从未”；Settings:350 `Last checked: … / never` 复用 |
| `lib/relativeTime.ts:6-14` | `just now` / `` `${s}s ago` `` / `` `${m}m ago` `` / `` `${h}h ago` `` / `` `${days}d ago` `` | `time.justNow` / `.secondsAgo` / `.minutesAgo` / `.hoursAgo` / `.daysAgo`（`{{n}}`） | translate | 建议“刚刚 / {{n}} 秒前 …”；`toLocaleDateString()`（:15，无 locale）与 `formatTime` 的 `"en-GB"`（:21）需改传应用语言（实施点，非文案） |
| `components/atoms/Flag.tsx:17` | 注释 `Region/City`（示例格式） | —（注释） | — | 非渲染 |
| `components/atoms/Flag.tsx:19-106` | `TZ_TO_CC` 表（~90 条 `America/New_York` 等 IANA tzid → 国别码） | — | protocol | 整表为数据，不逐条进字典 |
| `components/atoms/Flag.tsx:118-126` | `Intl.DisplayNames(["en"], { type: "region" })` + `United States` / `Luxembourg`（用例/回退显示？:121） | `locale.countryName`（经 Intl 按应用语言） | translate（机制） | `["en"]` 需改传应用语言（实施点）；国别显示名由 Intl 本地化，不进自建字典 |
| `components/atoms/PlatformIcon.tsx:15-16,43-44` | `Windows` / `Linux`（TITLE 表 + aria；macOS 同理） | — | keep-original | OS 名 |
| `components/profile/EmojiField.tsx:96` | `` `Auto — classifier picks ${derived}` ``（title 模板） | `profile.emoji.autoTitle`，`{{emoji}}` | translate | derived 为数据 |
| `components/profile/EmojiField.tsx:12` | `Auto`（emoji 选项） | `profile.emoji.auto` | translate | 建议“自动” |
| `components/profile/EmojiField.tsx:63,65` | title `Set an emoji avatar` / aria `Choose emoji avatar` | `profile.emoji.setTitle` / `.chooseAria` | translate | — |
| `components/profile/EmojiField.tsx:82` | `Emoji avatar`（picker 标题行） | `profile.emoji.title` | translate | — |
| `components/profile/EmojiField.tsx:13` | 注释 `no custom icon` / `value === undefined` | —（注释/代码） | — | 非渲染 |
| `components/profile/ProfileTile.tsx:133` | title `Click to set an emoji` | `profile.emoji.clickToSet` | translate | 建议“点击设置 emoji” |

## S. Rust 后端用户可见英文（`backend.*` / `errors.*`）

设计约束：不修改共享 `MultizenError` 的 Display（MCP/日志消费者）；UI 捕获时用操作级中文 + 原始详情；禁止英文猜测映射。

| 位置 | 当前字符串 | 建议 key / 处理 | 分类 | 备注 |
|---|---|---|---|---|
| `multizen-core/src/error.rs:5-27` | Display 9 条：`database error: {0}` / `io error: {0}` / `serde error: {0}` / `profile not found: {0}` / `profile already exists: {0}` / `config error: {0}` / `launch error: {0}` / `cdp error: {0}` / `mcp error: {0}` | 不建翻译 key（Display 冻结）；UI 侧按操作建 `errors.*` 中文模板 + `{{detail}}` 原文 | protocol | design.md 示例 `errors.profileNotFound` 指 UI 侧操作模板，非后端 Display |
| `commands/settings.rs:42` | `` `unsupported language: {lang}` ``（非法显式更新的返回错误） | UI 侧 `errors.settingsInvalidLanguage`（操作级）+ 原文 | external | 非法值错误，原文展示 |
| `commands/dialog.rs:22` | 原生 filter `Browser binary`（`&["exe","app","sh"]`） | `dialog.browserBinaryFilter` | translate | Rust 侧小字典；扩展名/系统按钮为 protocol/OS 例外 |
| `commands/extensions.rs:353,520` | 原生 filter `Extension`（`&["crx","zip"]` ×2） | `dialog.extensionFilter` | translate | 同上 |
| `commands/extensions.rs:50` | `` `Could not parse a 32-char extension ID from: {input}` `` | `errors.extIdUnparseable`（UI 操作级） | translate | input 为 user-data |
| `commands/extensions.rs:77,82,91,97,100` | `HTTP client build failed: {e}` / `Web Store download failed: {e}` / `Web Store returned HTTP {status} for extension {id}` / `Failed to read CRX body: {e}` / `Extension {id} isn't available from the Web Store (empty response). Try uploading the .crx/.zip instead.`（:86 同文） | `errors.extDownload.*`（UI 操作级模板） | translate（模板）/ external（`{e}`/`{status}` 原文） | Web Store 为 brand |
| `commands/extensions.rs:86` | `Extension {id} isn't available from the Web Store (it may be delisted or region-restricted). Try uploading the .crx/.zip instead.` | `errors.extUnavailable` | translate | 同上 |
| `commands/extensions.rs:151,157,161,164,167,170,173,175,181,186` | `Failed to open ZIP archive: {e}` / `mkdir failed: {e}` / `zip read entry {i}: {e}` / `zip entry {i} has unsafe path` / `mkdir entry/parent` / `create/write file` / `rename to …` / `unpack task panicked: {e}` | —（UI 侧统一归入操作级 `errors.extInstallFailed` + 原文） | external | 底层诊断原文展示，不逐条进字典 |
| `commands/extensions.rs:220` | `__MSG_` 前缀判断（本地化占位逻辑） | — | protocol | 扩展 manifest 内部约定 |
| `commands/extensions.rs:366,528` | `Failed to read file: {e}` | `errors.extReadFailed`（UI 操作级） | translate | — |
| `commands/extensions.rs:69,75` | CRX 下载 URL 模板 / UA `Mozilla/5.0 … Chrome/131.0.0.0 …` | — | protocol | — |
| `commands/companion.rs:146,153` | `Invalid companion signal` / `No extension ID in companion signal`（install-error 事件文案） | `errors.companionSignal`（UI 操作级） | translate | 底层原因原文可附 |
| `commands/companion.rs:203,226` | `` `Added it, but couldn't reopen the profile — launch it again. ({e})` `` / `` `Added it, but the profile didn't reopen — launch it again. ({e})` `` | `extensions.addedNoReopen`（二态统一一 key） | translate | — |
| `commands/update.rs` | 见 §L | — | — | — |
| `commands/archive.rs` | 见 §O | — | — | — |
| `commands/fingerprint.rs` | 见 §F 末 | — | — | — |
| `commands/kuaishou_init.rs` / `profiles.rs` / `business_accounts.rs` | 见 §Q4 | — | — | — |
| `commands/system.rs:25` | `http://127.0.0.1:7777`（MCP URL 默认值） | — | protocol | — |
| 各命令 `map_err(\|e\| e.to_string())`（groups/proxy/activity/kuaishou_identity 全文件、其余命令多数） | 透传后端 Display，无新增字面量 | UI 侧按操作建中文模板（§C/`errors.*`） | protocol→external | 透传链不断即满足设计 |

## T. 例外与零字符串文件（覆盖证明）

### T1. 明确例外（保留英文，不进字典）

| 项 | 位置 | 说明 |
|---|---|---|
| companion 注入按钮 | `crates/tauri-app/resources/companion/cs.js:45,74,78`（`Add to Cloaksession` 等） | 在被管理浏览器页面上下文运行，PRD 决策保留英文；前端引用串（App.tsx:69 等）视为协议信号 |
| Emoji Mart 内部文案 | `EmojiField.tsx:101-115`（`<Picker … skinTonePosition="search" />`，未传 locale，默认 en；`@emoji-mart/data/i18n/zh.json` 本地可用） | 第三方控件：external；实施时经其本地化入口切 zh（design.md），`zh.json` 本身不盘点 |
| OS 自带按钮/装饰 | 原生文件对话框按钮、窗口装饰、更新安装器 hé | 系统语言控制，不随应用语言（设计已定） |
| 浏览器自身界面/网页/指纹参数 | 启动的浏览器、网页内容、`--lang` 等 | 非应用 UI（PRD 出范围） |
| 用户数据 | profile 名/标签/备注、昵称、平台 ID、身份证号、路径、剪贴板内容 | 永不进字典 |
| 第三方扩展内容 | `extensionCatalog.ts` 条目名/描述、Web Store 标题/说明、feed/activity 外部任意文本 | external 保留原文 |
| 品牌与协议 | Cloaksession、CloakBrowser、Chromix、MCP 工具名（`multizen.*`）、JSON/TOML 键、IPC 命令名、枚举透传值、URL、文件魔数、SDK flag | brand/protocol 保留 |
| MCP 复制配置样例 | McpPanel 配置代码块、ExtensionsSection `profiles.create({ extensions })` | protocol 保留（PRD 出范围） |
| 开发者向串 | `KuaishouIdentityProvider is required`、`#root not found`、注释、console 日志、tracing（`companion poller started` 等 info!/warn!）、测试断言 | 不进字典（`commands/*.rs` 内 `info!/warn!` 日志同理） |
| `index.html` | `crates/tauri-app/ui/index.html:2` `<html lang="en" class="dark">` | 启动 lang 占位；实施时由 Provider 按已保存语言同步 `document.lang`（AC6），本身非词条 |

### T2. 经核验零用户可见字符串的文件（本轮已读，无渲染文案）

前端：`lib/businessAccounts.ts`（仅 IPC 类型+调用名）、`lib/cn.ts`（类名拼接）、`lib/profileEmoji.ts`（emoji 派生+渐变）、`lib/parseProxy.ts`（解析逻辑；注释非渲染）、`lib/emojiTint.ts`（canvas 取色；注释/字体栈非渲染）、`lib/persisted.ts`（localStorage 键）、`lib/proxyHealth.ts`（探测逻辑）、`data/extensionCatalogIcons.ts`（base64 图标）、`components/atoms/index.ts`（重导出）、`components/atoms/Button.tsx`（仅注释式变体说明）、`components/atoms/Pill.tsx`、`components/atoms/Kbd.tsx`、`components/atoms/Avatar.tsx`（样式+首字母数据）、`components/atoms/PlatformIcon.tsx`（SVG path；TITLE 中 OS 名见 §R）、`components/profile/ExtIcon.tsx`（`alt=""` 装饰）、`lib/ipc.ts`（注释+调用名；唯一渲染引用为 companion 信号名，见 §C）、`types.ts`（类型+注释；同上）。
Rust：`commands/groups.rs`、`commands/proxy.rs`、`commands/activity.rs`、`commands/kuaishou_identity.rs`（四文件零字面量，纯 `e.to_string()` 透传）；`commands/mod.rs`（模块声明）；其余命令文件的非用户串（日志宏、serde 属性、URL/UA 常量）已在上文归类。

## U. 交接（给后续批次）

1. **key 总量**：translate 约 430（前端应用文案 ~350、chromix 字段/校验 ~90、后端操作级/Rust 小字典 ~25，含中文补 en 的快手域 ~120）；keep-original/protocol/brand/user-data/external 约 180（含 fingerprint 设备名 19、locale 自命名 20、tz 表整表 1、扩展目录第三方 ~50、配置样例 4）。
2. **去重提示**：`common.*`（Close/Cancel/Confirm/Discard/Copy/Remove/Browse/Clear）被 10+ 文件复用；`activity.status.*` 与 profile 状态 pills 同词；`nav.settings` 被 McpPanel/TopBar/Settings/Subject 内联引用；`archive.*` 前后端同文（App.tsx:329 ↔ archive.rs:201）。
3. **Profile.locale 隔离**：本清单所有 `fingerprint.*`/`chromix.*` 的 locale 文案均为管理 UI 语言；`Profile.locale`（浏览器 `--lang`）与 `AppSettings.language` 无关，不在本清单建 key（design.md）。
4. **Emoji Mart 与 Profile.locale**：按 design.md 分别处理（§T1），不与应用字典混用。
5. **待实施确认**：FingerprintForm `WebGL vendor/renderer` 译法（§F 暂 keep）、归档默认文件名 `profile`（§O 暂 keep-original）、MCP LLM 引导 prompt 是否随 UI 译中（§I 暂 translate）、`OcrError` 中文 Display 的映射层位置（§Q4，implement 阶段定）。
6. **验收对照**：本文件覆盖全部 60 个前端文件（含 7 个零串证明）与 16 个 Rust 命令文件 + `error.rs` + `local-ocr/lib.rs`；每条含 file:line + 原文 + 建议 key + 分类。未改动任何产品代码。
