# 前端A组-账号与开播

## Goal

为 `kuaishou_auth`（3）、`mate_login`（3）、`live_launch`（7）、`live_room_monitor`（3）共 16 个命令补齐 TS 封装、类型、页面、导航、i18n。命令签名见父任务 `research/command-surface.md §1–4`。

## Requirements

- R1 `lib/kuaishouAuth.ts`：`connect/login/ensureAuth(profileId, targetId)`；`EnsureAuthResult{ok, scanned, error?}`；订阅 `kuaishou-auth-phase` 事件（await 注册/清理）。
- R2 `lib/mateLogin.ts`：`start/cancel/state(accountId)`；`MateLoginState` 全字段 + `MateLoginStage` kebab-case 字面量；扫码页展示 `qrImageDataUrl`，轮询 `mate_login_state` 直到终态；订阅 `mate-login-state-changed`。
- R3 `lib/liveLaunch.ts`：7 命令全封装（含 `prerequisites()` 无参、`streamStart(profileId, videoPath, controlUrl?)`）；`StreamCredentials` / `StreamingState` / `PrerequisitesReport` 类型；订阅 `live-launch-state-changed`。
- R4 `lib/liveRoomMonitor.ts`：`start(profileId, config)` / `stop()`（无参） / `state()`（同步）；`MonitorConfig` / `LiveRoomMonitorState` 全字段。
- R5 页面：小店扫码连接页（只读状态 + ensure 结果）、伴侣扫码登录页（二维码 + 阶段机）、开播页（前置检查 → 取流 → 心跳启停 → 推流启停，推流/开播需二次确认）、直播间监控页（配置表单 + 状态 + 启停）。
- R6 i18n 前缀：`biz.auth.*`、`biz.mate.*`、`biz.live.*`、`biz.monitor.*`，中英同步；错误展示后端 error 原文 + 操作级中文模板。
- R7 导航：在业务分组下注册 4 个入口（R1–R4 各一），不与 `1/2/,` 快捷键冲突。

## Acceptance Criteria

- [x] 16 命令 TS 封装齐全，`npm run build` 通过。
- [x] 字典测试通过；4 个页面 IPC-mock 渲染用例通过。
- [x] 桌面内可完成：小店 ensure 鉴权只读验证、伴侣扫码状态机展示、前置检查展示、监控启停。

## Notes

- 基线校正：开工时 `main` 已含 C/D/E 合入的共享 business 壳（`571f0c1`），
  A 组改为向该壳追加 4 tab，详见 `implement.md` 的「基线校正」节。
- 既有失败 `tests/kuaishou-subject.spec.ts`（4 例）在干净基线上同样失败，非本组引入。
