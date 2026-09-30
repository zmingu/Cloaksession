// Explicit opt-in real-browser regression. Only local content and temporary data are used.
// CHROMIX_TEST_BINARY must reference an already installed binary; no download or user DB.
import assert from 'node:assert/strict';
import { mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { launchPersistentContext } from '../vendor/chromix/index.js';
import { prepareOptions } from '../bridge.mjs';
import { applyWindowsFonts } from '../windows-fonts.mjs';

assert.equal(process.platform, 'win32', 'This regression targets Windows font selection');
assert.ok(process.env.CHROMIX_TEST_BINARY, 'Set CHROMIX_TEST_BINARY (no downloads)');
process.env.CLOAKBROWSER_BINARY_PATH = process.env.CHROMIX_TEST_BINARY;
process.env.CLOAKBROWSER_WIDEVINE = '0';
const observed = [];
for (const mode of ['sdk-default', 'sdk-fonts-dir', 'host-default', 'host-fonts-dir']) {
  const directory = await mkdtemp(join(tmpdir(), 'cloaksession-font-render-'));
  let context;
  try {
    const options = prepareOptions({
      type: 'launch', cdpPort: 19222, userDataDir: directory,
      options: { headless: true, args: ['--disable-background-networking'],
        ...(mode.endsWith('fonts-dir') ? { fontsDir: join(process.env.SystemRoot, 'Fonts') } : {}) },
    });
    // No CDP server needed for this direct SDK fixture.
    options.args = options.args.filter((arg) => !arg.startsWith('--remote-debugging-'));
    if (mode.startsWith('host-')) applyWindowsFonts(options);
    context = await launchPersistentContext(options);
    await context.route('**/*', (route) => route.abort());
    const page = context.pages()[0] ?? await context.newPage();
    await page.setContent('<meta charset="utf-8"><p id="latin" style="font:32px Arial">Abc 123</p><p id="cjk" style="font:32px Microsoft YaHei">快手小店扫码登录</p>');
    const cdp = await context.newCDPSession(page);
    await cdp.send('DOM.enable'); await cdp.send('CSS.enable');
    const { root } = await cdp.send('DOM.getDocument');
    const fonts = {};
    for (const selector of ['#latin', '#cjk']) {
      const { nodeId } = await cdp.send('DOM.querySelector', { nodeId: root.nodeId, selector });
      fonts[selector] = (await cdp.send('CSS.getPlatformFontsForNode', { nodeId })).fonts;
    }
    observed.push({ mode, fonts });
    if (mode.startsWith('host-')) {
      assert.ok(fonts['#cjk'].some((font) => font.familyName === 'Microsoft YaHei' && font.glyphCount === 8), JSON.stringify(fonts));
      assert.ok(fonts['#latin'].some((font) => font.familyName === 'Arial' && font.glyphCount === 7));
      assert.equal(options.stealthArgs, undefined, 'Do not disable fingerprinting');
      assert.ok(options.args.find((arg) => arg.startsWith('--uxr-font-whitelist=')).split('=')[1].includes('Microsoft YaHei'));
    }
  } finally {
    await context?.close();
    await rm(directory, { recursive: true, force: true });
  }
}
console.log(JSON.stringify(observed, null, 2));
