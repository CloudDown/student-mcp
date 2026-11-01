// Minimal Android WebView bridge for Omnivox's official mobile JavaScript.
// Protocol reference: Beat-YT/omnivox-mcp/docs/native-bridge.md (MIT).
// No credentials, code generation, UI scraping or automatic form submission.
(() => {
  if (location.protocol !== 'https:' || !(location.hostname === 'omnivox.ca' || location.hostname.endsWith('.omnivox.ca'))) return;
  const seed = __STUDENT_MOBILE_STATE__;
  // Login handoffs can carry the mobile login token in the fragment, which
  // is never sent to the HTTP server (same convention as the mobile client).
  for (const [key, value] of new URLSearchParams(location.hash.slice(1))) {
    if (key.toLowerCase() === 'token' && value) document.cookie = `LoginTokenAuthentification=${encodeURIComponent(value)}; Path=/; Secure; SameSite=Lax`;
  }
  const storageKey = 'student-mcp.mobile';
  let state = seed;
  try {
    const saved = JSON.parse(localStorage.getItem(storageKey));
    if (saved?.device === seed.device) state = {...seed, ...saved};
  } catch {}
  const ua = () => `OVX InfoDevice=Android-14_samsung_SM-X700_Build/UP1A.231005.007 AppVer=3.8.9 IdAppareil=${state.device} Code=${state.code || ''}`;
  const persist = () => { localStorage.setItem(storageKey, JSON.stringify(state)); window.studentMobileState(JSON.stringify(state)); };
  const callback = (name, value) => setTimeout(() => window.Ovx?.ExecuteCallback(name + 'CallBack', value), 0);
  window.OvxNatif = {ExecuteCommand(name, raw) {
    let args; try { args = JSON.parse(raw || '{}'); } catch { return 'none'; }
    if (name === 'Android.Device.IsTablet') return JSON.stringify({IsTablet: 'True'});
    let result;
    switch (name) {
      case 'Storage.SetCodeUserAgent':
        state.code = args.Code || ''; window.userAgentRequete = ua(); persist();
        result = {UserAgentRequete: ua()}; break;
      case 'Storage.SetInfo':
        state.info = {...state.info, [args.Key]: args.Value}; persist(); result = {UserAgentRequete: ua()}; break;
      case 'Storage.GetInfo': result = {Key: args.Key, Value: state.info?.[args.Key] || ''}; break;
      case 'WebUI.SetDefaultPage':
        try { state.home = new URL(decodeURIComponent(args.Url), location.href).href; persist(); } catch {}
        result = null; break;
      case 'WebUI.IsOnline': result = {IsOnline: 'True'}; break;
      case 'Application.GetDeviceInfo': result = {Manufacturer:'samsung',Model:'SM-X700',OsVersion:'14',Device:'gts8wifi',IsOsModified:false}; break;
      case 'Application.DeviceStats': result = {totalDiskSpace:0,freeDiskSpace:0,externalTotalDiskSpace:0,externalFreeDiskSpace:0,systemTotalDiskSpace:0,systemFreeDiskSpace:0,webViewVersionName:'128.0.0.0',webViewVersionCode:0}; break;
      case 'Biometry.GetCapabilities':
      case 'Biometry.GetState':
      case 'Biometry.GetCapabilitiesAndState':
        result = {BiometryUnavailable:true,IsDevicePasswordSet:false,CanUseHardwareStorage:false,KeyInStorage:false,BiometrySetupComplete:false}; break;
      case 'Biometry.GenerateSigningKey':
      case 'Biometry.SignData': result = {Success:false}; break;
      case 'Biometry.DeleteKey':
      case 'Biometry.SetSetupDone': result = {Success:true}; break;
      case 'Display.SetBrightness':
      case 'Display.Authenticate':
      case 'Display.OpenCamera':
      case 'WebUI.SetWindowOption': result = null; break;
      case 'Android.Device.HasCamera': result = {HasCamera:false}; break;
      case 'Android.Notification.IsGooglePlayServicesAvailable': result = {IsAvailable:'False',IsBackgroundRestricted:'False'}; break;
      case 'Theme.GetTheme': result = state.theme || {}; break;
      case 'Theme.SetTheme': state.theme = args; persist(); result = args; break;
      case 'Display.ViewLogin':
        location.href = '/intr/Module/Identification/Login/Mobile/LoginMobile.aspx?typeUtilisateur=Etudiant'; return 'none';
      case 'WebUI.ResetSession':
        state = {...seed,code:'',info:{},home:''}; persist(); result = null; break;
      default: return 'none';
    }
    callback(name, result); return 'none';
  }};
  window.userAgentRequete = ua();
  persist();
})();
