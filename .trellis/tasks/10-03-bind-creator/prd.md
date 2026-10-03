# bind-creator — 达人授权迁移

## Goal
迁移 jieger 的达人授权（bindCreator）：达人绑定授权流程。

## jieger 源文件
- `electron/main/tasks/jinniu/bindCreator.ts`（jieger jinniu task 1215 行中的 bindCreator 部分）
- `electron/main/ipc/handlers/jinniuHandler.ts`（107 行）— IPC（含 jinniu + bindCreator）
- `electron/main/services/database/repositories/jinniuAuthorize.ts` — 授权记录

## 功能点
1. **达人绑定授权**：发起达人授权流程，获取授权状态。
2. **授权记录持久化**：`jinniuAuthorize` 落库。
3. **授权状态查询**：查达人授权是否完成。

## 依赖
- 前置：`ks-platform-primitives`、`jinniu-promote`（共享金牛后台上下文）。

## 验收标准
- [ ] 达人授权流程经 TaskPage 或 HTTP 接口完成。
- [ ] 授权记录落库（复用 business_accounts 或新增表，决策在 design.md）。
- [ ] 授权状态可查询。
- [ ] 离线门禁通过；真号授权验收单独确认。

## Out of Scope
- 不实现 jinniu-promote 本身（见该子任务）。
- 不搬旧授权记录。

## Notes
- jieger 把 jinniu + bindCreator 放在同一 task（1215 行），本任务拆出 bindCreator 独立可验证。
- 关联 spec：`.trellis/spec/cdp-driver/backend/task-control.md`、`.trellis/spec/tauri-app/backend/ipc.md`。
