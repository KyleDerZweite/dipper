import http from 'node:http';
import { readFile, writeFile, mkdtemp, rm } from 'node:fs/promises';
import { spawn } from 'node:child_process';
import assert from 'node:assert/strict';
const site = new URL('../site/', import.meta.url).pathname;
const sample = (id, task, wer, latency) => ({id,label:`${id} · ${task}`,task,dataset:'Public fixture',split:'test',model:'Fixture model',prompt_id:'fixture-v1',samples:4,wer,cer:wer,exact_match_rate:wer === null ? null : 1- Math.min(wer,1),latency_median_s:latency,reference_status:'synthetic fixture',limitations:['Browser fixture only. Not a benchmark result.'],configuration:{prompt:'Preserve the wording. <script>test</script>',threads:4},examples:[{input:'public input',reference:'public reference',output:'<img src=x onerror=alert(1)> public output'}]});
let mode='valid';
const fixture={schema_version:1,runs:[sample('Run A','asr',0.12,1.2),sample('Run B','cleanup',null,0.3),sample('Run C','chain',1.2,2.3)]};
const server=http.createServer(async(req,res)=>{const path = new URL(req.url,'http://local').pathname;if(path.endsWith('/results.json')){if(mode==='error'){res.writeHead(500);res.end('fixture error');return;} res.setHeader('Content-Type','application/json');res.end(JSON.stringify(mode==='empty'?{schema_version:1,runs:[]}:mode==='invalid'?{schema_version:99,runs:[]}:fixture));return;}const file=path.endsWith('/')?'index.html':path.split('/').pop();if(!['index.html','app.js','style.css'].includes(file)){res.writeHead(404);res.end();return;}res.setHeader('Content-Type',{'index.html':'text/html','app.js':'application/javascript','style.css':'text/css'}[file]);res.end(await readFile(site+file));});
await new Promise(r=>server.listen(0,'127.0.0.1',r));const port=server.address().port;
const profile=await mkdtemp('/tmp/dipper-site-chrome-');
const child=spawn(process.env.BENCHMARK_CHROMIUM || 'chromium',['--headless','--no-sandbox','--disable-gpu','--remote-debugging-port=0',`--user-data-dir=${profile}`,'about:blank'],{stdio:['ignore','ignore','pipe']});
let ws;
try {
const endpoint=await new Promise((resolve,reject)=>{let output='';child.stderr.on('data',chunk=>{output+=chunk;if(output.includes('DevTools listening on '))resolve(output.match(/DevTools listening on (ws:\/\/\S+)/)[1]);});setTimeout(()=>reject(new Error('Chromium did not start')),10000).unref();});
ws=new WebSocket(endpoint);await new Promise(r=>ws.addEventListener('open',r,{once:true}));let seq=0;const pending=new Map();const events=[];ws.addEventListener('message',e=>{const msg=JSON.parse(e.data);if(msg.id){const p=pending.get(msg.id);pending.delete(msg.id);msg.error?p.reject(Error(JSON.stringify(msg.error))):p.resolve(msg.result);}else events.push(msg);});
const send=(method,params={},sessionId)=>new Promise((resolve,reject)=>{const id=++seq;pending.set(id,{resolve,reject});ws.send(JSON.stringify({id,method,params,sessionId}));});
const {targetId}=await send('Target.createTarget',{url:'about:blank'});const {sessionId}=await send('Target.attachToTarget',{targetId,flatten:true});const cmd=(method,params)=>send(method,params,sessionId);
await cmd('Runtime.enable');await cmd('Page.enable');await cmd('Network.enable');
const evaluate=async(expression)=>{const r=await cmd('Runtime.evaluate',{expression,returnByValue:true,awaitPromise:true});if(r.exceptionDetails)throw Error(JSON.stringify(r.exceptionDetails));return r.result.value;};
const until=async(expression)=>{for(let i=0;i<100;i++){if(await evaluate(expression))return;await new Promise(r=>setTimeout(r,40));}throw Error('Timeout '+expression);};
const navigate=async()=>{await cmd('Page.navigate',{url:`http://127.0.0.1:${port}/dipper/`});await until("document.querySelector('#count') && /runs|unavailable/.test(document.querySelector('#count').textContent)");};
await cmd('Emulation.setDeviceMetricsOverride',{width:1440,height:1100,deviceScaleFactor:1,mobile:false});await navigate();
assert.equal(await evaluate("document.querySelectorAll('#rows tr').length"),3);
assert.equal(await evaluate("document.querySelector('#chart').children.length"),3);
assert.ok((await evaluate("document.querySelector('#rows').textContent")).includes('120.0%'));
assert.equal(await evaluate("document.documentElement.scrollWidth > innerWidth"),false);
await writeFile('/tmp/dipper-benchmark-desktop.png',Buffer.from((await cmd('Page.captureScreenshot',{format:'png',captureBeyondViewport:true})).data,'base64'));
await evaluate("document.querySelector('#task').value='cleanup'; document.querySelector('#task').dispatchEvent(new Event('change'))");assert.equal(await evaluate("document.querySelectorAll('#rows tr').length"),1);assert.equal(await evaluate("document.querySelector('.chart-value').textContent"),'N/A');
await evaluate("document.querySelector('#dataset').add(new Option('None','None'));document.querySelector('#dataset').value='None';document.querySelector('#dataset').dispatchEvent(new Event('change'))");assert.match(await evaluate("document.querySelector('#status').textContent"),/No runs match/);
await evaluate("document.querySelector('#reset').click(); document.querySelector('#rows button').focus()");await cmd('Input.dispatchKeyEvent',{type:'rawKeyDown',key:'Enter',code:'Enter',windowsVirtualKeyCode:13});await cmd('Input.dispatchKeyEvent',{type:'char',text:'\r',unmodifiedText:'\r',key:'Enter',code:'Enter',windowsVirtualKeyCode:13});await cmd('Input.dispatchKeyEvent',{type:'keyUp',key:'Enter',code:'Enter',windowsVirtualKeyCode:13});
assert.equal(await evaluate('document.activeElement.id'),'detail-title');assert.match(await evaluate("document.querySelector('#detail').textContent"),/synthetic fixture/);assert.match(await evaluate("document.querySelector('#detail pre').textContent"),/<script>test<\/script>/);assert.equal(await evaluate("document.querySelector('#detail img')"),null);
await evaluate("document.querySelector('#detail details').open=true;document.querySelector('#detail details:last-child').open=true");
for(const metric of ['cer','exact_match_rate','latency_median_s']) {await evaluate("document.querySelector('#metric').value='"+metric+"';document.querySelector('#metric').dispatchEvent(new Event('change'))");assert.equal(await evaluate("document.querySelectorAll('#rows tr').length"),3);}
await evaluate("document.querySelector('#metric').value='wer';document.querySelector('#metric').dispatchEvent(new Event('change'))");
for(const expected of ['light','system','dark']){await evaluate("document.querySelector('#theme').click()");assert.equal(await evaluate("document.querySelector('#theme').textContent"),'Theme: '+expected);}
await evaluate("document.querySelector('#theme').click()");await navigate();assert.equal(await evaluate("document.querySelector('#theme').textContent"),'Theme: light');
for(const width of [320,375,768,850,851,1440]){await cmd('Emulation.setDeviceMetricsOverride',{width,height:900,deviceScaleFactor:1,mobile:false});assert.equal(await evaluate('document.documentElement.scrollWidth > innerWidth'),false,`Overflow at ${width}`);}
await cmd('Emulation.setDeviceMetricsOverride',{width:375,height:900,deviceScaleFactor:1,mobile:false});await evaluate('scrollTo(0,0)');await writeFile('/tmp/dipper-benchmark-mobile.png',Buffer.from((await cmd('Page.captureScreenshot',{format:'png',captureBeyondViewport:true})).data,'base64'));
mode='empty';await navigate();assert.match(await evaluate("document.querySelector('#status').textContent"),/No public runs/);
mode='error';await navigate();assert.match(await evaluate("document.querySelector('#status').textContent"),/HTTP 500/);assert.equal(await evaluate("document.querySelector('#retry').hidden"),false);
mode='valid';await evaluate("document.querySelector('#retry').click()");await until("document.querySelectorAll('#rows tr').length===3");
mode='invalid';await navigate();assert.match(await evaluate("document.querySelector('#status').textContent"),/Unsupported results schema/);
assert.equal(events.filter(e=>e.method==='Runtime.exceptionThrown').length,0);
assert.equal(events.filter(e=>e.method==='Network.requestWillBeSent').some(e=>!e.params.request.url.startsWith(`http://127.0.0.1:${port}/`)),false);
console.log('PASS: rendered desktop/mobile; 320–1440px no page overflow; base path; filters/empty selection/reset; metric missing/>100%; keyboard run selection/focus; prompt escaping; theme cycle/persistence; empty/error/retry/invalid-schema; zero browser exceptions or external requests.');
} finally {if(ws)ws.close();child.kill();await new Promise(r=>child.once('exit',r));server.close();await rm(profile,{recursive:true,force:true});}
