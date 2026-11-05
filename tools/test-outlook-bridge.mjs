// Synthetic Outlook panel; no school credentials or Microsoft requests.
import {readFileSync} from 'node:fs';
import {pathToFileURL} from 'node:url';
import assert from 'node:assert/strict';
const {chromium}=await import(process.env.PLAYWRIGHT_MODULE?pathToFileURL(process.env.PLAYWRIGHT_MODULE).href:'playwright');
const browser=await chromium.launch({headless:true,executablePath:process.env.STUDENT_MCP_BROWSER});
try {
  const page=await browser.newPage();
  let saved='';
  let on=false;
  await page.route('https://outlook.cloud.microsoft/**',route=>route.fulfill({contentType:'text/html; charset=utf-8',body:`<!doctype html><button aria-label="Gestionnaire de compte" id="account">Compte</button><div id="menu" role="dialog" hidden>student@school.edu</div><h1>Transfert</h1><label for="toggle">Activer le transfert</label><button role="switch" id="toggle" aria-checked="${on?'true':'false'}"></button><div id="fields" ${on?'':'hidden'}><label for="address">Transférer mon courrier à :</label><input id="address" value="${saved}"><label for="copy">Conserver une copie des messages transférés</label><input id="copy" type="checkbox" checked></div><button id="save">Enregistrer</button><script>
account.onclick=()=>menu.hidden=false;
toggle.onclick=()=>{const next=toggle.getAttribute('aria-checked')!=='true';toggle.setAttribute('aria-checked',next?'true':'false');fields.hidden=!next};
save.onclick=()=>{const enabled=toggle.getAttribute('aria-checked')==='true';fetch('/saved?to='+encodeURIComponent(enabled?address.value:'')+'&on='+(enabled?'1':'0'))};
</script>`}));
  await page.route('https://outlook.cloud.microsoft/saved?*',route=>{
    const url=new URL(route.request().url());
    on=url.searchParams.get('on')==='1';
    saved=on?(url.searchParams.get('to')||''):'';
    return route.fulfill({body:'ok'});
  });
  await page.goto('https://outlook.cloud.microsoft/mail/options/mail/forwarding');
  await page.addScriptTag({content:readFileSync(new URL('../src/resources/browser/outlook.js',import.meta.url),'utf8')});
  const step=(action,email='student@school.edu',to='you@gmail.com',wantOn=true)=>page.evaluate(([a,e,t,o])=>outlookForwarding(a,e,t,o),[action,email,to,wantOn]);
  assert.equal((await step('account')).state,'opened');
  assert.equal((await step('identity','different@school.edu')).state,'waiting');
  assert.equal((await step('identity')).state,'verified');
  assert.equal((await step('read')).state,'ready');
  assert.equal((await step('write')).state,'fields');
  assert.equal((await step('write')).state,'submitted');
  await page.waitForResponse(r=>r.url().includes('/saved?'));
  await page.reload();
  await page.addScriptTag({content:readFileSync(new URL('../src/resources/browser/outlook.js',import.meta.url),'utf8')});
  assert.equal((await step('read')).state,'verified');
  assert.equal((await step('read','student@school.edu','wrong@gmail.com')).state,'ready');
  assert.equal((await step('read','student@school.edu','',false)).state,'ready');
  assert.equal((await step('write','student@school.edu','',false)).state,'submitted');
  await page.waitForResponse(r=>r.url().includes('/saved?'));
  await page.reload();
  await page.addScriptTag({content:readFileSync(new URL('../src/resources/browser/outlook.js',import.meta.url),'utf8')});
  assert.equal((await step('read','student@school.edu','',false)).state,'verified');
  console.log('Outlook forwarding bridge fixture passed');
} finally {await browser.close()}
