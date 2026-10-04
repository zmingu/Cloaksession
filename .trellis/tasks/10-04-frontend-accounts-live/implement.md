# Implement — 前端A组-账号与开播

## 基线校正（重要）

开工时 `main` 已被 C/D/E 三组推进到 `571f0c1`，其中已包含**共享 business 壳**
`ui/src/components/business/BusinessSection.tsx`（唯一 `BusinessTabId` + `BUSINESS_TABS`，
`<BusinessSection profiles={profiles} />`，⌘3 进入）。A 组不再自建 section 壳，
而是向该壳追加 4 个 tab（`auth` / `mate` / `live` / `monitor`），并把默认 tab 设为 `auth`。

## 交付清单

1. `ui/src/lib/kuaishouAuth.ts` — 3 命令 + `onKuaishouAuthPhase`（Promise<UnlistenFn>）。
2. `ui/src/lib/mateLogin.ts` — 3 命令 + `onMateLoginStateChanged`。
3. `ui/src/lib/liveLaunch.ts` — 7 命令 + `onLiveLaunchStateChanged`。
4. `ui/src/lib/liveRoomMonitor.ts` — 3 命令 + `onLiveRoomMonitorStateChanged`。
5. `ui/src/types.ts` — 追加 A 组 18 个导出（`EnsureAuthResult`、`KuaishouAuthPhase`、
   `MateLoginState`、`MateLoginStage`、`MATE_LOGIN_TERMINAL_STAGES`、`StreamingState`、
   `StreamingStatus`、`StreamMode`、`StreamCredentials`、`LiveLaunchStateChanged`、
   `PrerequisitesReport`、`MonitorConfig`、`LiveRoomMonitorState`、`LiveRoomMonitorStatus`、
   `LiveRoomLiveStatus`、`TriggerBatchResult` 等）；不触碰其他组已有类型。
6. `ui/src/components/business/{KuaishouAuthPage,MateLoginPage,LiveLaunchPage,LiveRoomMonitorPage}.tsx`。
7. `BusinessSection.tsx` — 追加 4 tab；`CommandPalette.tsx` — 追加 `section:business`（⌘3）。
8. i18n：`biz.auth.*` / `biz.mate.*` / `biz.live.*` / `biz.monitor.*` 共 65 key ×2 字典。
9. 用例：`tests/accounts-live.spec.ts`（9 个 IPC-mock 渲染 + 参数形状用例）、
   `tests/aCommandCoverage.test.mjs`（16 命令覆盖 + 事件可 await + 无参契约 + streamKey 不落日志）。

## 验证命令

```sh
cd crates/tauri-app/ui
npx tsc -b
npm run build
node --experimental-strip-types src/i18n/dictionaries.test.mjs
node --experimental-strip-types tests/aCommandCoverage.test.mjs
npx playwright test --config playwright.config.ts --project=desktop-chrome
```

## 已知既有失败（非本组引入）

`tests/kuaishou-subject.spec.ts` 4 例在干净基线 `827a4f6` 上同样失败（spec 自基线未改动），
与本组无关，不在本任务内修复。

## 回滚点

仅前端文件；`git checkout -- <path>` 或丢弃 A 组新增文件即可，无数据迁移、无 Rust 改动。
