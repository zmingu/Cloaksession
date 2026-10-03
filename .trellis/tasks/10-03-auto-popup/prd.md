# auto-popup — 自动弹窗/弹品迁移

## Goal
迁移 jieger 的自动弹品：按时间间隔轮换自动讲解商品（弹品），支持 goodsKnowledge 知识库对接。

## jieger 源文件
- `electron/main/tasks/autoPopUp/index.ts`（主，405 行总）
- `electron/main/tasks/autoPopUp/goodsKnowledge.ts` — 商品知识库扫描
- `electron/main/tasks/autoPopUp/shortcutManager.ts` — 快捷键注册
- `electron/main/platforms/kuaishou/goodsList.ts` — `fetchGoodsList`/`explainGoods`/`cancelExplainGoods`
- `electron/main/ipc/handlers/autoPopUpHandler.ts`（56 行）— IPC

## 功能点
1. **AutoPopUpConfig**：goodsIds 序列 + interval[min,max] 全局间隔 + perGoodsInterval 单品覆盖 + goodsItems 增强队列（id+repeatCount+interval）+ random 随机选择 + retry 配置。
2. **队列轮换**：按顺序（或随机）轮换商品，每个商品按 interval 发送。
3. **explainGoods**：调用中控页 `explainGoods`（弹品动作）。
4. **fetchGoodsList**：取商品列表。
5. **goodsKnowledge**：扫描商品知识库（与 shop-product-script 对接）。
6. **shortcutManager**：全局快捷键注册（决策：Tauri global shortcut）。
7. **runWithRetry**：失败重试。
8. **start/stop**：注册到 taskRegistry。

## 依赖
- 前置：`sub-account`（小号会话）、`shop-product-script`（话术/知识库）。
- 调用：`goodsList.ts` 的 explainGoods（在中控页操作，需 ks-platform-primitives）。

## 验收标准
- [ ] 队列轮换 + 间隔调度在 Rust（tokio 周期任务）。
- [ ] explainGoods 经 TaskPage 调用中控页。
- [ ] 快捷键经 Tauri globalShortcutApi。
- [ ] 支持 cancel + retry。
- [ ] 离线门禁通过；真号弹品验收单独确认（涉及商品上架操作）。

## Out of Scope
- 不实现 shop-product-script 本身（见该子任务）。
- 不搬旧话术/知识库数据。

## Notes
- 本任务消费 shop-product-script 的知识库接口，需在 design.md 定义接口。
- 关联 spec：`.trellis/spec/cdp-driver/backend/task-control.md`、`.trellis/spec/tauri-app/backend/ipc.md`。
