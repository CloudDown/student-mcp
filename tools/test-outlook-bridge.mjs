// Synthetic Outlook panel; no school credentials or Microsoft requests.
import {readFileSync} from 'node:fs';
import {pathToFileURL} from 'node:url';
import assert from 'node:assert/strict';
const {chromium}=await import(process.env.PLAYWRIGHT_MODULE?pathToFileURL(process.env.PLAYWRIGHT_MODULE).href:'playwright');
const browser=await chromium.launch({headless:true,executablePath:process.env.STUDENT_MCP_BROWSER});
try {
  const page=await browser.newPage();
  let saved='';
  await page.route('https://outlook.cloud.microsoft/**',route=>route.fulfill({contentType:'text/html; charset=utf-8',body:`<button aria-label="Gestionnaire de compte" id="account">Compte</button><div id="menu" role="dialog" hidden>student@school.edu</div><h1>Transfert</h1><label for="toggle">Activer le transfert</label><button role="switch" id="toggle" aria-checked="${!!saved}"></button><div id="fields" ${saved?'':'hidden'}><label for="address">Transférer mon courrier à :</label><input id="address" value="${saved}"><label for="copy">Conserver une copie des messages transférés</label><input id="copy" type="checkbox" checked><button id="save">Enregistrer</button></div><script>account.onclick=()=>menu.hidden=false;toggle.onclick=()=>{toggle.setAttribute('aria-checked','true');fields.hidden=false};save.onclick=()=>fetch('/saved?to='+encodeURIComponent(address.value))</script>`}));
  await page.route('https://outlook.cloud.microsoft/saved?*',route=>{saved=new URL(route.request().url()).searchParams.get('to');return route.fulfill({body:'ok'});});
  await page.goto('https://outlook.cloud.microsoft/mail/options/mail/forwarding');
  await page.addScriptTag({content:readFileSync(new URL('../src/resources/browser/outlook.js',import.meta.url),'utf8')});
  const step=(action,email='student@school.edu',to='you@gmail.com')=>page.evaluate(([a,e,t])=>outlookForwarding(a,e,t),[action,email,to]);
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
  console.log('Outlook forwarding bridge fixture passed');
} finally {await browser.close()}
