# auto-message — 主播互动（时间轴弹幕）迁移

## Goal
迁移 jieger 的主播互动：按时间轴发送互动弹幕，支持变量插值。在 sub-account 的小号会话上运行。

## jieger 源文件
- `electron/main/tasks/autoMessage/index.ts`（主） + `variables.ts`（共 252 行）
- `electron/main/ipc/handlers/autoMessageHandler.ts`（38 行）— IPC

## 功能点
1. **时间轴发送**：按预设时间轴（offset 秒）依次发送弹幕。
2. **变量插值**：`interpolate`（`variables.ts`）——模板变量替换（如 `{accountName}`/`{liveRoomUrl}` 等）。
3. **发送调度**：每条独立 setTimeout，绝对时间戳计算（避免漂移，与 scenePlay 一致）。
4. **状态广播**：发送进度经 IPC 推前端。
5. **start/stop**：注册到 taskRegistry。

## 依赖
- 前置：`sub-account`（小号会话 + sendDanmaku 接口）。
- 复用：`sub-account` 的 sendDanmaku。

## 验收标准
- [ ] 时间轴调度在 Rust（tokio 周期任务 + 绝对时间戳）。
- [ ] 变量插值等价。
- [ ] 调用 sub-account 的 sendDanmaku 接口。
- [ ] 支持 cancel。
- [ ] 离线门禁通过；真号发送验收单独确认。

## Out of Scope
- 不实现 sendDanmaku 本身（见 sub-account）。
- 不做关键词回复（见 auto-reply）。

## Notes
- 本任务依赖 sub-account 定义稳定的「发弹幕」接口。
- 关联 spec：`.trellis/spec/tauri-app/backend/ipc.md`。
