// Executed only in the account's own origin. No redirects, bounded body, no retries.
async function browserRequest(input) {
  if (location.origin !== input.origin) return {error: 'reconnect'};
  if (input.mobile) {
    const worker = window.Skytech?.Commun?.Utils?.HttpRequestWorker;
    if (!worker?.PostJSON) return {error: 'reconnect'};
    return await new Promise(resolve => {
      const timeout = setTimeout(() => resolve({error: 'transport'}), 25000);
      const finish = value => { clearTimeout(timeout); resolve(value); };
      const onValue = value => {
        let text;
        try { text = JSON.stringify(value); } catch { return finish({error: 'transport'}); }
        finish(text.length <= input.limit ? {status: 200, text} : {error: 'limit'});
      };
      try {
        worker.PostJSON(input.path, input.body || {}, onValue, () => finish({error: 'transport'}));
      } catch { finish({error: 'transport'}); }
    });
  }
  try {
    const headers = {Accept: input.binary ? '*/*' : 'application/json'};
    if (input.contentType) headers['Content-Type'] = input.contentType;
    // The website supplies its own CSRF token. Do not synthesize one.
    const csrf = window.D2L?.LP?.Web?.Authentication?.Xsrf?.GetXsrfToken?.() ||
      window.localStorage.getItem('XSRF.Token') ||
      document.querySelector('meta[name="d2l-xsrf-token"],meta[name="csrf-token"]')?.content ||
      document.cookie.split('; ').find(c => c.startsWith('XSRF-TOKEN='))?.slice(11);
    if (csrf) headers['X-Csrf-Token'] = decodeURIComponent(csrf);
    let body;
    if (input.bodyBase64 !== undefined) body = Uint8Array.from(atob(input.bodyBase64), c => c.charCodeAt(0));
    const response = await fetch(input.path, {
      method: input.method, headers, body, credentials: 'same-origin',
      redirect: 'manual', signal: AbortSignal.timeout(25000)
    });
    if (response.type === 'opaqueredirect') return {error: 'redirect'};
    const reader = response.body?.getReader();
    const chunks = []; let size = 0;
    if (reader) for (;;) {
      const {done, value} = await reader.read(); if (done) break;
      size += value.length;
      if (size > input.limit) { await reader.cancel(); return {error: 'limit'}; }
      chunks.push(value);
    }
    const bytes = new Uint8Array(size); let offset = 0;
    for (const chunk of chunks) { bytes.set(chunk, offset); offset += chunk.length; }
    const result = {status: response.status, mime: response.headers.get('content-type') || '',
      disposition: response.headers.get('content-disposition') || ''};
    if (input.binary) {
      let text = '';
      for (let i = 0; i < bytes.length; i += 8192) text += String.fromCharCode(...bytes.subarray(i, i + 8192));
      result.base64 = btoa(text);
    } else result.text = new TextDecoder().decode(bytes);
    return result;
  } catch { return {error: 'transport'}; }
}
