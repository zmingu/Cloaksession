import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");

function read(rel) {
  return readFileSync(join(root, rel), "utf8");
}

// All 16 A-group commands must have a typed TS wrapper, per
// `.trellis/tasks/10-04-jieger-frontend-catchup/research/command-surface.md` §1-4.
const EXPECTED = {
  "src/lib/kuaishouAuth.ts": ["kuaishou_connect", "kuaishou_login", "ensure_kuaishou_auth"],
  "src/lib/mateLogin.ts": ["mate_login_start", "mate_login_cancel", "mate_login_state"],
  "src/lib/liveLaunch.ts": [
    "live_launch_status", "live_launch_prerequisites", "live_launch_credentials",
    "live_launch_heartbeat_start", "live_launch_heartbeat_stop",
    "live_launch_stream_start", "live_launch_stream_stop",
  ],
  "src/lib/liveRoomMonitor.ts": [
    "start_live_room_monitor", "stop_live_room_monitor", "get_live_room_monitor_state",
  ],
};

// Event names are the Rust consts; every subscription must be awaitable so the
// page can register and clean up (`await` + unlisten).
const EVENTS = {
  "src/lib/kuaishouAuth.ts": "kuaishou-auth-phase",
  "src/lib/mateLogin.ts": "mate-login-state-changed",
  "src/lib/liveLaunch.ts": "live-launch-state-changed",
  "src/lib/liveRoomMonitor.ts": "live-room-monitor-state-changed",
};

test("all 16 A-group commands have TS wrappers", () => {
  let total = 0;
  for (const [file, commands] of Object.entries(EXPECTED)) {
    const body = read(file);
    for (const command of commands) {
      assert.ok(body.includes(`"${command}"`), `${file} must invoke "${command}"`);
      total += 1;
    }
  }
  assert.equal(total, 16, "3 kuaishou-auth + 3 mate-login + 7 live-launch + 3 live-room-monitor");
});

test("A-group event subscriptions are awaitable and return an unlisten", () => {
  for (const [file, event] of Object.entries(EVENTS)) {
    const body = read(file);
    assert.ok(body.includes(`"${event}"`), `${file} must reference the ${event} event`);
    assert.ok(
      /Promise<UnlistenFn>/.test(body),
      `${file} must expose a Promise<UnlistenFn> subscription`,
    );
    assert.ok(body.includes("listen<"), `${file} must use listen<T>`);
  }
});

test("live_launch_prerequisites and stop_live_room_monitor take no arguments", () => {
  // The real contract is the invoke call: no second argument object at all.
  const launch = read("src/lib/liveLaunch.ts");
  assert.ok(
    launch.includes('invoke<PrerequisitesReport>("live_launch_prerequisites")'),
    "live_launch_prerequisites must be invoked with no arguments",
  );
  const monitor = read("src/lib/liveRoomMonitor.ts");
  assert.ok(
    monitor.includes('invoke<LiveRoomMonitorState>("stop_live_room_monitor")'),
    "stop_live_room_monitor takes no profileId",
  );
  assert.ok(
    monitor.includes('invoke<LiveRoomMonitorState>("get_live_room_monitor_state")'),
    "get_live_room_monitor_state takes no arguments",
  );
});

test("the stream key never leaves the invoke boundary as a logged value", () => {
  // The wrapper returns credentials verbatim; only the rtmp server may be
  // rendered. This guards against a future `console.log` of streamKey.
  for (const file of ["src/lib/liveLaunch.ts", "src/components/business/LiveLaunchPage.tsx"]) {
    const body = read(file);
    assert.ok(!/console\.\w+\([^)]*streamKey/.test(body), `${file} must not log streamKey`);
  }
});
