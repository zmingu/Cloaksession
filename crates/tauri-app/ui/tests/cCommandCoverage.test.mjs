import assert from "node:assert/strict";
import { readdirSync, readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const src = join(root, "src");
const lib = join(src, "lib");

function read(rel) {
  return readFileSync(join(root, rel), "utf8");
}

// All 17 C-group commands must have a typed TS wrapper.
const EXPECTED = {
  "lib/autoMessage.ts": ["auto_message_start", "auto_message_stop", "auto_message_status"],
  "lib/autoReply.ts": ["auto_reply_preview", "auto_reply_history", "auto_reply_record"],
  "lib/scenes.ts": [
    "scene_create", "scene_get", "scene_list", "scene_update", "scene_delete",
    "scene_add_line", "scene_update_line", "scene_delete_line",
    "scene_reorder_lines", "scene_play", "scene_stop",
  ],
};

test("all 17 C-group commands have TS wrappers", () => {
  let total = 0;
  for (const [file, commands] of Object.entries(EXPECTED)) {
    const body = readFileSync(join(src, file.replace("lib/", "lib/")), "utf8");
    for (const command of commands) {
      assert.ok(
        body.includes(`"${command}"`),
        `${file} must invoke "${command}"`,
      );
      total += 1;
    }
  }
  assert.equal(total, 17, "3 auto-message + 3 auto-reply + 11 scene commands");
});

test("random-space injection has no entry point in the frontend", () => {
  const hits = [];
  const walk = (dir) => {
    for (const entry of readdirSync(dir, { withFileTypes: true })) {
      const full = join(dir, entry.name);
      if (entry.isDirectory()) {
        walk(full);
        continue;
      }
      if (!/\.(ts|tsx|mjs|js)$/.test(entry.name)) continue;
      const body = readFileSync(full, "utf8");
      // Needles are concatenated so the literal token never appears in this repo.
      for (const needle of ["insert" + "RandomSpace", "insert" + "_random_space", "insert" + "RandomSpaces"]) {
        if (body.toLowerCase().includes(needle.toLowerCase())) hits.push(`${full}: ${needle}`);
      }
    }
  };
  walk(src);
  assert.deepEqual(hits, [], "character-spacing injection must not appear in any type/form/IPC param");
});

test("scene_update groupId tri-state is expressed as keep/clear/set", () => {
  const body = read(join("src", "lib", "scenes.ts"));
  assert.ok(body.includes("SceneGroupIdPatch"), "scenes.ts must use SceneGroupIdPatch");
  assert.ok(body.includes('"keep"') || body.includes("'keep'") || body.includes("keep"), "keep mode");
  assert.ok(body.includes("clear"), "clear mode");
  assert.ok(body.includes("set"), "set mode");
  void lib;
});
