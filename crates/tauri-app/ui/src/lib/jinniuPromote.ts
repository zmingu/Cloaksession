import { invoke } from "@tauri-apps/api/core";

import type {
  JinniuLiveUser,
  JinniuLiveUsers,
  StoreCreatePhase1Config,
  StoreCreateTab,
} from "../types";

export type {
  JinniuLiveUser,
  JinniuLiveUsers,
  StoreCreatePhase1Config,
  StoreCreateTab,
};

/**
 * 磁力金牛推广操作 (jieger `tasks/jinniuPromote` port).
 *
 * All six commands run against the Jinniu backend pages
 * (`niu.e.kuaishou.com`, `homeType=new` legacy generation enforced
 * Rust-side). `profileId` resolves the live browser session.
 *
 * Write operations (`applyPhase1` / `applyPhase2` / `submit` /
 * `selectLiveUser`) drive real ad-account state — callers must gate them
 * behind an explicit user confirmation.
 */
export const jinniuPromote = {
  /** Open (or reuse) the 全站推广创编 tab. Rejects when `homeType=new` is missing. */
  openStoreCreate: (profileId: string): Promise<StoreCreateTab> =>
    invoke<StoreCreateTab>("jinniu_promote_open_store_create", { profileId }),

  /** List promotable live users for the current sub-account context. */
  liveUsers: (profileId: string): Promise<JinniuLiveUsers> =>
    invoke<JinniuLiveUsers>("jinniu_promote_live_users", { profileId }),

  /** Select one promotable live user by `uid`. */
  selectLiveUser: (profileId: string, uid: string): Promise<JinniuLiveUser> =>
    invoke<JinniuLiveUser>("jinniu_promote_select_live_user", { profileId, uid }),

  /** Apply phase-1 config (net-ROI / budget / promote-type …) then enter the video library. */
  applyPhase1: (profileId: string, config: StoreCreatePhase1Config): Promise<void> =>
    invoke<void>("jinniu_promote_apply_phase1", { profileId, config }),

  /** Apply phase-2 (creative assembly). Returns generated ad-copy texts. */
  applyPhase2: (profileId: string): Promise<string[]> =>
    invoke<string[]>("jinniu_promote_apply_phase2", { profileId }),

  /** Submit the store-create plan. */
  submit: (profileId: string): Promise<void> =>
    invoke<void>("jinniu_promote_submit", { profileId }),
};
