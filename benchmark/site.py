#!/usr/bin/env python3
"""Render the benchmark as one self-contained HTML page.

    python3 benchmark/site.py          # -> benchmark/site/index.html

No build step, no dependency, no network: the results already produced by
`benchmark/run.py` are embedded as JSON, and the page works from file://.
The generated file is gitignored: it contains live school answers.
"""
from __future__ import annotations

import json
import sys
from datetime import date
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
RESULTS = ROOT / "benchmark" / "results"
OUT = ROOT / "benchmark" / "site" / "index.html"

TEMPLATE = r"""<!DOCTYPE html>
<html lang="fr">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>MCP benchmark</title>
<style>
:root{--fg:var(--foreground,#e8e8ea);--muted:var(--muted-foreground,#9a9aa0);
--acc:var(--accent,#7aa2f7);--bd:var(--border,#2a2a32);--card:var(--card,#16161c);
--good:#4ade80;--warn:#fbbf24;--bad:#f87171;--idle:#52525b}
*{box-sizing:border-box}
h1{font-size:1.25rem;font-weight:600;letter-spacing:-.02em;margin:0 0 .2rem}
h2{font-size:.72rem;font-weight:600;color:var(--muted);text-transform:uppercase;
letter-spacing:.08em;margin:1.6rem 0 .6rem}
small,.muted{color:var(--muted)}
.kpis{display:grid;grid-template-columns:repeat(auto-fit,minmax(120px,1fr));gap:8px;margin:14px 0}
.kpi{border:1px solid var(--bd);border-radius:10px;padding:10px 12px;background:var(--card)}
.kpi b{display:block;font-size:1.35rem;font-weight:600;letter-spacing:-.02em}
.kpi span{font-size:.7rem;color:var(--muted)}
.delta{color:var(--good)}
.bars{display:flex;align-items:flex-end;gap:6px;height:96px;padding:4px 0 2px;border-bottom:1px solid var(--bd)}
.bar{flex:1;display:flex;flex-direction:column;justify-content:flex-end;align-items:center;gap:3px;min-width:0}
.bar i{width:100%;border-radius:4px 4px 0 0;background:var(--idle);display:block}
.bar.hot i{background:var(--good)}
.bar span{font-size:.65rem;color:var(--muted);white-space:nowrap}
.bar b{font-size:.72rem;font-weight:600}
table{border-collapse:collapse;width:100%;font-size:.78rem;font-variant-numeric:tabular-nums}
th,td{text-align:right;padding:6px 7px;border-bottom:1px solid var(--bd);white-space:nowrap}
th:first-child,td:first-child{text-align:left}
th{color:var(--muted);font-weight:500;font-size:.68rem;text-transform:uppercase;letter-spacing:.04em}
tr.hi td{color:var(--good)}
.controls{display:flex;flex-wrap:wrap;gap:6px;align-items:center;margin:12px 0 8px}
select,input{font:inherit;font-size:.78rem;padding:5px 8px;border:1px solid var(--bd);
border-radius:7px;background:transparent;color:inherit}
input{min-width:160px;flex:1}
.chip{cursor:pointer;user-select:none;border:1px solid var(--bd);background:transparent;
color:var(--muted);padding:4px 10px;border-radius:999px;font-size:.72rem}
.chip.on{border-color:var(--acc);color:inherit}
.q{display:grid;grid-template-columns:28px minmax(0,1fr) 58px minmax(80px,140px) 56px 48px;
gap:8px;align-items:baseline;padding:7px 0;border-bottom:1px solid var(--bd);
font-size:.8rem;cursor:pointer}
.q:hover{background:color-mix(in srgb,var(--acc) 6%,transparent)}
.q .tool{color:var(--muted);font-size:.72rem;overflow:hidden;text-overflow:ellipsis}
.q .num{color:var(--muted);text-align:right;font-variant-numeric:tabular-nums}
.ev{display:none;grid-column:2/-1;color:var(--muted);font-size:.75rem;padding:2px 0 4px}
.q.open .ev{display:block}
.q.changed{outline:1px solid color-mix(in srgb,var(--acc) 35%,transparent);outline-offset:2px;border-radius:4px}
.badge{justify-self:start;padding:1px 7px;border-radius:5px;font-size:.65rem;font-weight:600}
.PASS{background:color-mix(in srgb,var(--good) 18%,transparent);color:var(--good)}
.PARTIAL{background:color-mix(in srgb,var(--warn) 18%,transparent);color:var(--warn)}
.FAIL{background:color-mix(in srgb,var(--bad) 16%,transparent);color:var(--bad)}
.SKIP{background:var(--card);color:var(--muted)}
.legend,footer{font-size:.7rem;color:var(--muted);margin-top:8px}
</style>
</head>
<body>
<h1>MCP benchmark</h1>
<small id="sub"></small>
<div class="kpis" id="kpis"></div>
<h2>Score par release</h2>
<div class="bars" id="bars"></div>
<div class="legend">Hauteur = temps total. Le chiffre = score. Un gain de score qui allonge le temps n'en est pas un.</div>
<h2>Détails par release</h2>
<table id="table"></table>
<h2>Les questions</h2>
<div class="controls">
<select id="version"></select>
<button class="chip on" data-f="">Toutes</button>
<button class="chip" data-f="PASS">PASS</button>
<button class="chip" data-f="PARTIAL">PARTIAL</button>
<button class="chip" data-f="FAIL">FAIL</button>
<button class="chip" data-f="SKIP">SKIP</button>
<button class="chip" id="gaps">écarts vs précédente</button>
<input id="find" placeholder="filtrer une question ou un outil">
</div>
<div id="questions"></div>
<footer id="foot"></footer>
<script>
const D = __DATA__;
const vers = D.versions;
const cur = vers[vers.length - 1];
const base = vers.find(v => v.version === 'v0.8.0') || vers[0];
const money = (n) => Number(n || 0).toLocaleString('fr-FR');
const esc = (s) => String(s ?? '').replace(/[&<>"']/g, c =>
  ({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]));
const set = (id, txt) => { document.getElementById(id).textContent = txt; };

document.getElementById('sub').textContent =
  `${D.questions} questions · ${vers.length} releases · Moodle réel vs API école · ${D.date}`;

const kpis = [
  ['score', cur.score + ' %', (cur.score - base.score) + ' pts vs 0.8.0'],
  ['temps total', cur.seconds_total.toFixed(1) + ' s', base.seconds_total.toFixed(1) + ' s en 0.8.0'],
  ['tokens lus', money(cur.tokens_total), money(base.tokens_total) + ' en 0.8.0'],
  ['tools/list', money(Math.round(cur.tools_bytes / 4)) + ' tok', cur.tools_count + ' outils'],
  ['binaire', (cur.binary_bytes / 1e6).toFixed(1) + ' Mo', ''],
  ['src MCP', Math.round(cur.source_bytes / 1024) + ' Ko', 'mcp.rs + services'],
];
document.getElementById('kpis').innerHTML = kpis.map(([k, v, d]) =>
  `<div class="kpi"><span>${esc(k)}</span><b>${esc(v)}</b><span class="delta">${esc(d || '\u00a0')}</span></div>`
).join('');

const maxT = Math.max(...vers.map(v => v.seconds_total)) || 1;
document.getElementById('bars').innerHTML = vers.map(v => `
  <div class="bar ${v === cur ? 'hot' : ''}" title="${esc(v.version)}: ${esc(v.score)} % · ${esc(v.seconds_total)}s">
    <b>${esc(v.score)}%</b>
    <i style="height:${Math.max(4, v.seconds_total / maxT * 68)}px"></i>
    <span>${esc(v.version.replace('v',''))}</span>
  </div>`).join('');

const cols = [
  ['version', 'release'], ['score', 'score'], ['PASS', 'PASS'], ['PARTIAL', 'PART'],
  ['FAIL', 'FAIL'], ['SKIP', 'SKIP'], ['seconds_total', 'temps'], ['seconds_median', 'méd'],
  ['tokens_median', 'tok/q'], ['tokens_total', 'tokens'], ['tools_bytes', 'tools/list'],
  ['binary_bytes', 'binaire'], ['source_bytes', 'src MCP'],
];
const fmt = (k, v) => {
  if (v == null) return '–';
  if (k === 'score') return v + ' %';
  if (k === 'seconds_total') return Number(v).toFixed(1) + ' s';
  if (k === 'seconds_median') return Number(v).toFixed(2) + ' s';
  if (k === 'binary_bytes') return (v / 1e6).toFixed(1) + ' Mo';
  if (k === 'source_bytes') return Math.round(v / 1024) + ' Ko';
  if (k === 'tools_bytes') return money(Math.round(v / 4)) + ' tok';
  return money(v);
};
document.getElementById('table').innerHTML =
  '<tr>' + cols.map(c => `<th>${c[1]}</th>`).join('') + '</tr>' +
  vers.map(v => `<tr class="${v === cur ? 'hi' : ''}">` +
    cols.map(([k]) => `<td>${esc(k === 'version' ? v.version : fmt(k, v[k]))}</td>`).join('') +
  '</tr>').join('');

const sel = document.getElementById('version');
sel.innerHTML = vers.map(v => `<option value="${esc(v.version)}">${esc(v.version)}</option>`).reverse().join('');
sel.value = cur.version;
let filter = '', gaps = false, text = '';
const prevOf = (name) => {
  const i = vers.findIndex(v => v.version === name);
  return i > 0 ? vers[i - 1] : null;
};

function render() {
  const v = vers.find(x => x.version === sel.value) || cur;
  const prev = prevOf(v.version);
  const before = prev ? Object.fromEntries(prev.rows.map(r => [r.question, r.verdict])) : {};
  const rows = v.rows.filter(r =>
    (!filter || r.verdict === filter) &&
    (!gaps || (prev && before[r.question] !== r.verdict)) &&
    (!text || (r.question + ' ' + r.tool + ' ' + (r.evidence || '')).toLowerCase().includes(text))
  );
  document.getElementById('questions').innerHTML = rows.map(r => {
    const changed = before[r.question] && before[r.question] !== r.verdict;
    const ev = (r.evidence || '') + (changed ? ' — avant : ' + before[r.question] : '');
    const verdict = ['PASS','PARTIAL','FAIL','SKIP'].includes(r.verdict) ? r.verdict : 'SKIP';
    return `<div class="q ${changed ? 'changed' : ''}">
      <span class="num">${esc(r.n)}</span>
      <span>${esc(r.question)}<span class="ev">${esc(ev)}</span></span>
      <span class="badge ${verdict}">${verdict}</span>
      <span class="tool">${esc(r.tool)}</span>
      <span class="num">${money(r.bytes)} o</span>
      <span class="num">${Number(r.seconds || 0).toFixed(2)}s</span>
    </div>`;
  }).join('') || `<p class="muted">aucune question ne correspond</p>`;
  set('foot',
    `${rows.length} question(s) · ${esc(v.score)} % · ${Number(v.seconds_total).toFixed(1)} s · ` +
    `${money(v.tokens_total)} tokens` +
    (v.slowest ? ' · plus lent : ' + esc(v.slowest) : '') +
    (v.heaviest ? ' · plus lourd : ' + esc(v.heaviest) : '')
  );
}
sel.addEventListener('change', render);
document.querySelectorAll('[data-f]').forEach(b => b.addEventListener('click', () => {
  document.querySelectorAll('[data-f]').forEach(x => x.classList.remove('on'));
  b.classList.add('on');
  filter = b.dataset.f;
  render();
}));
document.getElementById('gaps').addEventListener('click', e => {
  gaps = !gaps;
  e.currentTarget.classList.toggle('on', gaps);
  render();
});
document.getElementById('find').addEventListener('input', e => {
  text = e.target.value.toLowerCase();
  render();
});
document.getElementById('questions').addEventListener('click', e => {
  const q = e.target.closest('.q');
  if (q) q.classList.toggle('open');
});
render();
</script>
</body>
</html>
"""


