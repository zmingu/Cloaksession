# live-room-monitor — 直播间实时监控迁移

## Goal
迁移 jieger 的直播间监控：检测开播状态、触发小号互动场景与自动上车脚本的联动启停。

## jieger 源文件
- `electron/main/tasks/liveRoomMonitor/index.ts`（777 行）— 监控状态机 + 开播检测 + 联动触发
- `electron/main/ipc/handlers/liveRoomMonitorHandler.ts`（76 行）— IPC
- `electron/main/platforms/kuaishou/liveRoomActions.ts` — `isKuaishouLiveRoomUrl` 等观众侧操作
- `electron/main/platforms/kuaishou/huiboActions.ts` / `huiboSelectors.ts` — 跟播相关

## 功能点
1. **LiveRoomMonitorStatus**：`idle|checking|offline|live|triggering|triggered|error`。
2. **LiveRoomLiveStatus**：`unknown|offline|live`。
3. **StartLiveRoomMonitorInput**：liveRoomUrl + sceneId + groupId? + productScriptId? + productScriptAccountId? + autoExitSubAccounts?。
4. **监控循环**：周期检查直播间是否开播（轮询直播间页/接口）。
5. **联动触发**：检测到开播 → 触发 `scenePlay`（按 sceneId/groupId 拉起小号互动）+ 触发 `shopProductScript`（自动上车脚本）。
6. **autoExitSubAccounts**：直播结束后自动断开/退出范围内小号浏览器会话。
7. **状态广播**：status/liveStatus/lastCheckedAt/nextCheckAt/lastTriggeredAt/lastEnterAllResult 经 IPC 推前端。
8. **enteringRooms/exitingRooms**：批量进/退直播间状态。

## 依赖
- 前置：`ks-platform-primitives`。
- 协作（触发联动）：`sub-account`（小号池）、`scene-play`、`shop-product-script`、`huibo-live`。

## 验收标准
- [ ] 监控循环在 Rust（tokio 周期任务），支持 cancel。
- [ ] 开播检测逻辑经 TaskPage 或 HTTP 接口确认。
- [ ] 联动触发接口（scene/shopProductScript）可调用，触发结果回传。
- [ ] autoExitSubAccounts 退出小号会话正确。
- [ ] 离线门禁通过；真号直播间监控验收单独确认。

## Out of Scope
- 不实现 scenePlay/shopProductScript 本身（见各自子任务）。
- 不做 liveStats（统计，暂缓）。

## Notes
- 这是直播中控的「调度中枢」，依赖多个下游 task；建议在本任务定义清晰的「触发接口」契约，下游 task 实现该接口。
- 关联 spec：`.trellis/spec/cdp-driver/backend/task-control.md`、`.trellis/spec/tauri-app/backend/ipc.md`。
