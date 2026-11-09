// Node-only state/markup checks. No browser, local installation or credential is used.
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import vm from 'node:vm';

const html=readFileSync(new URL('../src/resources/web/index.html',import.meta.url),'utf8');
const source=html.match(/<script>([\s\S]*?)<\/script>/)[1];
new vm.Script(source); // Check the entire embedded script, including startup.
const listeners=new Map(),requests=[];
const node={hidden:true,dataset:{},classList:{add(){},remove(){},toggle(){}},setAttribute(){},removeAttribute(){},focus(){},addEventListener(){},querySelector(){return node},querySelectorAll(){return []}};
const apps=[
 {assistant:'opencode',name:'OpenCode',detected:true,configured:false,reload:'automatic'},
 {assistant:'cursor',name:'Cursor',detected:true,configured:false,reload:'restart'},
 {assistant:'grok-build',name:'Grok Build',detected:true,configured:false,reload:'restart'},
 {assistant:'omp',name:'Oh My Pi',detected:false,configured:false,reload:'mcp'},
];
let scanError=false,setupError=false,releaseSetup=null;
const context=vm.createContext({
 document:{documentElement:{},body:node,getElementById:()=>node,querySelector:()=>null,querySelectorAll:()=>[],addEventListener:(type,fn)=>{if(!listeners.has(type))listeners.set(type,[]);listeners.get(type).push(fn)}},
 window:{addEventListener(){}},location:{hash:'#fixture',pathname:'/'},history:{replaceState(){}},
 sessionStorage:{getItem(){return null},setItem(){}},localStorage:{getItem(){return 'fr'},setItem(){}},navigator:{language:'fr'},
 fetch:async(path,options)=>{
  const body=options.body?JSON.parse(options.body):null;requests.push({path,body});
  if(path==='/assistants')return {ok:!scanError,json:async()=>scanError?{error:'Fixture scan failed'}:{assistants:structuredClone(apps)}};
  assert.equal(path,'/action');assert.equal(body.action,'assistant_setup');
  if(releaseSetup)await new Promise(resolve=>{releaseSetup=resolve});
  const app=apps.find(app=>app.assistant===body.assistant);
  return {ok:!setupError,json:async()=>setupError?{error:'Fixture config refused'}:{...app,configured:true}};
 },
});
vm.runInContext(source.slice(0,source.lastIndexOf('refresh();const timer='))+`
 render=()=>{};
 globalThis.fixture={state,localAssistants,assistantJourney,openAssistant,assistantScreen,scanAssistants,configureAssistant};
`,context);
const ui=context.fixture;
async function click(attribute,value=''){
 const key=attribute.replace(/-([a-z])/g,(_,c)=>c.toUpperCase());
 const target={id:'',dataset:{[key]:value},closest(selector){return selector.split(',').some(s=>s.trim()===`[data-${attribute}]`)?this:null}};
 for(const handler of listeners.get('click'))await handler({target});
}

await ui.openAssistant('grok');
assert.equal(requests.length,0,'Opening a web guide must not scan or write');
assert.match(ui.assistantScreen(),/data-assistant-mode="web" aria-pressed="true"/);
assert.doesNotMatch(ui.assistantScreen(),/data-local-assistant|data-assistant-setup/);
ui.assistantJourney.step=1;
await click('assistant-mode','local');
assert.deepEqual(requests,[{path:'/assistants',body:null}]);
assert.equal(ui.assistantJourney.step,1,'Local setup must preserve the web step');
let rendered=ui.assistantScreen();
assert.doesNotMatch(rendered,/assistant-workspace|guide-shot|t3code/i);
assert.match(rendered,/data-local-assistant="cursor"/);
assert.match(rendered,/data-local-assistant="grok-build"/);
assert.doesNotMatch(rendered,/data-local-assistant="omp"/,'An undetected app cannot be configured');
assert.match(rendered,/Autres applications \(1\)/);
await click('local-assistant','cursor');
assert.equal(requests.length,1,'Selecting an app must not write before confirmation');
assert.match(ui.assistantScreen(),/data-assistant-confirm="cursor"/);
await click('assistant-cancel');
assert.equal(requests.length,1);
await click('local-assistant','opencode');
releaseSetup=true;
const setup=ui.configureAssistant('opencode');
await ui.configureAssistant('opencode');
assert.equal(requests.filter(r=>r.path==='/action').length,1,'Double-clicking must never replay setup');
assert.match(ui.assistantScreen(),/Connecting…|Connexion…/);
releaseSetup();releaseSetup=null;await setup;
assert.equal(ui.state.assistantStatus.configured,true);
assert.match(ui.assistantScreen(),/Les réglages s’appliquent automatiquement dans OpenCode/);
assert.equal(ui.localAssistants.items.find(app=>app.assistant==='cursor').configured,false);
await click('assistant-cancel');
setupError=true;
await click('local-assistant','grok-build');
await click('assistant-confirm','grok-build');
assert.equal(ui.state.assistantStatus.configured,false);
assert.equal(ui.state.notice.text,'Fixture config refused');
assert.match(ui.assistantScreen(),/data-assistant-confirm="grok-build"/,'A failed setup can be retried');
await click('assistant-mode','web');
assert.equal(ui.assistantJourney.step,1);
assert.match(ui.assistantScreen(),/assistant-workspace/);
scanError=true;await click('assistant-mode','local');await ui.scanAssistants();
assert.equal(ui.localAssistants.scanning,false);
assert.match(ui.assistantScreen(),/role="alert">Fixture scan failed/);
scanError=false;apps.forEach(app=>{app.detected=false});await ui.scanAssistants();
assert.match(ui.assistantScreen(),/Aucune application locale compatible/);
assert.doesNotMatch(ui.assistantScreen(),/data-local-assistant=/);
console.log('Local assistants passed: separate web/local views, read-only scan, selection/confirmation, duplicate-click protection, native setup status, errors, retry and empty state.');
