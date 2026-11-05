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

function writePage(fetchImpl) {
  const calls=[],codes=[];
  const context={location:{origin:'https://college.omnivox.ca'},AbortSignal,TextDecoder,Uint8Array,
    userAgentRequete:'OVX InfoDevice=test AppVer=3.8.9 IdAppareil=device Code=code',
    Skytech:{Commun:{Utils:{
      LocalStorage:{GetData:(key,fallback)=>({'X-K':'site-generated-challenge','X-T':{LEA:'site-generated-access'}}[key] ?? fallback)},
      HttpRequestWorker:{PostJSON:()=>assert.fail('a write must not use a retry-capable worker')}
    }}},
    Ovx:{Storage:{SetCodeUserAgent:code=>codes.push(code)}},
    fetch:async(url,options)=>{calls.push({url,options});return await fetchImpl(url,options)}
  };
  context.window=context;
  vm.createContext(context);vm.runInContext(request,context);
  const input={origin:context.location.origin,path:'/Mobl/Mio/SendMessage',method:'POST',body:{To:'recipient',Message:'Hello'},mobile:true,mobileWrite:true,contentType:'application/json',binary:false,limit:1024};
  return {context,calls,codes,input};
}

test('writes use site-produced tokens and one redirect-refusing fetch, not the official retry wrapper',async()=>{
  const page=writePage(async()=>new Response('true',{headers:{'Content-Type':'application/json','X-AuthHash':'rotated-code'}}));
  const result=await page.context.browserRequest(page.input);
  assert.equal(result.status,200);assert.equal(result.text,'true');
  assert.equal(page.calls.length,1);
  const {url,options}=page.calls[0];
  assert.equal(url,'/Mobl/Mio/SendMessage');
  assert.equal(options.method,'POST');
  assert.equal(options.redirect,'manual');
  assert.equal(options.credentials,'same-origin');
  assert.equal(options.headers['X-K'],'site-generated-challenge');
  assert.equal(options.headers['X-T'],'LEA=site-generated-access');
  assert.match(options.headers['X-User-Agent-Ovx'],/^OVX /);
  assert.deepEqual(JSON.parse(options.body),page.input.body);
  assert.deepEqual(page.codes,['rotated-code']);
  assert.ok(!JSON.stringify(result).includes('site-generated-challenge'));
});

test('write challenge expiry, redirects, transport failure and response limits never trigger a second attempt',async()=>{
  for (const [makeResponse,error] of [
    [()=>new Response('true',{headers:{'X-KE':'expired-challenge'}}),'challenge'],
    [()=>({type:'opaqueredirect'}),'redirect'],
    [()=>{throw new Error('timeout')},'transport'],
    [()=>new Response('x'.repeat(1025)),'limit'],
  ]) {
    const page=writePage(async()=>makeResponse());
    const result=await page.context.browserRequest(page.input);
    assert.equal(result.error,error);
    assert.equal(page.calls.length,1);
  }
  const page=writePage(async()=>new Response('false',{status:403}));
  assert.equal((await page.context.browserRequest(page.input)).status,403);
  assert.equal(page.calls.length,1);
});

test('writes stop before sending if the origin changes or the official session storage is unavailable',async()=>{
  const page=writePage(async()=>assert.fail('must not make a request'));
  page.context.location.origin='https://other.example';
  assert.equal((await page.context.browserRequest(page.input)).error,'reconnect');
  page.context.location.origin=page.input.origin;
  delete page.context.Skytech.Commun.Utils.LocalStorage;
  assert.equal((await page.context.browserRequest(page.input)).error,'reconnect');
  assert.equal(page.calls.length,0);
});

test('Canvas reads retain Link pagination without following redirects',async()=>{
  const link='<https://school.example/api/v1/conversations?page=opaque%2Bnext>; rel="next"';
  const calls=[];
  const context={location:{origin:'https://school.example'},AbortSignal,TextDecoder,Uint8Array,
    localStorage:{getItem:()=>null},document:{cookie:'',querySelector:()=>null},
    fetch:async(path,options)=>{calls.push({path,options});return new Response('[]',{headers:{'lInK':link,'Content-Type':'application/json'}})}};
  context.window=context;
  vm.createContext(context);vm.runInContext(request,context);
  const result=await context.browserRequest({origin:context.location.origin,path:'/api/v1/conversations?page=1',method:'GET',limit:1024});
  assert.equal(result.link,link);
  assert.equal(result.text,'[]');
  assert.equal(calls.length,1);
  assert.equal(calls[0].options.redirect,'manual');
  assert.equal(calls[0].options.credentials,'same-origin');
  assert.equal(calls[0].options.method,'GET');
});
