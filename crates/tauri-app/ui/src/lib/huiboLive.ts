import { invoke } from "@tauri-apps/api/core";

/** jieger `HuiboVideo` (`src/types/entities.ts`) — wire is camelCase. */
export type HuiboVideoStatus = "processing" | "success" | "failed" | "live";

export interface HuiboVideo {
  /** replayId — extracted from the "去使用" click URL; empty when unavailable */
  id: string;
  name: string;
  /** upload time `YYYY.MM.DD HH:mm` */
  uploadTime: string;
  segments: number;
  /** total size display string, e.g. `5.8GB` */
  size: string;
  /** total duration display string, e.g. `1小时41分` (string, not seconds) */
  duration: string;
  status: HuiboVideoStatus;
  usageCount: number;
  /** replay expiry `YYYY-MM-DD HH:mm:ss` */
  expiryTime: string;
  goodsCount: number;
}

export type ShopLiveStatus = "unknown" | "offline" | "live";

export interface ShopLiveState {
  profileId: string;
  status: ShopLiveStatus;
  liveRoomUrl: string | null;
  /** ms epoch */
  updatedAt: number;
  error?: string;
}

/**
 * 跟播/回播 (jieger `tasks/huiboLive`).
 *
 * `startLive` only drives the flow to "去开播"; success is NOT proven by the
 * trailing wait — the returned `ShopLiveState` (re-read from the control
 * page) is the source of truth.
 */
export const huiboLive = {
  videoList: (profileId: string): Promise<HuiboVideo[]> =>
    invoke<HuiboVideo[]>("get_huibo_video_list", { profileId }),
  startLive: (profileId: string, replayId: string): Promise<ShopLiveState> =>
    invoke<ShopLiveState>("start_huibo_live", { profileId, replayId }),
  liveState: (profileId: string): Promise<ShopLiveState> =>
    invoke<ShopLiveState>("get_shop_live_state", { profileId }),
  cancel: (profileId: string): Promise<boolean> =>
    invoke<boolean>("cancel_huibo_task", { profileId }),
};
