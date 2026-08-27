// What a mermaid shape actually LOOKS like — geometry AND computed style (bd-7ls21).
//
// ⚠️ THIS EXISTS BECAUSE A GEOMETRY-ONLY PROBE GOT TWO SHAPES WRONG, and the mistake was recorded in
// three test files as settled fact: `win-pane` and `datastore` were logged as "names mermaid 11.15.0
// publishes and draws as a PLAIN RECTANGLE … therefore never going to be implemented here".
//
//   datastore  really is a plain <rect> in the DOM. Its sides are erased by
//              stroke-dasharray="{width} {height}" — the dash draws the top edge, skips the right,
//              draws the bottom, skips the left. NOTHING in the geometry says so
//   win-pane   draws a box plus two interior rules as a <path>; a probe that reads the node's
//              first <rect> finds only the invisible label container and reports a plain box
//   text       is a <rect> sized to its label with fill:none and stroke-width:0px — a shape whose
//              entire definition is that nothing is painted
//
// So this dumps, per element: tag, class, stroke-dasharray, computed fill/stroke/stroke-width, and
// stroke-opacity. Run it BEFORE concluding that upstream renders something as a rectangle.
//
// Usage:  node scripts/headtohead/shape_style_probe.mjs win-pane datastore text rect
//
// Its companion for outlines that rough.js has made unreadable is the sampling approach described
// on bd-7ls21: getPointAtLength over the FIRST path, never a diff of the `d` string.
import fs from 'node:fs'; import os from 'node:os'; import path from 'node:path';
import { spawn } from 'node:child_process';
const PINS = JSON.parse(fs.readFileSync('/data/projects/frankenmermaid/scripts/headtohead/pins.json','utf8'));
const ROOT = path.join(os.homedir(),'snap','chromium','common');
async function launch(){const p=fs.mkdtempSync(path.join(ROOT,'fm-st-'));const proc=spawn(PINS.chromium.binary,['--headless=new','--remote-debugging-port=0',`--user-data-dir=${p}`,'--no-sandbox','--disable-gpu','--disable-dev-shm-usage','--no-first-run','--no-default-browser-check','--disable-extensions','--disable-background-networking','--disable-sync','--mute-audio','about:blank'],{stdio:['ignore','ignore','pipe']});let e='',port=null;proc.stderr.on('data',c=>{e+=String(c);const m=e.match(/DevTools listening on ws:\/\/127\.0\.0\.1:(\d+)/);if(m)port=Number(m[1]);});const d=Date.now()+30000;while(Date.now()<d){if(port){try{const r=await fetch(`http://127.0.0.1:${port}/json/version`);if(r.ok){const l=await(await fetch(`http://127.0.0.1:${port}/json/list`)).json();const pg=l.find(t=>t.type==='page');if(pg)return{proc,page:pg};}}catch{}}await new Promise(r=>setTimeout(r,120));}proc.kill('SIGKILL');throw new Error('noport');}
function attach(page){const ws=new WebSocket(page.webSocketDebuggerUrl);const pend=new Map();let id=0;ws.onmessage=ev=>{const m=JSON.parse(ev.data);if(m.id&&pend.has(m.id)){pend.get(m.id)(m);pend.delete(m.id);}};const send=(m,p)=>new Promise(r=>{const n=++id;pend.set(n,r);ws.send(JSON.stringify({id:n,method:m,params:p}));});const ready=new Promise((r,j)=>{ws.onopen=r;ws.onerror=j;});return{ws,send,ready};}
const shapes=process.argv.slice(2);
const {proc,page}=await launch(); const {ws,send,ready}=attach(page); await ready;
await send('Runtime.enable',{});
await send('Runtime.evaluate',{expression:fs.readFileSync('/home/ubuntu/.cache/fm-headtohead/mermaid-11.15.0.min.js','utf8')});
let i=0;
for(const sh of shapes){ i++;
 const src=`flowchart TD\n  A@{ shape: ${sh}, label: "Xy" }\n`;
 const expr=`(async()=>{const h=document.createElement('div');h.style.position='fixed';h.style.left='0';h.style.top='0';document.body.appendChild(h);
  try{mermaid.initialize({startOnLoad:false});const {svg}=await mermaid.render('st${i}',${JSON.stringify(src)});h.innerHTML=svg;
   const n=h.querySelector('g.node'); if(!n) return JSON.stringify({err:'no node'});
   const els=[...n.querySelectorAll('path,rect,circle,line,polygon')].map(el=>{const cs=getComputedStyle(el);
     return {t:el.tagName,cls:el.getAttribute('class'),dash:el.getAttribute('stroke-dasharray')||cs.strokeDasharray,
       fill:cs.fill,stroke:cs.stroke,sw:cs.strokeWidth,op:el.getAttribute('stroke-opacity')};});
   return JSON.stringify({els});}catch(e){return JSON.stringify({err:String(e&&e.message||e).slice(0,80)});}finally{h.remove();}})()`;
 const r=await send('Runtime.evaluate',{expression:expr,awaitPromise:true,returnByValue:true});
 const v=JSON.parse(r.result.result.value);
 console.log(`=== ${sh}`);
 if(v.err){console.log('  ERR',v.err);continue;}
 for(const el of v.els) console.log('  ',JSON.stringify(el));
}
ws.close(); proc.kill('SIGKILL');
