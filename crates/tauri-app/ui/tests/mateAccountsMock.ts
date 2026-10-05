import type { Page } from "@playwright/test";
import { defaultSettings, installTauriMock } from "./tauriMock";
import type { MateAccount, MateLoginStage, MateLoginState } from "../src/types";

/**
 * Browser-local contract fake for the 7 mate commands (`commands/mate_login.rs`).
 * Pure in-memory state: never launches a profile, never opens a browser, never
 * scans a real QR code.
 *
 * Wire shapes mirror the Rust serde exactly: `MateAccount` is `camelCase` (the
 * `mate_accounts` row); `MateLoginState.stage` is the kebab-case
 * `MateLoginStage` string.
 *
 * `mate-login-state-changed` is delivered by the test through
 * `window.__TEST_IPC__.emit(...)` (the base mock's listener registry).
 */
export function mateFixture(patch: Partial<MateAccount> = {}): MateAccount {
  return {
    id: "mate-1",
    label: "伴侣甲",
    platformUserId: null,
    userName: null,
    avatarUrl: null,
    matePassToken: null,
    mateToken: null,
    mateSt: null,
    mateH5St: null,
    mateLmtoken: null,
    loginAt: null,
    createdAt: "2026-01-01T00:00:00.000Z",
    updatedAt: "2026-01-01T00:00:00.000Z",
    ...patch,
  };
}

/**
 * The alias the backend stamps on a row created without a `label`
 * (`mate_account_add` with an empty payload) — mirrors `profile-manager`'s
 * placeholder. The account keeps it until its first successful scan, which
 * overwrites the alias with the platform nickname.
 */
export const MATE_PLACEHOLDER_LABEL = "未命名伴侣";

export const MATE_COMMANDS = [
  "mate_accounts_list",
  "mate_account_add",
  "mate_account_remove",
  "mate_account_rename",
  "mate_login_start",
  "mate_login_cancel",
  "mate_login_state",
] as const;

/** Boot straight into 快手账号 › 直播伴侣 with a seeded account list. */
export async function installMateMock(
  page: Page,
  accounts: MateAccount[] = [],
  language: "en" | "zh-CN" = "en",
): Promise<void> {
  await installTauriMock(page, { ...defaultSettings, language });
  // The mate tab mounts on first paint, so its `mate_accounts_list` call fires
  // before any post-navigation `page.evaluate` could wrap invoke. Register the
  // wrapper as an init script (runs after the base mock's, in registration
  // order) so the very first load already sees the fixture.
  await page.addInitScript(
    ({ seed, commands, placeholder }) => {
      localStorage.setItem("multizen.ui.section", JSON.stringify("kuaishou"));
      localStorage.setItem("multizen.ui.kuaishouTab", JSON.stringify("mate"));
      const internals = (window as any).__TAURI_INTERNALS__;
      const original = internals.invoke;
      const mock = {
        accounts: structuredClone(seed) as MateAccount[],
        states: {} as Record<string, MateLoginState>,
        requests: [] as Array<{ command: string; args: Record<string, any> }>,
        // One-shot failure switches so a spec can drive the add-flow rollback
        // paths (`onAdd` must not leave a placeholder row behind).
        failNextAdd: false,
        failNextStart: false,
      };
      Object.assign(window, { __TEST_MATE__: mock });

      const blank = (accountId: string, patch: Partial<MateLoginState> = {}): MateLoginState => ({
        accountId,
        stage: "idle",
        qrImageDataUrl: null,
        qrLoginToken: null,
        qrLoginSignature: null,
        expireAt: null,
        errorMessage: null,
        user: null,
        startedAt: null,
        finishedAt: null,
        ...patch,
      });
      const find = (id: string): MateAccount => {
        const target = mock.accounts.find((a) => a.id === id);
        if (!target) throw `直播伴侣账号 ${id} 不存在，请刷新后重试`;
        return target;
      };

      internals.invoke = async (command: string, args: Record<string, any> = {}) => {
        if (!(commands as readonly string[]).includes(command)) return original(command, args);
        mock.requests.push({ command, args: structuredClone(args) });
        switch (command) {
          case "mate_accounts_list":
            return structuredClone(mock.accounts);
          case "mate_account_add": {
            if (mock.failNextAdd) {
              mock.failNextAdd = false;
              throw "新建直播伴侣账号失败（fixture）";
            }
            // The backend mints a UUID; the fixture uses a collision-free
            // sequential id so duplicate-key rows never appear in the list.
            const taken = new Set(mock.accounts.map((a) => a.id));
            let seq = mock.accounts.length + 1;
            while (taken.has(`mate-${seq}`)) seq += 1;
            const next: MateAccount = {
              id: `mate-${seq}`,
              // No label → the backend falls back to the placeholder alias.
              label: args.label === undefined ? placeholder : String(args.label),
              platformUserId: null,
              userName: null,
              avatarUrl: null,
              matePassToken: null,
              mateToken: null,
              mateSt: null,
              mateH5St: null,
              mateLmtoken: null,
              loginAt: null,
              createdAt: "2026-01-02T00:00:00.000Z",
              updatedAt: "2026-01-02T00:00:00.000Z",
            };
            mock.accounts = [...mock.accounts, next];
            return structuredClone(next);
          }
          case "mate_account_remove": {
            const removed = find(String(args.id));
            mock.accounts = mock.accounts.filter((a) => a.id !== removed.id);
            delete mock.states[removed.id];
            return;
          }
          case "mate_account_rename": {
            const target = find(String(args.id));
            target.label = String(args.label);
            target.updatedAt = "2026-01-03T00:00:00.000Z";
            return structuredClone(target);
          }
          case "mate_login_start": {
            if (mock.failNextStart) {
              mock.failNextStart = false;
              throw "启动登录失败（fixture）";
            }
            find(String(args.accountId));
            const snapshot = blank(String(args.accountId), { stage: "starting", startedAt: 1 });
            mock.states[snapshot.accountId] = snapshot;
            return structuredClone(snapshot);
          }
          case "mate_login_cancel": {
            const id = String(args.accountId);
            // Real semantics: cancel aborts the flow and returns the *current*
            // snapshot; the flow task publishes `cancelled` asynchronously.
            return structuredClone(mock.states[id] ?? blank(id));
          }
          case "mate_login_state": {
            const id = String(args.accountId);
            return structuredClone(mock.states[id] ?? blank(id));
          }
          default:
            throw new Error(`Unhandled mate fixture IPC: ${command}`);
        }
      };
    },
    {
      seed: accounts,
      commands: MATE_COMMANDS as unknown as string[],
      placeholder: MATE_PLACEHOLDER_LABEL,
    },
  );
  await page.goto("/");
}

