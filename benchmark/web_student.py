#!/usr/bin/env python3
"""Student-web benchmark: questions for Claude.ai and ChatGPT.com.

The tool harness (`run.py`) calls MCP directly. This file does not. It builds
precise questions from the live Moodle account, writes a local grading page,
and you paste the answers those two web apps actually gave.

    python3 benchmark/web_student.py

Nothing here talks to OpenAI/Anthropic. Results stay in ~/.student-mcp.eval/
(school names, never git).
"""
from __future__ import annotations

import argparse
import json
import sys
import time
import urllib.error
import urllib.request
from datetime import date
from pathlib import Path
from typing import Any

CDP = "http://127.0.0.1:9222"

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT))

from benchmark.harness import School, default_config, load_account  # noqa: E402
from benchmark.services import moodle as m  # noqa: E402

OUT_DIR = Path.home() / ".student-mcp.eval" / "web-student"
MONTHS_FR = {
    1: "janvier",
    2: "février",
    3: "mars",
    4: "avril",
    5: "mai",
    6: "juin",
    7: "juillet",
    8: "août",
    9: "septembre",
    10: "octobre",
    11: "novembre",
    12: "décembre",
}

TEMPLATE = r"""<!DOCTYPE html>
<html lang="fr">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Parcours étudiant — Claude web / ChatGPT web</title>
<style>
:root{--fg:var(--foreground,#e8e8ea);--muted:var(--muted-foreground,#9a9aa0);
--acc:var(--accent,#7aa2f7);--bd:var(--border,#2a2a32);--card:var(--card,#16161c);
--good:#4ade80;--warn:#fbbf24;--bad:#f87171}
*{box-sizing:border-box}
h1{font-size:1.2rem;font-weight:600;margin:0 0 .3rem}
h2{font-size:.72rem;font-weight:600;color:var(--muted);text-transform:uppercase;
letter-spacing:.08em;margin:1.4rem 0 .5rem}
small,.muted{color:var(--muted)}
ol.setup{margin:.4rem 0 0;padding-left:1.2rem;font-size:.85rem;line-height:1.55}
.card{border:1px solid var(--bd);border-radius:10px;padding:12px 14px;margin:10px 0;background:var(--card)}
.q{font-size:.95rem;font-weight:550;margin:0 0 8px}
.row{display:grid;grid-template-columns:1fr 1fr;gap:8px}
@media(max-width:700px){.row{grid-template-columns:1fr}}
label{font-size:.7rem;color:var(--muted);text-transform:uppercase;letter-spacing:.04em}
textarea{width:100%;min-height:110px;font:inherit;font-size:.82rem;padding:8px;
border:1px solid var(--bd);border-radius:8px;background:transparent;color:inherit;resize:vertical}
.bar{display:flex;gap:6px;flex-wrap:wrap;margin:6px 0 8px}
button{font:inherit;font-size:.75rem;padding:4px 10px;border-radius:999px;
border:1px solid var(--bd);background:transparent;color:inherit;cursor:pointer}
button:hover{border-color:var(--acc)}
.badge{display:inline-block;padding:1px 8px;border-radius:5px;font-size:.7rem;font-weight:600;margin-right:6px}
.PASS{color:var(--good)} .PARTIAL{color:var(--warn)} .FAIL{color:var(--bad)} .EMPTY{color:var(--muted)}
.kpis{display:grid;grid-template-columns:repeat(auto-fit,minmax(140px,1fr));gap:8px;margin:10px 0}
.kpi{border:1px solid var(--bd);border-radius:10px;padding:10px 12px}
.kpi b{display:block;font-size:1.3rem}
footer{font-size:.7rem;color:var(--muted);margin-top:16px}
</style>
</head>
<body>
<h1>Parcours étudiant</h1>
<small id="sub"></small>
<h2>Avant de coller une réponse</h2>
<ol class="setup">
<li>student-mcp tourne, lien public allumé (page locale → Copier le lien).</li>
<li>Claude.ai → Customize → Connectors → colle le lien MCP → nouveau chat, active student-mcp.</li>
<li>ChatGPT.com → Plugins → Add custom MCP server, sans auth → nouveau chat, <code>@student-mcp</code>.</li>
<li>Colle <em>uniquement</em> la question. Ne nomme pas l’outil. Une conv neuve par question.</li>
<li>Colle ici la réponse finale de chaque site, puis note.</li>
</ol>
<div class="kpis" id="kpis"></div>
<h2>Questions</h2>
<div id="list"></div>
<footer>Vérité = API Moodle du compte connecté. Ce fichier est local, hors git. Noter n’envoie rien.</footer>
<script>
const D = __DATA__;
const fold = (s) => (s||'').normalize('NFKD').replace(/\p{M}/gu,'').toLowerCase().replace(/[^a-z0-9]+/g,' ').trim();
const has = (hay, n) => n && fold(hay).includes(fold(n));
function grade(text, s) {
  if (!text || !fold(text)) return 'EMPTY';
  const need = (s.needles||[]).filter(Boolean);
  const any = (s.any||[]).filter(Boolean);
  const hit = need.filter(n => has(text, n)).length;
  const anyOk = !any.length || any.some(n => has(text, n));
  if (hit === need.length && anyOk) return 'PASS';
  if (hit > 0 || (any.length && anyOk)) return 'PARTIAL';
  return 'FAIL';
}
function score(side) {
  const vals = D.scenarios.map(s => grade(document.getElementById(side+s.id).value, s));
  const g = vals.filter(v => v!=='EMPTY');
  const p = g.filter(v => v==='PASS').length;
  return {p, g:g.length, n:D.scenarios.length, label: g.length ? Math.round(100*p/g)+' %' : '—'};
}
function renderKpis() {
  const c = score('c'), g = score('g');
  document.getElementById('kpis').innerHTML = `
    <div class="kpi"><span class="muted">Claude web</span><b>${c.label}</b><span class="muted">${c.p}/${c.g || c.n} notées</span></div>
    <div class="kpi"><span class="muted">ChatGPT web</span><b>${g.label}</b><span class="muted">${g.p}/${g.g || g.n} notées</span></div>
    <div class="kpi"><span class="muted">questions</span><b>${D.scenarios.length}</b><span class="muted">${D.date}</span></div>`;
}
function card(s, i) {
  return `<div class="card" data-id="${s.id}">
    <p class="q">${i+1}. ${s.question}</p>
    <div class="bar">
      <button data-copy="${s.id}">copier la question</button>
      <span class="badge EMPTY" id="bc${s.id}">Claude</span>
      <span class="badge EMPTY" id="bg${s.id}">ChatGPT</span>
    </div>
    <div class="row">
      <div><label>Claude.ai</label><textarea id="c${s.id}" placeholder="colle la réponse Claude"></textarea></div>
      <div><label>ChatGPT.com</label><textarea id="g${s.id}" placeholder="colle la réponse ChatGPT"></textarea></div>
    </div>
  </div>`;
}
document.getElementById('sub').textContent =
  D.scenarios.length + ' questions précises · même vérité Moodle · pas d’API modèle';
document.getElementById('list').innerHTML = D.scenarios.map(card).join('');
document.querySelectorAll('[data-copy]').forEach(b => b.addEventListener('click', () => {
  const s = D.scenarios.find(x => x.id===b.dataset.copy);
  navigator.clipboard.writeText(s.question);
  b.textContent = 'copié';
  setTimeout(()=>b.textContent='copier la question', 1200);
}));
function refresh() {
  D.scenarios.forEach(s => {
    const vc = grade(document.getElementById('c'+s.id).value, s);
    const vg = grade(document.getElementById('g'+s.id).value, s);
    const bc = document.getElementById('bc'+s.id), bg = document.getElementById('bg'+s.id);
    bc.className = 'badge '+vc; bc.textContent = 'Claude '+vc;
    bg.className = 'badge '+vg; bg.textContent = 'ChatGPT '+vg;
  });
  renderKpis();
}
document.querySelectorAll('textarea').forEach(t => t.addEventListener('input', refresh));
refresh();
</script>
</body>
</html>
"""


