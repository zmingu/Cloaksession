// One live read-only diagnostic tab, on the already verified Chromix process.
// No platform mutations, document-value output, screenshots or raw protocol dumps.
// Retain a failed target for inspection; close only the successful target created here.
const base='http://127.0.0.1:42892';
const version=await(await fetch(base+'/json/version',{signal:AbortSignal.timeout(3000)})).json();
if(version.Browser!=='Chrome/151.0.7922.173')throw Error('unexpected-version');
const socket=new WebSocket(version.webSocketDebuggerUrl);
await new Promise((resolve,reject)=>{socket.onopen=resolve;socket.onerror=()=>reject(Error('connect-failed'));});
let next=0,target,session,crashed=false,success=false;
const requests=new Map();
socket.onmessage=({data})=>{const m=JSON.parse(String(data));if(m.id){const r=requests.get(m.id);if(r){clearTimeout(r.timer);requests.delete(m.id);m.error?r.reject(Error('cdp-error')):r.resolve(m.result);}return;}if((m.method==='Target.targetCrashed'&&m.params?.targetId===target)||(m.method==='Inspector.targetCrashed'&&m.sessionId===session)){crashed=true;console.log(JSON.stringify({event:'target-crashed',status:m.params?.status??null,errorCode:m.params?.errorCode??null}));}};
function send(method,params={},sid){const id=++next;return new Promise((resolve,reject)=>{const timer=setTimeout(()=>{requests.delete(id);reject(Error('cdp-timeout'));},3000);requests.set(id,{resolve,reject,timer});socket.send(JSON.stringify({id,method,params,...(sid?{sessionId:sid}:{})}));});}
try{
  await send('Target.setDiscoverTargets',{discover:true});
  target=(await send('Target.createTarget',{url:'about:blank',background:true})).targetId;
  session=(await send('Target.attachToTarget',{targetId:target,flatten:true})).sessionId;
  await send('Page.enable',{},session);await send('Runtime.enable',{},session);await send('Inspector.enable',{},session);
  console.log(JSON.stringify({phase:'owned-diagnostic-created'}));
  const nav=await send('Page.navigate',{url:'https://s.kwaixiaodian.com/zone/short-video-b/slice'},session);
  if(nav.errorText)throw Error('navigation-error');
  const until=Date.now()+20000;
  while(Date.now()<until&&!crashed){
    let r;try{r=await send('Runtime.evaluate',{expression:'({origin:location.origin,path:location.pathname,ready:document.readyState,entry:!!Array.from(document.querySelectorAll(".js-page-content button")).find(e=>["去设置","修改设置"].includes(e.textContent.trim())&&e.getClientRects().length),error:!!document.body?.innerText.includes("Crashpad_NotConnectedToHandler")})',returnByValue:true},session);}catch(error){if(crashed)break;throw error;}
    const value=r.result?.value;
    if(value?.error){crashed=true;console.log(JSON.stringify({event:'visible-crashpad-code'}));break;}
    if(value?.origin==='https://s.kwaixiaodian.com'&&value.path==='/zone/short-video-b/slice'&&value.entry){success=true;break;}
    await new Promise(resolve=>setTimeout(resolve,150));
  }
  console.log(JSON.stringify({phase:'live-slice-load',entryReady:success,crashed}));
  if(!success)process.exitCode=1;
}catch(error){console.log(JSON.stringify({phase:'live-slice-load',error:['cdp-error','cdp-timeout','navigation-error'].includes(error.message)?error.message:'probe-failed',crashed}));process.exitCode=1;}
finally{
  if(success&&target)await send('Target.closeTarget',{targetId:target}).catch(()=>{});
  console.log(JSON.stringify({diagnosticTarget:target??null,retained:!success&&!!target}));
  for(const r of requests.values()){clearTimeout(r.timer);r.reject(Error('probe-closed'));}requests.clear();socket.close();
}
