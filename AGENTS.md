# student-mcp — guide agents

Backend Rust + interface web locale + MCP pour Moodle, Omnivox et Brightspace.
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

## Cookies Omnivox et validation par courriel

Le parcours MFA supprime le cookie temporaire `omt` avec une valeur **non vide**
et `Expires=Thu, 01-Jan-1970 05:00:00 GMT`. Il ne faut pas le renvoyer : cela
provoque un **401 immédiat** sur `/api/mfa/login/validate-method/{id}`, même
avec des identifiants acceptés et une session neuve.

Le login Omnivox utilise `cookie_store::CookieStore` pour respecter les
expirations, suppressions, domaines et chemins. Ne pas le remplacer par une
simple map nom/valeur. Un refus HTTP 401/403 ne prouve pas une expiration ;
réserver ce message au résultat explicite `SessionExpired` de l'API.

## Session mobile Omnivox (`/Mobl/`)

`/Mobl/` **refuse une session web** obtenue sur `/Login/Account/Login` : elle
répond `302 /Mobl/Login/AuthNeeded`. C’est l’erreur
« Omnivox redirected the request ». Le connecteur doit donc se comporter comme
l’app mobile officielle de bout en bout :

- ouvrir `/intr/Module/Identification/Login/Mobile/LoginMobile.aspx?typeUtilisateur=Etudiant`
  et poster ce formulaire (et non le formulaire web Identifiant/Password) ;
- envoyer l’User-Agent `OVX InfoDevice=… AppVer=3.8.9 IdAppareil=<40 hex> Code=<AuthHash>`
  et l’en-tête `X-User-Agent-Ovx` sur le login, le MFA et chaque appel `/Mobl/` ;
- poser les cookies `ForceWeb=false` et `IndicateurNouveauLogin=true` avant le
  POST de login ;
- après le code MFA, suivre `reply.url` (le token peut être dans le fragment
  `#…Token=…`, que le serveur ne voit pas), puis appeler
  `/Mobl/App/Initialisation` pour obtenir `AuthHash` et prouver que la session
  mobile est valide ;
- persister `device_id` et `auth_hash` avec les cookies : le serveur peut lier
  la session à l’identité mobile. Ne jamais persister mot de passe ni code.

Le cookie `olt` s’écrit avec `Domain=omnivox.ca`, comme le JS MFA officiel,
pour couvrir un handoff vers un autre sous-domaine.

## Layout

- `src/web.rs` / `src/resources/web/index.html` — configuration web locale (hors tunnel)
- `src/services/{moodle,omnivox,brightspace}/mod.rs` — connecteurs par service
- `src/services/moodle/download.rs` — cache de téléchargements Moodle
- `src/services/moodle/catalog.rs` / `catalog/` — catalogue des fonctions Moodle et licences
- `src/session.rs` — cookies, formulaires HTML, `BROWSER_UA`
- `src/mcp.rs` / `http.rs` / `stdio.rs` — MCP
- `src/resources/` — interface et icônes embarquées
- `tools/` — scripts, tests de distribution et modèles d’installation
- `tools/GUIDE.md` — guide utilisateur et maintenance

```bash
cargo test mobile_token
cargo test
```