def _date_needles(ts: int) -> list[str]:
    local = time.localtime(ts)
    day = local.tm_mday
    month = MONTHS_FR[local.tm_mon]
    iso = time.strftime("%Y-%m-%d", local)
    return [
        iso,
        f"{day} {month}",
        f"{day:02d} {month}",
        f"{day} {month} {local.tm_year}",
    ]


def build_scenarios(facts: dict) -> list[dict]:
    rows: list[dict] = []
    academic = facts["academic"]
    codes = [row["code"] for row in academic if row["code"]]
    if codes:
        rows.append(
            {
                "id": "cours-session",
                "question": "Quels cours j'ai cette session ?",
                "needles": codes,
                "any": [],
            }
        )
    soon = facts["upcoming"][0] if facts["upcoming"] else None
    if soon:
        name = m._plain(soon.get("name") or "")
        rows.append(
            {
                "id": "prochain-devoir",
                "question": f"Quand est dû {name} de {soon['code']} ?",
                "needles": [soon["code"], name],
                "any": _date_needles(soon["duedate"]),
            }
        )
    heaviest = facts["heaviest"]
    if heaviest:
        code = heaviest["code"]
        td = m._pick_file(heaviest["files"], m.TD_RE, in_module=False)
        if td:
            rows.append(
                {
                    "id": "dernier-td",
                    "question": f"C'est quoi le dernier TD de {code} ?",
                    "needles": [code],
                    "any": [td["filename"], "TD"],
                }
            )
        plan = m._pick_file(heaviest["files"], m.PLAN_RE)
        if plan:
            rows.append(
                {
                    "id": "plan",
                    "question": f"Trouve le plan de cours de {code} et dis-moi le nom du fichier.",
                    "needles": [code],
                    "any": [plan["filename"], "plan de cours"],
                }
            )
        recent = next(
            (
                f
                for f in sorted(heaviest["files"], key=lambda item: item["mtime"], reverse=True)
                if f["mtime"] >= int(time.time()) - 21 * 86400
            ),
            None,
        )
        if recent:
            rows.append(
                {
                    "id": "fichier-recent",
                    "question": f"Trouve {recent['filename']} dans {code}.",
                    "needles": [recent["filename"]],
                    "any": [],
                }
            )
        graded = m._pick_graded(heaviest["grade_items"])
        if graded:
            expected = m._grade_str(graded).split()[0]
            rows.append(
                {
                    "id": "note-item",
                    "question": f"J'ai combien à {graded['itemname']} de {code} ?",
                    "needles": [graded["itemname"]],
                    "any": [expected],
                }
            )
        weighted = m._pick_weight(heaviest["grade_items"])
        if weighted and weighted.get("itemname"):
            rows.append(
                {
                    "id": "poids",
                    "question": f"Combien pèse {weighted['itemname']} dans {code} ?",
                    "needles": [weighted["itemname"], code],
                    "any": [
                        str(int(m._weight_val(weighted) or 0)),
                        (weighted.get("weightformatted") or "").replace("\xa0", " "),
                    ],
                }
            )
    other = next((row for row in academic if heaviest and row["id"] != heaviest["id"]), None)
    if other:
        corrigé = m._pick_file(other["files"], m.CORRIGE_RE, in_module=False)
        if corrigé:
            rows.append(
                {
                    "id": "corrige",
                    "question": f"Trouve {corrigé['filename']}.",
                    "needles": [corrigé["filename"]],
                    "any": [],
                }
            )
    return rows[:8]


