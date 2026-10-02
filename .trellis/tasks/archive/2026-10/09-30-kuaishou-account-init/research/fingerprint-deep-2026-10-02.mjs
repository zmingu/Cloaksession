// Deep fingerprint comparison: CloakBrowser vs Chromix. 2026-10-02.
// Captures CreepJS trust score, AudioContext hash, and multi-seed collision data.
// node <this> --run --engine=cloakbrowser
// node <this> --run --engine=chromix
import { spawn } from 'node:child_process';
import { mkdtemp, rm, access } from 'node:fs/promises';
import { createServer } from 'node:net';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { finished } from 'node:stream/promises';
import { createWriteStream } from 'node:fs';

const self = fileURLToPath(import.meta.url);
const research = dirname(self);
const repo = resolve(research, '../../../..');
const chromixBridge = join(repo, 'crates/tauri-app/resources/chromix/bridge.mjs');
const cloakBinary = 'C:/Users/Administrator/.cloakbrowser/chromium-146.0.7680.177.5/chrome.exe';
const chromixBinary = 'C:/Users/Administrator/.cache/chromix/v154.0.8037.57/win-x64/chromix/chrome.exe';

const option = name => process.argv.find(a => a.startsWith(`--${name}=`))?.slice(name.length + 3);
const engine = option('engine');
const seeds = (option('seeds') ?? '11111,22222,33333,44444,55555').split(',');
if (!engine || !['cloakbrowser', 'chromix'].includes(engine) || typeof WebSocket !== 'function') {
  throw Object.assign(new Error('USAGE: --run --engine=cloakbrowser|chromix [--seeds=a,b,c]'), { safeCode: 'USAGE' });
}
if (process.env.NODE_OPTIONS || process.env.NODE_USE_ENV_PROXY === '1') throw new Error('NODE_OVERRIDES_REFUSED');
await access(cloakBinary);
if (engine === 'chromix') await access(chromixBinary);
await access(chromixBridge);

const stamp = new Date().toISOString().replace(/[:.]/g, '-');
const outputPath = join(research, `fingerprint-deep-2026-10-02-${engine}-${stamp}.jsonl`);
const output = createWriteStream(outputPath, { flags: 'wx' });
await new Promise((r, j) => { output.once('open', r); output.once('error', j); });
const log = (event, details = {}) => output.write(`${JSON.stringify({ time: new Date().toISOString(), engine, event, ...details })}\n`);
log('run', { node: process.version, seeds, artifact: outputPath.split(/[\\/]/).pop() });

class Cdp {
  constructor(socket) {
    this.socket = socket; this.next = 0; this.pending = new Map();
    socket.addEventListener('message', ({ data }) => {
      const m = JSON.parse(String(data));
      if (m.id) {
        const p = this.pending.get(m.id);
        if (!p) return;
        this.pending.delete(m.id); clearTimeout(p.timer);
        if (m.error) p.reject(new Error(m.error.message ?? 'CDP_ERROR'));
        else p.resolve(m.result);
      }
    });
    socket.addEventListener('close', () => { for (const p of this.pending.values()) { clearTimeout(p.timer); p.reject(new Error('CLOSED')); } this.pending.clear(); });
  }
  send(method, params = {}, sessionId, ms = 15000) {
    const id = ++this.next;
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => { this.pending.delete(id); reject(new Error('NO_REPLY')); }, ms);
      this.pending.set(id, { resolve, reject, timer });
      this.socket.send(JSON.stringify({ id, method, params, ...(sessionId ? { sessionId } : {}) }));
    });
  }
  async evaluate(session, expression, ms = 20000) {
    const r = await this.send('Runtime.evaluate', { expression, returnByValue: true }, session, ms);
    if (r.exceptionDetails) throw new Error('JS_EXCEPTION');
    return r.result?.value;
  }
}
const sleep = ms => new Promise(r => setTimeout(r, ms));

// AudioContext fingerprint hash (uses OfflineAudioContext for stability)
const audioProbe = `(() => {
  return new Promise(async (resolve) => {
    try {
      const ctx = new OfflineAudioContext(1, 44100, 44100);
      const osc = ctx.createOscillator(); osc.type = 'triangle'; osc.frequency.value = 10000;
      const comp = ctx.createDynamicsCompressor();
      comp.threshold.value = -50; comp.knee.value = 40; comp.ratio.value = 12; comp.attack.value = 0; comp.release.value = 0.25;
      osc.connect(comp); comp.connect(ctx.destination);
      osc.start(0);
      const buffer = await ctx.startRendering();
      const data = buffer.getChannelData(0);
      let hash = 0;
      for (let i = 0; i < data.length; i++) hash = ((hash << 5) - hash + data[i] * 1e9 | 0);
      resolve({ audioHash: String(hash), sampleRate: ctx.sampleRate, length: data.length });
    } catch(e) { resolve({ audioHash: null, error: e.message }); }
  });
})()`;

