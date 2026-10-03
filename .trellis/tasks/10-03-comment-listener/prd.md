# comment-listener — 弹幕评论监听迁移

## Goal
迁移 jieger 的评论监听单例：每账号一个监听器，解析评论流，写库 + IPC 广播。与 Cloaksession 已有的独立 `tools/danmaku-pipeline` 弹幕识别 crate 对齐契约。

## jieger 源文件
- `electron/main/tasks/commentListener/index.ts`（114 行）— 单例监听 + broadcast
- `electron/main/platforms/kuaishou/commentParser.ts` — `KuaishouCommentParser`、`CommentEvent`
- `electron/main/services/database/repositories/liveEvents.ts` — `insertEvent` 写库
- `electron/main/ipc/handlers/commentListenerHandler.ts`（20 行）— IPC

## 功能点
1. **单例监听**：每账号一个 listener（`Map<accountId, ListenerState>`）。
2. **KuaishouCommentParser**：解析直播间评论流（DOM/接口），产出 `CommentEvent`。
3. **写库**：`insertEvent` 落库 liveEvents。
4. **IPC 广播**：CommentEvent 经 IPC 推渲染端 + 订阅者（`Map<accountId, Set<fn>>`）。
5. **start/stop**：`markTaskStarted`/`markTaskStopped` 注册到 taskRegistry。

## 依赖
- 前置：`ks-platform-primitives`（进入直播间页）。
- 契约对齐：`tools/danmaku-pipeline/`（独立 crate，上游产 JSON，下游发送端不在本仓库）。

## 验收标准
- [ ] Rust 单例监听器（per-account），复用 TaskPage 解析评论流。
- [ ] CommentEvent 经 Tauri event 广播。
- [ ] 与 danmaku-pipeline 的 `CONTRACT.md` + `schema/danmaku-script.schema.json` 契约对齐（监听端产的事件结构与 pipeline 输入兼容，`send_at` 为发送权威）。
- [ ] 写库复用 profile-manager 或新增 liveEvents 表（决策在 design.md）。
- [ ] 离线门禁通过；真号直播间监听验收单独确认。

## Out of Scope
- 不实现 danmaku-pipeline 本身（已存在）。
- 不实现下游发送端（不在本仓库，见 `snow-danmaku-reference` 记忆）。
- 不做 AI 弹幕识别（走 MCP/pipeline）。

## Notes
- 本任务是「监听端」，pipeline 是「识别端」，发送端在 jieger/另一会话。三者经 JSON 契约耦合，不经代码引用。
- 关联 spec：`.trellis/spec/cdp-driver/backend/task-control.md`、`.trellis/spec/tauri-app/backend/ipc.md`。
- 关联记忆：`snow-danmaku-reference`。
