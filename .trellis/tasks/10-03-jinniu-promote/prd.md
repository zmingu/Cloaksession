# jinniu-promote — 磁力金牛推广操作迁移

## Goal
迁移 jieger 的金牛推广操作（325 行）。Cloaksession 已有金牛账户隔离（business_accounts scope=jinniu），本任务补推广操作能力。

## jieger 源文件
- `electron/main/tasks/jinniuPromote/index.ts`（325 行）
- `electron/main/ipc/handlers/jinniuPromoteHandler.ts`（43 行）— IPC
- `electron/main/platforms/kuaishou/jinniuActions.ts` — 金牛操作动作
- `electron/main/platforms/kuaishou/jinniuSelectors.ts` — 金牛选择器

## 功能点
1. **金牛推广操作**：在 `niu.e.kuaishou.com/home?homeType=new` 执行推广相关动作。
2. **jinniuActions**：具体推广动作（决策在 design.md 确认具体是哪些操作——原项目是投流广告后台）。
3. **jinniuSelectors**：金牛后台选择器（顶栏 popover/账号信息/余额等）。
4. **账户上下文**：`__accountId__` 子户 ID 拼接到 URL。
5. **homeType=new 强制**：所有 URL 强制旧版（架构记忆硬约束）。

## 依赖
- 前置：`ks-platform-primitives`（KUAISHOU_JINNIU_CONFIG + buildStartupUrl）。
- 复用：business_accounts scope=jinniu（账户隔离已实现）。

## 验收标准
- [ ] 推广操作经 TaskPage 在金牛后台执行。
- [ ] `__accountId__` 子户上下文正确拼接。
- [ ] homeType=new 强制未破坏。
- [ ] 金牛 Profile 与小店 Profile 隔离未破坏（business_accounts scope）。
- [ ] 离线门禁通过；真号金牛推广验收单独确认（涉及广告账户写动作）。

## Out of Scope
- 不实现金牛账户隔离本身（已实现）。
- 不做账号选择弹窗自动子户搜索（ks-platform-primitives 已排除）。

## Notes
- **homeType 反命名陷阱**：快手命名是反的，new=旧版，super=新版；强制 new 避免新版 popover 结构变化（架构记忆）。
- jieger 的 jinniu task 有 1215 行（含 bindCreator），本任务只取 jinniuPromote 部分（325 行），bindCreator 拆到 bind-creator 子任务。
- 关联 spec：`.trellis/spec/cdp-driver/backend/task-control.md`、`.trellis/spec/profile-manager/backend/business-accounts.md`、`.trellis/spec/browser-launcher/backend/business-isolation.md`。
- 关联记忆：`cloaksession-architecture`（金牛独立 Profile）。
