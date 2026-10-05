/**
 * Live-room monitor IPC (commands/live_room_monitor.rs — 3 commands).
 * Note: 新契约下 `stop_live_room_monitor` / `get_live_room_monitor_state`
 * 都按 `profileId` 定位单个监控实例（上游 node-1 改版；旧版本无参）。
 */
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { LiveRoomMonitorState, MonitorConfig } from "../types";

export const LIVE_ROOM_MONITOR_STATE_CHANGED = "live-room-monitor-state-changed";

export const liveRoomMonitor = {
  /** `start_live_room_monitor` → starts polling / triggering. */
  start: (profileId: string, config: MonitorConfig): Promise<LiveRoomMonitorState> =>
    invoke<LiveRoomMonitorState>("start_live_room_monitor", { profileId, config }),

  /** `stop_live_room_monitor` → 按 profileId 停止该账号的监控。 */
  stop: (profileId: string): Promise<LiveRoomMonitorState> =>
    invoke<LiveRoomMonitorState>("stop_live_room_monitor", { profileId }),

  /** `get_live_room_monitor_state` → 按 profileId 取快照。 */
  state: (profileId: string): Promise<LiveRoomMonitorState> =>
    invoke<LiveRoomMonitorState>("get_live_room_monitor_state", { profileId }),

  /** `list_live_room_monitor_states` → 一次拉全所有账号的监控槽快照（无参）。 */
  states: (): Promise<LiveRoomMonitorState[]> =>
    invoke<LiveRoomMonitorState[]>("list_live_room_monitor_states"),
};

/** Subscribe to `live-room-monitor-state-changed` (await register/cleanup). */
export function onLiveRoomMonitorStateChanged(
  cb: (s: LiveRoomMonitorState) => void,
): Promise<UnlistenFn> {
  return listen<LiveRoomMonitorState>(LIVE_ROOM_MONITOR_STATE_CHANGED, (event) => {
    cb(event.payload);
  });
}
