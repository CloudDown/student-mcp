// Captures local configuration screens for the student README. No school logins.
import {readFileSync, mkdirSync} from 'node:fs';
import {pathToFileURL} from 'node:url';
const {chromium}=await import(process.env.PLAYWRIGHT_MODULE ? pathToFileURL(process.env.PLAYWRIGHT_MODULE).href : 'playwright');
const html=readFileSync(new URL('../src/resources/web/index.html',import.meta.url),'utf8');
const out=new URL('./screens/',import.meta.url);
mkdirSync(out,{recursive:true});
const state={
  accounts:[],
  mail:{connected:false},
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
  if(path==='/state')return route.fulfill({json:state});
  if(path==='/update')return route.fulfill({json:{available:false,current:'0.6.8',latest:'v0.6.8',checking:false,checked:true}});
  if(path.startsWith('/assets/')){
    const name=path.split('/').pop();
    const file=new URL(`../src/resources/assets/${name==='app-icon.png'?'app-icon-64.png':name.replace('.png','-icon.png')}`,import.meta.url);
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
await page.getByText('AI connection',{exact:true}).waitFor();
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
await page.getByRole('button',{name:'AI connection'}).click();
await page.getByText('Public link active',{exact:true}).waitFor();
await shot('link.png');
await page.getByRole('button',{name:'Back to services',exact:true}).click();
await page.getByRole('button',{name:'Omnivox connect',exact:true}).click();
await page.getByRole('button',{name:'Connect with browser',exact:true}).waitFor();
await shot('omnivox.png');
await page.getByRole('button',{name:'Back to services',exact:true}).click();
await page.getByRole('button',{name:'Mail transfer',exact:true}).click();
await page.getByLabel('School mail',{exact:true}).fill('student@school.edu');
await page.getByLabel('Personal mail',{exact:true}).fill('you@gmail.com');
await shot('mail-connect.png');
state.mail={connected:true,forwarding:true,mailbox:'student@school.edu',forward_to:'you@gmail.com',url:'https://outlook.office.com'};
await page.evaluate(()=>refresh());
await page.getByText('Forwarding on',{exact:true}).waitFor();
await shot('mail.png');
await browser.close();
console.log('Captured README screens (1540×866; fictitious empty account state).');
