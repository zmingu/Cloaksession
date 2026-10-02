// Standalone investigation, 2026-10-02. NEVER imported by the application/tests.
// All launches use a newly created, owned profile through the unchanged production bridge.
// No Cookie/token copy, subject extraction, checkbox click, form submit, or user endpoint.
// node <this file> --run --mode=synthetic --cohort=baseline --cycles=8
// mode=live allows only navigation, the two qualification tabs and opening the slice drawer.
// mode=detector-control deliberately crashes ONE synthetic owned target to validate detection.
import cp, { spawn } from 'node:child_process';
import { syncBuiltinESMExports } from 'node:module';
import { randomUUID } from 'node:crypto';
import { createWriteStream } from 'node:fs';
import { access, mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import { createServer } from 'node:net';
import { tmpdir } from 'node:os';
import { basename, dirname, join, resolve } from 'node:path';
import { createInterface } from 'node:readline';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { finished } from 'node:stream/promises';

const self = fileURLToPath(import.meta.url);
const research = dirname(self);
const repo = resolve(research, '../../../..');
const runtime = join(repo, 'crates/tauri-app/resources/chromix');
const option = name => process.argv.find(a => a.startsWith(`--${name}=`))?.slice(name.length + 3);
const fail = code => Object.assign(new Error(code), { safeCode: code });
const binary = option('binary') ?? 'C:/Users/Administrator/.cache/chromix/v151.0.7922.173/win-x64/chromix/chrome.exe';
const expectedVersion = option('expected-version') ?? 'Chrome/151.0.7922.173';
const bridge = join(runtime, 'bridge.mjs');
const sleep = ms => new Promise(resolve => setTimeout(resolve, ms));
async function bounded(promise, ms, code) {
  let timer;
  try { return await Promise.race([promise, new Promise((_, reject) => { timer = setTimeout(() => reject(fail(code)), ms); })]); }
  finally { clearTimeout(timer); }
}

// Child-only observation. All overrides exist in this dated diagnostic, never in production.
if (process.argv.includes('--bridge-child')) {
  const output = process.stdout.write.bind(process.stdout);
  const send = message => output(`${JSON.stringify(message)}\n`);
  process.stdout.write = process.stderr.write.bind(process.stderr);
  const original = cp.spawn;
  cp.spawn = function (command, args = [], options) {
    const owned = resolve(String(command)).toLowerCase() === resolve(binary).toLowerCase();
    const child = original.call(this, command, args, options);
    if (owned) {
      const sanitized = args.map(arg => arg.replace(/^(--(?:user-data-dir|remote-debugging-port|proxy-server|log-file)=).*/, '$1<OWNED>'));
      send({ type: 'audit', browserPid: child.pid,
        profileMatches: args.includes(`--user-data-dir=${process.env.CHROMIX_DIAG_PROFILE}`),
        flags: { disableBreakpad: args.includes('--disable-breakpad'), noSandbox: args.includes('--no-sandbox'),
          disableDevShmUsage: args.includes('--disable-dev-shm-usage'), pipe: args.includes('--remote-debugging-pipe'),
          headless: args.some(a => /^--headless(?:=|$)/.test(a)) },
        normalizedArgs: sanitized,
      });
    }
    return child;
  };
  syncBuiltinESMExports();
  const { runBridge } = await import(pathToFileURL(bridge).href);
  await runBridge({ send, forceExit: code => process.exit(code) });
  process.exit(0);
}

const mode = option('mode') ?? 'synthetic';
const cohort = option('cohort') ?? 'baseline';
const cycles = Number(option('cycles') ?? (mode === 'live' ? 2 : 8));
const loginSeconds = Number(option('login-seconds') ?? 0);
if (!process.argv.includes('--run') || process.platform !== 'win32' || typeof WebSocket !== 'function'
    || !['synthetic', 'live', 'detector-control'].includes(mode)
    || !['baseline', 'without-disable-breakpad', 'sandbox', 'without-disable-dev-shm-usage'].includes(cohort)
    || !Number.isInteger(cycles) || cycles < 1 || cycles > 20
    || !Number.isInteger(loginSeconds) || loginSeconds < 0 || loginSeconds > 300
    || (loginSeconds && mode !== 'live') || (mode === 'detector-control' && cohort !== 'baseline')) {
  throw fail('EXPLICIT_RUN_WINDOWS_AND_BOUNDED_OPTIONS_REQUIRED');
}
if (process.env.NODE_OPTIONS || process.env.NODE_USE_ENV_PROXY === '1') throw fail('NODE_OVERRIDES_REFUSED');
await access(binary); await access(bridge); await access(research);
const stamp = new Date().toISOString().replace(/[:.]/g, '-');
const outputPath = join(research, `chromix-root-cause-2026-10-02-${mode}-${cohort}-${stamp}.jsonl`);
const output = createWriteStream(outputPath, { flags: 'wx' });
await new Promise((resolve, reject) => { output.once('open', resolve); output.once('error', reject); });
const log = (event, details = {}) => {
  const row = JSON.stringify({ time: new Date().toISOString(), mode, cohort, event, ...details });
  output.write(`${row}\n`); console.log(row);
};
log('run', { node: process.version, cycles, loginSeconds, artifact: basename(outputPath) });

class Cdp {
  constructor(socket) {
    this.socket = socket; this.next = 0; this.pending = new Map();
    this.sessions = new Map(); this.targets = new Map(); this.crashes = []; this.onEvent = async () => {};
    socket.addEventListener('message', ({ data }) => {
      const m = JSON.parse(String(data));
      if (m.id) {
        const p = this.pending.get(m.id);
        if (!p) return;
        this.pending.delete(m.id); clearTimeout(p.timer);
        if (m.error) {
          const transitional = /Execution context was destroyed|Cannot find context|Cannot find default execution context/.test(m.error.message ?? '');
          p.reject(fail(transitional ? 'CONTEXT_TRANSITION' : 'CDP_ERROR'));
        } else p.resolve(m.result);
        return;
      }
      const role = this.targets.get(m.params?.targetId) ?? this.sessions.get(m.sessionId);
      if (role && ['Target.targetCrashed', 'Inspector.targetCrashed'].includes(m.method)) {
        const evidence = { method: m.method, role, status: ['crashed', 'killed', 'oom', 'abnormal', 'launch-failed'].includes(m.params?.status) ? m.params.status : null,
          errorCode: Number.isInteger(m.params?.errorCode) ? m.params.errorCode : null };
        this.crashes.push(evidence); log('crash-event', evidence);
      }
      if (role && m.method === 'Target.targetDestroyed') log('target-destroyed', { role });
      void this.onEvent(m).catch(error => { log('event-handler-error', { code: error.safeCode ?? 'HANDLER_FAILED' }); });
    });
    socket.addEventListener('close', () => {
      for (const p of this.pending.values()) { clearTimeout(p.timer); p.reject(fail('TRANSPORT_CLOSED')); }
      this.pending.clear();
    });
  }
  async send(method, params = {}, sessionId, ms = 2500) {
    const id = ++this.next;
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => { this.pending.delete(id); reject(fail('NO_REPLY')); }, ms);
      this.pending.set(id, { resolve, reject, timer });
      try { this.socket.send(JSON.stringify({ id, method, params, ...(sessionId ? { sessionId } : {}) })); }
      catch { clearTimeout(timer); this.pending.delete(id); reject(fail('TRANSPORT_CLOSED')); }
    });
  }
  async value(target, expression, ms = 2500) {
    const r = await this.send('Runtime.evaluate', { expression, returnByValue: true }, target.sessionId, ms);
    if (r.exceptionDetails) throw fail('PAGE_JS_EXCEPTION');
    return r.result?.value;
  }
}

