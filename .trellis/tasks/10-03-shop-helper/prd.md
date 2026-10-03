# shop-helper — 跟播助手上车迁移

## Goal
迁移 jieger 的跟播助手小黄车上车（`zs.kwaixiaodian.com/page/helper`）。**与已实现的 CPS 加货架（`cps.kwaixiaodian.com`）是两个不同流程，不要混淆。**

## jieger 源文件
- `electron/main/tasks/shopHelper/index.ts`（132 行）
- `electron/main/ipc/handlers/shopHelperHandler.ts`（53 行）— IPC
- `electron/main/platforms/kuaishou/shopHelperActions.ts` — 上车动作
- `electron/main/platforms/kuaishou/shopHelperSelectors.ts` — 选择器

## 功能点
1. **上车任务管理**：AddTaskDialog/GoodCard/ScriptCard 对应的任务/商品/脚本 CRUD。
2. **shopHelperActions**：在跟播助手页执行上车（加商品到小黄车）。
3. **shopHelperSelectors**：跟播助手页选择器。
4. **脚本化上车**：按脚本自动加商品。

## 依赖
- 前置：`ks-platform-primitives`（进入 zs.kwaixiaodian.com/page/helper 中控页）。
- 复用：已实现的 business_accounts scope=kuaishou。

## 验收标准
- [ ] 上车动作经 TaskPage 在跟播助手页执行。
- [ ] 与 CPS 加货架（cps.kwaixiaodian.com）代码与流程清晰隔离，无混淆。
- [ ] 脚本化上车支持。
- [ ] 支持 cancel。
- [ ] 离线门禁通过；真号上车验收单独确认（涉及商品上架写动作）。

## Out of Scope
- 不实现 CPS 加货架（已实现）。
- 不搬旧上车脚本/商品数据。

## Notes
- **关键边界**：CPS「加货架」=开播前准备（cps.kwaixiaodian.com，手动粘贴商品文本按 `ID[:：]\s*(\d+)` 解析）；跟播助手「上车/小黄车」=zs.kwaixiaodian.com/page/helper，是另一回事（架构记忆硬约束）。
- 关联 spec：`.trellis/spec/cdp-driver/backend/task-control.md`、`.trellis/spec/tauri-app/backend/ipc.md`。
- 关联记忆：`cloaksession-architecture`（CPS vs 上车区分）。
