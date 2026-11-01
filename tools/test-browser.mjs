// Local bridge tests: no credentials or school requests.
import {readFileSync} from 'node:fs';
import vm from 'node:vm';
import test from 'node:test';
import assert from 'node:assert/strict';

const bridge=readFileSync(new URL('../src/resources/browser/omnivox.js',import.meta.url),'utf8');
const request=readFileSync(new URL('../src/resources/browser/request.js',import.meta.url),'utf8');
const seed={device:'a'.repeat(40),code:'',info:{}};
function page(host='college.omnivox.ca',store=new Map()) {
  const updates=[],callbacks=[];
  const context={URL,URLSearchParams,encodeURIComponent,location:{protocol:'https:',hostname:host,href:`https://${host}/Mobl/App`,hash:'#Token=example%2Btoken'},document:{cookie:''},
    localStorage:{getItem:k=>store.get(k),setItem:(k,v)=>store.set(k,v)},
    setTimeout:fn=>{fn();return 1},studentMobileState:raw=>updates.push(JSON.parse(raw)),
    Ovx:{ExecuteCallback:(name,value)=>callbacks.push({name,value})}};
  context.window=context;
  vm.runInNewContext(bridge.replace('__STUDENT_MOBILE_STATE__',JSON.stringify(seed)),context);
  return {context,updates,callbacks,store};
}
test('mobile code, device and storage survive page recreation',()=>{
  const first=page();
  first.context.OvxNatif.ExecuteCommand('Storage.SetCodeUserAgent',JSON.stringify({Code:'rotated'}));
  first.context.OvxNatif.ExecuteCommand('Storage.SetInfo',JSON.stringify({Key:'test',Value:'persisted'}));
  assert.equal(first.updates.at(-1).device,seed.device);
  assert.equal(first.updates.at(-1).code,'rotated');
  assert.match(first.context.userAgentRequete,/Code=rotated$/);
  const second=page('college.omnivox.ca',first.store);
  second.context.OvxNatif.ExecuteCommand('Storage.GetInfo',JSON.stringify({Key:'test'}));
  assert.equal(second.callbacks.at(-1).value.Value,'persisted');
  assert.match(second.context.document.cookie,/LoginTokenAuthentification=example%2Btoken/);
  second.context.OvxNatif.ExecuteCommand('WebUI.ResetSession','{}');
  assert.equal(second.updates.at(-1).code,'');
});
test('mobile bridge is not injected into a foreign login provider',()=>{
  assert.equal(page('login.example').context.OvxNatif,undefined);
  assert.equal(page('omnivox.ca.attacker.example').context.OvxNatif,undefined);
});
test('reads use the official mobile worker and refuse a changed origin',async()=>{
  let calls=0;
  const context={location:{origin:'https://college.omnivox.ca'},setTimeout,clearTimeout,
    Skytech:{Commun:{Utils:{HttpRequestWorker:{PostJSON:(url,body,ok)=>{calls++;assert.equal(body.AnSession,'');ok({ListeCours:[]})}}}}}};
  context.window=context;
  vm.createContext(context);vm.runInContext(request,context);
  const input={origin:context.location.origin,path:'/Mobl/LeaCommun/GetDefaultModel',body:{AnSession:''},mobile:true,limit:1024};
  const result=await context.browserRequest(input);
  assert.equal(result.status,200);assert.deepEqual(JSON.parse(result.text),{ListeCours:[]});
  context.location.origin='https://other.example';
  assert.equal((await context.browserRequest(input)).error,'reconnect');
  assert.equal(calls,1);
});
