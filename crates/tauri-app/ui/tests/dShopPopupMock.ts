import type { Page } from "@playwright/test";
import { installTauriMock } from "./tauriMock";

/**
 * Browser-local fake for the 23 D-group commands. No profile launch,
 * no platform request — pure in-memory fixtures.
 *
 * - shop_product_script_*: 11 commands over an in-memory script table.
 * - shop_helper_*: 4 commands returning one fixture good per tab.
 * - auto_popup_*: 10 commands with a minimal running flag.
 */
export async function installDShopMock(page: Page): Promise<void> {
  await page.addInitScript(() => {
    localStorage.setItem("multizen.ui.section", JSON.stringify("live"));
    localStorage.setItem("multizen.ui.onboarded", "true");
  });
  await installTauriMock(page);
  await page.goto("/");
  await page.evaluate(() => {
    const internals = (window as any).__TAURI_INTERNALS__;
    const original = internals.invoke;
    const now = "2026-10-04T00:00:00Z";
    const mock = {
      requests: [] as Array<{ command: string; args: Record<string, any> }>,
      scripts: [
        { id: "script-1", name: "晚场 A", description: "fixture script", createdAt: now, updatedAt: now },
      ] as Array<Record<string, any>>,
      lines: {
        "script-1": [
          {
            id: "line-1", scriptId: "script-1", sortOrder: 0, action: "explain",
            goodsId: "1001", goodsName: "红苹果", videoTimeSec: 10, leadSec: 2,
            content: "好吃的苹果", createdAt: now, updatedAt: now,
          },
        ],
      } as Record<string, Array<Record<string, any>>>,
      popupRunning: false,
      playingScriptId: null as string | null,
      bindings: {} as Record<string, string>,
    };
    Object.assign(window, { __TEST_DSHOP__: mock });
    const lineCount = () => Object.values(mock.lines).reduce((n, rows) => n + rows.length, 0);

    internals.invoke = async (command: string, args: Record<string, any> = {}) => {
      if (
        command.startsWith("shop_product_script") ||
        command.startsWith("shop_helper_") ||
        command.startsWith("auto_popup_")
      ) {
        mock.requests.push({ command, args: structuredClone(args) });
      } else if (command === "business_accounts_list") {
        return [];
      } else if (command === "business_accounts_profile_state") {
        return { account: null, scope: null };
      } else {
        return original(command, args);
      }
      // --- product scripts (9) ---
      if (command === "shop_product_scripts_list") return structuredClone(mock.scripts);
      if (command === "shop_product_script_get") {
        const script = mock.scripts.find((s) => s.id === args.id) ?? null;
        if (!script) return null;
        return { script: structuredClone(script), lines: structuredClone(mock.lines[script.id] ?? []) };
      }
      if (command === "shop_product_script_create") {
        const next = {
          id: `script-${mock.scripts.length + 1}`,
          name: args.input.name,
          description: args.input.description ?? null,
          createdAt: now, updatedAt: now,
        };
        mock.scripts.unshift(next);
        mock.lines[next.id] = [];
        return structuredClone(next);
      }
      if (command === "shop_product_script_update") {
        const target = mock.scripts.find((s) => s.id === args.id);
        if (!target) throw `话术脚本 ${args.id} 不存在`;
        if (args.patch?.name !== undefined) target.name = args.patch.name;
        // Tri-state: missing key = keep; null = clear; string = set.
        if (args.patch && "description" in args.patch) target.description = args.patch.description;
        target.updatedAt = now;
        return structuredClone(target);
      }
      if (command === "shop_product_script_delete") {
        mock.scripts = mock.scripts.filter((s) => s.id !== args.id);
        delete mock.lines[args.id];
        return;
      }
      if (command === "shop_product_script_add_line") {
        const next = {
          id: `line-${lineCount() + 1}`,
          scriptId: args.input.scriptId,
          sortOrder: args.input.sortOrder ?? (mock.lines[args.input.scriptId]?.length ?? 0),
          action: args.input.action,
          goodsId: args.input.goodsId,
          goodsName: args.input.goodsName ?? null,
          videoTimeSec: args.input.videoTimeSec,
          leadSec: args.input.leadSec ?? 0,
          content: args.input.content ?? "",
          createdAt: now, updatedAt: now,
        };
        (mock.lines[next.scriptId] ??= []).push(next);
        return structuredClone(next);
      }
      if (command === "shop_product_script_update_line") {
        const row = Object.values(mock.lines).flat().find((l) => l.id === args.id);
        if (!row) throw `话术行 ${args.id} 不存在`;
        Object.assign(row, structuredClone(args.patch ?? {}));
        row.updatedAt = now;
        return structuredClone(row);
      }
      if (command === "shop_product_script_delete_line") {
        for (const key of Object.keys(mock.lines)) {
          mock.lines[key] = mock.lines[key].filter((l) => l.id !== args.id);
        }
        return;
      }
      if (command === "shop_product_script_reorder_lines") {
        const rows = mock.lines[args.scriptId] ?? [];
        const byId = new Map(rows.map((r) => [r.id, r]));
        const reordered = (args.orderedIds as string[]).map((id, i) => {
          const row = byId.get(id);
          if (!row) throw `话术行 ${id} 不属于该脚本`;
          return { ...row, sortOrder: i };
        });
        mock.lines[args.scriptId] = reordered;
        return structuredClone(reordered);
      }
      if (command === "shop_product_script_play") {
        const rows = mock.lines[args.scriptId] ?? [];
        if (rows.length === 0) throw "话术脚本没有话术行";
        mock.playingScriptId = args.scriptId;
        return { scriptId: args.scriptId, startedAtMs: 0, scheduledCount: rows.length };
      }
      if (command === "shop_product_script_stop") {
        const wasPlaying = mock.playingScriptId === args.scriptId;
        mock.playingScriptId = null;
        return wasPlaying;
      }
      // --- shop helper (4) ---
      const fixtureGood = (tab: string) => ({
        goodsId: "1001",
        goodsName: "红苹果",
        rawText: "红苹果 上车",
        availableActions: ["上车"],
        status: "available",
        sourceTab: tab,
      });
      if (command === "shop_helper_read_goods") return [fixtureGood(args.tab)];
      if (command === "shop_helper_switch_tab") return [fixtureGood(args.tab)];
      if (command === "shop_helper_add_to_cart" || command === "shop_helper_remove_from_cart") {
        const action = command === "shop_helper_add_to_cart" ? "on" : "off";
        return {
          ok: true, goodsId: args.goodsId, action,
          detail: `商品 ${args.goodsId}已执行并验证`,
          goods: [fixtureGood(action === "on" ? "inCart" : "toAdd")],
        };
      }
      // --- auto popup (10) ---
      const status = () => ({
        profileId: args.profileId,
        running: mock.popupRunning,
        queueLen: mock.popupRunning ? 1 : 0,
        lastGoodsId: null, lastError: null, updatedAt: now,
      });
      if (command === "auto_popup_start") {
        if (!args.config || !Array.isArray(args.config.interval)) throw "间隔范围无效";
        mock.popupRunning = true;
        return status();
      }
      if (command === "auto_popup_stop") {
        mock.popupRunning = false;
        return status();
      }
      if (command === "auto_popup_status") return status();
      if (command === "auto_popup_update_config") return status();
      if (command === "auto_popup_goods") {
        return [{ serial: "001", title: "苹果", price: "9.9" }];
      }
      if (command === "auto_popup_scan") {
        return {
          scannedCount: 1, diffs: [],
          candidates: { "001": { title: "苹果", price: "9.9" } },
        };
      }
      if (command === "auto_popup_explain_once") return;
      if (command === "auto_popup_register_shortcuts") {
        mock.bindings = { ...args.bindings };
        return { ok: true, registered: Object.keys(args.bindings), failed: [] };
      }
      if (command === "auto_popup_unregister_shortcuts") {
        mock.bindings = {};
        return;
      }
      if (command === "auto_popup_trigger_shortcut") {
        const goodsId = mock.bindings[args.accelerator];
        if (!goodsId) throw `快捷键未注册：${args.accelerator}`;
        return goodsId;
      }
      throw new Error(`Unhandled D-shop fixture IPC: ${command}`);
    };
  });
}

export async function dshopRequests(
  page: Page,
  command?: string,
): Promise<Array<{ command: string; args: Record<string, any> }>> {
  return page.evaluate((command) => {
    const all = (window as any).__TEST_DSHOP__.requests as Array<{ command: string; args: any }>;
    return command ? all.filter((r) => r.command === command) : all;
  }, command);
}

export async function openProfileEditor(page: Page, profileName = "Regression profile"): Promise<void> {
  await page.getByRole("button", { name: new RegExp(profileName) }).click();
  await page.getByRole("dialog").getByRole("button", { name: "General", exact: true }).click();
}
