// Explicit, standalone diagnostic. Not matched by npm test's *.test.mjs glob.
// Node >=22, Windows, the installed Chromix 151 only; no downloads/user endpoints.
// Example from repository root:
// node crates/tauri-app/resources/chromix/test/crashpad-lifecycle-smoke.mjs --run --cohort=bridge-defaults
// Add --scenario=cross-site for four bounded cross-site/close/recreate cycles;
// that scenario refuses flag cohorts and reports observed renderer-set differences.
// Worktree execution may point --runtime=F:/Cloaksession/crates/tauri-app/resources/chromix
// at the already installed production bridge dependencies (never npm install).
import { spawn } from 'node:child_process';
import { existsSync } from 'node:fs';
import { mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import { createServer } from 'node:net';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { createInterface } from 'node:readline';
import { fileURLToPath, pathToFileURL } from 'node:url';

const fail = code => Object.assign(new Error(code), { safeCode: code });
const option = name => process.argv.find(arg => arg.startsWith(`--${name}=`))?.slice(name.length + 3);
const cohort = option('cohort') ?? 'bridge-defaults';
const scenario = option('scenario') ?? 'baseline';
const allowed = ['bridge-defaults', 'bridge-without-disable-breakpad', 'bridge-sandbox', 'raw'];
if (!process.argv.includes('--run') || process.platform !== 'win32' || typeof WebSocket !== 'function' || !allowed.includes(cohort)) {
  throw fail('FIXTURE_REQUIRES_WINDOWS_NODE22_RUN_AND_KNOWN_COHORT');
}
if (!['baseline', 'cross-site'].includes(scenario) || (scenario === 'cross-site' && cohort !== 'bridge-defaults')) {
  throw fail('FIXTURE_SCENARIO_REQUIRES_UNCHANGED_BRIDGE_DEFAULTS');
}
if (scenario === 'cross-site' && option('cycles') !== undefined && option('cycles') !== '4') throw fail('CROSS_SITE_REQUIRES_FOUR_CYCLES');
if (process.env.NODE_OPTIONS || process.env.NODE_USE_ENV_PROXY === '1') throw fail('FIXTURE_NODE_OVERRIDES_REFUSED');
const binary = 'C:/Users/Administrator/.cache/chromix/v151.0.7922.173/win-x64/chromix/chrome.exe';
const runtime = resolve(option('runtime') ?? fileURLToPath(new URL('../', import.meta.url)));
const bridge = join(runtime, 'bridge.mjs');
if (!existsSync(binary) || (cohort !== 'raw' && !existsSync(bridge))) throw fail('FIXTURE_LOCAL_RUNTIME_MISSING');
const cycles = scenario === 'cross-site' ? 4 : Number(option('cycles') ?? 8);
const foreground = process.argv.includes('--foreground'); // explicit diagnostic, never an automatic workaround
if (!Number.isInteger(cycles) || cycles < 1 || cycles > 30) throw fail('FIXTURE_CYCLE_LIMIT');
const log = (phase, error = 'NONE', extra = {}) => console.log(JSON.stringify({ cohort, scenario, phase, error, ...extra }));
const limit = async (promise, ms, code) => {
  let timer;
  try { return await Promise.race([promise, new Promise((_, reject) => { timer = setTimeout(() => reject(fail(code)), ms); })]); }
  finally { clearTimeout(timer); }
};
const delay = ms => new Promise(resolve => setTimeout(resolve, ms));
const flags = args => ({
  disableBreakpad: args.includes('--disable-breakpad'),
  noSandbox: args.includes('--no-sandbox'),
  headless: args.some(arg => /^--headless(?:=|$)/.test(arg)),
  remotePipe: args.includes('--remote-debugging-pipe'),
});

class Cdp {
  constructor(socket) {
    this.socket = socket; this.next = 0; this.pending = new Map(); this.roles = new Map(); this.targets = new Map();
    this.crashed = new Set(); this.events = () => {};
    socket.addEventListener('message', ({ data }) => {
      let message;
      try { message = JSON.parse(String(data)); } catch { log('transport', 'INVALID_JSON'); return; }
      if (message.id) {
        const pending = this.pending.get(message.id);
        if (!pending) return;
        this.pending.delete(message.id); clearTimeout(pending.timer);
        if (message.error) {
          const transition = /Execution context was destroyed|Cannot find context|Cannot find default execution context/i.test(message.error.message ?? '');
          pending.reject(fail(transition ? 'CONTEXT_TRANSITION' : 'CDP_ERROR'));
        } else pending.resolve(message.result);
        return;
      }
      const target = message.params?.targetId;
      const role = this.roles.get(message.sessionId) ?? this.targets.get(target) ?? 'owned-unknown';
      if (message.method === 'Target.targetCrashed' || message.method === 'Inspector.targetCrashed') {
        this.crashed.add(role);
        log(role, 'TARGET_CRASHED', { errorCode: Number.isInteger(message.params?.errorCode) ? message.params.errorCode : null });
      }
      void Promise.resolve(this.events(message)).catch(() => log(role, 'FIXTURE_EVENT_FAILED'));
    });
    socket.addEventListener('close', () => {
      for (const pending of this.pending.values()) { clearTimeout(pending.timer); pending.reject(fail('CDP_DISCONNECTED')); }
      this.pending.clear();
    });
  }
  static async connect(endpoint) {
    const address = new URL(endpoint);
    if (address.hostname !== '127.0.0.1') throw fail('NONLOCAL_ENDPOINT_REFUSED');
    const response = await fetch(`${endpoint}/json/version`, { signal: AbortSignal.timeout(2000) });
    const info = await response.json();
    const ws = new URL(info.webSocketDebuggerUrl);
    if (ws.hostname !== '127.0.0.1' || ws.port !== address.port) throw fail('UNOWNED_ENDPOINT_REFUSED');
    const socket = new WebSocket(ws);
    try {
      await limit(new Promise((resolve, reject) => {
        socket.addEventListener('open', resolve, { once: true });
        socket.addEventListener('error', () => reject(fail('CDP_CONNECT_ERROR')), { once: true });
      }), 5000, 'CDP_CONNECT_TIMEOUT');
    } catch (error) { socket.close(); throw error; }
    return new Cdp(socket);
  }
  send(method, params = {}, sessionId, ms = 3000) {
    const id = ++this.next;
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => { this.pending.delete(id); reject(fail('CDP_TIMEOUT')); }, ms);
      this.pending.set(id, { resolve, reject, timer });
      try { this.socket.send(JSON.stringify({ id, method, params, ...(sessionId ? { sessionId } : {}) })); }
      catch { clearTimeout(timer); this.pending.delete(id); reject(fail('CDP_DISCONNECTED')); }
    });
  }
  async value(session, expression) {
    const response = await this.send('Runtime.evaluate', { expression, returnByValue: true }, session);
    if (response.exceptionDetails) throw fail('JS_EXCEPTION');
    return response.result?.value;
  }
}

