// Passive, bounded crash-event capture for one application-owned Chromix launch.
// Usage: node chromix-crash-capture.mjs --app-pid=<current tauri-app PID>
// No navigation, clicks, profile reads, crash dumps, account values or raw events.
import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
const exec = promisify(execFile);
const appPid = Number(process.argv.find(a => a.startsWith('--app-pid='))?.split('=')[1]);
if (!Number.isSafeInteger(appPid) || appPid <= 0 || process.platform !== 'win32') throw Error('valid-app-pid-required');
const emit = (phase, detail = {}) => console.log(JSON.stringify({time: new Date().toISOString(), phase, ...detail}));
const pause = ms => new Promise(resolve => setTimeout(resolve, ms));
const query = `$all=Get-CimInstance Win32_Process; $app=$all|Where-Object{$_.ProcessId -eq ${appPid} -and $_.ExecutablePath -eq 'F:\\Cloaksession\\target\\debug\\tauri-app.exe'}; $bridges=@($all|Where-Object{$_.ParentProcessId -eq ${appPid} -and $_.Name -eq 'node.exe'}|ForEach-Object{$_.ProcessId}); $rows=@($all|Where-Object{$bridges -contains $_.ParentProcessId -and $_.ExecutablePath -eq 'C:\\Users\\Administrator\\.cache\\chromix\\v151.0.7922.173\\win-x64\\chromix\\chrome.exe' -and $_.CommandLine -notmatch '--type='}|ForEach-Object{$m=[regex]::Match($_.CommandLine,'--remote-debugging-port=(\\d+)');if($m.Success){@{pid=[int]$_.ProcessId;port=[int]$m.Groups[1].Value}}}); @{appAlive=[bool]$app;browsers=$rows}|ConvertTo-Json -Compress`;
const inspect = async () => JSON.parse((await exec('pwsh', ['-NoProfile','-NonInteractive','-Command',query], {timeout:10000,windowsHide:true})).stdout);
function role(url) { try { const u = new URL(url); if(u.origin !== 'https://s.kwaixiaodian.com') return 'other'; return new Map([['/zone/home','home'],['/zone/shop/info/qualification','qualification'],['/zone/short-video-b/slice','slice']]).get(u.pathname) ?? 'shop-other'; } catch { return 'other'; } }
let socket;
try {
  emit('waiting-for-app-owned-chromix', {maximumSeconds:120});
  const deadline=Date.now()+120000;
  let browser;
  while(Date.now()<deadline) {
    const state=await inspect();
    if(!state.appAlive){emit('application-ended-before-browser');process.exit(0);}
    if(state.browsers.length){browser=state.browsers[0];break;}
    await pause(1500);
  }
  if(!browser){emit('browser-not-opened-within-window');process.exit(0);}
  const endpoint=`http://127.0.0.1:${browser.port}`;
  const version=await(await fetch(endpoint+'/json/version',{signal:AbortSignal.timeout(5000)})).json();
  if(version.Browser!=='Chrome/151.0.7922.173')throw Error('browser-version-changed');
  const ws=new URL(version.webSocketDebuggerUrl);
  if(ws.hostname!=='127.0.0.1'||Number(ws.port)!==browser.port)throw Error('endpoint-mismatch');
  socket=new WebSocket(ws);
  const pending=new Map();const roles=new Map();let next=0,crashes=0;
  const send=(method,params={})=>new Promise((resolve,reject)=>{const id=++next;const timer=setTimeout(()=>{pending.delete(id);reject(Error('command-timeout'));},5000);pending.set(id,{resolve,reject,timer});socket.send(JSON.stringify({id,method,params}));});
  socket.addEventListener('message',({data})=>{
    let m;try{m=JSON.parse(String(data));}catch{return;}
    if(m.id){const p=pending.get(m.id);if(p){clearTimeout(p.timer);pending.delete(m.id);m.error?p.reject(Error('command-rejected')):p.resolve(m.result);}return;}
    if(m.params?.targetInfo){const t=m.params.targetInfo;if(t.type==='page'){roles.set(t.targetId,role(t.url));emit('page-state',{role:role(t.url),event:m.method==='Target.targetCreated'?'created':'changed'});}}
    if(m.method==='Target.targetCrashed'){crashes++;emit('page-crashed',{role:roles.get(m.params.targetId)??'unknown',status:['crashed','killed','abnormal'].includes(m.params.status)?m.params.status:'other',errorCode:Number.isInteger(m.params.errorCode)?m.params.errorCode:null});}
    if(m.method==='Target.targetDestroyed'){emit('page-destroyed',{role:roles.get(m.params.targetId)??'unknown'});roles.delete(m.params.targetId);}
  });
  await new Promise((resolve,reject)=>{const timer=setTimeout(()=>reject(Error('connect-timeout')),5000);socket.addEventListener('open',()=>{clearTimeout(timer);resolve();},{once:true});socket.addEventListener('error',()=>{clearTimeout(timer);reject(Error('connect-failed'));},{once:true});});
  const info=await send('SystemInfo.getProcessInfo');
  if(!info.processInfo.some(p=>p.type==='browser'&&p.id===browser.pid))throw Error('browser-ownership-mismatch');
  await send('Target.setDiscoverTargets',{discover:true});
  emit('capture-ready',{browser:version.Browser,maximumSeconds:480});
  const reason=await new Promise(resolve=>{const timer=setTimeout(()=>resolve('capture-window-ended'),480000);socket.addEventListener('close',()=>{clearTimeout(timer);resolve('browser-disconnected');},{once:true});});
  emit(reason,{observedCrashes:crashes});
} catch(error) {
  const allowed=['valid-app-pid-required','browser-version-changed','endpoint-mismatch','command-timeout','command-rejected','connect-timeout','connect-failed','browser-ownership-mismatch'];
  emit('capture-failed',{reason:allowed.includes(error.message)?error.message:'local-capture-unavailable'});process.exitCode=1;
} finally { socket?.close(); }
