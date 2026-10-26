# student-mcp — guide agents

Rust popup (egui) + MCP local pour Moodle, Omnivox et Brightspace.
Un dépôt = un commit. Secrets dans `~/.student-mcp.json`, jamais dans git.

## HTTP scolaire — ne pas refaire cette erreur

Les sites école (surtout `moodle.polymtl.ca`) sont derrière un **WAF AWS**.
Le User-Agent par défaut de **ureq** (`ureq/2.x`) reçoit :

```
HTTP 202
x-amzn-waf-action: challenge
Content-Type: text/html
body vide
```

L’UI affichait alors **« Moodle did not return a token. »** puis
**« Moodle did not provide a mobile token. »** alors que le login
n’avait même pas atteint `token.php`.

**Règle :** tout client HTTP vers Moodle / Omnivox / Brightspace doit
utiliser `session::BROWSER_UA` (`src/session.rs`). Ça inclut :

- login `token.php` (`token_login_agent`)
- appels Web Service (`agent`)
- téléchargements de fichiers (`download_agent`)

Ne pas « durcir » Moodle en retirant ce User-Agent. Omnivox et
Brightspace l’avaient déjà ; Moodle l’avait perdu dans
`feat: expand Moodle tools` — c’est ce qui a cassé un login qui
marchait.

Vérifier : `curl` sans UA Chrome → 202 vide ; avec `Mozilla/5.0 … Chrome/128` → JSON Moodle.

## Redirections

- **Login Moodle POST** (`token_login_agent`) : suivre les redirects
  (http→https, slash). C’était le comportement d’avant l’expansion Moodle.
- **WS / fichiers / GET token de secours** : `redirects(0)`. Un 307/308
  rejouerait le token ou le mot de passe sur un autre hôte.
- Réponse HTML/non-JSON sur le POST `token.php` : ce n’est pas un hard
  fail, enchaîner le fallback GET. Ne pas toaster
  « Moodle did not return a token. » (parse JSON).

## Layout

- `src/gui.rs` — fenêtre
- `src/moodle.rs` / `omnivox.rs` / `brightspace.rs` — connecteurs
- `src/session.rs` — cookies, formulaires HTML, `BROWSER_UA`
- `src/mcp.rs` / `http.rs` / `stdio.rs` — MCP
- `docs/GUIDE.md` — doc utilisateur

```bash
cargo test mobile_token
cargo test
```
