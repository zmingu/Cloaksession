// Fingerprint benchmark: CloakBrowser vs Chromix. 2026-10-02.
// Same seed, no proxy, same test sites. Reports structural fingerprint data only.
// node <this> --run --engine=cloakbrowser
// node <this> --run --engine=chromix
import { spawn } from 'node:child_process';
import { mkdtemp, rm, writeFile, readFile, access } from 'node:fs/promises';
import { createServer } from 'node:net';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { createInterface } from 'node:readline';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { finished } from 'node:stream/promises';
import { createWriteStream } from 'node:fs';

const self = fileURLToPath(import.meta.url);
const research = dirname(self);
const repo = resolve(research, '../../../..');
const chromixRuntime = join(repo, 'crates/tauri-app/resources/chromix');
const chromixBridge = join(chromixRuntime, 'bridge.mjs');
const cloakBinary = 'C:/Users/Administrator/.cloakbrowser/chromium-146.0.7680.177.5/chrome.exe';
const chromixBinary = 'C:/Users/Administrator/.cache/chromix/v154.0.8037.57/win-x64/chromix/chrome.exe';
const cbRuntime = process.env.TEMP + '/cloakbrowser-pkg/install/node_modules/cloakbrowser';

const option = name => process.argv.find(a => a.startsWith(`--${name}=`))?.slice(name.length + 3);
const engine = option('engine');
const seed = option('seed') ?? '123456789';
if (!engine || !['cloakbrowser', 'chromix'].includes(engine) || typeof WebSocket !== 'function') {
  throw Object.assign(new Error('USAGE: --run --engine=cloakbrowser|chromix [--seed=N]'), { safeCode: 'USAGE' });
}
if (process.env.NODE_OPTIONS || process.env.NODE_USE_ENV_PROXY === '1') throw new Error('NODE_OVERRIDES_REFUSED');
await access(cloakBinary);
if (engine === 'chromix') await access(chromixBinary);
await access(chromixBridge);

const stamp = new Date().toISOString().replace(/[:.]/g, '-');
const outputPath = join(research, `fingerprint-bench-2026-10-02-${engine}-${stamp}.jsonl`);
const output = createWriteStream(outputPath, { flags: 'wx' });
await new Promise((r, j) => { output.once('open', r); output.once('error', j); });
const log = (event, details = {}) => output.write(`${JSON.stringify({ time: new Date().toISOString(), engine, seed, event, ...details })}\n`);
log('run', { node: process.version, artifact: outputPath.split(/[\\/]/).pop() });

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
  send(method, params = {}, sessionId, ms = 10000) {
    const id = ++this.next;
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => { this.pending.delete(id); reject(new Error('NO_REPLY')); }, ms);
      this.pending.set(id, { resolve, reject, timer });
      this.socket.send(JSON.stringify({ id, method, params, ...(sessionId ? { sessionId } : {}) }));
    });
  }
  async evaluate(session, expression, ms = 15000) {
    const r = await this.send('Runtime.evaluate', { expression, returnByValue: true }, session, ms);
    if (r.exceptionDetails) throw new Error('JS_EXCEPTION');
    return r.result?.value;
  }
}

const sleep = ms => new Promise(r => setTimeout(r, ms));
const bounded = (p, ms, code) => Promise.race([p, new Promise((_, rej) => setTimeout(() => rej(Object.assign(new Error(code), { safeCode: code })), ms))]);

