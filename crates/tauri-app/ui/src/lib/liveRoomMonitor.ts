/**
 * Live-room monitor IPC (commands/live_room_monitor.rs — 3 commands).
 * Note: `stop_live_room_monitor` takes NO profile_id; `get_*_state` is sync
 * with no args.
 */
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { LiveRoomMonitorState, MonitorConfig } from "../types";

export const LIVE_ROOM_MONITOR_STATE_CHANGED = "live-room-monitor-state-changed";

export const liveRoomMonitor = {
  /** `start_live_room_monitor` → starts polling / triggering. */
  start: (profileId: string, config: MonitorConfig): Promise<LiveRoomMonitorState> =>
    invoke<LiveRoomMonitorState>("start_live_room_monitor", { profileId, config }),

  /** `stop_live_room_monitor` → no args. */
  stop: (): Promise<LiveRoomMonitorState> =>
    invoke<LiveRoomMonitorState>("stop_live_room_monitor"),

  /** `get_live_room_monitor_state` → sync snapshot, no args. */
  state: (): Promise<LiveRoomMonitorState> =>
    invoke<LiveRoomMonitorState>("get_live_room_monitor_state"),
};

/** Subscribe to `live-room-monitor-state-changed` (await register/cleanup). */
export function onLiveRoomMonitorStateChanged(
  cb: (s: LiveRoomMonitorState) => void,
): Promise<UnlistenFn> {
  return listen<LiveRoomMonitorState>(LIVE_ROOM_MONITOR_STATE_CHANGED, (event) => {
    cb(event.payload);
  });
}
