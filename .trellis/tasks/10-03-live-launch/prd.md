# live-launch — 开播控制迁移

## Goal
迁移 jieger 的开播控制：本地视频循环推流（ffmpeg）、RTMP server/key 获取、直播伴侣推流启停。Rust 侧重写 ffmpeg 子进程管理 + TaskPage 取推流凭据。

## jieger 源文件
- `electron/main/tasks/liveLaunch/index.ts`（416 行）— 开播状态机 + ffmpeg 子进程 + heartbeats
- `electron/main/services/kuaishou/liveMate.ts` — `splitRtmpUrl`/`startLiveMatePush`/`stopLiveMatePush`
- `electron/main/ipc/handlers/liveLaunchHandler.ts`（34 行）— IPC

## 功能点
1. **StreamingStatus 状态机**：`idle|starting|streaming|stopping|stopped|error`。
2. **StreamCredentialsResult**：从直播中控页取 RTMP server/streamKey/liveStreamId（placeholder 标志位）。
3. **StartStreamingInput**：accountId + videoPath + rtmpServer + streamKey。
4. **ffmpeg 子进程**：spawn ffmpeg，本地视频循环推流到 RTMP；placeholder（黑屏+静音）参数；stderr tail 收集；exitCode/error 记录。
5. **heartbeats**：保活子进程。
6. **start/stop**：启动推流、停止推流（kill ffmpeg + 清理）。
7. **StreamingState 广播**：状态变化经 IPC 推到渲染端。

## 依赖
- 前置：`ks-platform-primitives`（connect/login 原语进中控页取凭据）。
- 协作：`mate-login`（直播伴侣登录态）。

## 验收标准
- [ ] ffmpeg 子进程管理在 Rust（tokio::process），支持 cancel + 状态广播。
- [ ] RTMP 凭据经 TaskPage 从中控页提取，不硬编码。
- [ ] placeholder 推流（无视频时黑屏静音）支持。
- [ ] 离线门禁通过；真实推流验收单独确认（涉及真实开播）。

## Out of Scope
- 不迁移 ffmpeg 二进制本身（用户环境自带）。
- 不做直播伴侣登录（见 mate-login）。

## Notes
- 真实开播是平台侧写动作，需单独授权验收。
- 关联 spec：`.trellis/spec/cdp-driver/backend/task-control.md`、`.trellis/spec/tauri-app/backend/ipc.md`。
