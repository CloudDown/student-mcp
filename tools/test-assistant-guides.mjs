// Fixture-only setup checks. No school, assistant account or real secret is used.
// PLAYWRIGHT_MODULE / STUDENT_MCP_BROWSER select existing development installations.
// STUDENT_MCP_GUIDE_SCREENS optionally saves screenshots for visual review.
import {readFileSync,mkdirSync} from 'node:fs';
import {pathToFileURL} from 'node:url';
import {join} from 'node:path';
import assert from 'node:assert/strict';
const loaded=await import(process.env.PLAYWRIGHT_MODULE?pathToFileURL(process.env.PLAYWRIGHT_MODULE).href:'playwright');
const chromium=loaded.chromium||loaded.default?.chromium;
const browser=await chromium.launch({headless:true,executablePath:process.env.STUDENT_MCP_BROWSER});
const shots=process.env.STUDENT_MCP_GUIDE_SCREENS;
if(shots)mkdirSync(shots,{recursive:true});
const html=readFileSync(new URL('../src/resources/web/index.html',import.meta.url),'utf8');
const assetNames={moodle:'moodle-icon',omnivox:'omnivox-icon',brightspace:'brightspace-icon',canvas:'canvas-icon',grok:'grok-icon',claude:'claude-icon',chatgpt:'chatgpt-icon',gemini:'gemini-icon','app-icon':'app-icon-64'};
const fixture={accounts:[{id:'fixture',service:'moodle',url:'https://school.example',enabled:true}],tunnel:'Ready(fixture)',local:true,public:true,autostart:false,link:'https://student.example/mcp/EXAMPLE-NOT-A-REAL-SECRET',local_link:'http://127.0.0.1:3000/mcp/EXAMPLE'};
try{
 const context=await browser.newContext({viewport:{width:1440,height:1000}});
 await context.addInitScript(()=>{
  localStorage.setItem('student-mcp.language','fr');
  Object.defineProperty(navigator,'clipboard',{value:{writeText:async value=>{window.fixtureCopied=value}}});
 });
 const page=await context.newPage(),errors=[],actions=[],requests=[];
 page.on('pageerror',error=>errors.push(error.message));
 await page.route('http://student.test/**',async route=>{
  const path=new URL(route.request().url()).pathname;requests.push(path);
  if(path==='/')return route.fulfill({contentType:'text/html',body:html});
  if(path==='/state')return route.fulfill({json:fixture});
  if(path==='/update')return route.fulfill({json:{available:false,current:'test'}});
  if(path.startsWith('/assets/')){
   const file=path.split('/').at(-1),name=file.replace(/\.png$/,'');
   if(assetNames[name])return route.fulfill({contentType:'image/png',body:readFileSync(new URL(`../src/resources/assets/${assetNames[name]}.png`,import.meta.url))});
    if(/^guide-[a-z-]+\.webp$/.test(file))return route.fulfill({contentType:'image/webp',body:readFileSync(new URL('../src/resources/assets/'+file,import.meta.url))});
  }
   if(['/assistant/others','/assistant/grok','/assistant/chatgpt'].includes(path))throw Error('Web-only guides must not call a local setup endpoint');
  if(path.startsWith('/assistant/'))return route.fulfill({json:{detected:true,configured:false}});
  if(path==='/action'){
   const action=route.request().postDataJSON();actions.push(action);
   if(action.action==='assistant_setup')return route.fulfill({json:{detected:true,configured:true}});
   return route.fulfill({json:{}});
  }
  return route.fulfill({status:404,body:''});
 });
  const snapshot=async name=>{if(shots){await page.locator('img').evaluateAll(images=>Promise.all(images.map(img=>{img.loading='eager';return img.decode().catch(()=>{})})));await page.evaluate(()=>new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(resolve))));await page.screenshot({path:join(shots,name+'.png'),fullPage:true})}};
 const noOverflow=async()=>assert.ok(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth+1),'Page overflows horizontally');
 const openGeneral=()=>page.locator('[data-assistant-choose]').click();
 const home=()=>page.getByRole('button',{name:'Retour aux services',exact:true}).click();
 await page.goto('http://student.test/#fixture-only');
 await page.locator('.assistant-entry').waitFor();
 assert.equal(await page.locator('.ai').count(),0,'Assistant choices must not be on the home screen');
 for(const size of [1440,768,390,320]){
  await page.setViewportSize({width:size,height:1000});
  await page.evaluate(()=>new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(resolve))));
  const branching=await page.evaluate(()=>{
   const branches=[...document.querySelectorAll('[data-map-assistant]')];
   const root=document.querySelector('.mind-map').getBoundingClientRect(),link=document.querySelector('.link-card').getBoundingClientRect();
   return {count:branches.length,trunk:!!document.querySelector('[data-map-trunk]'),
    attached:branches.every(branch=>{const from=branch.getPointAtLength(0),to=branch.getPointAtLength(branch.getTotalLength()),button=document.querySelector('.assistant-entry').getBoundingClientRect();return Math.abs(from.x-(link.left-root.left+link.width/2))<1&&Math.abs(from.y-(link.bottom-root.top))<1&&Math.abs(to.x-(button.left-root.left+button.width/2))<1&&Math.abs(to.y-(button.top-root.top))<1&&branch.hasAttribute('marker-end')})};
  });
  assert.deepEqual(branching,{count:1,trunk:false,attached:true},'Home must have one arrow to a single assistant button');
  await noOverflow();
  const box=await page.locator('.assistant-entry').boundingBox();assert.ok(box.width>=44&&box.height>=44);
  await snapshot('home-'+size);
 }
  await openGeneral();
  assert.equal(await page.locator('.assistant-picker,.assistant-choice-map').count(),0,'There must be no intermediate choice page');
  assert.equal(await page.locator('.assistant-switcher button').count(),4);
   assert.equal(await page.locator('.assistant-switcher [aria-pressed="true"]').getAttribute('data-assistant'),'grok');
   assert.equal(await page.locator('[data-assistant="others"]').count(),0,'The general MCP guide must be removed');
   assert.equal(await page.locator('.journey-node').count(),3,'The Grok tutorial must open immediately');
   assert.equal(requests.filter(path=>path.startsWith('/assistant/')).length,0,'Grok must not detect or configure a local assistant');
  for(const width of [1440,768,390,320]){await page.setViewportSize({width,height:1000});await noOverflow();for(const button of await page.locator('.assistant-switcher button').all()){const box=await button.boundingBox();assert.ok(box.width>=44&&box.height>=44)}await snapshot('general-default-'+width)}
 await page.getByRole('button',{name:'Retour aux services',exact:true}).click();
 await page.setViewportSize({width:1440,height:1000});
  for(const [id,title] of [['claude','Configurer Claude'],['chatgpt','Configurer ChatGPT'],['gemini','Configurer Gemini'],['grok','Configurer Grok']]){
   await openGeneral();
   await page.getByRole('button',{name:title,exact:true}).click();
   assert.equal(await page.locator('.assistant-switcher [aria-pressed="true"]').getAttribute('data-assistant'),id);
   const accents={grok:'rgb(29, 29, 31)',claude:'rgb(201, 100, 66)',chatgpt:'rgb(15, 157, 120)',gemini:'rgb(59, 130, 246)'};
   assert.equal(await page.locator('.setup-footer .setup-next').evaluate(el=>getComputedStyle(el).backgroundColor),accents[id],'Primary actions follow the assistant colour');
   assert.equal(await page.locator('.journey-node[aria-current="step"] .journey-number').evaluate(el=>getComputedStyle(el).backgroundColor),accents[id],'The active step follows the assistant colour');
   assert.equal(await page.locator('.journey-node[aria-current="step"]').getAttribute('data-guide-step'),'0','Switching a tutorial must start its web guide');
  assert.equal(await page.locator('.journey-node').count(),3);
  assert.equal(await page.locator('.journey-node[aria-current="step"]').count(),1);
   assert.equal(actions.length,0,'Opening a guide must not write configuration');
   assert.equal(await page.locator('[data-assistant-route],.assistant-routes,.connection-flow,.setup-intro').count(),0,'Only the web guide remains; remove the route picker, intro and decorative school flow');
   assert.equal(await page.locator('.assistant-local').count(),0,'Automatic local setup is closed by default');
   assert.equal(await page.locator('[data-assistant-setup]').count(),['claude','gemini'].includes(id)?1:0);
   await page.locator('[data-guide-step="1"]').click();
   await page.getByRole('button',{name:'Copier le lien',exact:true}).click();
   assert.equal(await page.evaluate(()=>window.fixtureCopied),fixture.link);
  for(const width of [1440,768,390,320]){
   await page.setViewportSize({width,height:1000});
   for(let step=0;step<3;step++){
    await page.locator(`[data-guide-step="${step}"]`).click();
    assert.equal(await page.locator('.journey-node[aria-current="step"]').getAttribute('data-guide-step'),String(step));
    await noOverflow();
     assert.ok(await page.locator('.journey-long,.journey-short').evaluateAll(labels=>labels.filter(label=>label.getClientRects().length).every(label=>{const box=label.getBoundingClientRect(),button=label.parentElement.getBoundingClientRect();return box.left>=button.left-1&&box.right<=button.right+1})),'Step labels must not collide on narrow screens');
     assert.ok(await page.locator('.assistant-switcher').evaluate(track=>[...track.querySelectorAll('button')].every(button=>{const a=track.getBoundingClientRect(),b=button.getBoundingClientRect();return b.left>=a.left&&b.right<=a.right})),'Assistant buttons must fit inside their control');
     assert.equal(await page.locator('.assistant-workspace').count(),1,'The guide uses one coherent workspace');
     assert.equal(await page.locator('.assistant-journey .card').count(),0,'Web setup must not use nested tiles');
    assert.equal(await page.locator('.setup-details,.guide-caption,.guide-legend').count(),0,'No disclosure blocks or small grey captions on the guide');
      assert.ok((await page.locator('.setup-stage').innerText()).split(/\s+/).length<100,'Each step must be labels and visuals, not paragraphs');
    if(width!==320)await snapshot(`${id}-${step+1}-${width}`);
      assert.equal(await page.locator('.guide-shot').count(),1,'One screenshot per step');
      if(width===1440){
     await page.waitForFunction(()=>[...document.querySelectorAll('.guide-shot')].every(img=>img.complete&&img.naturalWidth>0));
      const image=page.locator('.guide-shot').first();
       assert.ok((await image.boundingBox()).width>150);
       assert.ok(await image.evaluate(img=>img.naturalWidth===1440&&img.naturalHeight===1080),'Guide captures are a focused 480×360 panel rendered at 3×');
      const firstShot=page.locator('.guide-shot-link').first();
      await firstShot.click();
      assert.equal(await page.locator('#lightbox').evaluate(dialog=>dialog.open),true,'The screenshot opens in the page, not a new tab');
      await page.locator('[data-lightbox-close]').click();
      assert.equal(await page.locator('#lightbox').evaluate(dialog=>dialog.open),false,'The close button leaves the viewer');
      await firstShot.click();
       await page.keyboard.press('Escape');
       assert.equal(await page.locator('#lightbox').evaluate(dialog=>dialog.open),false,'Escape closes the viewer');
    }
    if(width===390){
     await page.setViewportSize({width,height:844});
     assert.ok(await page.evaluate(()=>document.documentElement.scrollHeight<=innerHeight+1),`The ${id} guide step ${step+1} must fit vertically at 390×844 without scrolling`);
     await page.setViewportSize({width,height:1000});
    }
   }
  }
  await page.locator('[data-guide-previous]').click();
  assert.equal(await page.locator('[aria-current="step"]').getAttribute('data-guide-step'),'1');
  await page.locator('[data-guide-next]').click();
  assert.equal(await page.locator('[aria-current="step"]').getAttribute('data-guide-step'),'2');
   assert.equal(await page.locator('[data-assistant-route]').count(),0,'No web/local route choice');
  await home();
 }
 await page.setViewportSize({width:1440,height:1000});
  await openGeneral();
 await page.getByRole('button',{name:'Configurer Claude',exact:true}).click();
  await page.locator('[data-guide-step="2"]').click();
  await page.getByRole('button',{name:'Configurer l’appli locale',exact:true}).click();
  assert.equal(await page.locator('[aria-current="step"]').getAttribute('data-guide-step'),'2','Automatic setup must not replace the web guide or reset its step');
 assert.equal(actions.length,0,'The first setup click must not write configuration');
 await page.getByRole('button',{name:'Annuler',exact:true}).click();
 assert.equal(actions.length,0);
 await page.getByRole('button',{name:'Configurer l’appli locale',exact:true}).click();
 await page.getByRole('button',{name:'Confirmer la configuration',exact:true}).click();
 await page.getByText('Configuration locale terminée',{exact:true}).waitFor();
 assert.ok((await page.locator('.assistant-local .local-close').boundingBox()).height<64,'Configured status must remain a compact button');
  assert.equal(await page.locator('.assistant-local svg').count(),0,'No oversized status illustration');
 assert.deepEqual(actions,[{action:'assistant_setup',assistant:'claude'}]);
 await snapshot('claude-local-configured');
  await page.getByRole('button',{name:'Fermer',exact:true}).click();
 await page.locator('[data-guide-step="2"]').click();
 await page.evaluate(()=>refresh());
 assert.equal(await page.locator('[aria-current="step"]').getAttribute('data-guide-step'),'2','Polling must preserve the active step');
 // Unavailable links must not be exposed as ready; language changes preserve the route.
 await page.locator('[data-guide-step="1"]').click();
 fixture.public=false;fixture.tunnel='Stopped';fixture.link='';
 await page.evaluate(()=>refresh());
 assert.equal(await page.locator('[data-copy="assistant"]').count(),0);
 await page.getByRole('button',{name:'Ouvrir les réglages du lien',exact:true}).waitFor();
 await page.locator('#language-button').click();
 await page.getByRole('button',{name:'Español'}).click();
  await page.getByRole('heading',{name:'Añade student-mcp',exact:true}).waitFor();
 await page.locator('#language-button').click();
 await page.getByRole('button',{name:'English'}).click();
  await page.getByRole('heading',{name:'Add student-mcp',exact:true}).waitFor();
 // Keyboard navigation and reduced-motion behavior retain the same guide content.
 await page.emulateMedia({reducedMotion:'reduce'});
 await page.locator('[data-guide-step="2"]').focus();await page.keyboard.press('Enter');
 await page.getByRole('heading',{name:'Ask your first question',exact:true}).waitFor();
 await snapshot('reduced-motion');
 assert.deepEqual(errors,[]);
   console.log('Assistant guides passed: Grok replaces general MCP, minimal web-only steps, refined screenshot markers, in-page lightbox, 320/390/768/1440px, FR/ES/EN, clipboard, polling, keyboard and separate opt-in automatic local setup.');
}finally{await browser.close()}
