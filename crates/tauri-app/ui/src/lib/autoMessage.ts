import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

import type {
  AutoMessageLine,
  AutoMessageStarted,
  AutoMessageState,
  AutoMessageStartOptions,
  AutoMessageStopped,
} from "../types";

/**
 * 主播互动（时间轴弹幕，jieger `tasks/autoMessage`）。
 *
 * Covers the 3 `auto_message_*` commands. Character-spacing injection is
 * deliberately NOT exposed here: no form, type, or IPC call in the C-group
 * passes it.
 */
export const autoMessage = {
  /**
   * `auto_message_start` → `AutoMessageStarted`.
   * `lines` fire at `started_at + offset_sec` against each line's
   * `account_id` session. Optional `nickname`/`anchor` override the
   * interpolation context; `startAt` pins the shared epoch-ms origin.
   */
  start: (
    lines: AutoMessageLine[],
    options?: AutoMessageStartOptions,
  ): Promise<AutoMessageStarted> =>
    invoke<AutoMessageStarted>("auto_message_start", {
      lines,
      nickname: options?.nickname ?? null,
      anchor: options?.anchor ?? null,
      startAt: options?.startAt ?? null,
    }),

  /** `auto_message_stop` → `()`. Idempotent (unknown/finished runs are Ok). */
  stop: (runId: string): Promise<void> =>
    invoke<void>("auto_message_stop", { runId }),

  /**
   * `auto_message_status` → `AutoMessageState`.
   * Finished/stopped runs are removed — querying them rejects (`not running`).
   */
  status: (runId: string): Promise<AutoMessageState> =>
    invoke<AutoMessageState>("auto_message_status", { runId }),
};

/** `auto-message:progress` push event (state snapshot after every dispatch). */
export function onAutoMessageProgress(
  cb: (state: AutoMessageState) => void,
): Promise<UnlistenFn> {
  return listen<AutoMessageState>("auto-message:progress", (event) => {
    cb(event.payload);
  });
}

/** `auto-message:stopped` push event (`{ run_id, reason }`). */
export function onAutoMessageStopped(
  cb: (stopped: AutoMessageStopped) => void,
): Promise<UnlistenFn> {
  return listen<AutoMessageStopped>("auto-message:stopped", (event) => {
    cb(event.payload);
  });
}
