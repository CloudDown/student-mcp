/** The whole dashboard is one static page; data comes from /api/status. */
export function renderDashboard(): string {
  return DASHBOARD_HTML;
}

const DASHBOARD_HTML = `<!doctype html>
<html lang="fr">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>student-mcp</title>
<style>
  :root { color-scheme: light; --ink: #1c2330; --muted: #5a6473; --line: #e6e9ef; --brand: #2f6fed; --ok: #12855f; }
  * { box-sizing: border-box; }
  body { margin: 0; background: #ffffff; color: var(--ink); font: 15px/1.55 -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, Inter, sans-serif; }
  main { max-width: 900px; margin: 0 auto; padding: 40px 20px 60px; }
  .head { display: flex; align-items: center; gap: 12px; margin-bottom: 6px; }
  .logo { width: 30px; height: 30px; border-radius: 8px; background: var(--brand); color: #fff; display: flex; align-items: center; justify-content: center; font-weight: 700; font-size: 15px; }
  h1 { font-size: 22px; margin: 0; }
  .sub { color: var(--muted); margin: 0 0 26px; }
  h2 { font-size: 15px; text-transform: uppercase; letter-spacing: .06em; color: var(--muted); margin: 26px 0 12px; }
  .grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(250px, 1fr)); gap: 12px; }
  .card { border: 1px solid var(--line); border-radius: 14px; padding: 16px; cursor: pointer; transition: border-color .12s; background: #fff; text-align: left; font: inherit; color: inherit; }
  .card:hover { border-color: #b9c4d8; }
  .card .top { display: flex; align-items: center; gap: 10px; margin-bottom: 8px; }
  .mark { width: 34px; height: 34px; border-radius: 10px; color: #fff; font-weight: 700; display: flex; align-items: center; justify-content: center; flex: none; }
  .name { font-weight: 600; }
  .tag { color: var(--muted); font-size: 13.5px; }
  .pill { margin-left: auto; font-size: 11.5px; padding: 3px 9px; border-radius: 999px; border: 1px solid var(--line); color: var(--muted); flex: none; }
  .pill.on { color: var(--ok); border-color: #bfe3d4; background: #f0faf6; }
  .card.planned { cursor: default; opacity: .55; background: #fafbfc; }
  .card.planned:hover { border-color: var(--line); }
  .panel { border: 1px solid var(--line); border-radius: 14px; padding: 18px; }
  .row { display: flex; align-items: center; gap: 10px; margin: 10px 0; flex-wrap: wrap; }
  .url { font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace; font-size: 12.5px; background: #f6f8fb; border: 1px solid var(--line); border-radius: 8px; padding: 8px 10px; overflow-wrap: anywhere; flex: 1; min-width: 260px; }
  button { font: inherit; border: 1px solid var(--line); background: #fff; border-radius: 9px; padding: 7px 13px; cursor: pointer; color: var(--ink); }
  button:hover { border-color: #b9c4d8; }
  button.primary { background: var(--brand); border-color: var(--brand); color: #fff; }
  button.primary:hover { filter: brightness(1.07); }
  button.danger { color: #b3261e; border-color: #eec7c4; }
  button.danger:hover { background: #fdf3f2; }
  .hint { color: var(--muted); font-size: 13px; margin: 8px 0 0; }
  .steps { margin: 10px 0 0; padding-left: 20px; color: var(--muted); font-size: 13.5px; }
  .steps li { margin: 4px 0; }
  code { font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace; font-size: 13px; background: #f6f8fb; border-radius: 6px; padding: 1px 5px; }
  .overlay { position: fixed; inset: 0; background: rgba(20,24,32,.42); display: none; align-items: center; justify-content: center; padding: 18px; z-index: 5; }
  .overlay.show { display: flex; }
  .modal { background: #fff; border-radius: 16px; max-width: 460px; width: 100%; max-height: 90vh; overflow: auto; padding: 22px; box-shadow: 0 18px 50px rgba(15,20,30,.18); }
  .modal h3 { margin: 0 0 4px; }
  .label { display: block; font-size: 13px; font-weight: 600; margin: 14px 0 5px; }
  .field input, .field select { width: 100%; border: 1px solid var(--line); border-radius: 9px; padding: 9px 11px; font: inherit; }
  .field input:focus { outline: 2px solid #cfd9ec; border-color: var(--brand); }
  .help { color: var(--muted); font-size: 12px; margin-top: 4px; }
  .agree { display: flex; gap: 9px; align-items: flex-start; background: #fafbfc; border: 1px solid var(--line); border-radius: 10px; padding: 10px 12px; font-size: 12.5px; color: var(--muted); margin-top: 16px; }
  .agree input { margin-top: 2px; }
  .msg { display: none; border-radius: 9px; padding: 9px 12px; font-size: 13px; margin-top: 12px; }
  .msg.err { display: block; background: #fdf1f0; color: #b3261e; border: 1px solid #f3c4c0; }
  .msg.lines { display: block; background: #f0faf6; color: var(--ok); border: 1px solid #bfe3d4; white-space: pre-line; }
  .actions { display: flex; gap: 8px; justify-content: flex-end; margin-top: 18px; }
  .actions .left { margin-right: auto; }
  footer { color: var(--muted); font-size: 12.5px; margin-top: 34px; padding-top: 14px; border-top: 1px solid var(--line); }
  a { color: var(--brand); }
</style>
</head>
<body>
<main>
  <div class="head"><div class="logo">s</div><h1>student-mcp</h1></div>
  <p class="sub">Ton serveur MCP local : connecte tes services étudiants, puis ton assistant IA (Claude&hellip;).</p>

  <h2>Services</h2>
  <div class="grid" id="services"></div>

  <h2>Connecter ton assistant IA</h2>
  <div class="panel">
    <div class="row"><span style="min-width:120px;color:var(--muted)">Local (ce PC)</span><code class="url" id="url-local">&hellip;</code><button data-copy="url-local">Copier</button></div>
    <div class="row"><span style="min-width:120px;color:var(--muted)">Public (Funnel)</span><code class="url" id="url-public">&hellip;</code><button data-copy="url-public">Copier</button><button id="funnel-btn">Exposer via Tailscale</button></div>
    <div id="funnel-msg" class="msg"></div>
    <p class="hint">Copie l'URL dans Claude&nbsp;: <b>Paramètres &rarr; Connecteurs &rarr; Ajouter un connecteur personnalisé</b>. Utilise l'URL publique pour l'app et les autres appareils.</p>
    <ol class="steps">
      <li>Active un service ci-haut avec tes identifiants.</li>
      <li>Copie l'URL du connecteur ci-dessus.</li>
      <li>Colle-la dans «&#8202;URL du serveur MCP&#8202;» et donne le nom que tu veux.</li>
    </ol>
  </div>

  <footer id="foot"></footer>
</main>

<div class="overlay" id="overlay">
  <div class="modal" role="dialog" aria-modal="true">
    <h3 id="m-title">Service</h3>
    <p class="tag" id="m-tag"></p>
    <div id="m-notes"></div>
    <div id="m-fields"></div>
    <label class="agree"><input type="checkbox" id="m-ack"> <span>J'utilise mes identifiants pour ce service via student-mcp, sur mon appareil. Je peux les retirer à tout moment et je respecte les conditions d'utilisation du service.</span></label>
    <div class="msg" id="m-msg"></div>
    <div class="actions">
      <button class="left danger" id="m-disable">Désactiver</button>
      <button id="m-cancel">Annuler</button>
      <button class="primary" id="m-save">Enregistrer</button>
    </div>
  </div>
</div>

<script>
  var token = new URLSearchParams(location.search).get('token') || '';
  var state = null;
  var current = null;

  function api(path) {
    var params = new URLSearchParams();
    if (token) params.set('token', token);
    var q = params.toString() ? (path.indexOf('?') >= 0 ? '&' : '?') + params.toString() : '';
    return path + q;
  }

  function post(path, body) {
    return fetch(api(path), { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(body || {}) });
  }

  function el(tag, className, text) {
    var node = document.createElement(tag);
    if (className) node.className = className;
    if (text !== undefined && text !== null) node.textContent = text;
    return node;
  }

  function copyText(value) {
    if (navigator.clipboard && navigator.clipboard.writeText) return navigator.clipboard.writeText(value);
    var area = el('textarea'); area.value = value; document.body.appendChild(area); area.select();
    document.execCommand('copy'); document.body.removeChild(area);
    return Promise.resolve();
  }

  function mark(style, text) {
    var node = el('div', 'mark', text);
    node.style.background = style;
    return node;
  }

  function render(data) {
    var grid = document.getElementById('services');
    grid.textContent = '';
    data.services.forEach(function (service) {
      var card = el('button', 'card');
      card.type = 'button';
      var flex = el('div');
      flex.style.cssText = 'display:flex;align-items:center;gap:8px;flex:1;min-width:0';
      flex.appendChild(el('div', 'name', service.name));
      flex.appendChild(el('span', 'pill' + (service.enabled ? ' on' : ''), service.enabled ? 'Actif' : 'Configurer'));
      card.appendChild(mark(service.accent, service.mark));
      card.appendChild(flex);
      card.appendChild(el('div', 'tag', service.tagline));
      card.onclick = function () { openModal(service); };
      grid.appendChild(card);
    });
    data.planned.forEach(function (service) {
      var card = el('div', 'card planned');
      var head = el('div');
      head.style.cssText = 'display:flex;align-items:center;gap:8px';
      head.appendChild(mark(service.accent, service.mark));
      head.appendChild(el('div', 'name', service.name));
      head.appendChild(el('span', 'pill', 'Plus tard'));
      card.appendChild(head);
      card.appendChild(el('div', 'tag', service.tagline));
      grid.appendChild(card);
    });

    document.getElementById('url-local').textContent = data.connector.localMcpUrl;
    var publicText = data.connector.publicMcpUrl || 'Pas encore exposé : Tailscale installe et exécute tailscale login, puis « Exposer via Tailscale »';
    document.getElementById('url-public').textContent = publicText;

    var funnel = data.connector.tailscale;
    var btn = document.getElementById('funnel-btn');
    btn.disabled = funnel.funnelOn;
    btn.textContent = funnel.funnelOn ? 'Funnel actif' : (funnel.present && funnel.loggedIn ? 'Exposer via Tailscale' : (funnel.present ? 'Exposer (connecte Tailscale)' : 'Exposer (installe Tailscale)'));

    var foot = document.getElementById('foot');
    foot.textContent = 'student-mcp v' + data.version + ' — port ' + data.gateway.port + ' — ton token est un secret : partage cette page uniquement avec toi.';
  }

  function refresh() {
    fetch(api('/api/status')).then(function (res) {
      if (!res.ok) throw new Error('Statut indisponible');
      return res.json();
    }).then(render).catch(function (error) {
      if (error && error.message) console.error(error);
    });
  }

  function openModal(service) {
    current = service;
    var fields = document.getElementById('m-fields');
    fields.textContent = '';
    document.getElementById('m-title').textContent = service.name;
    document.getElementById('m-tag').textContent = service.tagline;
    var notes = document.getElementById('m-notes');
    notes.textContent = '';
    (service.notes || []).forEach(function (note) {
      notes.appendChild(el('div', 'help', '· ' + note));
    });
    service.fields.forEach(function (field) {
      var wrap = el('div', 'field');
      var label = el('label', 'label', field.label);
      label.htmlFor = 'f-' + field.id;
      var input = el('input');
      input.id = 'f-' + field.id;
      input.type = field.type === 'url' ? 'url' : (field.type === 'password' ? 'password' : 'text');
      if (field.placeholder) input.placeholder = field.placeholder;
      if (field.required) input.setAttribute('required', 'required');
      input.dataset.id = field.id;
      wrap.appendChild(label);
      wrap.appendChild(input);
      if (field.help) wrap.appendChild(el('div', 'help', field.help));
      fields.appendChild(wrap);
    });
    if (service.allowWriteOptional) {
      var wrap = el('div', 'field');
      var label = el('label', 'label', 'Écriture');
      label.htmlFor = 'f-allowWrite';
      var input = el('input');
      input.id = 'f-allowWrite';
      input.type = 'checkbox';
      wrap.appendChild(label);
      wrap.appendChild(input);
      wrap.appendChild(el('div', 'help', service.writeLabel + ' (désactivé par défaut).'));
      fields.appendChild(wrap);
    }
    var ack = document.getElementById('m-ack');
    ack.checked = service.enabled;
    document.getElementById('m-disable').style.display = service.enabled ? 'inline-block' : 'none';
    setMsg('');
    document.getElementById('overlay').className = 'overlay show';
  }

  function setMsg(text, kind) {
    var area = document.getElementById('m-msg');
    area.className = text ? ('msg ' + kind) : 'msg';
    area.textContent = text || '';
  }

  function closeModal() {
    document.getElementById('overlay').className = 'overlay';
  }

  document.getElementById('m-cancel').onclick = closeModal;
  document.getElementById('overlay').onclick = function (event) {
    if (event.target === document.getElementById('overlay')) closeModal();
  };
  document.getElementById('m-save').onclick = function () {
    if (!current) return;
    var body = { ack: document.getElementById('m-ack').checked };
    var writeInput = document.getElementById('f-allowWrite');
    if (writeInput) body.allowWrite = writeInput.checked;
    document.querySelectorAll('#m-fields input[type=url], #m-fields input[type=text], #m-fields input[type=password], #m-fields input[type=checkbox]').forEach(function (input) {
      if (input.id === 'f-allowWrite') return;
      body[input.dataset.id || input.id.slice(2)] = input.type === 'checkbox' ? input.checked : input.value;
    });
    setMsg('Vérification du service en cours…', 'lines');
    post('/api/services/' + encodeURIComponent(current.id), body).then(async function (res) {
      var data = await res.json();
      if (!res.ok) { setMsg(data.error || 'Le service a refusé la connexion.', 'err'); return; }
      refresh();
      closeModal();
    }).catch(function () {
      setMsg('Erreur réseau avec le serveur local.', 'err');
    });
  };
  document.getElementById('m-disable').onclick = function () {
    if (!current) return;
    post('/api/services/' + encodeURIComponent(current.id), { action: 'disable' }).then(function () {
      closeModal(); refresh();
    });
  };
  document.querySelectorAll('button[data-copy]').forEach(function (btn) {
    btn.onclick = function () {
      copyText(document.getElementById(btn.dataset.copy).textContent).then(function () {
        btn.textContent = 'Copié'; setTimeout(function () { btn.textContent = 'Copier'; }, 1200);
      });
    };
  });
  document.getElementById('funnel-btn').onclick = function () {
    var btn = document.getElementById('funnel-btn');
    var area = document.getElementById('funnel-msg');
    area.className = 'msg';
    area.textContent = 'Activation du funnel en cours…';
    post('/api/funnel', { action: btn.disabled ? 'disable' : 'enable' }).then(function (res) { return res.json(); }).then(function (data) {
      area.className = data.ok ? 'msg lines' : 'msg err';
      area.textContent = data.message || (data.ok ? 'Funnel actif.' : 'Impossible pour le moment.');
      refresh();
    });
  };

  document.addEventListener('keydown', function (event) { if (event.key === 'Escape') closeModal(); });
  refresh();
</script>
</body>
</html>
`;
