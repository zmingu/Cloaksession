import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

import type {
  PlaySceneOptions,
  PlayStarted,
  Scene,
  SceneFinishedPayload,
  SceneGroupIdPatch,
  SceneLine,
  SceneLineAction,
  SceneProgressPayload,
  SceneStartedPayload,
  SceneTriggerMode,
} from "../types";

/**
 * 场景剧本（jieger `tasks/scenePlay`）。
 *
 * Covers the 11 `scene_*` commands: Scene/SceneLine CRUD plus
 * play/stop. `scene_update`'s `group_id` is a Rust
 * `Option<Option<String>>` — the tri-state is expressed with
 * `SceneGroupIdPatch` (`keep` omits the field, `clear` sends null,
 * `set` sends the new id) so callers can never confuse "leave alone"
 * with "clear".
 */
export const scenes = {
  /** `scene_create` → `Scene`. */
  create: (
    name: string,
    triggerMode?: SceneTriggerMode | null,
    groupId?: string | null,
  ): Promise<Scene> =>
    invoke<Scene>("scene_create", {
      name,
      triggerMode: triggerMode ?? null,
      groupId: groupId ?? null,
    }),

  /** `scene_get` → `Scene | null`. */
  get: (id: number): Promise<Scene | null> =>
    invoke<Scene | null>("scene_get", { id }),

  /** `scene_list` → `Scene[]`. */
  list: (): Promise<Scene[]> => invoke<Scene[]>("scene_list"),

  /** `scene_update` → `Scene`. `groupId` tri-state via `SceneGroupIdPatch`. */
  update: (
    id: number,
    patch: {
      name?: string | null;
      triggerMode?: SceneTriggerMode | null;
      groupId?: SceneGroupIdPatch;
    },
  ): Promise<Scene> => {
    const groupId =
      patch.groupId == null || patch.groupId.mode === "keep"
        ? undefined
        : patch.groupId.mode === "clear"
          ? null
          : patch.groupId.groupId;
    return invoke<Scene>("scene_update", {
      id,
      name: patch.name ?? null,
      triggerMode: patch.triggerMode ?? null,
      // `undefined` keys are dropped from the JSON args, decoding as the
      // outer `None` ("keep"); `null` decodes as "clear".
      ...(groupId === undefined ? {} : { groupId }),
    });
  },

  /** `scene_delete` → `()`. Cascades to the scene's lines. */
  delete: (id: number): Promise<void> =>
    invoke<void>("scene_delete", { id }),

  /** `scene_add_line` → `SceneLine` (appended at max ord + 1). */
  addLine: (
    sceneId: number,
    message: string,
    timeOffsetSec: number,
    actionType?: SceneLineAction | null,
  ): Promise<SceneLine> =>
    invoke<SceneLine>("scene_add_line", {
      sceneId,
      message,
      timeOffsetSec,
      actionType: actionType ?? null,
    }),

  /** `scene_update_line` → `SceneLine`. */
  updateLine: (
    id: number,
    patch: {
      message?: string | null;
      timeOffsetSec?: number | null;
      actionType?: SceneLineAction | null;
    },
  ): Promise<SceneLine> =>
    invoke<SceneLine>("scene_update_line", {
      id,
      message: patch.message ?? null,
      timeOffsetSec: patch.timeOffsetSec ?? null,
      actionType: patch.actionType ?? null,
    }),

  /** `scene_delete_line` → `()`. */
  deleteLine: (id: number): Promise<void> =>
    invoke<void>("scene_delete_line", { id }),

  /** `scene_reorder_lines` → `SceneLine[]` (new ord order). */
  reorderLines: (sceneId: number, lineIds: number[]): Promise<SceneLine[]> =>
    invoke<SceneLine[]>("scene_reorder_lines", { sceneId, lineIds }),

  /** `scene_play` → `PlayStarted`. Emits `scene:started`, then progress. */
  play: (sceneId: number, options?: PlaySceneOptions): Promise<PlayStarted> =>
    invoke<PlayStarted>("scene_play", {
      sceneId,
      options: options ?? null,
    }),

  /** `scene_stop` → `boolean` (false = nothing was playing). */
  stop: (sceneId: number): Promise<boolean> =>
    invoke<boolean>("scene_stop", { sceneId }),
};

/** `scene:started` push event (`{ scene_id, schedule, started_at }`). */
export function onSceneStarted(
  cb: (payload: SceneStartedPayload) => void,
): Promise<UnlistenFn> {
  return listen<SceneStartedPayload>("scene:started", (event) => {
    cb(event.payload);
  });
}

/** `scene:progress` push event (per dispatched line). */
export function onSceneProgress(
  cb: (payload: SceneProgressPayload) => void,
): Promise<UnlistenFn> {
  return listen<SceneProgressPayload>("scene:progress", (event) => {
    cb(event.payload);
  });
}

/** `scene:finished` push event (`{ scene_id, stopped?, reason? }`). */
export function onSceneFinished(
  cb: (payload: SceneFinishedPayload) => void,
): Promise<UnlistenFn> {
  return listen<SceneFinishedPayload>("scene:finished", (event) => {
    cb(event.payload);
  });
}
