// Captures local configuration screens for the student README. No school logins.
import {readFileSync, mkdirSync, existsSync} from 'node:fs';
import {pathToFileURL} from 'node:url';
const {chromium}=await import(process.env.PLAYWRIGHT_MODULE ? pathToFileURL(process.env.PLAYWRIGHT_MODULE).href : 'playwright');
const html=readFileSync(new URL('../src/resources/web/index.html',import.meta.url),'utf8');
const css=readFileSync(new URL('../src/resources/web/app.css',import.meta.url),'utf8');
const js=readFileSync(new URL('../src/resources/web/app.js',import.meta.url),'utf8');
const out=new URL('./screens/',import.meta.url);
mkdirSync(out,{recursive:true});
const state={
  accounts:[],
  browser_login:null,
  verification:null,
  tunnel:'Idle',
  local:true,
  public:false,
  autostart:false,
  link:'',
  local_link:'http://127.0.0.1:8420/mcp/your-link',
};
const browser=await chromium.launch({headless:true,executablePath:process.env.STUDENT_MCP_BROWSER});
const page=await browser.newPage({viewport:{width:1540,height:866},deviceScaleFactor:1});
const errors=[];
page.on('pageerror',e=>errors.push(e.message));
await page.addInitScript(()=>localStorage.setItem('student-mcp.language','en'));
await page.route('http://student.test/**',async route=>{
  const path=new URL(route.request().url()).pathname;
  if(path==='/')return route.fulfill({contentType:'text/html',body:html});
  if(path==='/app.css')return route.fulfill({contentType:'text/css',body:css});
  if(path==='/app.js')return route.fulfill({contentType:'text/javascript',body:js});
  if(path==='/state')return route.fulfill({json:state});
  if(path==='/update')return route.fulfill({json:{available:false,current:'0.8.0',latest:'v0.8.0',checking:false,checked:true}});
  if(path==='/assistants')return route.fulfill({json:{assistants:[
    {assistant:'claude',name:'Claude Desktop',detected:true,configured:true,reload:'restart'},
    {assistant:'opencode',name:'OpenCode',detected:true,configured:false,reload:'automatic'},
    {assistant:'cursor',name:'Cursor',detected:true,configured:false,reload:'restart'},
    {assistant:'vscode',name:'GitHub Copilot / VS Code',detected:true,configured:false,reload:'restart'},
    {assistant:'gemini',name:'Gemini CLI',detected:true,configured:false,reload:'restart'},
    {assistant:'omp',name:'Oh My Pi',detected:false,configured:false,reload:'mcp'},
  ]}});
  if(path==='/action')return route.fulfill({json:{message:'Connected'}});
  if(path.startsWith('/assets/')){
    const name=path.split('/').pop();
    const assets=new URL('../src/resources/assets/',import.meta.url);
    const direct=new URL(name,assets);
    const fallback=new URL(name==='app-icon.png'?'app-icon-64.png':name.replace('.png','-icon.png'),assets);
    const file=name==='app-icon.png'||!existsSync(direct)?fallback:direct;
    try {return route.fulfill({contentType:'image/png',body:readFileSync(file)});}
    catch {return route.fulfill({status:404,body:''});}
  }
  return route.fulfill({status:404,body:''});
});
async function shot(name){
  await page.evaluate(()=>document.fonts.ready);
  if(errors.length)throw new Error(errors.join('\n'));
  await page.screenshot({path:new URL(name,out).pathname});
}
await page.goto('http://student.test/#readme-capture');
await page.waitForSelector('#school-find');
await page.getByRole('button',{name:'Canvas connect',exact:true}).waitFor();
await shot('home.png');
await page.getByRole('button',{name:'Moodle connect',exact:true}).click();
await page.getByRole('button',{name:'Connect with browser',exact:true}).waitFor();
await shot('moodle.png');
state.accounts=[{id:'moodle',service:'moodle',url:'https://studium.umontreal.ca',enabled:true}];
state.tunnel='Ready(https://example.srv.us)';
state.public=true;
state.link='https://example.srv.us/mcp/your-link';
await page.getByRole('button',{name:'Back to services',exact:true}).click();
await page.evaluate(()=>refresh());
await page.locator('.link-open').click();
await page.waitForSelector('[data-link-card="public"] .link-detail');
await shot('link.png');
await page.getByRole('button',{name:'Back to services',exact:true}).click();
state.accounts=[{id:'moodle',service:'moodle',url:'https://studium.umontreal.ca',name:'Léa',enabled:true}];
await page.evaluate(()=>refresh());
await page.getByRole('button',{name:'Moodle manage connection',exact:true}).click();
await page.locator('.account-row-title').filter({hasText:'Léa'}).waitFor();
await shot('profile.png');
await page.getByRole('button',{name:'Back to services',exact:true}).click();
await page.locator('[data-assistant-choose]').click();
await page.locator('[data-assistant-mode="local"]').click();
await page.locator('.local-app-card').first().waitFor();
await page.locator('details').evaluate(element=>element.open=true);
await shot('assistant.png');
await page.getByRole('button',{name:'Back to services',exact:true}).click();
await page.getByRole('button',{name:'Omnivox connect',exact:true}).click();
await page.getByRole('button',{name:'Connect with browser',exact:true}).waitFor();
await shot('omnivox.png');
await browser.close();
console.log('Captured README screens (1540×866; fictitious account names, URLs and tokens).');
