/**
 * Live launch IPC.
 *
 * - profile 模式（历史路径，浏览器抓中控页）：`live_launch_*` 7 条命令；
 * - mate 模式（直播伴侣账号，**不启浏览器**，对齐 jieger）：`live_launch_mate_*`
 *   3 条命令，以 `mateAccountId` 为键。
 *
 * streamKey is only passed into invoke args / ffmpeg argv, never logged.
 */
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  LiveLaunchStateChanged,
  PrerequisitesReport,
  StreamCredentials,
  StreamingState,
} from "../types";

export const LIVE_LAUNCH_STATE_CHANGED = "live-launch-state-changed";

export const liveLaunch = {
  /** `live_launch_status` → per-profile snapshot. */
  status: (profileId: string): Promise<StreamingState> =>
    invoke<StreamingState>("live_launch_status", { profileId }),

  /** `live_launch_prerequisites` → no args (uses AppHandle resource_dir). */
  prerequisites: (): Promise<PrerequisitesReport> =>
    invoke<PrerequisitesReport>("live_launch_prerequisites"),

  /** `live_launch_credentials` → fetch RTMP credentials for the control page. */
  credentials: (profileId: string, controlUrl?: string): Promise<StreamCredentials> =>
    invoke<StreamCredentials>("live_launch_credentials", {
      profileId,
      controlUrl: controlUrl ?? null,
    }),

  /** `live_launch_heartbeat_start` → placeholder (black-screen) stream. */
  heartbeatStart: (profileId: string, controlUrl?: string): Promise<StreamingState> =>
    invoke<StreamingState>("live_launch_heartbeat_start", {
      profileId,
      controlUrl: controlUrl ?? null,
    }),

  /** `live_launch_heartbeat_stop` → stops heartbeat / stream for the profile. */
  heartbeatStop: (profileId: string): Promise<StreamingState> =>
    invoke<StreamingState>("live_launch_heartbeat_stop", { profileId }),

  /** `live_launch_stream_start` → local-video loop push. */
  streamStart: (
    profileId: string,
    videoPath: string,
    controlUrl?: string,
  ): Promise<StreamingState> =>
    invoke<StreamingState>("live_launch_stream_start", {
      profileId,
      videoPath,
      controlUrl: controlUrl ?? null,
    }),

  /** `live_launch_stream_stop` → stops the active push. */
  streamStop: (profileId: string): Promise<StreamingState> =>
    invoke<StreamingState>("live_launch_stream_stop", { profileId }),

  // --- mate（伴侣）模式 ------------------------------------------------------

  /**
   * `live_launch_mate_credentials` → 伴侣账号取流（liveMate 三步开播，不启浏览器）。
   * 返回 `placeholder=false` 的真实 RTMP 凭据（含 `liveStreamId`）。
   */
  mateCredentials: (mateAccountId: string): Promise<StreamCredentials> =>
    invoke<StreamCredentials>("live_launch_mate_credentials", { mateAccountId }),

  /** `live_launch_mate_stream_start` → 取流 + 本地视频循环推流。 */
  mateStreamStart: (mateAccountId: string, videoPath: string): Promise<StreamingState> =>
    invoke<StreamingState>("live_launch_mate_stream_start", { mateAccountId, videoPath }),

  /** `live_launch_mate_stream_stop` → 先关播再停 ffmpeg。 */
  mateStreamStop: (mateAccountId: string): Promise<StreamingState> =>
    invoke<StreamingState>("live_launch_mate_stream_stop", { mateAccountId }),
};

/** Subscribe to `live-launch-state-changed` (await register/cleanup). */
export function onLiveLaunchStateChanged(
  cb: (s: LiveLaunchStateChanged) => void,
): Promise<UnlistenFn> {
  return listen<LiveLaunchStateChanged>(LIVE_LAUNCH_STATE_CHANGED, (event) => {
    cb(event.payload);
  });
}
