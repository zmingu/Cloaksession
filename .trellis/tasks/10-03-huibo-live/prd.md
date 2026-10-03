# huibo-live — 跟播/回播迁移

## Goal
迁移 jieger 的跟播/回播（51 行，最小模块）。

## jieger 源文件
- `electron/main/tasks/huiboLive/index.ts`（51 行）
- `electron/main/ipc/handlers/huiboLiveHandler.ts`（25 行）— IPC
- `electron/main/platforms/kuaishou/huiboActions.ts` — 跟播动作
- `electron/main/platforms/kuaishou/huiboSelectors.ts` — 跟播选择器

## 功能点
1. **跟播/回播能力**：跟随直播或回放直播相关操作（具体语义在 design.md 确认——51 行最小模块）。
2. **huiboActions**：跟播动作。
3. **huiboSelectors**：跟播选择器。

## 依赖
- 前置：`ks-platform-primitives`。
- 协作：`live-room-monitor`（监控联动跟播）。

## 验收标准
- [ ] 跟播动作经 TaskPage 执行。
- [ ] 支持 cancel。
- [ ] 离线门禁通过；真号跟播验收单独确认。

## Out of Scope
- 不实现 live-room-monitor（见该子任务）。

## Notes
- jieger 最小模块（51 行），细节需在 design.md 对照源码确认。
- 关联 spec：`.trellis/spec/cdp-driver/backend/task-control.md`、`.trellis/spec/tauri-app/backend/ipc.md`。
