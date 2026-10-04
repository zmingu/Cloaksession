import { expect, type Page } from "@playwright/test";
import type { KuaishouIdentitySnapshot } from "../src/lib/kuaishouIdentity";
import { FIRST_PROFILE, SECOND_PROFILE, closeLabel, installBusinessMock, type MockLanguage } from "./businessAccountsMock";

export const PNG = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAusB9Wl6lqQAAAAASUVORK5CYII=";
export function identityFixture(patch: Partial<KuaishouIdentitySnapshot> = {}): KuaishouIdentitySnapshot {
  return { profileId: FIRST_PROFILE, status: "detected", platformUserId: "00123456", nickname: "本地小店",
    avatarKey: "avatar-first", checkedAt: "2026-09-30T08:00:00Z", lastSeenAt: "2026-09-30T08:00:00Z", message: null, ...patch };
}

/** All commands here are browser-local fakes. No profile launch or external request. */
export async function installIdentityMock(page: Page, snapshots = [identityFixture()], running = [FIRST_PROFILE, SECOND_PROFILE], language: MockLanguage = "en"): Promise<void> {
  await installBusinessMock(page, language);
  await page.evaluate(({ snapshots, running, png, first, second }) => {
    const internals = (window as any).__TAURI_INTERNALS__;
    const base = (window as any).__TEST_IPC__;
    const business = (window as any).__TEST_BUSINESS__;
    business.running = running;
    const original = internals.invoke;
    const mock = {
      snapshots,
      detections: {} as Record<string, KuaishouIdentitySnapshot>,
      avatars: { "avatar-first": png } as Record<string, string | null>,
      requests: [] as Array<{ command: string; args: Record<string, any> }>,
      hold: {} as Record<string, boolean>,
      failures: {} as Record<string, string>,
      pending: {} as Record<string, Array<() => void>>,
      release: (key: string) => { mock.pending[key]?.splice(0).forEach((resolve) => resolve()); },
    };
    Object.assign(window, { __TEST_IDENTITY__: mock, __COPIED_ID__: null });
    Object.defineProperty(navigator, "clipboard", { configurable: true, value: {
      writeText: async (text: string) => { (window as any).__COPIED_ID__ = text; },
    } });
    internals.invoke = async (command: string, args: Record<string, any> = {}) => {
      if (command === "profiles_list") return [
        { ...base.profile(), id: first, name: "Regression profile", icon: "🧭" },
        { ...base.profile(), id: second, name: "Second profile", icon: "🌙" },
      ].map((profile) => ({ ...profile, isRunning: business.running.includes(profile.id), timezone: profile.fingerprint.timezone }));
      if (!command.startsWith("kuaishou_identity_")) return original(command, args);
      mock.requests.push({ command, args: structuredClone(args) });
      const key = command === "kuaishou_identity_detect" ? `${command}:${args.profileId}`
        : command === "kuaishou_identity_avatar" ? `${command}:${args.avatarKey}` : command;
      const failure = mock.failures[key];
      delete mock.failures[key];
      const result = command === "kuaishou_identity_list" ? structuredClone(mock.snapshots)
        : command === "kuaishou_identity_detect" ? structuredClone(mock.detections[args.profileId] ?? mock.snapshots.find((item) => item.profileId === args.profileId))
        : mock.avatars[args.avatarKey] ?? null;
      if (mock.hold[key]) {
        delete mock.hold[key];
        await new Promise<void>((resolve) => (mock.pending[key] ??= []).push(resolve));
      }
      if (failure) throw failure;
      if (command === "kuaishou_identity_detect" && result) {
        mock.snapshots = [...mock.snapshots.filter((item) => item.profileId !== args.profileId), result as KuaishouIdentitySnapshot];
      }
      return result;
    };
  }, { snapshots, running, png: PNG, first: FIRST_PROFILE, second: SECOND_PROFILE });
  // Closing the existing editor asks App to refresh its profile summaries, including mock running flags.
  await page.getByRole("button", { name: /Regression profile/ }).click();
  await page.getByRole("dialog").getByRole("button", { name: closeLabel(language), exact: true }).click();
  await expect(page.getByTestId(`kuaishou-summary-${SECOND_PROFILE}`)).toBeVisible();
  await refreshIdentity(page);
}

export async function refreshIdentity(page: Page): Promise<void> {
  const button = page.getByRole("button", { name: "刷新检测状态", exact: true }).first();
  await button.click();
  await expect(button).toBeEnabled();
}
export async function identityCalls(page: Page, command: string) {
  return page.evaluate((command) => (window as any).__TEST_IDENTITY__.requests.filter((call: any) => call.command === command), command);
}
export async function holdIdentity(page: Page, key: string): Promise<void> {
  await page.evaluate((key) => { (window as any).__TEST_IDENTITY__.hold[key] = true; }, key);
}
export async function waitIdentity(page: Page, key: string): Promise<void> {
  await expect.poll(() => page.evaluate((key) => (window as any).__TEST_IDENTITY__.pending[key]?.length ?? 0, key)).toBe(1);
}
export async function releaseIdentity(page: Page, key: string): Promise<void> {
  await page.evaluate((key) => (window as any).__TEST_IDENTITY__.release(key), key);
}
