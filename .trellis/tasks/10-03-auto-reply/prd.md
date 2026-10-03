# auto-reply — 自动回复（关键词）迁移

## Goal
迁移 jieger 的自动回复：关键词匹配回复，replyEngine。消费 comment-listener 事件，经 sub-account 发送。

## jieger 源文件
- `electron/main/tasks/autoReply/index.ts`（主） + `replyEngine.ts`（共 247 行）
- `electron/main/ipc/handlers/autoReplyHandler.ts`（36 行）— IPC
- `electron/main/services/database/repositories/autoReplyRecords.ts` — 回复记录

## 功能点
1. **replyEngine**：关键词匹配引擎（精确/模糊/正则，决策在 design.md）。
2. **事件消费**：订阅 comment-listener 的 CommentEvent 流。
3. **回复发送**：匹配命中 → 经 sub-account sendDanmaku 发送回复。
4. **回复记录**：`autoReplyRecords` 落库。
5. **start/stop**：注册到 taskRegistry。
6. **AI 回复大脑**：jieger 有 aiChat 集成；Cloaksession 走 MCP（不迁 aiChat），replyEngine 预留 MCP 调用钩子但不在本任务实现 AI。

## 依赖
- 前置：`comment-listener`（事件源）、`sub-account`（发送接口）。

## 验收标准
- [ ] 关键词匹配引擎等价。
- [ ] 订阅 comment-listener 事件流。
- [ ] 回复经 sub-account sendDanmaku 发送。
- [ ] 回复记录落库。
- [ ] 支持 cancel。
- [ ] 离线门禁通过；真号回复验收单独确认。

## Out of Scope
- 不实现 aiChat（走 MCP，架构记忆已确认）。
- 不实现 comment-listener 或 sendDanmaku。

## Notes
- 本任务是 comment-listener（产）与 sub-account（发）之间的「大脑」，定义清晰的订阅接口。
- 关联 spec：`.trellis/spec/tauri-app/backend/ipc.md`。
- 关联记忆：`cloaksession-architecture`（aiChat 不迁，走 MCP）。