def collect() -> list[dict]:
    versions: list[dict] = []
    for path in sorted(RESULTS.glob("*.json")):
        if path.name.endswith(".metrics.json"):
            continue
        version = path.stem
        rows = json.loads(path.read_text())
        if not isinstance(rows, list) or not rows:
            continue
        if not all(isinstance(row, dict) and "verdict" in row for row in rows):
            continue
        metrics_path = RESULTS / f"{version}.metrics.json"
        if not metrics_path.exists():
            continue
        metrics = json.loads(metrics_path.read_text())
        counts = {
            key: sum(1 for row in rows if row.get("verdict") == key)
            for key in ("PASS", "PARTIAL", "FAIL", "SKIP")
        }
        graded = counts["PASS"] + counts["PARTIAL"] + counts["FAIL"]
        metrics.update(counts)
        metrics["score"] = round(100 * counts["PASS"] / graded) if graded else 0
        metrics["version"] = version
        metrics["rows"] = rows
        versions.append(metrics)
    order = {
        name: index
        for index, name in enumerate(
            [
                "v0.6.2",
                "v0.6.4",
                "v0.6.6",
                "v0.6.9",
                "v0.8.0",
                "v0.8.1",
                "v0.8.3",
                "v0.8.4",
                "current",
            ]
        )
    }
    versions.sort(key=lambda v: order.get(v["version"], 99))
    return versions


def main() -> int:
    versions = collect()
    if not versions:
        print("no results; run python3 benchmark/run.py first", file=sys.stderr)
        return 1
    current = next((v for v in versions if v["version"] == "current"), versions[-1])
    payload = {
        "versions": versions,
        "questions": len(current["rows"]),
        "date": date.today().isoformat(),
    }
    OUT.parent.mkdir(parents=True, exist_ok=True)
    serialized = json.dumps(payload, ensure_ascii=False)
    # The payload contains school-controlled text. Prevent it from closing the
    # inline script element or introducing JavaScript line separators.
    serialized = (
        serialized.replace("<", "\\u003c")
        .replace("\u2028", "\\u2028")
        .replace("\u2029", "\\u2029")
    )
    OUT.write_text(TEMPLATE.replace("__DATA__", serialized))
    print(f"wrote {OUT.relative_to(ROOT)} ({OUT.stat().st_size // 1024} Ko, {len(versions)} releases)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
