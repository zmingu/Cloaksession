# scene-play — 场景剧本迁移

## Goal
迁移 jieger 的场景播放：每个场景 = 多条有序台词，配 (message, timeOffsetSec, actionType)。触发后取已登录小号池 round-robin 分配，按 actionType 派发（danmaku/like/follow）。

## jieger 源文件
- `electron/main/tasks/scenePlay/index.ts`（365 行）
- `electron/main/ipc/handlers/sceneHandler.ts`（132 行）— IPC
- `electron/main/services/database/repositories/scene.ts` — Scene/SceneLine/SceneLineAction CRUD
- `electron/main/platforms/kuaishou/liveRoomActions.ts` — `sendDanmaku`/`clickLike`/`clickFollow`

## 功能点
1. **Scene 模型**：多条有序 SceneLine，每条 (message, timeOffsetSec, actionType: danmaku|like|follow)。
2. **触发模式**：`relative-time`（启动时刻为 0 点）+ `local-time`（当天目标时刻）。
3. **调度**：每条独立 setTimeout，绝对时间戳计算（避免漂移）。
4. **小号分配**：取已登录小号池（按 scene.groupId 过滤），按 N % poolSize round-robin 分配。
5. **actionType 派发**：danmaku → sendDanmaku；like → clickLike（幂等）；follow → clickFollow（幂等）。
6. **互动落库**：`recordInteraction` 每条派发结果。
7. **ScenePlayState 广播**：sceneId/startedAt/totalCount/sentCount/schedule。

## 依赖
- 前置：`sub-account`（小号池 + sendDanmaku/clickLike/clickFollow 接口）。
- 协作：`live-room-monitor`（监控触发 scenePlay）。

## 验收标准
- [ ] 两种触发模式（relative/local-time）等价。
- [ ] round-robin 分配 + groupId 过滤。
- [ ] clickLike/clickFollow 幂等。
- [ ] 每条派发结果落库。
- [ ] 支持 cancel。
- [ ] 离线门禁通过；真号场景播放验收单独确认。

## Out of Scope
- 不实现 sendDanmaku/clickLike/clickFollow（见 sub-account）。
- 不实现 Scene CRUD 表（复用 settings-store 或新增表，决策在 design.md）。

## Notes
- 本任务与 auto-message 都是「按时间轴派发」，但 scene-play 多了 actionType（like/follow）与小号分配。可考虑共用调度底层（design.md 决策）。
- 关联 spec：`.trellis/spec/tauri-app/backend/ipc.md`。
