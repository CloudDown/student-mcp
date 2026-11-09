// Renders tools/assistant-guides.html scenes into src/resources/assets/guide-*.webp.
// Replaces the "Connect an assistant" screenshots. No school logins, no personal data.
import {readFileSync,writeFileSync,mkdirSync} from 'node:fs';
import {pathToFileURL} from 'node:url';
import {spawnSync} from 'node:child_process';
import {join} from 'node:path';
const loaded=await import(process.env.PLAYWRIGHT_MODULE?pathToFileURL(process.env.PLAYWRIGHT_MODULE).href:'playwright');
const chromium=loaded.chromium||loaded.default?.chromium;
const htmlPath=new URL('./assistant-guides.html',import.meta.url);
const outDir=new URL('../src/resources/assets/',import.meta.url);
const sceneW=480,sceneH=360,scale=3;
const browser=await chromium.launch({headless:true,executablePath:process.env.STUDENT_MCP_BROWSER});
try{
 const page=await browser.newPage({viewport:{width:sceneW,height:sceneH},deviceScaleFactor:scale});
 await page.goto(new URL(htmlPath.href).href);
 await page.evaluate(()=>document.fonts.ready);
 const scenes=await page.locator('section.scene').evaluateAll(els=>els.map(el=>({id:el.id,file:el.dataset.file})));
 if(!scenes.length)throw new Error('No section.scene found in assistant-guides.html');
 const rows=[];
 for(const scene of scenes){
  const box=await page.locator('#'+scene.id).evaluate(el=>({w:el.offsetWidth,h:el.offsetHeight,
   sw:el.scrollWidth,sh:el.scrollHeight}));
  if(box.w!==sceneW||box.h!==sceneH||box.sw>sceneW||box.sh>sceneH)
   throw new Error(`Scene ${scene.id} is not a clean ${sceneW}×${sceneH} (measured ${box.w}×${box.h}, content ${box.sw}×${box.sh})`);
  await page.evaluate(()=>new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(resolve))));
  const png=await page.locator('#'+scene.id).screenshot();
  const width=png.readUInt32BE(16),height=png.readUInt32BE(20);
  if(width!==sceneW*scale||height!==sceneH*scale)
   throw new Error(`Scene ${scene.id} produced ${width}×${height}, expected ${sceneW*scale}×${sceneH*scale}`);
  const webp=spawnSync('magick',['png:-','-quality','90','webp:-'],
   {input:png,maxBuffer:32*1024*1024,encoding:'buffer'});
  if(webp.error)throw webp.error;
  if(webp.status!==0||!webp.stdout.length)
   throw new Error(`magick failed for ${scene.file}: ${webp.stderr.toString().trim()}`);
  writeFileSync(new URL(scene.file+'.webp',outDir),webp.stdout);
  rows.push({file:scene.file+'.webp',width,height,bytes:webp.stdout.length});
 }
 const width=Math.max(...rows.map(r=>r.file.length));
 console.log('file'.padEnd(width)+'\tdimension\tbytes');
 for(const row of rows)console.log(`${row.file.padEnd(width)}\t${row.width}×${row.height}\t${row.bytes}`);
 const tooBig=rows.filter(r=>r.bytes>120*1024);
 const badSize=rows.filter(r=>r.width!==sceneW*scale||r.height!==sceneH*scale);
 if(badSize.length)throw new Error(`Captures not ${sceneW*scale}×${sceneH*scale}: ${badSize.map(r=>r.file).join(', ')}`);
 if(tooBig.length)throw new Error(`Captures over 120 KiB: ${tooBig.map(r=>`${r.file} (${r.bytes} bytes)`).join(', ')}`);
 console.log(`Rendered ${rows.length} assistant guide captures into src/resources/assets.`);
}finally{await browser.close()}
