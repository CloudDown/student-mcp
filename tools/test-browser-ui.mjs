// Run with Playwright installed for development (not an application dependency).
import {readFileSync} from 'node:fs';
import {pathToFileURL} from 'node:url';
import assert from 'node:assert/strict';
const {chromium}=await import(process.env.PLAYWRIGHT_MODULE ? pathToFileURL(process.env.PLAYWRIGHT_MODULE).href : 'playwright');
const browser=await chromium.launch({headless:true,executablePath:process.env.STUDENT_MCP_BROWSER});
try {
  const page=await browser.newPage();
  const errors=[];page.on('pageerror',error=>errors.push(error.message));
  await page.addInitScript(()=>localStorage.setItem('student-mcp.language','en'));
  const html=readFileSync(new URL('../src/resources/web/index.html',import.meta.url),'utf8');
  const state={accounts:[],browser_login:null,verification:null,tunnel:'Idle',local:true,public:false,autostart:false,link:'',local_link:''};
  const actions=[];
  await page.route('http://student.test/**',async route=>{
    const path=new URL(route.request().url()).pathname;
    if(path==='/')return route.fulfill({contentType:'text/html',body:html});
    if(path==='/state')return route.fulfill({json:state});
    if(path==='/update')return route.fulfill({json:{available:false}});
    if(path==='/action'){
      const action=route.request().postDataJSON();actions.push(action);
      if(action.action==='browser_connect'){
        state.browser_login={service:action.service,id:'fixture',state:'waiting',error:null,progress:{phase:'preparing'}};
        return route.fulfill({json:{browser_login:state.browser_login}});
      }
      if(action.action==='cancel')state.browser_login={...state.browser_login,state:'cancelled'};
      return route.fulfill({json:{message:'Connected'}});
    }
    return route.fulfill({status:404,body:''});
  });
  await page.goto('http://student.test/#test-secret');
  await page.getByRole('button',{name:'Brightspace connect',exact:true}).click();
  await page.getByLabel('Site address',{exact:true}).fill('https://school.brightspace.com');
  await page.getByRole('button',{name:'Connect with browser',exact:true}).click();
  await page.getByText('Preparing the sign-in browser…',{exact:true}).waitFor();
  state.browser_login.progress={phase:'downloading',downloaded:10485760,total:104857600};
  await page.evaluate(()=>refresh());
  await page.getByText('Downloading Chrome for Testing… 10.0 / 100.0 MB',{exact:true}).waitFor();
  state.browser_login.progress={phase:'extracting'};
  await page.evaluate(()=>refresh());
  await page.getByText('Installing the sign-in browser…',{exact:true}).waitFor();
  state.browser_login.progress={phase:'signin'};
  await page.evaluate(()=>refresh());
  await page.getByText('Finish signing in in the browser window. This page will update automatically.',{exact:true}).waitFor();
  assert.equal(actions.at(-1).action,'browser_connect');
  assert.equal(actions.at(-1).service,'brightspace');
  assert.equal(actions.at(-1).url,'https://school.brightspace.com');
  state.accounts=[{id:'fixture',service:'brightspace',url:'https://school.brightspace.com'}];
  state.browser_login={...state.browser_login,state:'connected'};
  await page.evaluate(()=>refresh());
  await page.getByRole('button',{name:'Change login',exact:true}).waitFor();
  await page.getByRole('button',{name:'Change login',exact:true}).click();
  await page.getByRole('button',{name:'Other connection methods',exact:true}).click();
  await page.getByLabel('d2lSessionVal',{exact:true}).waitFor();
  await page.getByRole('button',{name:'Back to services',exact:true}).click();
  await page.getByRole('button',{name:'Omnivox connect',exact:true}).click();
  await page.getByLabel('College address',{exact:true}).fill('https://college.omnivox.ca');
  await page.getByRole('button',{name:'Connect with browser',exact:true}).click();
  await page.getByRole('button',{name:'Cancel',exact:true}).click();
  await page.getByRole('button',{name:'Connect with browser',exact:true}).waitFor();
  assert.equal(actions.at(-1).action,'cancel');
  assert.equal(state.accounts.length,1);
  assert.deepEqual(errors,[]);
  console.log('Browser UI passed: preparation, download progress, installation, sign-in, completion, manual fallback and cancellation.');
} finally {await browser.close()}
