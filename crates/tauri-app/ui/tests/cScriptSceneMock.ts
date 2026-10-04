import type { Page } from "@playwright/test";
import { installTauriMock } from "./tauriMock";

/** In-memory scene row used by the C-group contract fake. */
interface MockSceneLine {
  id: number;
  sceneId: number;
  ord: number;
  message: string;
  timeOffsetSec: number;
  actionType: string;
}

interface MockScene {
  id: number;
  name: string;
  triggerMode: string;
  groupId: string | null;
  lines: MockSceneLine[];
  createdAt: number;
  updatedAt: number;
}

/**
 * Browser-local contract fake for the C-group (auto-message / auto-reply /
 * scene-play) 17 commands. Never launches a profile or contacts a platform.
 * Every request is recorded on `window.__TEST_CGROUP__.requests` for
 * assertions (including the no-character-spacing-injection guard).
 */
export async function installCGroupMock(page: Page): Promise<void> {
  await page.addInitScript(() => {
    localStorage.setItem("multizen.ui.section", JSON.stringify("business"));
    localStorage.setItem("multizen.ui.onboarded", "true");
  });
  await installTauriMock(page);
  await page.goto("/");
  await page.evaluate(() => {
    const internals = (window as any).__TAURI_INTERNALS__;
    const originalInvoke = internals.invoke;
    let nextSceneId = 1;
    let nextLineId = 1;
    const mock = {
      requests: [] as Array<{ command: string; args: Record<string, any> }>,
      scenes: [] as MockScene[],
    };
    Object.assign(window, { __TEST_CGROUP__: mock });
    internals.invoke = async (command: string, args: Record<string, any> = {}) => {
      const cgroup = new Set([
        "auto_message_start", "auto_message_stop", "auto_message_status",
        "auto_reply_preview", "auto_reply_history", "auto_reply_record",
        "scene_create", "scene_get", "scene_list", "scene_update", "scene_delete",
        "scene_add_line", "scene_update_line", "scene_delete_line",
        "scene_reorder_lines", "scene_play", "scene_stop",
      ]);
      if (!cgroup.has(command)) return originalInvoke(command, args);
      mock.requests.push({ command, args: structuredClone(args) });
      switch (command) {
        case "auto_message_start":
          return {
            run_id: "run-1",
            started_at: Date.now(),
            scheduled_count: (args.lines as unknown[]).length,
          };
        case "auto_message_stop":
          return null;
        case "auto_message_status":
          return { started_at: Date.now(), total_count: 1, sent_count: 0, schedule: [] };
        case "auto_reply_preview":
          return {
            ok: true,
            reply: `命中：${args.question}`,
            source: "knowledge",
            goodsId: "preview-goods",
            intent: null,
            error: null,
            knowledgeHit: true,
          };
        case "auto_reply_history":
          return [];
        case "auto_reply_record":
          return {
            id: 1, accountId: args.accountId, content: args.content,
            reply: args.reply, source: args.source, goodsId: args.goodsId ?? null,
            createdAt: new Date().toISOString(),
          };
        case "scene_create": {
          const scene: MockScene = {
            id: nextSceneId++,
            name: args.name,
            triggerMode: args.triggerMode ?? "relative-time",
            groupId: args.groupId ?? null,
            lines: [],
            createdAt: Date.now(),
            updatedAt: Date.now(),
          };
          mock.scenes.push(scene);
          return structuredClone({ ...scene, triggerMode: scene.triggerMode });
        }
        case "scene_get":
          return structuredClone(mock.scenes.find((s) => s.id === args.id) ?? null);
        case "scene_list":
          return structuredClone(mock.scenes);
        case "scene_update": {
          const scene = mock.scenes.find((s) => s.id === args.id);
          if (!scene) throw "场景不存在";
          if (args.name !== undefined && args.name !== null) scene.name = args.name;
          if (args.triggerMode !== undefined && args.triggerMode !== null) scene.triggerMode = args.triggerMode;
          // Tri-state: omitted key = keep; null = clear; string = set.
          if ("groupId" in args) scene.groupId = args.groupId;
          scene.updatedAt = Date.now();
          return structuredClone(scene);
        }
        case "scene_delete":
          mock.scenes = mock.scenes.filter((s) => s.id !== args.id);
          return null;
        case "scene_add_line": {
          const scene = mock.scenes.find((s) => s.id === args.sceneId);
          if (!scene) throw "场景不存在";
          const line: MockSceneLine = {
            id: nextLineId++,
            sceneId: scene.id,
            ord: scene.lines.length,
            message: args.message,
            timeOffsetSec: args.timeOffsetSec,
            actionType: args.actionType ?? "danmaku",
          };
          scene.lines.push(line);
          return structuredClone(line);
        }
        case "scene_update_line": {
          const line = mock.scenes.flatMap((s) => s.lines).find((l) => l.id === args.id);
          if (!line) throw "台词不存在";
          if (args.message !== undefined && args.message !== null) line.message = args.message;
          if (args.timeOffsetSec !== undefined && args.timeOffsetSec !== null) line.timeOffsetSec = args.timeOffsetSec;
          if (args.actionType !== undefined && args.actionType !== null) line.actionType = args.actionType;
          return structuredClone(line);
        }
        case "scene_delete_line":
          mock.scenes.forEach((s) => {
            s.lines = s.lines.filter((l) => l.id !== args.id);
          });
          return null;
        case "scene_reorder_lines": {
          const scene = mock.scenes.find((s) => s.id === args.sceneId);
          if (!scene) throw "场景不存在";
          const order = args.lineIds as number[];
          scene.lines.sort((a, b) => order.indexOf(a.id) - order.indexOf(b.id));
          scene.lines.forEach((l, i) => { l.ord = i; });
          return structuredClone(scene.lines);
        }
        case "scene_play": {
          const scene = mock.scenes.find((s) => s.id === args.sceneId);
          if (!scene) throw "场景不存在";
          return {
            sceneId: scene.id,
            scheduledCount: scene.lines.length,
            schedule: scene.lines.map((l) => ({
              lineId: l.id, ord: l.ord, accountId: "account-1", profileId: "fixture-profile",
              accountName: "小号", message: l.message, actionType: l.actionType,
              triggerAtMs: Date.now(),
            })),
          };
        }
        case "scene_stop":
          return true;
        default:
          throw new Error(`Unhandled C-group fixture command: ${command}`);
      }
    };
  });
}

/** Recorded C-group IPC requests (in call order). */
export async function cgroupRequests(page: Page): Promise<Array<{ command: string; args: Record<string, any> }>> {
  return page.evaluate(() => structuredClone((window as any).__TEST_CGROUP__.requests));
}
