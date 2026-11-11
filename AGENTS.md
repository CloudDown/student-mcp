# student-mcp — guide agents

Backend Rust + interface web locale + MCP pour Moodle, Omnivox et Brightspace.
Un dépôt = un commit. Secrets dans `~/.student-mcp.json`, jamais dans git.

## Mémoire de travail

Le contexte de la session sur les documents, la recherche Moodle et les
optimisations internes est conservé dans [`tools/SESSION-MEMORY.md`](tools/SESSION-MEMORY.md).
Le lire pour reprendre ces travaux : décisions, contrats, preuves, limites et
état des changements y sont regroupés. L’état Git indiqué est historique ;
revérifier le dépôt avant toute modification.

- Priorité utilisateur : retrouver les informations vite et éviter de confondre
  aperçu/extrait avec lecture complète. Aucun couplage à Polytechnique.
- Optimisations demandées : mêmes entrées/sorties, classement, erreurs et
  pagination ; modifications internes mesurées, sans changement visuel ou MCP.
- `src/services/moodle/research/performance.rs` conserve des empreintes de sorties
  capturées **avant** optimisation. Ne pas les remplacer pour faire passer une
  régression. Méthode et mesures : `tools/PERFORMANCE.md`.
- `coverage` décrit des données transmises/recherchées, jamais une garantie de
  compréhension du modèle. Pas d’OCR local ou distant ; préparation documentaire
  locale, conversion Office isolée, aucun repli de conversion non isolé.

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

## Connexion par navigateur persistant

Brightspace, Omnivox et Canvas peuvent stocker une session dans un profil Chromium
propre au compte (`src/browser.rs`). Le jeton enregistré est
`browser-profile:<32 hex>`, jamais un cookie ou un mot de passe. Le profil vit
à côté de la configuration (`~/.student-mcp.browsers/`, mode `0700` sur Unix) et
ne doit jamais entrer dans git.

- Seule l’interface locale ouvre une fenêtre visible (`Login::start`). Un appel
  MCP reprend le profil en headless, puis le referme à la fin de l’outil.
- Le verrou `profile.lock` sérialise les processus. Ne pas le retirer : deux
  Chrome sur le même `--user-data-dir` corrompent la session.
- Les requêtes navigateur sont limitées à l’origine du compte et à
  `/d2l/api/`, `/Mobl/`, Canvas `/api/v1/` `/files/`. Pas de redirection suivie, pas de rejeu automatique
  d’une écriture, taille de réponse bornée.
- Brightspace : utiliser le jeton CSRF fourni par la page
  (`D2L…GetXsrfToken`, `XSRF.Token`). Ne pas en fabriquer un.
- Omnivox : le pont `src/resources/browser/omnivox.js` ne s’injecte que sur un
  hôte `*.omnivox.ca` en HTTPS. Il doit garder l’User-Agent mobile, les
  callbacks `…CallBack` et la persistance `device`/`code`. Les lectures passent
  par `Skytech.Commun.Utils.HttpRequestWorker.PostJSON`, pas par un cookie de
  session web exporté.
- Un login annulé ou échoué supprime le profil seulement s’il vient d’être
  créé ; une reconnexion ne doit jamais effacer la session existante.
- Ne jamais journaliser ni renvoyer le jeton de profil. Les tests navigateur
  locaux n’utilisent aucun identifiant réel : voir `tools/BROWSER-AUTH.md`.

## Configuration des assistants locaux

`src/assistants.rs` écrit dans la configuration de Claude Desktop
(`claude_desktop_config.json`) ou de Gemini CLI (`~/.gemini/settings.json`)
seulement après **Set up on this computer** puis **Confirm setup** dans l’UI.

- Fusionner `mcpServers.student-mcp` sans supprimer les autres serveurs.
- Sauvegarder le fichier existant avant toute écriture, puis remplacer
  atomiquement (fichier temporaire `0600`, permissions conservées).
- Refuser un fichier qui n’est pas un objet JSON, un lien symbolique ou un
  `mcpServers` invalide ; ne jamais l’écraser.
- Ne jamais écrire d’identifiant scolaire, de cookie ni de jeton MCP. Le local
  passe par stdio ; le web utilise le lien public.
- `status()` est en lecture seule ; `configure()` ajoute `student-mcp`, `deconfigure()`
  le retire. Les deux sauvegardent le fichier et ne touchent pas aux autres serveurs.
- ChatGPT n’a pas de configuration locale : son écran explique le lien HTTPS
  public et ouvre les réglages officiels.

## Graphe de code local (Graphify)

Pour une exploration transversale, si `graphify-out/graph.json` existe, commencer
par `graphify query "<symboles ou question>"` ou `graphify explain "<symbole>"`.
Lire ensuite les sources indiquées pour confirmer ; le graphe AST ne prouve pas
les appels dynamiques, les macros Rust ou les échanges Rust ↔ JavaScript.
Une recherche ciblée reste adaptée si le fichier est déjà connu.

Construire ou rafraîchir après des changements : `sh tools/graphify.sh`.
Extraction locale du code et du workspace Cargo, sans LLM. Le catalogue Moodle
vendu avec le projet et les images sont exclus ; les sorties restent hors git.
Si Graphify est absent, utiliser les recherches habituelles.
Installation et exemples : `tools/GUIDE.md`, section « Local code graph ».

## Layout

- `src/web.rs` / `src/resources/web/` — configuration web locale (hors tunnel) : `index.html` (coquille), `app.css`, `app.js`
- `src/assistants.rs` — configuration opt-in de Claude Desktop / Gemini CLI (fusion JSON, sauvegarde)
- `src/browser.rs` — profils Chromium par compte, CDP, login interactif
- `src/resources/browser/` — pont mobile Omnivox et requête navigateur
- `src/services/{moodle,omnivox,brightspace}/mod.rs` — connecteurs par service
- `src/services/canvas.rs` — Canvas / ZoneCours, session navigateur, lecture seule
- `src/services/moodle/download.rs` — cache de téléchargements Moodle
- `src/services/moodle/research.rs` / `index.rs` — recherche de métadonnées Moodle,
  consignes paginées et index privé par compte (`~/.student-mcp.index/`, hors git).
  L’index ne contient pas les octets des fichiers ; ne pas présenter ses extraits
  comme une lecture complète. `refresh=true` ne doit pas masquer une panne avec
  une ancienne réponse. Les lectures ne doivent jamais accuser réception d’un document.
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