// Full fingerprint probe with all vectors
const fullProbe = `(() => {
  const fp = {
    ua: navigator.userAgent,
    platform: navigator.platform,
    languages: navigator.languages,
    hardwareConcurrency: navigator.hardwareConcurrency,
    deviceMemory: navigator.deviceMemory,
    webdriver: navigator.webdriver,
    vendor: navigator.vendor,
    maxTouchPoints: navigator.maxTouchPoints,
    timezone: Intl.DateTimeFormat().resolvedOptions().timeZone,
    timezoneOffset: new Date().getTimezoneOffset(),
    screen: { w: screen.width, h: screen.height, aw: screen.availWidth, ah: screen.availHeight, depth: screen.colorDepth },
  };
  // Canvas hash
  try {
    const c = document.createElement('canvas'); c.width = 280; c.height = 60;
    const ctx = c.getContext('2d');
    ctx.textBaseline = 'top'; ctx.font = "14px 'Arial'";
    ctx.fillStyle = '#f60'; ctx.fillRect(125, 1, 62, 20);
    ctx.fillStyle = '#069'; ctx.fillText('CryptoGraphic\\u00e9', 2, 15);
    ctx.fillStyle = 'rgba(102,204,0,0.7)'; ctx.fillText('CryptoGraphic\\u00e9', 4, 17);
    fp.canvasHash = c.toDataURL().slice(-64);
  } catch { fp.canvasHash = null; }
  // WebGL
  try {
    const c = document.createElement('canvas'); const gl = c.getContext('webgl');
    if (gl) { const mi = gl.getExtension('WEBGL_debug_renderer_info');
      fp.webgl = { vendor: gl.getParameter(gl.VENDOR), renderer: gl.getParameter(gl.RENDERER),
        unmaskedVendor: mi ? gl.getParameter(mi.UNMASKED_VENDOR_WEBGL) : null,
        unmaskedRenderer: mi ? gl.getParameter(mi.UNMASKED_RENDERER_WEBGL) : null }; }
  } catch { fp.webgl = null; }
  // Client Hints
  if (navigator.userAgentData) {
    fp.uaData = { platform: navigator.userAgentData.platform, brands: navigator.userAgentData.brands?.map(b => b.brand + '/' + b.version) };
  }
  // Fonts (check 20 common fonts)
  fp.fonts = ['Arial','Times New Roman','Courier New','Segoe UI','Microsoft YaHei','Calibri','Cambria','Candara','Consolas','Constantia','Corbel','Franklin Gothic Medium','Gabriola','Gadugi','Georgia','Impact','Lucida Console','Malgun Gothic','MS Gothic','Noto Sans SC'].filter(f => document.fonts?.check('12px "' + f + '"'));
  return fp;
})()`;

let child, cdp, root, profile;
const env = { ...process.env };
for (const k of ['HTTP_PROXY','HTTPS_PROXY','ALL_PROXY','NO_PROXY','NODE_OPTIONS','NODE_USE_ENV_PROXY']) delete env[k];

