import assert from 'node:assert/strict';
import { mkdtemp, mkdir, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';
import { applyWindowsFonts } from '../windows-fonts.mjs';

// Minimal OpenType name-table fixture; exercise the pinned parser, not a mocked family list.
function nameTableFont(names) {
  const strings = names.map((name) => Buffer.from(name, 'utf16le').swap16());
  const nameOffset = 28;
  const stringOffset = 6 + names.length * 12;
  const font = Buffer.alloc(nameOffset + stringOffset + strings.reduce((sum, str) => sum + str.length, 0));
  font.writeUInt32BE(0x00010000, 0); font.writeUInt16BE(1, 4);
  font.write('name', 12); font.writeUInt32BE(nameOffset, 20);
  font.writeUInt16BE(names.length, nameOffset + 2); font.writeUInt16BE(stringOffset, nameOffset + 4);
  let offset = 0;
  strings.forEach((str, index) => {
    const record = nameOffset + 6 + index * 12;
    font.writeUInt16BE(3, record); font.writeUInt16BE(1, record + 2);
    font.writeUInt16BE(1, record + 6); font.writeUInt16BE(str.length, record + 8);
    font.writeUInt16BE(offset, record + 10);
    str.copy(font, nameOffset + stringOffset + offset); offset += str.length;
  });
  return font;
}

test('Windows defaults retain real ASCII families and omit Unicode aliases in every args layer', async () => {
  const root = await mkdtemp(join(tmpdir(), 'cloaksession-font-names-'));
  try {
    await mkdir(join(root, 'Fonts'));
    await writeFile(join(root, 'Fonts', 'fixture.ttf'), nameTableFont(['Arial', 'Microsoft YaHei', '微软雅黑', '宋体', 'SimSun']));
    const options = { args: ['--custom'], launchOptions: { args: [] }, contextOptions: { args: ['--other'] } };
    applyWindowsFonts(options, { platform: 'win32', env: { SystemRoot: root } });
    const flag = '--uxr-font-whitelist=Arial,Microsoft YaHei,SimSun';
    assert.deepEqual(options.args, ['--custom', flag]);
    assert.deepEqual(options.launchOptions.args, [flag]);
    assert.deepEqual(options.contextOptions.args, ['--other', flag]);
    assert.equal(Object.hasOwn(options, 'stealthArgs'), false);
    assert.equal(Object.hasOwn(options, 'fontsDir'), false);
    const custom = { fontsDir: join(root, 'Fonts') };
    applyWindowsFonts(custom, { platform: 'win32', env: {} });
    assert.deepEqual(custom.args, [flag]);
    assert.equal(custom.fontsDir, join(root, 'Fonts'));
  } finally { await rm(root, { recursive: true, force: true }); }
});

test('non-Windows and explicit font/fingerprint policies remain unchanged', () => {
  for (const options of [{ stealthArgs: false }, { fontsDir: null }, { fontsDir: '' },
    ...['args', 'launchOptions', 'contextOptions'].flatMap((layer) =>
      ['--uxr-font-policy=native', '--fingerprint-font-policy=restricted', '--uxr-font-whitelist=自定义', '--fingerprint-font-whitelist=Custom', '--fingerprint=off'].map((flag) =>
        layer === 'args' ? { args: [flag] } : { [layer]: { args: [flag] } }))]) {
    const before = structuredClone(options);
    applyWindowsFonts(options, { platform: 'win32', env: {} });
    assert.deepEqual(options, before);
  }
  for (const platform of ['linux', 'darwin']) {
    const options = { args: [] };
    applyWindowsFonts(options, { platform, env: {} });
    assert.deepEqual(options, { args: [] });
  }
});

test('missing fonts fail with actionable guidance rather than an empty whitelist', () => {
  assert.throws(() => applyWindowsFonts({}, { platform: 'win32', env: {} }), /configure Chromix fontsDir/);
  assert.throws(() => applyWindowsFonts({ fontsDir: '/missing-font-fixture' }, { platform: 'win32' }), /installed ASCII family names/);
});
