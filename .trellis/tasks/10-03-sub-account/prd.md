# sub-account — 小号互动体系迁移（批量登录+互动）

## Goal
迁移 jieger 最大模块（1753 行）：小号 CRUD、批量登录、进直播间、发弹幕、互动历史、分组。是 auto-message/auto-reply/auto-popup/scene-play 的运行宿主。

## jieger 源文件
- `electron/main/tasks/subAccount/index.ts`（主） + `batchLogin.ts` + `groups.ts` + `importExport.ts` + `utils.ts`（共 1753 行）
- `electron/main/ipc/handlers/subAccountHandler.ts`（173 行）— IPC
- `electron/main/services/database/repositories/account.ts` — 小号 CRUD
- `electron/main/services/database/repositories/subAccountInteraction.ts` — 互动历史
- `electron/main/platforms/kuaishou/liveRoomActions.ts` — `sendDanmaku`/`waitForLiveRoomReady`/`isKuaishouLiveRoomUrl`
- `electron/main/services/store/configStore.ts` — `getAccountStorageDir`/`getActiveLiveAccountId` 等

## 功能点
1. **Account CRUD**：创建/读取/更新/删除小号（profileName/profileId/avatarUrl）。
2. **批量登录**：`batchLogin` 多账号扫码，登录等待（`KS_VERIFY_MS=20s`/`KS_LOGIN_MS=3min`/`KS_SUB_LOGIN_DETECT_MS=30s`/`KS_SUB_LOGIN_POLL_MS=3s`）。
3. **登录轮询**：后台 3s 轮询登录态（`subLoginPollTimers`）。
4. **进入直播间**：`enterLiveRoom`，`waitForLiveRoomReady`，随机延迟 3-8s 防风控，`RATE_LIMIT_RETRIES=5`。
5. **发弹幕**：`sendDanmaku`，`sendLocks` per-account 串行。
6. **互动历史**：`recordInteraction` 落库（totalSent/successCount/failCount/lastError/lastSendTime）。
7. **分组**：`groups.ts` 小号分组管理。
8. **导入导出**：`importExport.ts`（只迁功能，不搬旧数据）。
9. **运行时状态**：`liveRoomStatus`(idle/entering/entered/error) + stats。
10. **浏览器会话**：`getOrCreateSession`/`closeSession`/`persistSessionStorageState`，`browserCloseGrace=150ms` 宽限。

## 依赖
- 前置：`ks-platform-primitives`（小号观众平台配置 KUAISHOU_SUB_ACCOUNT_CONFIG）。
- 协作：`comment-listener`（监听评论流）。
- 复用：`profile-manager::business_accounts`（scope=kuaishou）——**不新增账号表**，小号作为 business_account 的一个 kind 或独立 kind（决策在 design.md）。

## 验收标准
- [ ] 小号 CRUD 复用 business_accounts（不新增表），或在 design.md 论证为何新增 kind。
- [ ] 批量登录 + 后台轮询 + cancel 在 Rust 等价（tokio::select! + 周期轮询）。
- [ ] 进直播间延迟/重试逻辑保留（防风控）。
- [ ] sendDanmaku per-account 串行（sendLocks 等价）。
- [ ] 互动历史落库。
- [ ] 分组管理。
- [ ] 离线门禁通过；真号小号登录/发弹幕验收单独确认（涉及真实账号写动作）。

## Out of Scope
- 不搬旧小号数据。
- 不做账号选择弹窗自动子户搜索（ks-platform-primitives 已排除）。
- 不实现 danmaku-pipeline。

## Notes
- 这是 4 大组的「运行宿主」，auto-message/auto-reply/auto-popup/scene-play 都在 sub-account 会话上运行。本任务定义「小号会话 + 发弹幕」的对外接口，下游 task 调用。
- jieger 小号登录是「观众身份」（kuaishou.com 主站），与「小店中控身份」（zs.kwaixiaodian.com）不同，不要混淆（见 `platformConfig.ts` 两套配置）。
- 关联 spec：`.trellis/spec/cdp-driver/backend/task-control.md`、`.trellis/spec/profile-manager/backend/business-accounts.md`、`.trellis/spec/tauri-app/backend/ipc.md`。
- 关联记忆：`cloaksession-architecture`（CPS vs 上车 vs 小号身份区分）。
