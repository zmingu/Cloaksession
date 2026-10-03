# Wave3 批次分域与 spec 模板

Wave3 = 4 路并行组件迁移(可用 agent: pi, opencode, codex ×2;cline 启动失败,freebuff 已停)。
依赖:Wave2 codex 必须先完成 `ui/src/i18n/` 骨架(Provider/t()/en.ts/zh-CN.ts)。
所有权边界严格无重叠;字典键由 Wave2 骨架定义,Wave3 各路只在自己组件里调用 `t()`,**不改字典骨架结构**(发现缺键时在各自域 section 追加,键名前缀按 string-inventory.md)。

## 4a — pi — Sections A+B+N+P

**Files**: `components/atoms/Modal.tsx`, `components/screens/Confirm.tsx`, `components/screens/Sidebar.tsx`, `components/screens/LeftRail.tsx`, `components/screens/TopBar.tsx`, `components/palette/CommandPalette.tsx`, `components/profile/Constellation.tsx`, `components/profile/EmptyState.tsx`, `App.tsx`(共通 toast/确认部分), 任何 §A/B/N/P 列出的 file:line。
**Keys**: `common.*`, `nav.*`, `palette.*`, `group.*`, `profile.list.*`。
**Don't touch**: 其他 Wave3 路的文件域(新建编辑/指纹/Chromix/代理/扩展/引导/MCP/活动/设置/更新/归档/业务账号/时间地区)。
**Observable**: `npm run build` 通过;涉及的组件在语言切换时即时更新;无未登记英文残留(§A/B/N/P 范围内)。

## 4b — opencode — Sections D+F+G

**Files**: `components/profile/NewProfileSheet.tsx`, `components/profile/FingerprintForm.tsx`, `components/profile/ChromixFingerprintForm.tsx`, `components/profile/ChromixProfileOptions.tsx`, `lib/chromixFingerprint.ts`(字段标签部分), §D/F/G 列出的 file:line。
**Keys**: `profile.*`, `fingerprint.*`, `chromix.*`。
**Don't touch**: 其他路文件域。
**Observable**: `npm run build` 通过;指纹表单中英文切换即时更新;Profile.locale 与 AppSettings.language 隔离不变;§U5 待确认的 WebGL vendor/renderer 译法在 PR 说明。

## 4c — codex — Sections E+H+M

**Files**: `components/profile/ProxyTest.tsx` 或 §E 列出文件, `components/profile/ExtensionsSection.tsx`, `data/extensionCatalog.ts`(仅应用自有文案,第三方标题/描述保留原文), `components/onboarding/FirstRun.tsx`, §E/H/M 列出的 file:line。
**Keys**: `proxy.*`, `extensions.*`, `onboarding.*`。
**Don't touch**: 其他路文件域。
**Observable**: `npm run build` 通过;代理连通性/扩展/onboarding 中英文切换即时更新;第三方扩展标题/描述保留原文(external)。

## 4d — codex#2 — Sections I+J+K(剩余)+L

**Files**: `components/mcp/McpPanel.tsx`, `components/activity/ActivityDrawer.tsx`, `components/screens/Settings.tsx`(Wave2 已迁移语言切换 UI 之外的所有文案), `components/UpdateBanner.tsx`, §I/J/K/L 列出的 file:line。
**Keys**: `mcp.*`, `activity.*`, `settings.*`(剩余), `update.*`。
**Don't touch**: 其他路文件域;不动 Wave2 已建的 Settings 语言切换 UI。
**Observable**: `npm run build` 通过;MCP/活动/设置/更新横幅中英文切换即时更新;MCP 工具名/协议样例保留原文(protocol)。

## Wave3 启动条件(已满足 2026-10-03)

1. ✅ Wave2 codex 完成:`ui/src/i18n/{en.ts,zh-CN.ts,translate.ts,LanguageProvider.tsx,dictionaries.test.mjs,index.ts}` 就位
2. ✅ 字典已预置 **584 键**(en/zh-CN 完全对齐),覆盖 string-inventory.md 全部域
3. ✅ main.tsx 接好 LanguageProvider + 错误边界;Settings.tsx 重写完成;index.html 改完
4. ✅ `npm run build` 通过

**关键修订(落实)**:Wave2 codex 实际预置了 584 键(远超 430 估计),en/zh-CN 完全对齐。Wave3 各路 **只调用 `t('key')`,不改字典文件**(键已全在)。如发现个别缺键,在该路组件文件内用 fallback 文案占位,登记到 wave3-findings.md,Wave4 统一补键。这彻底消除并发写字典冲突。

## Wave3 派发(4 路并行)

各路 spec:`--worktree current` 共享工作目录;只改自己文件域;调 `t()`;不动 i18n/ 字典;不动其他路文件。