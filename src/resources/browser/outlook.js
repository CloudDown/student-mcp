// Runs only inside the account-owned Outlook tab. Never return page HTML,
// account data or authentication material to the local configuration UI.
// wantOn=true enables forwarding to destination; wantOn=false turns it off.
function outlookForwarding(action, expected, destination, wantOn) {
  if (location.origin !== 'https://outlook.cloud.microsoft' ||
      !location.pathname.startsWith('/mail/')) return {state:'waiting'};
  const on = wantOn !== false;
  const dest = (destination || '').trim().toLowerCase();
  const norm=s=>(s||'').replace(/\s+/g,' ').trim();
  const visible=e=>!!(e.getClientRects().length);
  const controls=()=>Array.from(document.querySelectorAll('button,[role="button"]')).filter(visible);
  const account=controls().filter(e=>/account manager|gestionnaire de compte|compte microsoft/i.test(norm(e.getAttribute('aria-label')||e.getAttribute('title'))));
  if (action==='account') {
    if (account.length!==1) return {state:'waiting'};
    account[0].click();
    return {state:'opened'};
  }
  if (action==='identity') {
    // The signed-in address must be in the account menu, never merely in mail
    // content (which could mention an unrelated address).
    const menus=Array.from(document.querySelectorAll('[role="dialog"],[role="menu"],[data-testid*="account" i]')).filter(visible);
    const found=menus.some(e=>norm(e.textContent).toLowerCase().includes(expected.toLowerCase()));
    return {state:found?'verified':'waiting'};
  }
  const labels=Array.from(document.querySelectorAll('label,span,div')).filter(visible);
  const labeled=(pattern,selector)=>{
    let matches=labels.filter(e=>e.tagName==='LABEL'&&pattern.test(norm(e.textContent)));
    if(!matches.length) matches=labels.filter(e=>e.children.length<2&&pattern.test(norm(e.textContent)));
    if(matches.length!==1)return null;
    const label=matches[0];
    return (label.htmlFor&&document.getElementById(label.htmlFor))||label.querySelector(selector)||label.parentElement?.querySelector(selector)||label.parentElement?.parentElement?.querySelector(selector)||null;
  };
  const toggle=labeled(/^(Activer le transfert|Enable forwarding)$/i,'[role="switch"],input[type="checkbox"]');
  if(!toggle||!visible(toggle))return {state:'waiting'};
  const enabled=toggle.getAttribute('aria-checked')==='true'||(toggle.matches('input')&&toggle.checked);
  if(!on){
    if(action==='read')return {state:enabled?'ready':'verified'};
    if(action!=='write')return {state:'waiting'};
    if(enabled)toggle.click();
    const saves=controls().filter(e=>/^(Save|Enregistrer)$/i.test(norm(e.textContent||e.getAttribute('aria-label')))&&!e.disabled);
    if(saves.length!==1)return {state:'no-save'};
    saves[0].click();
    return {state:'submitted'};
  }
  const field=labeled(/^(Transférer mon courrier à\s*:|Forward my email to\s*:|Forward my email to)$/i,'input');
  const copy=labeled(/^(Conserver une copie des messages transférés|Keep a copy of forwarded messages)$/i,'input[type="checkbox"],[role="checkbox"]');
  const value=norm(field?.value).toLowerCase();
  const kept=copy?.getAttribute('aria-checked')==='true'||(copy?.matches('input')&&copy.checked);
  if(action==='read')return {state:enabled&&value===dest&&kept?'verified':'ready'};
  if(action!=='write')return {state:'waiting'};
  if(!enabled)toggle.click();
  // A newly enabled switch reveals the address and copy fields asynchronously.
  if(!field||!copy||!visible(field)||!visible(copy))return {state:'fields'};
  if(!kept)copy.click();
  const setter=Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set;
  setter.call(field,destination);
  field.dispatchEvent(new Event('input',{bubbles:true}));
  field.dispatchEvent(new Event('change',{bubbles:true}));
  const saves=controls().filter(e=>/^(Save|Enregistrer)$/i.test(norm(e.textContent||e.getAttribute('aria-label')))&&!e.disabled);
  if(saves.length!==1)return {state:'no-save'};
  saves[0].click();
  return {state:'submitted'};
}
