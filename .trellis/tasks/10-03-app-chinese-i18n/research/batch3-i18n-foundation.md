# BATCH 3 交付记录 — 翻译基础与首屏（Wave2）

日期：2026-10-03。任务：`.trellis/tasks/10-03-app-chinese-i18n` BATCH 3。
所有权：仅新增 `ui/src/i18n/*`，修改 `main.tsx`、`Settings.tsx`（语言切换 UI + 本页文案）、
`types.ts`（batch2 已加 `AppLanguage`）、`index.html` lang、`tests/tauriMock.ts`（fixture 默认 en）、
新增 `tests/i18n-language.spec.ts`。未迁移其他组件（Wave3）。

## 交付物

- `ui/src/i18n/en.ts` — 584 键，`TranslationKey = keyof typeof en` 单一事实来源。
- `ui/src/i18n/zh-CN.ts` — `Record<TranslationKey, string>`，缺键即编译错误；
  246 条未定稿值以英文占位并标 `// REVIEW`，供 Wave3 按域填充。
- `ui/src/i18n/translate.ts` — 纯 `translate(language,key,params)`、具名 `{{name}}` 插值、
  仅文本节点（无 HTML 注入）、`normalizeLanguage`、模块级 `t()`/`setCurrentLanguage`（供
  Provider-free 错误边界与入口门禁使用）。
- `ui/src/i18n/LanguageProvider.tsx` — `LanguageProvider` / `useLanguage` / `useT`；
  `setLanguage` 仅在 IPC 保存成功后更新全局语言；同步 `document.lang`。
- `ui/src/i18n/index.ts` — Wave3 的 barrel 入口。
- `ui/src/i18n/dictionaries.test.mjs` — Node `--experimental-strip-types`，断言两字典键集
  与具名占位符集完全一致、插值不改写、无 HTML 解析、`normalizeLanguage` 边界。
- `ui/src/main.tsx` — 入口门禁：挂载完整 UI 前读取 settings；轻量无文字启动占位避免
  英文用户先看到中文；加载失败回退中文 + 可操作提示 + 重试；Provider-free 错误边界
  （纯翻译，原始堆栈保留原文）。
- `ui/src/components/screens/Settings.tsx` — 语言切换 UI（保存期间禁用重复提交、
  失败保留旧语言并提示、不卸载 App/不丢表单），本页全部文案改走 `t()`。
- `ui/index.html` — `<html lang="zh-CN">`（设置加载后由 Provider 同步）。

## 验证（实际运行）

- `npm --prefix crates/tauri-app/ui run build` — 通过。
- `npx tsc --noEmit` — 通过（clean）。
- `node --experimental-strip-types src/i18n/dictionaries.test.mjs` — 6/6 通过。
- `npx playwright test`（`PLAYWRIGHT_CHANNEL=chromium`）— 桌面 53 通过；新增
  `i18n-language.spec.ts` 桌面/移动各 4 通过。移动端 `chromix-settings.spec.ts:225`
  窄屏溢出失败为**既有问题**，在还原本次改动后同样失败，与本批次无关。

## 交接 Wave3

- 只调用 `t("key")`，不得新增顶层键；缺键在对应域追加。
- `zh-CN.ts` 中 `// REVIEW` 条目需按域填入正式中文。
- Sidebar/导航等其余组件按 `research/wave3-batch-plan.md` 分域并行迁移。
- 未引入 i18next/react-intl；未耦合 `Profile.locale`；未改 `Cargo.toml`。