async function runSeed(seed) {
  const seedRoot = await mkdtemp(join(tmpdir(), `fpdeep-${engine}-${seed}-`));
  const seedProfile = join(seedRoot, 'profile');
  const reserve = createServer(); await new Promise(r => reserve.listen(0, '127.0.0.1', r));
  const port = reserve.address().port; await new Promise(r => reserve.close(r));
  const endpoint = `http://127.0.0.1:${port}`;
  let seedCdp = null, seedChild = null;
  try {
    if (engine === 'cloakbrowser') {
      const args = [
        `--user-data-dir=${seedProfile}`, `--remote-debugging-port=${port}`, '--remote-debugging-address=127.0.0.1',
        '--no-first-run', '--no-default-browser-check', '--disable-features=Translate',
        `--fingerprint=${seed}`, '--fingerprint-platform=windows', '--fingerprint-noise=false',
        '--window-size=1280,800', 'about:blank',
      ];
      seedChild = spawn(cloakBinary, args, { stdio: ['ignore', 'pipe', 'pipe'], env });
    } else {
      seedChild = spawn(process.execPath, [chromixBridge], { stdio: ['pipe', 'pipe', 'pipe'], env: { ...env, CLOAKBROWSER_BINARY_PATH: chromixBinary } });
      seedChild.stdin.write(JSON.stringify({ type: 'launch', options: { headless: false, geoip: false, extensionPaths: [], args: [`--fingerprint=${seed}`] }, binaryPath: chromixBinary, skipDownload: true, cdpPort: port, userDataDir: seedProfile, proxy: null, extensionPaths: [], startUrl: null }) + '\n');
    }
    seedChild.stderr?.on('data', () => {});

    // Wait for CDP
    const deadline = Date.now() + 30000;
    while (Date.now() < deadline) {
      try {
        const info = await (await fetch(`${endpoint}/json/version`, { signal: AbortSignal.timeout(2000) })).json();
        const ws = new WebSocket(info.webSocketDebuggerUrl);
        await new Promise((resolve, reject) => { ws.addEventListener('open', resolve, { once: true }); ws.addEventListener('error', () => reject(new Error('WS_FAIL')), { once: true }); });
        seedCdp = new Cdp(ws); break;
      } catch { await sleep(200); }
    }
    if (!seedCdp) { log('seed-failure', { seed, code: 'CDP_TIMEOUT' }); return; }
    await seedCdp.send('Target.setDiscoverTargets', { discover: true });

    // 1. Native fingerprint probe (about:blank)
    const { targetId } = await seedCdp.send('Target.createTarget', { url: 'about:blank', background: false });
    const { sessionId } = await seedCdp.send('Target.attachToTarget', { targetId, flatten: true });
    await seedCdp.send('Page.enable', {}, sessionId);
    await seedCdp.send('Runtime.enable', {}, sessionId);
    const nativeFp = await seedCdp.evaluate(sessionId, fullProbe);
    log('seed-fingerprint', { seed, ...nativeFp });

    // 2. AudioContext hash
    const audioFp = await seedCdp.evaluate(sessionId, audioProbe);
    log('seed-audio', { seed, ...(audioFp ?? {}) });

    // 3. CreepJS trust score — visit and wait for score
    log('creepjs-start', { seed });
    await seedCdp.send('Page.navigate', { url: 'https://abrahamjuliot.github.io/creepjs/' }, sessionId, 20000);
    // CreepJS takes time to compute; poll for score up to 30s
    let creepData = null;
    for (let i = 0; i < 40; i++) {
      await sleep(1000);
      creepData = await seedCdp.evaluate(sessionId, `(() => {
        const scoreEl = document.querySelector('.unblurred + .unblurred, [class*="trust-score"], .score, [class*="score"]');
        const liesEl = document.querySelector('[class*="lies"], .lies');
        const fpIdEl = document.querySelector('[class*="visitor"], [class*="fp-id"], .visitor-id');
        return {
          scoreText: scoreEl?.textContent?.trim() ?? null,
          scoreNum: scoreEl?.textContent?.match(/\\d+(?:\\.\\d+)?/)?.[0] ?? null,
          liesText: liesEl?.textContent?.trim() ?? null,
          liesNum: liesEl?.textContent?.match(/\\d+/)?.[0] ?? null,
          fpId: fpIdEl?.textContent?.trim() ?? null,
          title: document.title,
          ready: document.readyState,
        };
      })()`, 10000).catch(() => null);
      if (creepData?.scoreNum || creepData?.scoreText) break;
    }
    log('creepjs-data', { seed, ...(creepData ?? {}) });
  } catch (e) {
    log('seed-error', { seed, code: e.message ?? String(e) });
  } finally {
    if (seedCdp) { try { await seedCdp.send('Browser.close'); } catch {} seedCdp.socket.close(); }
    if (seedChild?.exitCode === null) { try { seedChild.kill(); } catch {} await sleep(2000); }
    try { await rm(seedRoot, { recursive: true, force: true }); } catch {}
  }
}

try {
  for (const seed of seeds) {
    log('seed-start', { seed });
    await runSeed(seed);
  }
  log('result', { verdict: 'COMPLETED', seedsRun: seeds.length });
} catch (e) {
  log('failure', { code: e.message ?? 'FAILED' });
} finally {
  output.end(); await finished(output);
}
