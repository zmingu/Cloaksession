import type { Page } from "@playwright/test";
import { installTauriMock } from "./tauriMock";
import type { BusinessAccount } from "../src/lib/businessAccounts";
import type {
  CommentEvent,
  ListenerStatus,
  LiveEventRow,
} from "../src/lib/commentListener";
import type {
  SubAccountInteraction,
  SubAccountLoginResult,
} from "../src/lib/subAccounts";

/**
 * Browser-local contract fake for B-group commands. Never launches a
 * profile or contacts a platform. Wire shapes mirror the Rust serde
 * exactly: `CommentEvent` / `LiveEventRow` are snake_case (no
 * `rename_all`), everything else is camelCase.
 */
export function subFixture(patch: Partial<BusinessAccount> = {}): BusinessAccount {
  return {
    id: "sub-1",
    kind: "kuaishou-sub",
    displayName: "小号甲",
    platformUserId: "ks-001",
    profileId: "fixture-profile",
    createdAt: "2026-10-01T00:00:00Z",
    updatedAt: "2026-10-01T00:00:00Z",
    ...patch,
  };
}

export async function installListenSubMock(page: Page): Promise<void> {
  await page.addInitScript(() => {
    localStorage.setItem("multizen.ui.section", JSON.stringify("live"));
    localStorage.setItem("multizen.ui.onboarded", "true");
  });
  await installTauriMock(page);
  await page.goto("/");
  await page.evaluate(() => {
    const internals = (window as any).__TAURI_INTERNALS__;
    const originalInvoke = internals.invoke;
    const mock = {
      running: {} as Record<string, ListenerStatus>,
      events: [] as CommentEvent[],
      historyRows: [] as LiveEventRow[],
      accounts: [] as BusinessAccount[],
      interactions: [] as SubAccountInteraction[],
      requests: [] as Array<{ command: string; args: Record<string, any> }>,
    };
    Object.assign(window, { __TEST_LISTENSUB__: mock });
    internals.invoke = async (command: string, args: Record<string, any> = {}) => {
      const handled =
        command.startsWith("comment_listener_") ||
        command.startsWith("comment_events_") ||
        command === "save_sub_account" ||
        command === "list_sub_accounts" ||
        command === "unbind_sub_account" ||
        command === "sub_account_login" ||
        command === "batch_login_sub_accounts" ||
        command === "sub_account_enter_live_room" ||
        command === "sub_account_send_danmaku" ||
        command === "sub_account_interactions";
      if (!handled) return originalInvoke(command, args);
      mock.requests.push({ command, args: structuredClone(args) });
      if (command === "comment_listener_start") {
        const status: ListenerStatus = {
          accountId: args.profileId,
          running: true,
          mode: "live",
          targetId: args.targetId ?? "target-live",
          seenCount: mock.events.length,
          recentCount: mock.events.length,
          startedAt: "2026-10-04T00:00:00Z",
          lastEventAt: null,
          lastError: null,
        };
        mock.running[args.profileId] = status;
        return structuredClone(status);
      }
      if (command === "comment_listener_stop") {
        const existed = mock.running[args.profileId]?.running ?? false;
        if (mock.running[args.profileId]) mock.running[args.profileId].running = false;
        return existed;
      }
      if (command === "comment_listener_status") {
        return structuredClone(mock.running[args.profileId] ?? null);
      }
      if (command === "comment_listener_status_all") {
        return structuredClone(Object.values(mock.running));
      }
      if (command === "comment_events_recent") {
        return structuredClone(mock.events.slice(0, args.limit ?? 100));
      }
      if (command === "comment_events_history") {
        return structuredClone(mock.historyRows.slice(0, args.limit ?? 100));
      }
      if (command === "comment_events_next") {
        return structuredClone(mock.events[0] ?? null);
      }
      if (command === "list_sub_accounts") return structuredClone(mock.accounts);
      if (command === "save_sub_account") {
        const input = args.input;
        const next: BusinessAccount = {
          ...input,
          id: input.id ?? `sub-${mock.accounts.length + 1}`,
          createdAt: "2026-10-04T00:00:00Z",
          updatedAt: "2026-10-04T00:00:00Z",
        };
        mock.accounts = [...mock.accounts.filter((a) => a.id !== next.id), next];
        return structuredClone(next);
      }
      if (command === "unbind_sub_account") {
        mock.accounts = mock.accounts.filter((a) => a.id !== args.id);
        return;
      }
      if (command === "sub_account_login") {
        const result: SubAccountLoginResult = { accountId: args.accountId, ok: true, error: null };
        return structuredClone(result);
      }
      if (command === "batch_login_sub_accounts") {
        return structuredClone(
          (args.accountIds as string[]).map(
            (id): SubAccountLoginResult => ({ accountId: id, ok: true, error: null }),
          ),
        );
      }
      if (command === "sub_account_enter_live_room") return;
      if (command === "sub_account_send_danmaku") {
        const row: SubAccountInteraction = {
          id: mock.interactions.length + 1,
          accountId: args.accountId,
          sceneId: null,
          action: "danmaku",
          message: args.content,
          liveRoomUrl: null,
          ok: true,
          error: null,
          durationMs: 120,
          createdAt: 1728000000000,
        };
        mock.interactions = [...mock.interactions, row];
        return structuredClone(row);
      }
      if (command === "sub_account_interactions") {
        return structuredClone(mock.interactions.filter((r) => r.accountId === args.accountId));
      }
      throw new Error(`Unhandled listen-sub fixture IPC: ${command}`);
    };
  });
  // NOTE: this deliberately does NOT enter the interact tab — the sub-account /
  // comment-listener panels mount eagerly now, so callers must seed the fixture
  // first and then call `enterInteract` to mount them against the fixture.
}

/** Mount the interact tab (互动账号脚本互动 hosts the scene-script panel). */
export async function enterInteract(page: Page): Promise<void> {
  await page.getByRole("tab", { name: /直播互动|Live interact/ }).click();
}

export async function setListenSubFixture(
  page: Page,
  patch: {
    events?: CommentEvent[];
    historyRows?: LiveEventRow[];
    accounts?: BusinessAccount[];
    interactions?: SubAccountInteraction[];
  },
): Promise<void> {
  await page.evaluate((patch) => {
    Object.assign((window as any).__TEST_LISTENSUB__, patch);
  }, patch);
}

export async function listenSubRequests(
  page: Page,
): Promise<Array<{ command: string; args: Record<string, any> }>> {
  return page.evaluate(() => (window as any).__TEST_LISTENSUB__.requests);
}
