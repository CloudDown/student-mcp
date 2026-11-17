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
const css=readFileSync(new URL('../src/resources/web/app.css',import.meta.url),'utf8');
const js=readFileSync(new URL('../src/resources/web/app.js',import.meta.url),'utf8');
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
  if(path==='/app.css')return route.fulfill({contentType:'text/css',body:css});
  if(path==='/app.js')return route.fulfill({contentType:'text/javascript',body:js});
  if(path==='/state')return route.fulfill({json:fixture});
   if(path==='/update')return route.fulfill({json:{available:false,current:'0.8.1'}});
  if(path.startsWith('/assets/')){
   const file=path.split('/').at(-1),name=file.replace(/\.png$/,'');
   if(assetNames[name])return route.fulfill({contentType:'image/png',body:readFileSync(new URL(`../src/resources/assets/${assetNames[name]}.png`,import.meta.url))});
    if(/^guide-(chatgpt|claude|grok|gemini)-(menu|form|chat)\.(mp4|png)$/.test(file))return route.fulfill({contentType:file.endsWith('.mp4')?'video/mp4':'image/png',body:readFileSync(new URL('../src/resources/assets/guides/'+file,import.meta.url))});
  }
   if(['/assistant/others','/assistant/grok','/assistant/chatgpt'].includes(path))throw Error('Web-only guides must not call a local setup endpoint');
   if(path==='/assistants')return route.fulfill({json:{assistants:[{assistant:'claude',name:'Claude Desktop',detected:true,configured:false,reload:'restart'},{assistant:'opencode',name:'OpenCode',detected:true,configured:false,reload:'automatic'},{assistant:'omp',name:'Oh My Pi',detected:false,configured:false}]}});
   if(path.startsWith('/assistant/'))throw Error('Web guides must not scan individual local assistants');
  if(path==='/action'){
   const action=route.request().postDataJSON();actions.push(action);
   if(action.action==='assistant_setup')return route.fulfill({json:{assistant:action.assistant,detected:true,configured:true,reload:action.assistant==='opencode'?'automatic':'restart'}});
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
    attached:branches.every(branch=>{const from=branch.getPointAtLength(0),to=branch.getPointAtLength(branch.getTotalLength()),button=document.querySelector('.assistant-entry').getBoundingClientRect();return Math.abs(from.x-(link.left-root.left+link.width/2))<1&&Math.abs(from.y-(link.bottom-root.top))<1&&Math.abs(to.x-(button.left-root.left+button.width/2))<1&&Math.abs(to.y-(button.top-root.top))<1&&!branch.hasAttribute('marker-end')})};
  });
  assert.deepEqual(branching,{count:1,trunk:false,attached:true},'Home must have one arrow to a single assistant button');
  await noOverflow();
  const box=await page.locator('.assistant-entry').boundingBox();assert.ok(box.width>=44&&box.height>=44);
  await snapshot('home-'+size);
 }
  await openGeneral();
  assert.equal(await page.locator('.assistant-picker,.assistant-choice-map').count(),0,'There must be no intermediate choice page');
  assert.equal(await page.locator('.assistant-switcher button').count(),4);
   assert.equal(await page.locator('.assistant-switcher [aria-pressed="true"]').getAttribute('data-assistant'),'chatgpt');
   assert.equal(await page.locator('[data-assistant="others"]').count(),0,'The general MCP guide must be removed');
    assert.equal(await page.locator('.video-steps button').count(),2,'The video guide has previous/next controls');
    assert.equal(await page.locator('.video-steps span').innerText(),'1 / 3');
    assert.equal(await page.locator('.guide-shot source').getAttribute('src'),'/assets/guide-chatgpt-menu.mp4');
   assert.equal(requests.filter(path=>path.startsWith('/assistant/')).length,0,'ChatGPT must not detect or configure a local assistant');
  for(const width of [1440,768,390,320]){await page.setViewportSize({width,height:1000});await noOverflow();for(const button of await page.locator('.assistant-switcher button').all()){const box=await button.boundingBox();assert.ok(box.width>=44&&box.height>=44)}await snapshot('general-default-'+width)}
 await page.getByRole('button',{name:'Retour aux services',exact:true}).click();
 await page.setViewportSize({width:1440,height:1000});
  for(const [id,title] of [['claude','Configurer Claude'],['chatgpt','Configurer ChatGPT'],['gemini','Configurer Gemini'],['grok','Configurer Grok']]){
   await openGeneral();
   await page.getByRole('button',{name:title,exact:true}).click();
   assert.equal(await page.locator('.assistant-switcher [aria-pressed="true"]').getAttribute('data-assistant'),id);
   const accents={grok:'rgb(29, 29, 31)',claude:'rgb(201, 100, 66)',chatgpt:'rgb(15, 157, 120)',gemini:'rgb(59, 130, 246)'};
    assert.equal(await page.locator('.video-steps span').innerText(),'1 / 3','Switching a tutorial must start its web guide');
    assert.equal(await page.locator('.guide-shot source').getAttribute('src'),`/assets/guide-${id}-menu.mp4`);
   assert.equal(actions.length,0,'Opening a guide must not write configuration');
   assert.equal(await page.locator('[data-assistant-route],.assistant-routes,.connection-flow,.setup-intro').count(),0,'Only the web guide remains; remove the route picker, intro and decorative school flow');
   assert.equal(await page.locator('.assistant-local').count(),0,'Automatic local setup is closed by default');
   assert.equal(await page.locator('[data-assistant-setup]').count(),0,'Local setup belongs in its own view');
   assert.equal(await page.locator('[data-assistant-mode]').count(),2);
    assert.equal(await page.locator('.guide-link').getAttribute('href'),({
      claude:'https://claude.ai/customize/connectors',
      chatgpt:'https://chatgpt.com/plugins',
      gemini:'https://gemini.google.com/apps',
      grok:'https://grok.com/'
    })[id]);
   for(const width of [1440,768,390,320]){
    await page.setViewportSize({width,height:1000});
    while(await page.locator('.video-steps span').innerText()!=='1 / 3')await page.locator('[data-guide-previous]').click();
    for(let step=0;step<3;step++){
      if(step>0)await page.locator('[data-guide-next]').click();
      assert.equal(await page.locator('.video-steps span').innerText(),`${step+1} / 3`);
      assert.equal(await page.locator('.guide-shot source').getAttribute('src'),`/assets/guide-${id}-${['menu','form','chat'][step]}.mp4`);
     await page.evaluate(()=>document.querySelector('.screen').scrollTop=0);
     assert.ok(await page.evaluate(()=>{
      const back=document.querySelector('.page-nav .back').getBoundingClientRect();
      const badge=document.querySelector('#update-label').getBoundingClientRect();
      return !badge.width || back.top>=badge.bottom+4 || back.left>=badge.right+4;
     }),'Version badge must not overlap the back button');
    await noOverflow();
      assert.ok(await page.locator('.assistant-switcher').evaluate(track=>[...track.querySelectorAll('button')].every(button=>{const a=track.getBoundingClientRect(),b=button.getBoundingClientRect();return b.left>=a.left&&b.right<=a.right})),'Assistant buttons must fit inside their control');
     assert.equal(await page.locator('.assistant-workspace').count(),1,'The guide uses one coherent workspace');
     assert.equal(await page.locator('.assistant-journey .card').count(),0,'Web setup must not use nested tiles');
    assert.equal(await page.locator('.setup-details,.guide-caption,.guide-legend').count(),0,'No disclosure blocks or small grey captions on the guide');
      assert.ok((await page.locator('.setup-stage').innerText()).split(/\s+/).length<100,'Each step must be labels and visuals, not paragraphs');
     if(width!==320)await snapshot(`${id}-${step+1}-${width}`);
      assert.equal(await page.locator('.guide-shot').count(),1,'One media element per step');
      if(width===1440){
     const isVideo=true;
     if(isVideo){
      await page.waitForFunction(()=>{const v=document.querySelector('.guide-shot');return v&&v.tagName==='VIDEO'&&v.readyState>=2&&v.videoWidth>0});
      const video=page.locator('.guide-shot').first();
      assert.ok((await video.boundingBox()).width>150);
      assert.ok(await video.evaluate(v=>v.videoWidth===1920&&v.videoHeight===1080),'Recorded guide videos are 1920×1080');
      assert.ok(await video.evaluate(v=>v.autoplay&&v.muted&&v.loop&&v.playsInline));
      await page.waitForFunction(()=>{const v=document.querySelector('.guide-shot');return v&&v.readyState>=2&&!v.paused});
     }else{
      await page.waitForFunction(()=>[...document.querySelectorAll('.guide-shot')].every(img=>img.complete&&img.naturalWidth>0));
      const image=page.locator('.guide-shot').first();
      assert.ok((await image.boundingBox()).width>150);
      assert.ok(await image.evaluate(img=>img.naturalWidth===1440&&img.naturalHeight===1080),'Guide captures are a focused 480×360 panel rendered at 3×');
     }
      const firstShot=page.locator('.guide-shot-link').first();
      await firstShot.click();
      assert.equal(await page.locator('#lightbox').evaluate(dialog=>dialog.open),true,'The media opens in the page, not a new tab');
       if(isVideo){
        await page.locator('#lightbox-video').waitFor({state:'visible'});
        assert.equal(await page.locator('#lightbox-video').isVisible(),true,'The enlarged viewer uses the video');
       }
      await page.locator('[data-lightbox-close]').click();
      assert.equal(await page.locator('#lightbox').evaluate(dialog=>dialog.open),false,'The close button leaves the viewer');
      await firstShot.click();
       await page.keyboard.press('Escape');
       assert.equal(await page.locator('#lightbox').evaluate(dialog=>dialog.open),false,'Escape closes the viewer');
    }
    if(width===390){
     await page.setViewportSize({width,height:844});
     await noOverflow();
      await page.locator('.video-steps').scrollIntoViewIfNeeded();
      assert.ok(await page.locator('.video-steps').isVisible(),`The ${id} guide step ${step+1} controls must remain reachable at 390×844`);
     await page.setViewportSize({width,height:1000});
    }
   }
  }
  await page.locator('[data-guide-previous]').click();
   assert.equal(await page.locator('.video-steps span').innerText(),'2 / 3');
   await page.locator('[data-guide-next]').click();
   assert.equal(await page.locator('.video-steps span').innerText(),'3 / 3');
   assert.equal(await page.locator('[data-assistant-route]').count(),0,'No web/local route choice');
  await home();
 }
 await page.setViewportSize({width:1440,height:1000});
  await openGeneral();
 await page.getByRole('button',{name:'Configurer Claude',exact:true}).click();
   await page.locator('[data-guide-next]').click();
   await page.locator('[data-assistant-mode="local"]').click();
   await page.locator('[data-local-assistant="claude"]').waitFor();
   assert.equal(await page.locator('.assistant-workspace').count(),0,'The local view must not show web instructions');
   assert.equal(await page.locator('.local-app-row').count(),0,'Local apps use the compact gallery, not list rows');
   assert.equal(await page.locator('.local-app-card').count(),3);
   assert.equal(await page.locator('.local-app-card .local-app-logo').count(),3,'Known apps use their bundled logos');
   assert.equal(await page.locator('.local-app-card .local-app-mark').count(),0,'Bundled logos replace monogram marks');
   await page.locator('[data-local-assistant="claude"]').click();
 assert.equal(actions.length,0,'The first setup click must not write configuration');
 await page.getByRole('button',{name:'Confirmer la configuration',exact:true}).click();
  await page.locator('[data-assistant-remove="claude"]').waitFor();
  assert.deepEqual(actions,[{action:'assistant_setup',assistant:'claude'}]);
 await snapshot('claude-local-configured');
   await page.locator('[data-local-assistant="claude"]').click();
  for(const width of [1440,768,390,320]){await page.setViewportSize({width,height:1000});await noOverflow();await snapshot('local-apps-'+width)}
  await page.locator('[data-assistant-mode="web"]').click();
   assert.equal(await page.locator('.video-steps span').innerText(),'2 / 3','Switching views preserves the web step');
  await page.locator('[data-guide-next]').click();
  await page.evaluate(()=>refresh());
  assert.equal(await page.locator('.video-steps span').innerText(),'3 / 3','Polling must preserve the active step');
 await page.locator('#language-button').click();
 await page.getByRole('button',{name:'Español'}).click();
    await page.waitForFunction(()=>document.querySelector('.setup-stage h2')?.textContent==='Haz tu primera pregunta');
 await page.locator('#language-button').click();
 await page.getByRole('button',{name:'English'}).click();
    await page.waitForFunction(()=>document.querySelector('.setup-stage h2')?.textContent==='Ask your first question');
 // Keyboard navigation and reduced-motion behavior retain the same guide content.
 await page.emulateMedia({reducedMotion:'reduce'});
  await page.locator('[data-guide-previous]').focus();await page.keyboard.press('Enter');
  assert.equal(await page.locator('.video-steps span').innerText(),'2 / 3');
  await page.locator('[data-guide-next]').focus();await page.keyboard.press('Enter');
  await page.waitForFunction(()=>document.querySelector('.setup-stage h2')?.textContent==='Ask your first question');
  await page.locator('[data-assistant="chatgpt"]').click();
 const reducedVideo=page.locator('.guide-shot');
 await reducedVideo.waitFor();
 assert.equal(await reducedVideo.evaluate(v=>v.tagName),'VIDEO');
 assert.ok(await reducedVideo.evaluate(v=>v.controls&&!v.autoplay&&v.paused),'Reduced motion disables autoplay and shows controls');
 assert.equal(await page.locator('.guide-shot-link').count(),0,'Reduced-motion video is not wrapped in the zoom button');
 await snapshot('reduced-motion');
 assert.deepEqual(errors,[]);
    console.log('Assistant guides passed: separate web/local views, minimal web guides, lightbox, 320/390/768/1440px, FR/ES/EN, clipboard, polling, keyboard and opt-in automatic local setup.');
}finally{await browser.close()}
