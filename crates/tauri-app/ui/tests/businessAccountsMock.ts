import type { Page } from "@playwright/test";
import { defaultSettings, installTauriMock } from "./tauriMock";
import type { BusinessAccount, BusinessProfileState, SaveBusinessAccountInput } from "../src/lib/businessAccounts";

export const FIRST_PROFILE = "fixture-profile";
export const SECOND_PROFILE = "second-profile";

/** UI language for the mock; zh-CN specs pass it so localized labels match. */
export type MockLanguage = "en" | "zh-CN";

/** Localized nav label for the Profiles section (Sidebar `navLabel`). */
export function navProfilesLabel(language: MockLanguage): string {
  return language === "zh-CN" ? "浏览器配置" : "Profiles";
}

/** Localized Close button label (dialog close control). */
export function closeLabel(language: MockLanguage): string {
  return language === "zh-CN" ? "关闭" : "Close";
}

export function accountFixture(patch: Partial<BusinessAccount> = {}): BusinessAccount {
  return {
    id: "account-fixture", kind: "kuaishou-shop", displayName: "原登记",
    platformUserId: "platform-001", profileId: FIRST_PROFILE,
    createdAt: "2026-01-01T00:00:00Z", updatedAt: "2026-01-01T00:00:00Z",
    ...patch,
  };
}

/** Browser-local contract fake only: never launches a profile or contacts a platform. */
export async function installBusinessMock(page: Page, language: MockLanguage = "en"): Promise<void> {
  await installTauriMock(page, { ...defaultSettings, language });
  await page.goto("/");
  await page.evaluate(({ secondId }) => {
    const base = (window as any).__TEST_IPC__;
    const internals = (window as any).__TAURI_INTERNALS__;
    const originalInvoke = internals.invoke;
    const second = { ...base.profile(), id: secondId, name: "Second profile" };
    const mock = {
      accounts: [] as BusinessAccount[],
      scopes: {} as Record<string, BusinessProfileState["scope"]>,
      running: [] as string[],
      requests: [] as Array<{ command: string; args: Record<string, any> }>,
      failNext: {} as Record<string, string>,
      holdNext: "",
      pending: [] as Array<() => void>,
      release: () => { mock.pending.splice(0).forEach((resolve) => resolve()); },
    };
    Object.assign(window, { __TEST_BUSINESS__: mock });
    async function gate(command: string): Promise<void> {
      if (mock.holdNext === command) {
        mock.holdNext = "";
        await new Promise<void>((resolve) => mock.pending.push(resolve));
      }
      if (mock.failNext[command]) {
        const message = mock.failNext[command];
        delete mock.failNext[command];
        throw message; // Tauri rejects Result<_, String> with a string, not Error.
      }
    }
    internals.invoke = async (command: string, args: Record<string, any> = {}) => {
      if (command === "profiles_list") {
        await gate(command);
        return [base.profile(), second].map((item) => ({
          id: item.id, name: item.name, tags: item.tags,
          isRunning: mock.running.includes(item.id), timezone: item.fingerprint.timezone,
        }));
      }
      if (command === "profiles_get" && args.id === second.id) return structuredClone(second);
      if (!command.startsWith("business_accounts_")) return originalInvoke(command, args);
      mock.requests.push({ command, args: structuredClone(args) });
      if (command === "business_accounts_list") {
        const result = structuredClone(mock.accounts);
        await gate(command);
        return result;
      }
      if (command === "business_accounts_profile_state") {
        const result = structuredClone({
          account: mock.accounts.find((item) => item.profileId === args.profileId) ?? null,
          scope: mock.scopes[args.profileId] ?? null,
        });
        await gate(command);
        return result;
      }
      await gate(command);
      if (command === "business_accounts_save") {
        const input = args.input as SaveBusinessAccountInput;
        if (mock.running.includes(input.profileId)) throw "Profile must be stopped (backend)";
        const previous = input.id ? mock.accounts.find((item) => item.id === input.id) : undefined;
        if (input.id && !previous) throw "Account no longer exists";
        if (previous?.profileId && previous.profileId !== input.profileId) throw "Account already bound to another profile";
        if (mock.accounts.some((item) => item.profileId === input.profileId && item.id !== input.id)) throw "Profile already has an account";
        if (previous && previous.kind !== input.kind) throw "Account kind cannot change";
        const scope = input.kind === "jinniu" ? "jinniu" : "kuaishou";
        if (mock.scopes[input.profileId] && mock.scopes[input.profileId] !== scope) throw "Profile scope conflict";
        const next: BusinessAccount = {
          ...input, id: previous?.id ?? `account-${mock.accounts.length + 1}`,
          createdAt: previous?.createdAt ?? "2026-09-30T00:00:00Z", updatedAt: "2026-09-30T00:00:00Z",
        };
        mock.accounts = [...mock.accounts.filter((item) => item.id !== next.id), next];
        mock.scopes[input.profileId] = scope;
        return structuredClone(next);
      }
      if (command === "business_accounts_unbind") {
        const target = mock.accounts.find((item) => item.id === args.id);
        if (!target) throw "Account no longer exists";
        if (target.profileId && mock.running.includes(target.profileId)) throw "Profile must be stopped (backend)";
        target.profileId = null;
        return;
      }
      throw new Error(`Unhandled business fixture IPC: ${command}`);
    };
  }, { secondId: SECOND_PROFILE });
  await page.getByRole("button", { name: navProfilesLabel(language), exact: true }).click();
}

export async function setBusinessFixture(page: Page, accounts: BusinessAccount[], scopes: Record<string, BusinessProfileState["scope"]> = {}): Promise<void> {
  await page.evaluate(({ accounts, scopes }) => {
    Object.assign((window as any).__TEST_BUSINESS__, { accounts, scopes });
  }, { accounts, scopes });
}

export async function businessRequests(page: Page): Promise<Array<{ command: string; args: Record<string, any> }>> {
  return page.evaluate(() => (window as any).__TEST_BUSINESS__.requests);
}

export async function savedAccounts(page: Page): Promise<BusinessAccount[]> {
  return page.evaluate(() => (window as any).__TEST_BUSINESS__.accounts);
}
