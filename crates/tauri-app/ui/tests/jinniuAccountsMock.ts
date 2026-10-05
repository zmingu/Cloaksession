import type { Page } from "@playwright/test";
import { defaultSettings, installTauriMock } from "./tauriMock";
import type { JinniuAccountWithStatus, JinniuStatePayload } from "../src/types";

/**
 * Browser-local contract fake for the 8 Jinniu master-account commands
 * (`commands/jinniu.rs`). Pure in-memory state: never launches a profile,
 * never opens a browser, never contacts a platform.
 *
 * Wire shapes mirror the Rust serde exactly: every struct is `camelCase`;
 * `status` is the kebab-case `JinniuStatus` string.
 *
 * `jinniu-status-changed` / `jinniu-accounts-changed` are delivered by the test
 * through `window.__TEST_IPC__.emit(...)` (the base mock's listener registry).
 */
export function jinniuFixture(patch: Partial<JinniuAccountWithStatus> = {}): JinniuAccountWithStatus {
  return {
    id: "jinniu-1",
    profileId: "profile-jinniu-1",
    label: "大户甲",
    status: "disconnected",
    isActive: false,
    error: null,
    targetAccountId: null,
    masterName: null,
    masterId: null,
    masterAvatarUrl: null,
    currentSubAccountId: null,
    currentSubAccountName: null,
    balanceText: null,
    ...patch,
  };
}

const COMMANDS = [
  "jinniu_accounts_list",
  "jinniu_account_add",
  "jinniu_account_remove",
  "jinniu_account_set_active",
  "jinniu_account_get_active",
  "jinniu_login",
  "jinniu_disconnect",
  "jinniu_status",
] as const;

/** Boot straight into the Jinniu section with a seeded account list. */
export async function installJinniuMock(
  page: Page,
  accounts: JinniuAccountWithStatus[] = [],
  language: "en" | "zh-CN" = "en",
): Promise<void> {
  await installTauriMock(page, { ...defaultSettings, language });
  // The Jinniu section mounts on first paint, so its `jinniu_accounts_list`
  // call fires before any post-navigation `page.evaluate` could wrap invoke.
  // Register the wrapper as an init script (runs after the base mock's, in
  // registration order) so the very first load already sees the fixture.
  await page.addInitScript(
    ({ seed, commands, section }) => {
      localStorage.setItem("multizen.ui.section", JSON.stringify(section));
      const internals = (window as any).__TAURI_INTERNALS__;
      const original = internals.invoke;
      const mock = {
        accounts: structuredClone(seed) as JinniuAccountWithStatus[],
        requests: [] as Array<{ command: string; args: Record<string, any> }>,
        failNext: {} as Record<string, string>,
      };
      Object.assign(window, { __TEST_JINNIU__: mock });

      const payload = (a: JinniuAccountWithStatus): JinniuStatePayload => ({
        accountId: a.id,
        status: a.status,
        targetAccountId: a.targetAccountId ?? null,
        error: a.error ?? null,
        master:
          a.masterName && a.masterId
            ? { name: a.masterName, id: a.masterId, avatarUrl: a.masterAvatarUrl ?? null }
            : null,
        currentSubAccountId: a.currentSubAccountId ?? null,
        currentSubAccountName: a.currentSubAccountName ?? null,
        balanceText: a.balanceText ?? null,
      });
      const find = (id: string): JinniuAccountWithStatus => {
        const target = mock.accounts.find((a) => a.id === id);
        if (!target) throw `金牛大户 ${id} 不存在，请刷新后重试`;
        return target;
      };

      internals.invoke = async (command: string, args: Record<string, any> = {}) => {
        if (!(commands as readonly string[]).includes(command)) return original(command, args);
        mock.requests.push({ command, args: structuredClone(args) });
        if (mock.failNext[command]) {
          const message = mock.failNext[command];
          delete mock.failNext[command];
          throw message; // Tauri rejects Result<_, String> with a string.
        }
        switch (command) {
          case "jinniu_accounts_list":
            return structuredClone(mock.accounts);
          case "jinniu_account_add": {
            const next: JinniuAccountWithStatus = {
              id: `jinniu-${mock.accounts.length + 1}`,
              profileId: `profile-jinniu-${mock.accounts.length + 1}`,
              label: String(args.label),
              status: "disconnected",
              isActive: mock.accounts.length === 0,
              error: null,
              targetAccountId: null,
              masterName: null,
              masterId: null,
              masterAvatarUrl: null,
              currentSubAccountId: null,
              currentSubAccountName: null,
              balanceText: null,
            };
            mock.accounts = [...mock.accounts, next];
            return structuredClone(next);
          }
          case "jinniu_account_remove": {
            const removed = find(String(args.id));
            mock.accounts = mock.accounts.filter((a) => a.id !== removed.id);
            if (removed.isActive && mock.accounts.length > 0) mock.accounts[0].isActive = true;
            return;
          }
          case "jinniu_account_set_active": {
            const id = (args.id as string | null) ?? null;
            if (id !== null) find(id);
            for (const account of mock.accounts) account.isActive = account.id === id;
            return id;
          }
          case "jinniu_account_get_active":
            return mock.accounts.find((a) => a.isActive)?.id ?? null;
          case "jinniu_login": {
            const account = find(String(args.id));
            account.status = "connecting";
            account.error = null;
            return structuredClone(payload(account));
          }
          case "jinniu_disconnect": {
            const account = find(String(args.id));
            account.status = "disconnected";
            account.error = null;
            account.targetAccountId = null;
            account.masterName = null;
            account.masterId = null;
            account.masterAvatarUrl = null;
            account.currentSubAccountId = null;
            account.currentSubAccountName = null;
            account.balanceText = null;
            return structuredClone(payload(account));
          }
          case "jinniu_status":
            return structuredClone(payload(find(String(args.id))));
          default:
            throw new Error(`Unhandled jinniu fixture IPC: ${command}`);
        }
      };
    },
    { seed: accounts, commands: COMMANDS as unknown as string[], section: "jinniu" },
  );
  await page.goto("/");
}

export async function jinniuRequests(
  page: Page,
  command?: string,
): Promise<Array<{ command: string; args: Record<string, any> }>> {
  return page.evaluate((cmd) => {
    const all = (window as any).__TEST_JINNIU__.requests as Array<{ command: string; args: any }>;
    return cmd ? all.filter((r) => r.command === cmd) : all;
  }, command ?? null);
}

/** Replace the fixture's account list in place (used before emitting an event). */
export async function setJinniuAccounts(
  page: Page,
  accounts: JinniuAccountWithStatus[],
): Promise<void> {
  await page.evaluate((next) => {
    (window as any).__TEST_JINNIU__.accounts = next;
  }, accounts);
}