// Minimal fingerprint probe — returns structural data, no PII.
const probe = `(() => {
  const nav = navigator;
  const fp = {
    ua: nav.userAgent,
    platform: nav.platform,
    languages: nav.languages,
    hardwareConcurrency: nav.hardwareConcurrency,
    deviceMemory: nav.deviceMemory,
    webdriver: nav.webdriver,
    vendor: nav.vendor,
    appName: nav.appName,
    appVersion: nav.appVersion,
    cookieEnabled: nav.cookieEnabled,
    doNotTrack: nav.doNotTrack,
    maxTouchPoints: nav.maxTouchPoints,
    pdfViewerEnabled: nav.pdfViewerEnabled,
    languagesJoined: (nav.languages || []).join(','),
  };
  // Screen
  fp.screen = { w: screen.width, h: screen.height, aw: screen.availWidth, ah: screen.availHeight, depth: screen.colorDepth };
  // Canvas hash (stable per fingerprint)
  try {
    const c = document.createElement('canvas'); c.width = 200; c.height = 50;
    const ctx = c.getContext('2d'); ctx.textBaseline = 'top'; ctx.font = '14px Arial'; ctx.fillText('FpTest,\\u00e9\\u00e8\\u00e0', 2, 2);
    fp.canvasHash = c.toDataURL().slice(-64);
  } catch { fp.canvasHash = null; }
  // WebGL
  try {
    const c = document.createElement('canvas'); const gl = c.getContext('webgl') || c.getContext('experimental-webgl');
    if (gl) { const mi = gl.getExtension('WEBGL_debug_renderer_info');
      fp.webgl = { vendor: gl.getParameter(gl.VENDOR), renderer: gl.getParameter(gl.RENDERER),
        unmaskedVendor: mi ? gl.getParameter(mi.UNMASKED_VENDOR_WEBGL) : null,
        unmaskedRenderer: mi ? gl.getParameter(mi.UNMASKED_RENDERER_WEBGL) : null }; }
  } catch { fp.webgl = null; }
  // Timezone
  fp.timezone = Intl.DateTimeFormat().resolvedOptions().timeZone;
  fp.timezoneOffset = new Date().getTimezoneOffset();
  // Audio context (unique fingerprint)
  try { const ac = new (window.AudioContext || window.webkitAudioContext)();
    fp.audioSampleRate = ac.sampleRate; fp.audioState = ac.state; ac.close();
  } catch { fp.audio = null; }
  // Client Hints
  if (nav.userAgentData) {
    fp.uaData = { platform: nav.userAgentData.platform, brands: nav.userAgentData.brands?.map(b => b.brand + '/' + b.version) };
  }
  // Plugins
  fp.plugins = Array.from(nav.plugins || []).map(p => p.name);
  // Fonts check (partial — just a few common ones)
  fp.fontsCheck = 'Arial,Times New Roman,Courier New,Segoe UI,Microsoft YaHei'.split(',').filter(f => document.fonts?.check('12px "' + f + '"'));
  return fp;
})()`;

const sites = [
  { name: 'sannysoft', url: 'https://bot.sannysoft.com/' },
  { name: 'creepjs', url: 'https://abrahamjuliot.github.io/creepjs/' },
  { name: 'browserleaks', url: 'https://browserleaks.com/canvas' },
  { name: 'whoer', url: 'https://whoer.net/' },
];

let child, cdp, profile, root;
let verified = false, closing = false;
const env = { ...process.env };
for (const k of ['HTTP_PROXY','HTTPS_PROXY','ALL_PROXY','NO_PROXY','NODE_OPTIONS','NODE_USE_ENV_PROXY']) delete env[k];

