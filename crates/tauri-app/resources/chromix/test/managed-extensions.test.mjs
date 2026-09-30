import assert from 'node:assert/strict';
import test from 'node:test';
import { buildArgs } from '@xiaoxiaofeihh/chromix';
import { prepareOptions } from '../bridge.mjs';

const hostRequest = (options, extensionPaths) => ({
  type: 'launch', options, cdpPort: 19222,
  userDataDir: '/tmp/chromix-extension-test', extensionPaths,
});
const extensionFlags = (args) => args.filter((arg) =>
  /^--(?:load-extension|disable-extensions-except)=/.test(arg));

const pathCases = {
  windows: [String.raw`C:\Users\Test User\AppData\Local\companion`, String.raw`D:\扩展\100% #test`],
  unc: [String.raw`\\server\share\My Extension`],
  posix: ['/home/test user/companion', '/opt/扩展/100% #test'],
};

for (const [platform, paths] of Object.entries(pathCases)) {
  for (const layer of ['top', 'launchOptions', 'contextOptions']) {
    test(`managed ${platform} extension paths survive ${layer} and the pinned SDK`, () => {
      const options = layer === 'top' ? {} : { [layer]: { args: ['--custom-flag'] } };
      const request = hostRequest(options, paths);
      const original = structuredClone(request);
      const prepared = prepareOptions(request);
      const expected = [
        `--load-extension=${paths.join(',')}`,
        `--disable-extensions-except=${paths.join(',')}`,
      ];
      // The SDK's extensionPaths convenience field treats native paths as URLs.
      // Managed paths must reach Chromium through raw flags instead.
      assert.equal(Object.hasOwn(prepared, 'extensionPaths'), false);
      assert.deepEqual(extensionFlags(prepared.args), expected);
      const effectiveArgs = layer === 'top' ? prepared.args : prepared[layer].args;
      assert.deepEqual(extensionFlags(effectiveArgs), expected);
      const sdkArgs = buildArgs({
        stealthArgs: false,
        extraArgs: effectiveArgs,
        extensionPaths: prepared.extensionPaths,
      });
      assert.deepEqual(extensionFlags(sdkArgs), expected);
      assert.deepEqual(request, original);
    });
  }
}

test('managed extension defaults do not override explicit SDK extension choices', () => {
  const paths = pathCases.windows;
  for (const explicit of [[], [String.raw`E:\Custom Extension`]]) {
    const prepared = prepareOptions(hostRequest({ extensionPaths: explicit }, paths));
    assert.deepEqual(prepared.extensionPaths, explicit);
    assert.deepEqual(extensionFlags(prepared.args), []);
  }
  for (const layer of ['top', 'launchOptions', 'contextOptions']) {
    const flags = ['--load-extension=E:/custom', '--disable-extensions-except=E:/custom'];
    const options = layer === 'top' ? { args: flags } : { [layer]: { args: flags } };
    const prepared = prepareOptions(hostRequest(options, paths));
    const effectiveArgs = layer === 'top' ? prepared.args : prepared[layer].args;
    assert.deepEqual(extensionFlags(effectiveArgs), flags);
  }
});

test('no managed extensions means no extension flags', () => {
  for (const paths of [[], undefined]) {
    const prepared = prepareOptions(hostRequest({}, paths));
    assert.deepEqual(extensionFlags(prepared.args), []);
    assert.equal(Object.hasOwn(prepared, 'extensionPaths'), false);
  }
});