const routes = new Map([
  ['https://s.kwaixiaodian.com/zone/home', 'home'],
  ['https://s.kwaixiaodian.com/zone/shop/info/qualification', 'qualification'],
  ['https://s.kwaixiaodian.com/zone/short-video-b/slice', 'slice'],
]);
const crossA = index => `https://fixture-a-${index}.invalid/boot`;
const crossB = index => `https://fixture-b-${index}.invalid/teardown`;
if (scenario === 'cross-site') {
  for (let index = 1; index <= 4; index++) {
    // Distinct registrable sites, not same-site subdomains. Responses remain synthetic.
    routes.set(crossA(index), `cross-a-${index}`);
    routes.set(crossB(index), `cross-b-${index}`);
  }
}
const home = [...routes.keys()][0], qualification = [...routes.keys()][1], slice = [...routes.keys()][2];
const html = marker => `<!doctype html><meta charset="utf-8"><link rel="icon" href="data:,"><title>Isolated renderer fixture</title><main>Local synthetic document</main><script>window.__fixtureMarker=${JSON.stringify(marker)};</script>`;
const root = await mkdtemp(join(tmpdir(), 'chromix-crashpad-fixture-'));
const profile = join(root, 'profile');
let child, exited, cdp, lines, anchor, browserPid;
let verifiedBrowser = false;
let closing = false, crashpadObserved = false;
function observeChild(owned) {
  let tail = '', reported = false;
  owned.stderr?.on('data', chunk => {
    tail = (tail + String(chunk)).slice(-8192);
    if (!reported && tail.includes('Crashpad_NotConnectedToHandler')) {
      reported = true; crashpadObserved = true; log('browser-stderr', 'Crashpad_NotConnectedToHandler');
    }
    tail = tail.slice(-256);
  });
  exited = new Promise(resolve => {
    owned.once('error', () => { log('owned-process', 'PROCESS_ERROR'); resolve(); });
    owned.once('exit', code => {
      if (!closing) log('owned-process', 'PROCESS_EXIT', { exitCode: Number.isInteger(code) ? code : null });
      resolve();
    });
  });
}
const sockets = new Set();
const sink = createServer(socket => {
  sockets.add(socket); socket.on('error', () => {}); socket.on('close', () => sockets.delete(socket));
  socket.end('HTTP/1.1 502 Bad Gateway\r\nContent-Length: 0\r\nConnection: close\r\n\r\n');
});
await new Promise(resolve => sink.listen(0, '127.0.0.1', resolve));
const reserve = createServer();
await new Promise(resolve => reserve.listen(0, '127.0.0.1', resolve));
const port = reserve.address().port;
await new Promise(resolve => reserve.close(resolve));
const endpoint = `http://127.0.0.1:${port}`;
const common = [
  '--disable-background-networking', '--disable-component-update', '--disable-sync', '--disable-default-apps', '--disable-extensions',
  '--disable-quic', '--no-first-run', '--no-default-browser-check', '--metrics-recording-only', '--safebrowsing-disable-auto-update',
  '--host-resolver-rules=MAP * ~NOTFOUND, EXCLUDE 127.0.0.1', `--proxy-server=http://127.0.0.1:${sink.address().port}`,
  '--proxy-bypass-list=<-loopback>', '--force-webrtc-ip-handling-policy=disable_non_proxied_udp', '--window-size=1000,800',
  '--fingerprint=123456789', '--fingerprint-platform=windows',
];
const env = { ...process.env };
for (const key of ['HTTP_PROXY', 'HTTPS_PROXY', 'ALL_PROXY', 'NO_PROXY', 'NODE_OPTIONS', 'NODE_USE_ENV_PROXY']) delete env[key];
// No user settings are imported. All cohorts share quarantine and a synthetic seed.
// Raw omits Playwright defaults and host/SDK augmentation; it is a stack control,
// not a one-flag comparison. The two bridge variants each change one launch option.
try {
  if (cohort === 'raw') {
    const args = [...common, `--remote-debugging-port=${port}`, '--remote-debugging-address=127.0.0.1', `--user-data-dir=${profile}`, 'about:blank'];
    log('launch-flags', 'NONE', flags(args));
    child = spawn(binary, args, { stdio: ['ignore', 'ignore', 'pipe'], env });
    browserPid = child.pid;
    observeChild(child);
  } else {
    // Observe only the fixture-owned browser spawn. Never persist/log full argv.
    const hook = join(root, 'observe-spawn.mjs');
    const flagFile = join(root, 'flags.json');
    const pidFile = join(root, 'owned-browser-pid.json');
    await writeFile(hook, `import cp from 'node:child_process'; import {writeFileSync} from 'node:fs'; import {resolve} from 'node:path'; import {syncBuiltinESMExports} from 'node:module';
const original=cp.spawn; cp.spawn=function(command,args=[],options){ const owned=resolve(String(command)).toLowerCase()===resolve(${JSON.stringify(binary)}).toLowerCase(); if(owned){writeFileSync(${JSON.stringify(flagFile)},JSON.stringify({disableBreakpad:args.includes('--disable-breakpad'),noSandbox:args.includes('--no-sandbox'),headless:args.some(a=>/^--headless(?:=|$)/.test(a)),remotePipe:args.includes('--remote-debugging-pipe')}));} const child=original.call(this,command,args,options); if(owned&&Number.isInteger(child.pid)){writeFileSync(${JSON.stringify(pidFile)},JSON.stringify(child.pid));} return child;}; syncBuiltinESMExports();`);
    const options = { headless: false, args: common, extensionPaths: [], geoip: false };
    if (cohort === 'bridge-without-disable-breakpad') options.launchOptions = { ignoreDefaultArgs: ['--enable-automation', '--disable-breakpad'] };
    if (cohort === 'bridge-sandbox') options.launchOptions = { chromiumSandbox: true };
    child = spawn(process.execPath, ['--import', pathToFileURL(hook).href, bridge], { stdio: ['pipe', 'pipe', 'pipe'], env });
    observeChild(child);
    lines = createInterface({ input: child.stdout });
    const protocol = lines[Symbol.asyncIterator]();
    child.stdin.write(`${JSON.stringify({ type: 'launch', options, binaryPath: binary, skipDownload: true, cdpPort: port, userDataDir: profile, proxy: null, extensionPaths: [], startUrl: null })}\n`);
    const reply = await limit(protocol.next(), 30000, 'BRIDGE_READY_TIMEOUT');
    if (reply.done) throw fail('BRIDGE_EXIT_BEFORE_READY');
    let message;
    try { message = JSON.parse(reply.value); } catch { throw fail('BRIDGE_PROTOCOL_ERROR'); }
    if (message.type !== 'ready' || message.cdpEndpoint !== endpoint) throw fail('BRIDGE_LAUNCH_ERROR');
    log('launch-flags', 'NONE', JSON.parse(await readFile(flagFile, 'utf8')));
    browserPid = JSON.parse(await readFile(pidFile, 'utf8')); // internal ownership proof only; never logged
  }
  if (cohort === 'raw') {
    const deadline = Date.now() + 15000;
    while (true) {
      try { cdp = await Cdp.connect(endpoint); break; }
      catch { if (Date.now() >= deadline || child.exitCode !== null) throw fail('RAW_READY_TIMEOUT'); await delay(50); }
    }
  } else cdp = await Cdp.connect(endpoint);
  const processes = await cdp.send('SystemInfo.getProcessInfo');
  if (!Number.isInteger(browserPid) || !processes.processInfo.some(process => process.type === 'browser' && process.id === browserPid)) {
    throw fail('OWNED_BROWSER_UNVERIFIED');
  }
  verifiedBrowser = true;
  await cdp.send('Target.setDiscoverTargets', { discover: true });
  cdp.events = async message => {
    if (message.method !== 'Fetch.requestPaused') return;
    const { requestId, request } = message.params;
    const marker = routes.get(request.url);
    if (marker && request.method === 'GET') {
      await cdp.send('Fetch.fulfillRequest', { requestId, responseCode: 200,
        responseHeaders: [{ name: 'Content-Type', value: 'text/html; charset=utf-8' }, { name: 'Cache-Control', value: 'no-store' }],
        body: Buffer.from(html(marker)).toString('base64') }, message.sessionId);
    } else await cdp.send('Fetch.failRequest', { requestId, errorReason: 'BlockedByClient' }, message.sessionId);
  };
  let scenarioDeadline = Infinity;
  const observedBirths = new Set();
  let processEvidenceIncomplete = false;
  function checkCrossSiteBudget() {
    if (scenario !== 'cross-site') return;
    if (crashpadObserved) throw fail('Crashpad_NotConnectedToHandler');
    if (cdp.crashed.size) throw fail('TARGET_CRASHED');
    if (Date.now() >= scenarioDeadline) throw fail('FIXTURE_TOTAL_DEADLINE');
  }
  function observeRendererSets(phase, before, after) {
    checkCrossSiteBudget();
    if (!before || !after) { processEvidenceIncomplete = true; log(phase, 'PROCESS_INFO_UNAVAILABLE'); return; }
    const added = [...after].filter(id => !before.has(id));
    const removed = [...before].filter(id => !after.has(id));
    const reused = [...after].filter(id => before.has(id));
    for (const id of added) observedBirths.add(id); // IDs stay internal; only set-difference counts are emitted.
    if (added.length) log(phase, 'RENDERER_ADDED', { count: added.length });
    if (removed.length) log(phase, 'RENDERER_REMOVED', { count: removed.length });
    if (reused.length) log(phase, 'RENDERER_REUSED', { count: reused.length });
    if (!added.length && !removed.length && !reused.length) {
      processEvidenceIncomplete = true; log(phase, 'NO_RENDERERS_OBSERVED');
    }
  }
  async function target(role) {
    checkCrossSiteBudget();
    const before = scenario === 'cross-site' && role === 'work' ? await rendererSet() : null;
    checkCrossSiteBudget();
    const { targetId } = await cdp.send('Target.createTarget', { url: 'about:blank', background: true });
    const { sessionId } = await cdp.send('Target.attachToTarget', { targetId, flatten: true });
    cdp.roles.set(sessionId, role); cdp.targets.set(targetId, role);
    await cdp.send('Page.enable', {}, sessionId);
    await cdp.send('Runtime.enable', {}, sessionId);
    await cdp.send('Inspector.enable', {}, sessionId);
    await cdp.send('Fetch.enable', { patterns: [{ urlPattern: '*', requestStage: 'Request' }] }, sessionId);
    if (role === 'work' && foreground) await cdp.send('Target.activateTarget', { targetId });
    if (scenario === 'cross-site' && role === 'work') observeRendererSets('new-target-renderers', before, await rendererSet());
    return { targetId, sessionId, role };
  }
  async function rendererSet() {
    try {
      const value = await cdp.send('SystemInfo.getProcessInfo');
      const renderers = value.processInfo.filter(process => process.type === 'renderer');
      if (scenario === 'cross-site' && renderers.some(process => !Number.isInteger(process.id) || process.id <= 0)) throw fail('PROCESS_INFO_INVALID');
      return new Set(renderers.map(process => process.id));
    } catch {
      if (scenario === 'cross-site') throw fail('PROCESS_INFO_UNAVAILABLE');
      return null;
    }
  }
  async function navigate(target, url, reload = false, phase = reload ? 'same-target-reload' : 'navigation') {
    checkCrossSiteBudget();
    const renderersBefore = await rendererSet();
    const previous = await cdp.value(target.sessionId, 'performance.timeOrigin');
    checkCrossSiteBudget();
    const start = Date.now();
    const response = await cdp.send(reload ? 'Page.reload' : 'Page.navigate', reload ? { ignoreCache: true } : { url }, target.sessionId);
    if (response.errorText) throw fail('NAVIGATION_ERROR');
    while (Date.now() - start < 10000) {
      checkCrossSiteBudget();
      if (cdp.crashed.has(target.role)) throw fail('TARGET_CRASHED');
      let value;
      try { value = await cdp.value(target.sessionId, '({marker:window.__fixtureMarker,epoch:performance.timeOrigin,top:window===window.top})'); }
      catch (error) { if (error.safeCode !== 'CONTEXT_TRANSITION') throw error; }
      checkCrossSiteBudget();
      if (scenario === 'cross-site' && Date.now() - start >= 10000) throw fail('DOCUMENT_READY_TIMEOUT');
      if (value?.top && value.marker === routes.get(url) && value.epoch !== previous) {
        log(phase, 'NONE', { elapsedMs: Date.now() - start });
        const after = await rendererSet();
        if (scenario === 'cross-site') {
          if (target.role === 'work') observeRendererSets(`${phase}-renderers`, renderersBefore, after);
        } else {
          log('renderer-lifecycle', !renderersBefore || !after ? 'PROCESS_INFO_UNAVAILABLE'
            : after.size !== renderersBefore.size || [...after].some(id => !renderersBefore.has(id)) ? 'RENDERER_SET_CHANGED' : 'RENDERER_SET_UNCHANGED');
        }
        return;
      }
      await delay(25); // bounded observation polling, never a blind navigation-success assumption
    }
    throw fail('DOCUMENT_READY_TIMEOUT');
  }
  async function health(target) {
    checkCrossSiteBudget();
    if (await cdp.value(target.sessionId, '2') !== 2) throw fail('HEALTH_RESPONSE_INVALID');
    if (scenario === 'cross-site') log('anchor-control', 'ANCHOR_RESPONSIVE');
  }
  async function closeAndProve(target, phase) {
    checkCrossSiteBudget();
    const before = await rendererSet();
    checkCrossSiteBudget();
    const response = await cdp.send('Target.closeTarget', { targetId: target.targetId });
    if (response.success === false) throw fail('TARGET_CLOSE_REFUSED');
    const until = Math.min(Date.now() + 5000, scenarioDeadline);
    while (Date.now() < until) {
      checkCrossSiteBudget();
      const inventory = await cdp.send('Target.getTargets', {}, undefined, Math.max(1, Math.min(3000, until - Date.now())));
      checkCrossSiteBudget();
      if (Date.now() >= until) throw fail('TARGET_ABSENCE_TIMEOUT');
      if (!Array.isArray(inventory.targetInfos)) throw fail('INVENTORY_RESPONSE_INVALID');
      if (!inventory.targetInfos.some(info => info.targetId === target.targetId)) {
        log(phase, 'TARGET_ABSENCE_PROVED');
        observeRendererSets(`${phase}-renderers`, before, await rendererSet());
        return;
      }
      await delay(25); // every wait is followed by another authoritative inventory read
    }
    throw fail('TARGET_ABSENCE_TIMEOUT');
  }
  anchor = await target('anchor');
  await navigate(anchor, home);
  if (scenario === 'cross-site') {
    scenarioDeadline = Date.now() + 90000;
    await limit((async () => {
      let work = await target('work');
      log('phase-start', 'CROSS_SITE_FOUR_CYCLES');
      for (let i = 1; i <= 4; i++) {
        checkCrossSiteBudget();
        await navigate(work, crossA(i), false, `cross-${i}-site-a`); await health(anchor);
        await navigate(work, slice, false, `cross-${i}-slice`); await health(anchor);
        await navigate(work, slice, true, `cross-${i}-same-target-reload`); await health(anchor);
        await navigate(work, crossB(i), false, `cross-${i}-site-b`); await health(anchor);
        await closeAndProve(work, `cross-${i}-close`); await health(anchor);
        if (i < 4) work = await target('work'); // only after absence proof; no target adoption
      }
    })(), 90000, 'FIXTURE_TOTAL_DEADLINE');
  } else {
    let work = await target('work');
    log('visibility-control', foreground ? 'FOREGROUND_REQUESTED' : 'BACKGROUND_REQUESTED');
    const deadline = Date.now() + 90000;
    log('phase-start', 'SAME_TARGET_LOOP');
    for (let i = 0; i < cycles; i++) {
      if (Date.now() >= deadline) throw fail('FIXTURE_TOTAL_DEADLINE');
      await navigate(work, qualification); await navigate(work, slice); await navigate(work, slice, true); await health(anchor);
    }
    await cdp.send('Target.closeTarget', { targetId: work.targetId });
    log('phase-start', 'NEW_TARGET_LOOP');
    for (let i = 0; i < cycles; i++) {
      if (Date.now() >= deadline) throw fail('FIXTURE_TOTAL_DEADLINE');
      work = await target('work');
      await navigate(work, slice); await navigate(work, slice, true); await health(anchor);
      await cdp.send('Target.closeTarget', { targetId: work.targetId });
    }
  }
  if (crashpadObserved) throw fail('Crashpad_NotConnectedToHandler');
  if (cdp.crashed.size) throw fail('TARGET_CRASHED');
  if (scenario === 'cross-site' && (!observedBirths.size || processEvidenceIncomplete)) {
    log('result', !observedBirths.size ? 'INCONCLUSIVE_RENDERER_REUSE' : 'INCONCLUSIVE_PROCESS_COVERAGE');
    process.exitCode = 2;
  } else log('result', 'NOT_REPRODUCED_IN_SYNTHETIC_FIXTURE');
} catch (error) {
  log('result', crashpadObserved ? 'Crashpad_NotConnectedToHandler' : error.safeCode ?? 'FIXTURE_FAILURE');
  if (cdp && anchor) {
    try { await cdp.value(anchor.sessionId, '2'); log('anchor-control', 'ANCHOR_RESPONSIVE'); }
    catch { log('anchor-control', 'ANCHOR_UNRESPONSIVE'); }
  }
  process.exitCode = 1;
} finally {
  closing = true;
  if (child?.stdin?.writable) child.stdin.end('{"type":"close"}\n');
  if (cdp) {
    if (verifiedBrowser) await cdp.send('Browser.close', {}, undefined, 2000).catch(() => {});
    cdp.socket.close();
  }
  if (child && Number.isInteger(child.pid) && child.exitCode === null) {
    await limit(exited ?? new Promise(resolve => child.once('exit', resolve)), 10000, 'OWNED_CLOSE_TIMEOUT').catch(async () => {
      // Only this harness-owned process tree; never process-name or user-browser kill.
      const killer = spawn('taskkill', ['/PID', String(child.pid), '/T', '/F'], { stdio: 'ignore' });
      await limit(new Promise(resolve => { killer.once('exit', resolve); killer.once('error', resolve); }), 5000, 'OWNED_KILL_TIMEOUT').catch(() => {});
    });
  }
  lines?.close();
  for (const socket of sockets) socket.destroy();
  await new Promise(resolve => sink.close(resolve));
  await rm(root, { recursive: true, force: true, maxRetries: 20, retryDelay: 50 }).catch(() => log('cleanup', 'OWNED_TEMP_RETAINED'));
}