export async function mateRequests(
  page: Page,
  command?: string,
): Promise<Array<{ command: string; args: Record<string, any> }>> {
  return page.evaluate((cmd) => {
    const all = (window as any).__TEST_MATE__.requests as Array<{ command: string; args: any }>;
    return cmd ? all.filter((r) => r.command === cmd) : all;
  }, command ?? null);
}

/** Replace the fixture's account list in place (used before emitting an event). */
export async function setMateAccounts(page: Page, accounts: MateAccount[]): Promise<void> {
  await page.evaluate((next) => {
    (window as any).__TEST_MATE__.accounts = next;
  }, accounts);
}

/** Make the next `mate_account_add` reject (drives the `onAdd` rollback path). */
export async function failNextMateAdd(page: Page): Promise<void> {
  await page.evaluate(() => {
    (window as any).__TEST_MATE__.failNextAdd = true;
  });
}

/** Make the next `mate_login_start` reject (drives the `onAdd` rollback path). */
export async function failNextMateStart(page: Page): Promise<void> {
  await page.evaluate(() => {
    (window as any).__TEST_MATE__.failNextStart = true;
  });
}

/** Deliver a `mate-login-state-changed` push exactly as the driver would. */
export async function emitMateState(
  page: Page,
  accountId: string,
  stage: MateLoginStage,
  patch: Partial<MateLoginState> = {},
): Promise<void> {
  await page.evaluate(
    ({ id, next, extra }) => {
      const payload = {
        accountId: id,
        stage: next,
        qrImageDataUrl: null,
        qrLoginToken: null,
        qrLoginSignature: null,
        expireAt: null,
        errorMessage: null,
        user: null,
        startedAt: null,
        finishedAt: null,
        ...extra,
      };
      const mock = (window as any).__TEST_MATE__;
      mock.states[id] = payload;
      (window as any).__TEST_IPC__.emit("mate-login-state-changed", payload);
    },
    { id: accountId, next: stage, extra: patch },
  );
}
