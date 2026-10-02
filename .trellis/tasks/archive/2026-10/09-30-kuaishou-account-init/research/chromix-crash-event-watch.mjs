// Read-only, finite event observation of the already identified live Chromix browser.
// No page contents, account identifiers, raw argv or crash dumps are emitted.
const base='http://127.0.0.1:42892';
const target='608E11FDF718B37A6CA1D498886E11A5';
const version=await(await fetch(base+'/json/version',{signal:AbortSignal.timeout(3000)})).json();
if(version.Browser!=='Chrome/151.0.7922.173')throw Error('unexpected-version');
const pages=await(await fetch(base+'/json/list',{signal:AbortSignal.timeout(3000)})).json();
const chosen=pages.find(p=>p.id===target&&p.type==='page');
if(!chosen||new URL(chosen.url).pathname!=='/zone/short-video-b/slice')throw Error('target-changed');
const w=new WebSocket(version.webSocketDebuggerUrl);
await new Promise((resolve,reject)=>{w.onopen=resolve;w.onerror=()=>reject(Error('connection-failed'));});
let crashes=0;
w.onmessage=({data})=>{const m=JSON.parse(String(data));if(m.method==='Target.targetCrashed'&&m.params?.targetId===target){crashes++;console.log(JSON.stringify({event:'target-crashed',status:m.params.status,errorCode:m.params.errorCode}));}if(m.id===1)console.log(JSON.stringify({watching:!m.error}));};
w.send(JSON.stringify({id:1,method:'Target.setDiscoverTargets',params:{discover:true}}));
await new Promise(resolve=>{const timer=setTimeout(resolve,45000);w.addEventListener('close',()=>{clearTimeout(timer);resolve();},{once:true});});
w.close();console.log(JSON.stringify({observedCrashes:crashes}));
