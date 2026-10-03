# shop-product-script — 商品话术库迁移

## Goal
迁移 jieger 的商品话术库 CRUD（264 行）。只迁功能，不搬旧话术数据。

## jieger 源文件
- `electron/main/tasks/shopProductScript/index.ts`（264 行）— `playProductScript`
- `electron/main/ipc/handlers/shopProductScriptHandler.ts`（154 行）— IPC
- `electron/main/services/database/repositories/shopProductScript.ts` — 话术 CRUD

## 功能点
1. **商品话术 CRUD**：创建/读取/更新/删除商品话术（关联商品 ID + 话术文本）。
2. **playProductScript**：播放话术（被 auto-popup / live-room-monitor 调用）。
3. **话术关联**：话术与商品/知识的关联结构。

## 依赖
- 无前置子任务（独立，可与波 0 同步）。
- 被调用方：`auto-popup`、`live-room-monitor`（经 productScriptId 触发）。

## 验收标准
- [ ] 话术 CRUD 在 Rust 落地（复用 settings-store 或新增表，决策在 design.md）。
- [ ] playProductScript 接口可被下游调用。
- [ ] 离线门禁通过。
- [ ] 不搬旧话术数据。

## Out of Scope
- 不搬旧话术数据。
- 不实现 auto-popup/live-room-monitor（调用方）。

## Notes
- 本任务是 auto-popup 与 live-room-monitor 的共用依赖（productScriptId 触发），建议早做。
- 存储决策：jieger 用独立表；Cloaksession 可复用 settings-store JSON 或新增 SQLite 表，design.md 论证。
- 关联 spec：`.trellis/spec/settings-store/backend/persistence.md`、`.trellis/spec/tauri-app/backend/ipc.md`。