const qualification = 'https://s.kwaixiaodian.com/zone/shop/info/qualification';
const slice = 'https://s.kwaixiaodian.com/zone/short-video-b/slice';
const login = 'https://login.kwaixiaodian.com/?biz=zone&redirect_url=https%3A%2F%2Fs.kwaixiaodian.com%2Fzone%2Fhome';
// Return structural booleans/counts only. Never return identity/document/image/header/body data.
const snapshot = `(() => {
  const u = new URL(location.href), visible = e => e.getClientRects().length > 0 && getComputedStyle(e).visibility !== 'hidden';
  const shop = u.origin === 'https://s.kwaixiaodian.com';
  const route = shop && u.pathname === '/zone/shop/info/qualification' ? 'qualification' : shop && u.pathname === '/zone/short-video-b/slice' ? 'slice' : shop && u.pathname === '/zone/home' ? 'home' : u.hostname === 'login.kwaixiaodian.com' ? 'login' : u.protocol === 'about:' ? 'blank' : u.hostname.endsWith('.invalid') ? 'synthetic-anchor' : 'other';
  const tabs = [...document.querySelectorAll('[role="tab"]')].filter(visible);
  const tab = label => tabs.filter(e => e.textContent.trim() === label);
  const buttons = [...document.querySelectorAll('.js-page-content button')].filter(e => visible(e) && ['修改设置','去设置'].includes(e.textContent.trim()) && !e.disabled);
  const drawers = [...document.querySelectorAll('.kwaishop-tianhe-shortVideoB-pc-drawer-content')].filter(e => visible(e) && e.querySelector('.kwaishop-tianhe-shortVideoB-pc-drawer-title')?.textContent.trim() === '直播切片托管设置');
  return { route, ready: document.readyState, epoch: performance.timeOrigin,
    main: tab('主体信息').length, talent: tab('达人主体信息').length,
    mainSelected: tab('主体信息')[0]?.getAttribute('aria-selected') === 'true', talentSelected: tab('达人主体信息')[0]?.getAttribute('aria-selected') === 'true',
    entry: buttons.length, drawers: drawers.length };
})()`;
function clickExpression(action) {
  const isTab = action === 'main' || action === 'talent';
  const label = action === 'main' ? '主体信息' : '达人主体信息';
  return `(() => {
    const u = new URL(location.href), visible = e => e.getClientRects().length > 0 && getComputedStyle(e).visibility !== 'hidden';
    if (window !== window.top || u.origin !== 'https://s.kwaixiaodian.com' || u.pathname !== ${JSON.stringify(isTab ? '/zone/shop/info/qualification' : '/zone/short-video-b/slice')}) return 'WRONG_ROUTE';
    if ([...document.querySelectorAll('[role="dialog"],.ant-modal,.kwaishop-tianhe-shortVideoB-pc-modal')].some(visible)) return 'UNKNOWN_DIALOG';
    const candidates = ${isTab ? `[...document.querySelectorAll('[role="tab"]')].filter(e => visible(e) && e.textContent.trim() === ${JSON.stringify(label)})` : `[...document.querySelectorAll('.js-page-content button')].filter(e => visible(e) && ['修改设置','去设置'].includes(e.textContent.trim()) && !e.disabled)`};
    if (candidates.length !== 1 || candidates[0].closest('form') || candidates[0].matches('input,select,textarea,[role="checkbox"],[role="switch"]')) return 'UNSAFE_OR_AMBIGUOUS';
    const element = candidates[0];
    if (${isTab} && element.getAttribute('aria-selected') === 'true') return 'ALREADY_SELECTED';
    element.click(); return 'DISPATCHED';
  })()`;
}
function html(url) {
  const base = '<!doctype html><meta charset="utf-8"><link rel="icon" href="data:,"><title>Owned synthetic diagnostic</title>';
  if (url === qualification) return base + `<div role="tab" aria-selected="true" onclick="document.querySelectorAll('[role=tab]').forEach(e=>e.setAttribute('aria-selected','false'));this.setAttribute('aria-selected','true')">主体信息</div><div role="tab" aria-selected="false" onclick="document.querySelectorAll('[role=tab]').forEach(e=>e.setAttribute('aria-selected','false'));this.setAttribute('aria-selected','true')">达人主体信息</div><div class="ant-tabs-tabpane-active">Synthetic; no documents</div>`;
  if (url === slice) return base + '<div class="js-page-content"><button type="button" onclick="document.querySelector(\'.kwaishop-tianhe-shortVideoB-pc-drawer-content\').style.display=\'block\'">修改设置</button></div><div class="kwaishop-tianhe-shortVideoB-pc-drawer-content" style="display:none"><span class="kwaishop-tianhe-shortVideoB-pc-drawer-title">直播切片托管设置</span><p>No permission controls exist in this synthetic fixture.</p></div>';
  return base + '<main>Synthetic independent-origin health control</main>';
}

