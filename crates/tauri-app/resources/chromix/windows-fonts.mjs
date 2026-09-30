import { join } from 'node:path';
import { fontFamiliesInDir } from './vendor/chromix/_fonts.js';

/** Windows Chromix 151 discards a whitelist containing non-ASCII family aliases.
 * Keep the installed fonts' ASCII family names; never disable the fingerprint.
 * Explicit whitelist/policy choices remain SDK-owned. No profile/settings writes.
 */
export function applyWindowsFonts(options, { platform = process.platform, env = process.env } = {}) {
  if (platform !== 'win32' || options.stealthArgs === false) return;
  const layers = [options, options.launchOptions, options.contextOptions].filter(Boolean);
  const args = layers.flatMap((layer) => layer.args ?? []);
  if (args.some((arg) => /^--(?:uxr|fingerprint)-font-(?:whitelist|policy)(?:=|$)/.test(arg)
    || /^--fingerprint=(?:off|false|0|disable|disabled)$/i.test(arg))) return;
  // A supplied null/empty fontsDir is an explicit opt-out, not a system default.
  if (Object.hasOwn(options, 'fontsDir') && !options.fontsDir) return;
  const root = env.SystemRoot || env.WINDIR;
  const directory = options.fontsDir ?? (root ? join(root, 'Fonts') : null);
  if (!directory) throw new Error('Windows font directory unavailable; configure Chromix fontsDir');
  const families = fontFamiliesInDir(directory).filter((family) => /^[\x20-\x7e]+$/.test(family) && !family.includes(','));
  const whitelist = families.join(',');
  if (!families.length || families.length > 256 || Buffer.byteLength(whitelist) > 4096) {
    throw new Error('Windows font whitelist needs 1–256 installed ASCII family names (max 4096 bytes); configure a smaller fontsDir or an explicit font-whitelist');
  }
  const flag = `--uxr-font-whitelist=${whitelist}`;
  options.args = [...(options.args ?? []), flag];
  // Persistent contextOptions can replace SDK-built args entirely.
  for (const layer of layers.slice(1)) {
    if (Object.hasOwn(layer, 'args')) layer.args = [...layer.args, flag];
  }
}
