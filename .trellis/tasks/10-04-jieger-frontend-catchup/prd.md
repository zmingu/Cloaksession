# jieger业务前端补齐

## Goal

为已合入 main 的 15 个 jieger 业务后端模块补齐前端入口与页面，使桌面应用内可逐个验证。完整命令面见 `research/command-surface.md`（84 个命令，已有 TS 封装 7 个）。

## Scope

- 仅前端：`crates/tauri-app/ui/src/` 下的 IPC 封装（`lib/*.ts`）、类型（`types.ts`）、页面组件、导航入口、i18n 字典（`en.ts` + `zh-CN.ts` 同步）。
- 不动 Rust 后端：命令签名以现有后端为准；发现后端 bug 只记录，不顺手修（另开任务）。

## Requirements

- R1 导航：在 `Sidebar`/`App.tsx` 的 `Section` 中新增业务入口（分组或独立 section），键盘快捷键不与现有 `1/2/,` 冲突。
- R2 IPC 封装：为 77 个未覆盖命令补 `lib/*.ts` invoke 封装；通道名 = Rust 函数 snake_case 名，参数键 camelCase；结构体字段与后端 serde 一致。
- R3 类型：`types.ts` 补全各模块请求/响应类型（含枚举字面量：`MateLoginStage` kebab-case、`StreamingStatus`、`TriggerMode`、`SceneLineAction`、`ReplySource`、`ShopLiveStatus`、`HuiboVideoStatus`、`CommentEventType` 等，以后端为准）。
- R4 页面：每组一个可验证页面（列表/状态/启停/历史），只读先行；写操作（开播、发送、推广、挂车、授权）需二次确认并展示后端返回的 error 字符串。
- R5 事件：订阅后端 push 事件（`mate-login-state-changed`、`live-launch-state-changed`、`live-room-monitor-state-changed`、`auto-popup:state/event`、`shop-helper:goods-changed`、`kuaishou-auth-phase`），`listen` 必须 await 注册/清理。
- R6 i18n：所有新增 UI 字符串使用域前缀语义 key，同时加 `en.ts` + `zh-CN.ts`；`{{name}}` 双花括号插值；品牌/IPC/用户数据/外部文本不翻译。
- R7 安全约束：**`auto_message_start` 的 `insertRandomSpace` 参数前端不得提供**（不实现规避检测选项）；真号写操作仅手动触发，不做自动批量。

## Non-goals

- 不改 Rust 命令/driver；不做自动批量执行；不迁旧账号/配置/话术/历史数据。

## Acceptance Criteria

- [ ] AC1 `npm run build`（tsc -b + vite）在 `crates/tauri-app/ui` 通过。
- [ ] AC2 `node --experimental-strip-types src/i18n/dictionaries.test.mjs` 通过（中英 key 集一致）。
- [ ] AC3 Playwright 现有用例通过；新增页面有 IPC-mock 用例覆盖主要渲染路径。
- [ ] AC4 桌面应用内可逐个打开 15 模块入口，完成只读验证（授权列表同步、回放视频列表等）。
- [ ] AC5 写操作有二次确认 + 错误展示；`insertRandomSpace` 在前端无任何入口。

## Notes

- 子任务按 5 组拆分（见 task.json children / 各子任务 prd）：A 账号与开播、 B 监听与小号、 C 话术与场控、 D 商品与弹窗、 E 金牛与跟播。
- 组间无代码依赖，可并行；共享 `types.ts` / i18n key 前缀需在 design.md 约定避免冲突。