const root = await mkdtemp(join(tmpdir(), 'chromix-root-cause-2026-10-02-'));
const marker = randomUUID();
await writeFile(join(root, 'owner'), marker, { flag: 'wx' });
const profile = join(root, 'profile');
let child, cdp, lines, childExited, anchor, lastTarget, audit, verified = false, closing = false;
let completed = 0, totalCrashes = 0, noReplyTargets = 0, notConnectedLines = 0;
let verdict = 'INCONCLUSIVE_HARNESS';
const sockets = new Set();
const sink = createServer(socket => { sockets.add(socket); socket.on('error', () => {}); socket.on('close', () => sockets.delete(socket)); socket.end('HTTP/1.1 502 Bad Gateway\r\nContent-Length: 0\r\nConnection: close\r\n\r\n'); });
await new Promise(resolve => sink.listen(0, '127.0.0.1', resolve));
const reserve = createServer(); await new Promise(resolve => reserve.listen(0, '127.0.0.1', resolve));
const port = reserve.address().port; await new Promise(resolve => reserve.close(resolve));
const endpoint = `http://127.0.0.1:${port}`;
async function target(role, background = true) {
  const { targetId } = await cdp.send('Target.createTarget', { url: 'about:blank', background });
  cdp.targets.set(targetId, role);
  const { sessionId } = await cdp.send('Target.attachToTarget', { targetId, flatten: true });
  cdp.sessions.set(sessionId, role);
  const result = { targetId, sessionId, role }; lastTarget = result;
  for (const method of ['Page.enable', 'Runtime.enable', 'Inspector.enable']) await cdp.send(method, {}, sessionId);
  if (mode !== 'live') await cdp.send('Fetch.enable', { patterns: [{ urlPattern: '*', requestStage: 'Request' }] }, sessionId);
  return result;
}
async function renderers() {
  const r = await cdp.send('SystemInfo.getProcessInfo');
  return r.processInfo.filter(p => p.type === 'renderer').map(p => p.id).sort((a,b) => a-b);
}
async function waitSnapshot(t, predicate, ms = 12000) {
  const end = Date.now() + ms; let last;
  while (Date.now() < end) {
    if (cdp.crashes.some(c => c.role === t.role)) throw fail('CRASH_EVENT_OBSERVED');
    try { last = await cdp.value(t, snapshot); }
    catch (e) { if (e.safeCode !== 'CONTEXT_TRANSITION') throw e; }
    if (last && predicate(last)) return last;
    if (last?.route === 'login' && mode === 'live') throw fail('LOGIN_REQUIRED');
    await sleep(100);
  }
  log('readiness-limit', { role: t.role, snapshot: last ?? null });
  throw fail('PAGE_READINESS_UNPROVEN');
}
async function navigate(t, url, route) {
  const before = await renderers(), started = Date.now();
  const old = await cdp.value(t, 'performance.timeOrigin');
  const r = await cdp.send('Page.navigate', { url }, t.sessionId, 10000);
  if (r.errorText) throw fail('NAVIGATION_ERROR');
  const s = await waitSnapshot(t, s => s.epoch !== old && s.route === route && s.ready !== 'loading');
  const after = await renderers();
  log('navigation', { role: t.role, route, elapsedMs: Date.now() - started, rendererAdded: after.filter(p => !before.includes(p)), rendererRemoved: before.filter(p => !after.includes(p)), snapshot: s });
}
async function health(t) {
  try { return await cdp.value(t, '1', 2000) === 1 ? 'RESPONSIVE' : 'INVALID_RESPONSE'; }
  catch (error) { return error.safeCode ?? 'PROBE_FAILED'; }
}
async function failureEvidence(t) {
  let present = null;
  try { present = (await cdp.send('Target.getTargets')).targetInfos.some(i => i.targetId === t.targetId); } catch {}
  const probes = [];
  for (let i = 0; i < 3; i++) probes.push(await health(t));
  const anchorHealth = anchor && anchor !== t ? await health(anchor) : null;
  const crashEvents = cdp.crashes.filter(c => c.role === t.role);
  if (probes.every(p => p === 'NO_REPLY')) noReplyTargets++;
  log('failure-evidence', { role: t.role, stillInInventory: present, probes, anchorHealth, crashEvents });
  return { present, probes, anchorHealth, crashEvents };
}
async function closeOwned(t) {
  const r = await cdp.send('Target.closeTarget', { targetId: t.targetId });
  if (!r.success) throw fail('OWNED_CLOSE_REFUSED');
  const end = Date.now() + 3000;
  while (Date.now() < end) {
    if (!(await cdp.send('Target.getTargets')).targetInfos.some(i => i.targetId === t.targetId)) {
      log('owned-target-cleaned', { role: t.role }); return;
    }
    await sleep(50);
  }
  throw fail('OWNED_TARGET_REMAINS');
}
try {
  const args = ['--window-size=1100,850', '--disable-extensions', '--fingerprint=123456789', '--fingerprint-platform=windows'];
  if (mode !== 'live') args.push('--disable-quic', '--host-resolver-rules=MAP * ~NOTFOUND, EXCLUDE 127.0.0.1', `--proxy-server=http://127.0.0.1:${sink.address().port}`, '--proxy-bypass-list=<-loopback>');
  const options = { headless: false, geoip: false, extensionPaths: [], args };
  if (cohort === 'without-disable-breakpad') options.launchOptions = { ignoreDefaultArgs: ['--enable-automation', '--disable-breakpad'] };
  if (cohort === 'sandbox') options.launchOptions = { chromiumSandbox: true };
  if (cohort === 'without-disable-dev-shm-usage') options.launchOptions = { ignoreDefaultArgs: ['--enable-automation', '--disable-dev-shm-usage'] };
  const env = { ...process.env, CHROMIX_DIAG_PROFILE: profile };
  for (const key of Object.keys(env)) if (/^(HTTP_PROXY|HTTPS_PROXY|ALL_PROXY|NO_PROXY|NODE_OPTIONS|NODE_USE_ENV_PROXY|CLOAKBROWSER_.+|CHROMIX_.+)$/i.test(key) && key !== 'CHROMIX_DIAG_PROFILE') delete env[key];
  const childArgs = [self, '--bridge-child'];
  if (option('binary')) childArgs.push(`--binary=${option('binary')}`);
  child = spawn(process.execPath, childArgs, { stdio: ['pipe', 'pipe', 'pipe'], env });
  child.stdin.on('error', () => {});
  childExited = new Promise(resolve => { child.once('exit', code => { log('owned-bridge-exit', { code, duringCleanup: closing }); resolve(); }); child.once('error', () => resolve()); });
  let stderr = '';
  child.stderr.on('data', chunk => {
    stderr += String(chunk);
    const rows = stderr.split(/\r?\n/); stderr = rows.pop().slice(-1024);
    for (const row of rows) if (/crashpad_client_win\.cc:\d+.*not connected|Crashpad_NotConnectedToHandler/.test(row)) { notConnectedLines++; log('crash-reporting-message', { count: notConnectedLines }); }
  });
  lines = createInterface({ input: child.stdout });
  const ready = new Promise((resolve, reject) => {
    lines.on('line', line => {
      let m; try { m = JSON.parse(line); } catch { reject(fail('BRIDGE_PROTOCOL_ERROR')); return; }
      if (m.type === 'audit') { audit = m; log('launch-audit', { browserPid: m.browserPid, profileMatches: m.profileMatches, flags: m.flags, normalizedArgs: m.normalizedArgs }); }
      if (m.type === 'ready') resolve(m);
      if (m.type === 'error') reject(fail('BRIDGE_LAUNCH_ERROR'));
    });
    child.once('exit', () => reject(fail('BRIDGE_EXIT_BEFORE_READY')));
  });
  child.stdin.write(`${JSON.stringify({ type: 'launch', options, binaryPath: binary, skipDownload: true, cdpPort: port, userDataDir: profile, proxy: null, extensionPaths: [], startUrl: null })}\n`);
  const response = await bounded(ready, 45000, 'BRIDGE_READY_TIMEOUT');
  if (response.cdpEndpoint !== endpoint || !audit?.profileMatches) throw fail('OWNERSHIP_UNVERIFIED');
  const info = await (await fetch(`${endpoint}/json/version`, { signal: AbortSignal.timeout(3000) })).json();
  if (info.Browser !== expectedVersion) throw fail('WRONG_BROWSER_VERSION');
  const wsUrl = new URL(info.webSocketDebuggerUrl);
  if (wsUrl.hostname !== '127.0.0.1' || wsUrl.port !== String(port)) throw fail('ENDPOINT_UNOWNED');
  const socket = new WebSocket(wsUrl);
  await bounded(new Promise((resolve, reject) => { socket.addEventListener('open', resolve, { once: true }); socket.addEventListener('error', () => reject(fail('WS_FAILED')), { once: true }); }), 4000, 'WS_TIMEOUT');
  cdp = new Cdp(socket);
  const processes = await cdp.send('SystemInfo.getProcessInfo');
  if (!processes.processInfo.some(p => p.type === 'browser' && p.id === audit.browserPid)) throw fail('BROWSER_PID_MISMATCH');
  verified = true; log('ownership-verified', { browser: info.Browser, protocol: info['Protocol-Version'] });
  await cdp.send('Target.setDiscoverTargets', { discover: true });
  cdp.onEvent = async m => {
    if (m.method !== 'Fetch.requestPaused' || mode === 'live') return;
    const { requestId, request } = m.params;
    if (request.method === 'GET' && (request.url === qualification || request.url === slice || /^https:\/\/owned-\d+\.invalid\/$/.test(request.url))) {
      await cdp.send('Fetch.fulfillRequest', { requestId, responseCode: 200, responseHeaders: [{ name: 'Content-Type', value: 'text/html; charset=utf-8' }, { name: 'Cache-Control', value: 'no-store' }], body: Buffer.from(html(request.url)).toString('base64') }, m.sessionId);
    } else await cdp.send('Fetch.failRequest', { requestId, errorReason: 'BlockedByClient' }, m.sessionId);
  };
  anchor = await target('anchor', false);
  if (mode !== 'live') await navigate(anchor, 'https://owned-0.invalid/', 'synthetic-anchor');
  if (mode === 'live' && loginSeconds) {
    log('manual-login-window', { seconds: loginSeconds, disposableProfile: true });
    await cdp.send('Page.navigate', { url: login }, anchor.sessionId, 10000);
    await cdp.send('Target.activateTarget', { targetId: anchor.targetId });
    const until = Date.now() + loginSeconds * 1000; let loggedIn = false;
    while (Date.now() < until) {
      try { const s = await cdp.value(anchor, snapshot); if (s.route === 'home' && s.ready !== 'loading') { loggedIn = true; break; } }
      catch (e) { if (e.safeCode !== 'CONTEXT_TRANSITION') throw e; }
      await sleep(500);
    }
    if (!loggedIn) throw fail('MANUAL_LOGIN_NOT_COMPLETED');
    log('manual-login-route-ready', { identityCollected: false });
  }
  if (mode === 'detector-control') {
    const t = await target('intentional-synthetic-crash');
    await navigate(t, qualification, 'qualification');
    log('intentional-fault', { method: 'Page.crash', targetIsSyntheticAndOwned: true });
    await cdp.send('Page.crash', {}, t.sessionId, 3000).catch(e => log('intentional-fault-command', { code: e.safeCode }));
    const evidence = await failureEvidence(t);
    verdict = evidence.crashEvents.length && evidence.present && evidence.probes.every(p => p === 'NO_REPLY') && evidence.anchorHealth === 'RESPONSIVE' ? 'DETECTOR_CAUGHT_CRASH_WITH_RETAINED_TARGET' : 'DETECTOR_CONTROL_INCOMPLETE';
  } else {
    for (let i = 1; i <= cycles; i++) {
      const t = await target(`cycle-${i}`);
      log('cycle-start', { cycle: i });
      if (mode === 'synthetic') await navigate(t, `https://owned-${i}.invalid/`, 'synthetic-anchor');
      await navigate(t, qualification, 'qualification');
      await waitSnapshot(t, s => s.main === 1 && s.talent === 1);
      for (const action of ['main', 'talent']) {
        const result = await cdp.value(t, clickExpression(action));
        log('readonly-ui-action', { role: t.role, action, result });
        if (!['DISPATCHED', 'ALREADY_SELECTED'].includes(result)) throw fail('ACTION_NOT_ADMITTED');
        await waitSnapshot(t, s => action === 'main' ? s.mainSelected : s.talentSelected);
      }
      await navigate(t, slice, 'slice');
      await waitSnapshot(t, s => s.entry === 1);
      const result = await cdp.value(t, clickExpression('open'));
      log('readonly-ui-action', { role: t.role, action: 'open-settings', result });
      if (result !== 'DISPATCHED') throw fail('ACTION_NOT_ADMITTED');
      await waitSnapshot(t, s => s.drawers === 1);
      const probe = await health(t), anchorHealth = await health(anchor);
      log('cycle-complete', { cycle: i, probe, anchorHealth, permissionsTouched: false });
      if (probe !== 'RESPONSIVE' || anchorHealth !== 'RESPONSIVE') throw fail('HEALTH_FAILED');
      completed++;
      await closeOwned(t);
    }
    verdict = mode === 'synthetic' ? 'NOT_REPRODUCED_SYNTHETIC_ONLY' : 'NOT_REPRODUCED_LIVE_READONLY_SEQUENCE';
  }
} catch (error) {
  verdict = error.safeCode ?? 'HARNESS_FAILED';
  log('failure', { code: verdict });
  if (cdp && lastTarget) await failureEvidence(lastTarget).catch(() => log('failure-evidence-unavailable'));
} finally {
  totalCrashes = new Set(cdp?.crashes.map(c => c.role) ?? []).size;
  log('result', { verdict, completedCycles: completed, requestedCycles: cycles, targetsWithCrashEvent: totalCrashes, targetsWithThreeNoReplies: noReplyTargets, crashReportingLines: notConnectedLines });
  closing = true;
  if (child?.stdin?.writable) child.stdin.end('{"type":"close"}\n');
  if (cdp) { if (verified) await cdp.send('Browser.close', {}, undefined, 2000).catch(() => {}); cdp.socket.close(); }
  if (child && child.exitCode === null) {
    try { await bounded(childExited, 12000, 'OWNED_EXIT_TIMEOUT'); }
    catch {
      log('cleanup-owned-process-tree', { bridgePid: child.pid });
      const killer = spawn('taskkill', ['/PID', String(child.pid), '/T', '/F'], { stdio: 'ignore' });
      await bounded(new Promise(resolve => { killer.once('exit', resolve); killer.once('error', resolve); }), 5000, 'OWNED_KILL_TIMEOUT').catch(() => {});
    }
  }
  lines?.close();
  for (const socket of sockets) socket.destroy();
  await new Promise(resolve => sink.close(resolve));
  try {
    if (await readFile(join(root, 'owner'), 'utf8') !== marker) throw fail('CLEANUP_OWNERSHIP_CHANGED');
    await rm(root, { recursive: true, force: true, maxRetries: 30, retryDelay: 100 });
    log('cleanup', { disposableProfileRemoved: true });
  } catch { log('cleanup', { disposableProfileRemoved: false, ownedRoot: root }); }
  output.end(); await finished(output);
}
process.exitCode = ['NOT_REPRODUCED_SYNTHETIC_ONLY', 'NOT_REPRODUCED_LIVE_READONLY_SEQUENCE', 'DETECTOR_CAUGHT_CRASH_WITH_RETAINED_TARGET'].includes(verdict) ? 0 : ['LOGIN_REQUIRED', 'MANUAL_LOGIN_NOT_COMPLETED', 'PAGE_READINESS_UNPROVEN'].includes(verdict) ? 2 : 1;
