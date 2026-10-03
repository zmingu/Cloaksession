# 中文化现状调研

日期:2026-10-03。来源:只读 Explore 调研;未修改业务代码。数量为估计,非精确词条统计。

## 技术栈

- Rust workspace(9 crate)+ Tauri 2.11 + React 19.2 + TS 5.9 + Vite 6.4 + Tailwind 4.3。证据:`crates/tauri-app/ui/package.json:13-33`、`Cargo.toml:4-16`、`package-lock.json`。
- 前端无 i18n 库(i18next/react-intl/fluent 全无)、无 locale 目录、无语言检测、无 `AppSettings.language` 字段、`crates/tauri-app/ui/index.html:2` 写死 `<html lang="en" class="dark">`。

## UI 文案现状

- 硬编码混合语言,大部分英文,约 419 候选行/40 文件,量级数百条。代表:`Sidebar.tsx:18-22`(导航)、`Settings.tsx:99-435`(设置)、`FirstRun.tsx:9,98-206`(引导)、`App.tsx:281-559`(toast/确认/导入导出)、`chromixFingerprint.ts:32-127`(指纹字段标签)、`extensionCatalog.ts:27-87`(扩展目录)、`relativeTime.ts:1-21`(相对时间)、`main.tsx:24-36`(错误边界)。
- 快手/业务账号已有硬编码中文:`BusinessAccountSection.tsx:13-19`("快手小店")、`KuaishouSubject.tsx:6-14`("中文 OCR 不可用"等初始化错误)、`kuaishouSubject.ts:31`(验证标签 map)。
- companion 扩展注入网页按钮英文:`crates/tauri-app/resources/companion/cs.js:45,74,78`("Add to Cloaksession" 等)。

## 后端 Rust 用户可见字符串

- `Result<…,String>` + `.map_err(|e| e.to_string())`,MultizenError Display 英文。流入前端 toast。证据:`multizen-core/src/error.rs:3-33`、`tauri-app/src/commands/profiles.rs:28-99`(profile not found)、`archive.rs:201,370,392`(归档/导入错误)、`update.rs:267`(更新错误)、`companion.rs:203`(扩展安装事件错误)、`dialog.rs:22`(文件对话框 filter "Browser binary")。
- 业务账号错误已中文:`business_accounts.rs:9`("业务账号操作失败：{error}…")。OCR 错误已中文:`local-ocr/src/lib.rs:33`("本机中文 OCR 语言资源不可用…")。
- 指纹自命名多语言:`fingerprint.rs:97-118,186-206`("English (United States)"、"中文 (简体, 中国大陆)"、"中文 (繁體, 台灣)" 等)。

## 持久化

- AppSettings:`settings-store/src/defaults.rs:20-79`,JSON 文件 `settings.json` 在 `app_local_data_dir`(回退 `app_config_dir` 再回退 cwd);无 language 字段。Rust 侧在 `tauri-app/src/lib.rs:81-91` 解析目录,app id `com.cloaksession.browser`(`tauri.conf.json:5`)。
- 前端:`persisted.ts:3-35`,localStorage `multizen.ui.*`(section/drawerOpen/onboarded/profilesView),无语言 key。
- Profile 数据:SQLite `profiles.db`,`profile-manager/src/manager.rs:26-35`(WAL+迁移),`rusqlite` 0.32。

## 邻接能力(非应用 UI 语言,不重复造)

- Emoji Mart:`@emoji-mart/data` 1.2.1 含 22 语言 JSON 含 `zh.json`(`node_modules/@emoji-mart/data/i18n/zh.json`),应用未传 locale,默认 "en"。证据:`EmojiField.tsx:1-3,101-115`、`emoji-mart/dist/module.js:591-592`。
- Profile 指纹 locale:20 预设含 zh-CN/zh-TW,影响浏览器启动参数(`browser-launcher/src/args.rs:146-147` 的 `--lang`/`--accept-lang`),与管理 UI 隔离。证据:`fingerprint.rs:50-55`、`FingerprintForm.tsx:85-113,181-188,298-312`、`cdp-driver/src/bootstrap.rs:74-75`。
- 本地 OCR 固定 `zh-Hans-CN`(`local-ocr/src/lib.rs:25`)。
- 原生 `Intl` 使用零散:`Flag.tsx:115-125` 显式 `["en"]`、`relativeTime.ts:18-21` 显式 `en-GB`、`Constellation.tsx:144` 无 locale 的 `toLocaleLowerCase`。

## 验证入口

- `npm --prefix crates/tauri-app/ui run build`
- `cargo check --workspace --locked`
- `cargo test --workspace --locked`
- `npx playwright test --config playwright.config.ts`(在 `crates/tauri-app/ui`)
- `node --experimental-strip-types src/lib/chromixFingerprint.test.mjs`(在 `crates/tauri-app/ui`)

## 结论

现状为硬编码混合语言、无统一 i18n 层、后端英文字符串直显。需:新建字典 + Context + `AppSettings.language` 字段(与 Profile.locale 隔离);已有中文散落纳入字典;后端错误不动 Display、UI 层加操作级翻译 + 原始详情;原生提示 Rust 小字典;companion cs.js 列例外。仅 2 语言、无 ICU 复数,自建轻量方案足够,无需引依赖。