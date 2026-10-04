import { invoke } from "@tauri-apps/api/core";

/**
 * 评论监听 (jieger `tasks/commentListener` port).
 *
 * Covers all 7 `comment_listener_*` / `comment_events_*` commands registered
 * in `crates/tauri-app/src/commands/comment_listener.rs`.
 *
 * ## Wire format (verified against Rust source, not the research doc)
 *
 * - Invoke channel = snake_case Rust function name; argument keys are
 *   camelCase (repo-wide Tauri convention, see `lib/ipc.ts`).
 * - `ListenerStatus` has `#[serde(rename_all = "camelCase")]` → camelCase.
 * - `CommentEvent` and `LiveEventRow` have **no** `rename_all`, so they stay
 *   **snake_case** on the wire (`send_at`, `user_id`, `account_id`,
 *   `msg_id`). The research doc (`command-surface.md §5`) lists them as
 *   camelCase (`sendAt`); that is wrong — source wins
 *   (`driver/comment_listener.rs:115`, `live_events.rs:18`, plus the
 *   `comment_event_serde_aligns_with_danmaku_script_schema` test asserting
 *   `v["send_at"]`). The danmaku-pipeline contract is snake_case too
 *   (`tools/danmaku-pipeline/schema/danmaku-script.schema.json#/$defs/event`).
 * - `send_at` is Unix epoch **seconds** (f64). Pass it through untouched —
 *   never convert to milliseconds.
 * - `CommentEventType` serializes as PascalCase variant names
 *   (`Comment` / `RoomEnter` / `RoomLike` / `RoomFollow` / `LiveOrder` /
 *   `LivePaid`); unknown wire strings leniently parse to `Comment`.
 */

/** Wire `CommentEventType` — PascalCase variant names. */
export type CommentEventType =
  | "Comment"
  | "RoomEnter"
  | "RoomLike"
  | "RoomFollow"
  | "LiveOrder"
  | "LivePaid";

/**
 * One live-room event. Field names are snake_case on the wire (see above).
 */
export interface CommentEvent {
  id: string;
  /** Rust field `event_type` renamed to `type`. */
  type: CommentEventType;
  /** Unix epoch seconds (f64) — authoritative send-time base. Not millis. */
  send_at: number;
  text: string;
  user_id?: string | null;
  nickname?: string | null;
  account_id: string;
  /** Wall-clock observation time (RFC 3339). */
  time: string;
}

/** `ListenerStatus` — camelCase on the wire (`rename_all = "camelCase"`). */
export interface ListenerStatus {
  accountId: string;
  running: boolean;
  mode: string;
  targetId: string;
  seenCount: number;
  recentCount: number;
  startedAt: string;
  lastEventAt?: string | null;
  lastError?: string | null;
}

/** Persisted `live_events` row — snake_case on the wire (no `rename_all`). */
export interface LiveEventRow {
  msg_id: string;
  /** Rust field `event_type` renamed to `type`; plain variant name string. */
  type: string;
  user_id?: string | null;
  nickname?: string | null;
  content: string;
  /** Wall-clock time (RFC 3339) when the event was observed. */
  time: string;
  /** Listening account (profile id) that observed the event. */
  account_id: string;
}

export const commentListener = {
  /**
   * `comment_listener_start` → `ListenerStatus`.
   * `targetId` optional; when omitted/empty the backend picks the live target.
   */
  start: (profileId: string, targetId?: string): Promise<ListenerStatus> =>
    invoke<ListenerStatus>("comment_listener_start", {
      profileId,
      targetId: targetId ?? null,
    }),

  /** `comment_listener_stop` → was-running. */
  stop: (profileId: string): Promise<boolean> =>
    invoke<boolean>("comment_listener_stop", { profileId }),

  /** `comment_listener_status` → status or null when never started. */
  status: (profileId: string): Promise<ListenerStatus | null> =>
    invoke<ListenerStatus | null>("comment_listener_status", { profileId }),

  /** `comment_listener_status_all` → every known listener snapshot. */
  statusAll: (): Promise<ListenerStatus[]> =>
    invoke<ListenerStatus[]>("comment_listener_status_all"),

  /**
   * `comment_events_recent` → in-memory recent events, newest first.
   * Backend clamps `limit` to 1–500 (default 100).
   */
  recent: (profileId: string, limit?: number): Promise<CommentEvent[]> =>
    invoke<CommentEvent[]>("comment_events_recent", {
      profileId,
      limit: limit ?? null,
    }),

  /**
   * `comment_events_history` → durable `live_events` rows, newest first.
   * Backend clamps `limit` to 1–500 (default 100).
   */
  history: (profileId: string, limit?: number): Promise<LiveEventRow[]> =>
    invoke<LiveEventRow[]>("comment_events_history", {
      profileId,
      limit: limit ?? null,
    }),

  /**
   * `comment_events_next` → blocks (server-side, up to `timeoutMs`,
   * capped at 30s, default 5s) for the next broadcast event, or null on
   * timeout. Prefer `recent()` polling for UI streams; use `next()` only
   * for in-process-style consumers that need wake-on-event semantics.
   */
  next: (timeoutMs?: number): Promise<CommentEvent | null> =>
    invoke<CommentEvent | null>("comment_events_next", {
      timeoutMs: timeoutMs ?? null,
    }),
};
