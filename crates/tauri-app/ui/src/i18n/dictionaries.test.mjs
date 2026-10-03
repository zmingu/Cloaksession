import assert from "node:assert/strict";
import { test } from "node:test";
import { en } from "./en.ts";
import { zhCN } from "./zh-CN.ts";
import { DICTIONARIES, FALLBACK_LANGUAGE, normalizeLanguage, translate } from "./translate.ts";

const PLACEHOLDER = /\{\{\s*([A-Za-z0-9_.-]+)\s*\}\}/g;

function placeholders(value) {
  const names = new Set();
  for (const match of value.matchAll(PLACEHOLDER)) names.add(match[1]);
  return names;
}

test("en and zh-CN expose identical key sets", () => {
  const enKeys = Object.keys(en).sort();
  const zhKeys = Object.keys(zhCN).sort();
  assert.deepEqual(zhKeys, enKeys, "zh-CN keys must match en keys exactly");
  assert.deepEqual(Object.keys(DICTIONARIES["zh-CN"]).sort(), enKeys);
});

test("every key has an identical named-placeholder set across dictionaries", () => {
  for (const key of Object.keys(en)) {
    const enPh = placeholders(en[key]);
    const zhPh = placeholders(zhCN[key]);
    assert.deepEqual(
      [...zhPh].sort(),
      [...enPh].sort(),
      `placeholder mismatch for key ${key}: en=${[...enPh]} zh=${[...zhPh]}`,
    );
  }
});

test("translate interpolates named placeholders as plain text", () => {
  const out = translate("zh-CN", "nav.item.shortcutTitle", { label: "设置", kbd: "," });
  assert.equal(out, "设置 · ⌘,");
  const en = translate("en", "update.status.available", { version: "1.2.3" });
  assert.equal(en, "v1.2.3 available");
});

test("translate never interprets HTML in interpolated params", () => {
  const out = translate("en", "nav.group.deleteAria", { label: "<img src=x onerror=alert(1)>" });
  assert.ok(out.includes("<img src=x onerror=alert(1)>"), "value must be inserted verbatim");
  assert.equal(out.includes("<img src=x onerror=alert(1)>"), true);
});

test("missing params are left visible rather than dropped", () => {
  assert.equal(translate("en", "update.status.available", {}), "v{{version}} available");
});

test("normalizeLanguage only accepts the two wire values", () => {
  assert.equal(normalizeLanguage("en"), "en");
  assert.equal(normalizeLanguage("zh-CN"), "zh-CN");
  assert.equal(normalizeLanguage("fr"), FALLBACK_LANGUAGE);
  assert.equal(normalizeLanguage(null), FALLBACK_LANGUAGE);
  assert.equal(normalizeLanguage(42), FALLBACK_LANGUAGE);
});
