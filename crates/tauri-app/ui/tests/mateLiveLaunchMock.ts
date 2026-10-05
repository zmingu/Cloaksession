import type { Page } from "@playwright/test";
import { defaultSettings, installTauriMock } from "./tauriMock";
import type { MateAccount } from "../src/types";

/**
 * Browser-local contract fake for the mate (伴侣) live-launch flow:
 * `mate_accounts_list` + the three `live_launch_mate_*` commands.
 *
 * Pure in-memory state: never launches a profile, never opens a browser, never
 * fetches a real stream key, never pushes a real stream. Wire shapes mirror the
 * Rust serde exactly (`camelCase`).
 *
 * `live-launch-state-changed` is delivered through
 * `window.__TEST_IPC__.emit(...)` (the base mock's listener registry).
 */
export function mateAccountFixture(patch: Partial<MateAccount> = {}): MateAccount {
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

export const MATE_LIVE_COMMANDS = [
  "mate_accounts_list",
  "live_launch_prerequisites",
  "live_launch_mate_credentials",
  "live_launch_mate_stream_start",
  "live_launch_mate_stream_stop",
] as const;

/** Boot straight into 直播 › 正式开播 with a seeded companion-account list. */
export async function installMateLiveMock(
  page: Page,
  accounts: MateAccount[] = [],
  language: "en" | "zh-CN" = "en",
): Promise<void> {
  await installTauriMock(page, { ...defaultSettings, language });
  await page.addInitScript(
    ({ seed, commands }) => {
      localStorage.setItem("multizen.ui.section", JSON.stringify("live"));
      localStorage.setItem("multizen.ui.liveTab", JSON.stringify("live"));
      const internals = (window as any).__TAURI_INTERNALS__;
      const original = internals.invoke;
      const mock = {
        accounts: structuredClone(seed) as MateAccount[],
        requests: [] as Array<{ command: string; args: Record<string, any> }>,
      };
      Object.assign(window, { __TEST_MATELIVE__: mock });

      const state = (accountId: string, status: string, patch: Record<string, any> = {}) => ({
        profileId: accountId,
        subjectKind: "mate",
        status,
        mode: null,
        target: "rtmp://push.fixture/live/***",
        pid: status === "streaming" ? 4242 : null,
        stderrTail: [],
        exitCode: null,
        error: null,
        placeholderCredentials: false,
        liveStreamId: "LS-1",
        startedAt: null,
        ...patch,
      });

      internals.invoke = async (command: string, args: Record<string, any> = {}) => {
        if (!(commands as readonly string[]).includes(command)) return original(command, args);
        mock.requests.push({ command, args: structuredClone(args) });
        switch (command) {
          case "mate_accounts_list":
            return structuredClone(mock.accounts);
          case "live_launch_prerequisites":
            return { available: true, ffmpegPath: "/fixture/ffmpeg", searched: [], error: null };
          case "live_launch_mate_credentials":
            return {
              rtmpServer: "rtmp://push.fixture/live",
              streamKey: "key-fixture",
              liveStreamId: "LS-1",
              placeholder: false,
            };
          case "live_launch_mate_stream_start":
            return state(String(args.mateAccountId), "streaming", {
              startedAt: "2026-10-05T00:00:00Z",
            });
          case "live_launch_mate_stream_stop":
            return state(String(args.mateAccountId), "stopped");
          default:
            throw new Error(`Unhandled mate-live fixture IPC: ${command}`);
        }
      };
    },
    { seed: accounts, commands: MATE_LIVE_COMMANDS as unknown as string[] },
  );
  await page.goto("/");
}

export async function mateLiveRequests(
  page: Page,
  command?: string,
): Promise<Array<{ command: string; args: Record<string, any> }>> {
  return page.evaluate((cmd) => {
    const all = (window as any).__TEST_MATELIVE__.requests as Array<{
      command: string;
      args: any;
    }>;
    return cmd ? all.filter((r) => r.command === cmd) : all;
  }, command ?? null);
}

/** Deliver a `live-launch-state-changed` push exactly as the driver would. */
export async function emitLiveLaunchState(
  page: Page,
  payload: Record<string, unknown>,
): Promise<void> {
  await page.evaluate((next) => {
    (window as any).__TEST_IPC__.emit("live-launch-state-changed", next);
  }, payload);
}