try {
  root = await mkdtemp(join(tmpdir(), `fpbench-${engine}-`));
  profile = join(root, 'profile');
  const reserve = createServer(); await new Promise(r => reserve.listen(0, '127.0.0.1', r));
  const port = reserve.address().port; await new Promise(r => reserve.close(r));
  const endpoint = `http://127.0.0.1:${port}`;

  if (engine === 'cloakbrowser') {
    // Direct spawn with CloakBrowser stealth args (matches app's build_spawn_args for Cloakbrowser)
    const args = [
      `--user-data-dir=${profile}`, `--remote-debugging-port=${port}`, '--remote-debugging-address=127.0.0.1',
      '--no-first-run', '--no-default-browser-check', '--disable-features=Translate',
      `--fingerprint=${seed}`, '--fingerprint-platform=windows', '--fingerprint-noise=false',
      '--window-size=1280,800', 'about:blank',
    ];
    child = spawn(cloakBinary, args, { stdio: ['ignore', 'pipe', 'pipe'], env });
  } else {
    // Chromix via bridge
    child = spawn(process.execPath, [chromixBridge], { stdio: ['pipe', 'pipe', 'pipe'], env: { ...env, CLOAKBROWSER_BINARY_PATH: chromixBinary } });
    child.stdin.write(JSON.stringify({ type: 'launch', options: { headless: false, geoip: false, extensionPaths: [], args: [`--fingerprint=${seed}`] }, binaryPath: chromixBinary, skipDownload: true, cdpPort: port, userDataDir: profile, proxy: null, extensionPaths: [], startUrl: null }) + '\n');
  }

  let stderr = '';
  child.stderr?.on('data', c => { stderr += String(c); });

  // Wait for CDP
  const deadline = Date.now() + 30000;
  while (Date.now() < deadline) {
    try {
      const info = await (await fetch(`${endpoint}/json/version`, { signal: AbortSignal.timeout(2000) })).json();
      const ws = new WebSocket(info.webSocketDebuggerUrl);
      await new Promise((resolve, reject) => { ws.addEventListener('open', resolve, { once: true }); ws.addEventListener('error', () => reject(new Error('WS_FAIL')), { once: true }); });
      cdp = new Cdp(ws);
      break;
    } catch { await sleep(200); }
  }
  if (!cdp) throw new Error('CDP_TIMEOUT');
  await cdp.send('Target.setDiscoverTargets', { discover: true });
  log('browser-ready', { port });

  // Create test tab
  const { targetId } = await cdp.send('Target.createTarget', { url: 'about:blank', background: false });
  const { sessionId } = await cdp.send('Target.attachToTarget', { targetId, flatten: true });
  await cdp.send('Page.enable', {}, sessionId);
  await cdp.send('Runtime.enable', {}, sessionId);

  // 1. Native fingerprint probe (about:blank)
  const nativeFp = await cdp.evaluate(sessionId, probe);
  log('native-fingerprint', nativeFp);

  // 2. Visit each test site, capture structural data
  for (const site of sites) {
    log('site-start', { name: site.name, url: site.url });
    try {
      await cdp.send('Page.navigate', { url: site.url }, sessionId, 20000);
      // Wait for load
      for (let i = 0; i < 30; i++) {
        const ready = await cdp.evaluate(sessionId, 'document.readyState');
        if (ready === 'complete') break;
        await sleep(500);
      }
      await sleep(2000); // let JS run

      // Capture structured fingerprint data from the page
      const data = await cdp.evaluate(sessionId, `(() => {
        const out = { title: document.title, url: location.href, ready: document.readyState };
        // sannysoft: capture table rows
        if (location.hostname.includes('sannysoft')) {
          const rows = [...document.querySelectorAll('tr')];
          out.table = rows.map(r => [...r.querySelectorAll('td')].map(td => td.textContent?.trim())).filter(r => r.length);
        }
        // creepjs: capture trust score
        if (location.hostname.includes('creepjs')) {
          const score = document.querySelector('.trust-score, [class*="score"]');
          out.scoreText = score?.textContent?.trim() ?? null;
        }
        // whoer: capture score
        if (location.hostname.includes('whoer')) {
          const score = document.querySelector('.score-card, [class*="score"]');
          out.scoreText = score?.textContent?.trim() ?? null;
        }
        // browserleaks: capture canvas hash
        if (location.hostname.includes('browserleaks')) {
          const hash = document.querySelector('#canvasHash, [id*="hash"]');
          out.hashText = hash?.textContent?.trim() ?? null;
        }
        return out;
      })()`, 20000);
      log('site-data', { name: site.name, ...data });
    } catch (e) {
      log('site-error', { name: site.name, error: e.message ?? String(e) });
    }
  }

  log('result', { verdict: 'COMPLETED' });
} catch (e) {
  log('failure', { code: e.safeCode ?? e.message ?? 'FAILED' });
} finally {
  closing = true;
  if (cdp) { try { await cdp.send('Browser.close'); } catch {} cdp.socket.close(); }
  if (child?.exitCode === null) { try { child.kill(); } catch {} await sleep(2000); }
  if (root) { try { await rm(root, { recursive: true, force: true }); } catch {} }
  output.end(); await finished(output);
}