def cdp_get(path: str) -> Any:
    req = urllib.request.Request(CDP + path, headers={"Host": "127.0.0.1:9222"})
    with urllib.request.urlopen(req, timeout=3) as response:
        return json.loads(response.read())


def cdp_probe() -> int:
    """See if Helium was restarted with the debug port."""
    try:
        version = cdp_get("/json/version")
        pages = cdp_get("/json/list")
    except (urllib.error.URLError, TimeoutError, json.JSONDecodeError) as exc:
        print("Helium n'écoute pas sur 127.0.0.1:9222.", file=sys.stderr)
        print("Ferme Helium, relance-le, reconnecte Claude.ai et ChatGPT.com, puis redis « prêt ».", file=sys.stderr)
        print(f"({type(exc).__name__})", file=sys.stderr)
        return 2
    browser = (version.get("Browser") or "?")
    print(f"CDP ok · {browser} · {len(pages)} onglet(s)")
    for page in pages:
        url = page.get("url") or ""
        title = page.get("title") or ""
        kind = page.get("type") or ""
        if kind != "page":
            continue
        mark = ""
        if "claude.ai" in url:
            mark = " [claude]"
        elif "chatgpt.com" in url:
            mark = " [chatgpt]"
        print(f"  {title[:60]}{mark}")
        print(f"    {url[:90]}")
    claude = any("claude.ai" in (p.get("url") or "") for p in pages)
    gpt = any("chatgpt.com" in (p.get("url") or "") for p in pages)
    if not claude or not gpt:
        print("Ouvre un onglet Claude.ai ET un onglet ChatGPT.com, déjà connectés, MCP actif.", file=sys.stderr)
        return 1
    print("Les deux sites sont ouverts. Je peux enchaîner les questions.")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cdp", action="store_true", help="probe Helium on 127.0.0.1:9222")
    args = parser.parse_args()
    if args.cdp:
        return cdp_probe()

    account = load_account(default_config(), "moodle")
    if not account:
        print("no Moodle account; connect one in student-mcp first", file=sys.stderr)
        return 2
    facts = m.inspect_account(School(account), int(time.time()))
    scenarios = build_scenarios(facts)
    if not scenarios:
        print("no scenarios from this account", file=sys.stderr)
        return 1
    payload = {"date": date.today().isoformat(), "scenarios": scenarios}
    OUT_DIR.mkdir(parents=True, exist_ok=True)
    (OUT_DIR / "run.json").write_text(json.dumps(payload, ensure_ascii=False, indent=2))
    page = OUT_DIR / "index.html"
    page.write_text(TEMPLATE.replace("__DATA__", json.dumps(payload, ensure_ascii=False)))
    print(f"{len(scenarios)} questions → {page}")
    print()
    for index, row in enumerate(scenarios, 1):
        print(f"{index}. {row['question']}")
    print()
    print("Ouvre la page, copie chaque question dans un chat neuf Claude puis ChatGPT, colle les réponses.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
