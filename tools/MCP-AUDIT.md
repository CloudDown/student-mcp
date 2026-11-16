# MCP quality audit

`tools/mcp-audit.py` rejoue de vraies demandes étudiantes contre le serveur MCP
stdio, puis compare ce que les outils renvoient avec ce que l'API de l'école
contient réellement. Il mesure les trois échecs qui comptent :

1. **charge utile illisible** — octets et tokens qu'un modèle doit avaler avant
   de répondre ;
2. **donnée cachée** — ce que l'outil omet alors que l'école le fournit ;
3. **bruit** — erreurs scolaires brutes et champs de contrôle dans la réponse.

Ce n'est pas un test de contrat : les tests Rust garantissent la forme, cet
audit mesure l'utilité. Il faut un compte Moodle connecté.

## Lancer

```bash
python3 tools/mcp-audit.py                  # target/release/student-mcp
python3 tools/mcp-audit.py --bin target/debug/student-mcp
python3 tools/mcp-audit.py --write-baseline # fige la référence
python3 tools/mcp-audit.py --baseline ~/.student-mcp.eval/baseline.json
python3 tools/mcp-audit.py --offline        # sans compte, sans vérité terrain
```

Code de sortie `1` dès qu'un constat `high` ou `blocker` apparaît : utilisable
comme garde avant un commit. Rapports dans `~/.student-mcp.eval/` (hors dépôt).
Le jeton scolaire est lu dans `~/.config/student-mcp/config.json` (ou l'ancien `~/.student-mcp.json`) et n'est jamais affiché.

## Ce qu'il vérifie

| Mesure | Source | Signal |
|--------|--------|--------|
| octets / tokens par appel | réponse MCP | > 12 000 octets = le modèle ne lira pas tout |
| part des champs de contrôle | `coverage`, `note`, `hints`, `continuation` | > 0,3 = la réponse parle d'elle-même plus que du cours |
| `nextPage` obligatoire | réponse | première page ≠ contenu complet |
| latence par appel | horloge, session isolée | > 5 s = outil abandonné par l'assistant |
| sections / activités / fichiers | `core_course_get_contents` | écart = donnée perdue avant le modèle |
| notes par cours | `gradereport_overview_get_course_grades` | écart = omission non annoncée |
| outils et `tools/list` | `tools/list` | croissance = choix d'outil plus dur |

Le cours audité est le cours de la session en cours qui a **le plus de
fichiers** : c'est le cas où un assistant se perd.

## Les quatre portes

Rien ne doit être jugé sur une seule d'entre elles :

| porte | commande | ce qu'elle prouve |
|---|---|---|
| contrat | `cargo test` | la forme, les erreurs, les cas limites |
| transport + UI | `python3 tools/smoke-web.py target/release/student-mcp` | HTTP local, sessions, arrêt |
| surface d'outils | `python3 tools/mcp-audit.py` | descriptions, annotations, taille de `tools/list`, charges utiles, fidélité, latence |
| compréhension | `python3 tools/mcp-audit.py --intents` | 20 demandes réelles notées PASS/PARTIAL/FAIL |

La porte « surface » vérifie pour chaque outil : `readOnlyHint` présent,
`destructiveHint` sur les écritures, pas de nom dupliqué, description ≥ 60
caractères, **chaque paramètre décrit**, `tools/list` sous 25 000 octets. Un
paramètre sans description oblige le modèle à deviner son unité, et un mauvais
guess coûte un aller-retour.

## Règle anti-inflation

Une amélioration n'entre que si elle répond à une question qui échoue :

1. une ligne de `--intents` passe de PASS à FAIL, ou un constat `high` apparaît ;
2. on corrige **sans ajouter d'outil** ; un paramètre de plus seulement si la
   question ne peut pas être répondue autrement ;
3. le score remonte et aucun constat n'est ajouté ;
4. `--write-baseline` fige le nouvel état.

Si un changement fait grossir `tools/list` ou une réponse de plus de 25 % sans
faire remonter le score, il est refusé. C'est ce qui empêche d'empiler des modes
comme la session `backend/mcp` l'avait fait.

## Constats du 2026-11-15 (v0.8.4, compte réel)

Cours audité : COURS101 — 14 sections, 214 activités, 235 fichiers.

| outil | octets | ~tokens | contrôle | latence |
|-------|--------|---------|----------|---------|
| `moodle_search` | 45 748 | 11 437 | 0,02 | 0,0 s |
| `moodle_course_content` | 43 187 | 10 796 | 0,02 | 0,1 s |
| `moodle_assignments` | 24 315 | 6 078 | 0,15 | 9,9 s |
| `moodle_recent_grades` | 8 405 | 2 101 | 0,07 | 3,3 s |

- Une seule recherche pèse ~11 400 tokens pour **un** cours. Le brief de la
  conversation `backend/mcp` (« trop de champs de contrôle ») est faux sur ce
  point : les champs de contrôle représentent 2 %. Le poids vient du **volume
  par activité** : ~1 350 octets par activité, dont `groupIds`, `groupingIds`,
  `links`, `uservisible`, `inlineContentAvailable`, `richContentOmitted` le plus
  souvent vides.
- `moodle_course_content` s'arrête à 6 sections sur 14 : sans suivre
  `nextPage`, l'assistant ne voit ni les fichiers des sections suivantes ni
  leurs 235 fichiers.
- `moodle_grades` annonce 1 cours noté ; le carnet en rapporte 5. Le filtrage
  des cours passés est un choix défendable, mais la réponse ne le dit pas.
- `moodle_assignments` prend ~10 s : c'est l'outil le plus lent du parcours
  « qu'est-ce qui est dû ».

## Boucle d'amélioration

1. `--write-baseline` avant de toucher un outil.
2. Modifier le serveur.
3. `--baseline` : aucun nouveau `high`, aucune charge utile qui gonfle de plus
   de 25 %.
4. Relire le tableau, pas seulement le code de sortie : un gain de tokens qui
   fait disparaître un fichier est une régression.

## Limites

- Les tokens sont estimés (`octets / 4`), pas comptés par le client.
- L'audit ne prouve pas ce que Claude, ChatGPT ou Grok affichent : il prouve ce
  que le serveur envoie.
- Il ne juge pas la **pertinence** d'un résultat (bon TD vs TD d'un autre
  groupe) : c'est le rôle des questions de référence ci-dessous.
- Un compte Moodle est nécessaire pour la vérité terrain ; en `--offline` les
  comparaisons disparaissent.

## Vingt questions étudiantes

```bash
python3 tools/mcp-audit.py --intents
```

Chaque question est une demande réelle. `PASS` = les outils portent assez
d'information juste pour répondre ; `PARTIAL` = une réponse plausible serait
incomplète ; `FAIL` = le modèle doit deviner ; `SKIP` = ce compte n'a pas la
donnée (forum absent, achèvement désactivé…), ce n'est pas un défaut.

| # | Question | Outil | Ce que ça teste |
|---|----------|-------|-----------------|
| 1 | Quels cours j'ai cette session ? | `moodle_my_courses` | filtrage annoncé |
| 2 | Qu'est-ce que je dois remettre cette semaine ? | `moodle_deadlines` | dates exactes |
| 3 | Qu'est-ce qui est déjà en retard ? | `include_expired` | échus vs à venir |
| 4 | C'est quoi le dernier TD de mon cours ? | `moodle_search` | rang du bon fichier |
| 5 | Donne-moi le corrigé des exercices | `moodle_search` | vocabulaire réel |
| 6 | Trouve le plan de cours | `moodle_search` | alias de document |
| 7 | Montre-moi le contenu du cours | `moodle_course_content` | pagination suivie |
| 8 | Consignes complètes de l'activité | `detail=full` | lecture exhaustive |
| 9 | Lis-moi ce PDF | `moodle_file format=text` | couche texte |
| 10 | Montre-moi la figure de la première page | `format=images` | blocs image |
| 11 | Quelles notes j'ai eues ? | `moodle_grades` | filtre silencieux |
| 12 | J'ai combien dans ce cours ? | `moodle_grades courseid` | détail par cours |
| 13 | Quelle est la pondération du final ? | `moodle_grades` | poids ou refus honnête |
| 14 | Quelles annonces le prof a postées ? | `moodle_announcements` | tri, texte |
| 15 | Quelles discussions sur le forum ? | `moodle_forums` → `_discussions` | chaîne d'ids |
| 16 | J'ai des messages non lus ? | `moodle_messages` | compte |
| 17 | Quand est mon examen ? | `detail=full` | date dans une description |
| 18 | Mon horaire de la semaine | `moodle_calendar` | événements |
| 19 | Où j'en suis dans ce cours ? | `moodle_completion` | achèvement |
| 20 | Et sur Omnivox, mes échéances ? | `omnivox_deadlines` | refus explicite |

### Score du 2026-11-15 (v0.8.4)

**14 PASS · 1 PARTIAL · 2 FAIL · 3 SKIP — 82 % des questions évaluables.**

- **FAIL #5** — `moodle_search` ne fait ni racinisation ni repli : « corrigé »
  ne trouve pas `correction probleme doptimisation.pdf`, et « TD corrigé »
  renvoie 0 résultat alors que les TD existent (tous les mots sont exigés).
  C'est le défaut le plus coûteux pour un étudiant qui parle naturellement.
- **FAIL #11** — `moodle_grades` affiche 1 cours noté sur 5 sans dire qu'il a
  filtré les cours passés. ChatGPT s'arrête à ce seul cours.
- **PARTIAL #1** — `moodle_my_courses` liste 19 cours sur 26, filtre non annoncé.
- Les 3 `SKIP` sont des données absentes du compte, pas des erreurs d'outil.

## Ce que font vraiment Claude et ChatGPT

Même question, même MCP, un vrai agent (OpenCode, `claude-haiku-5-5` et
`gpt-6-luna`), sur le compte réel :

- **« Qu'est-ce que je dois remettre cette semaine ? »** — réponses identiques et
  exactes (titre du travail, cours, date et heure). Le MCP suffit.
- **« Quelles notes ? »** — Claude énumère 5 cours (il est allé chercher
  `moodle_recent_grades`) ; ChatGPT répond **un seul cours et une seule note**, et
  s'arrête. La cause est côté serveur : `moodle_grades` renvoie 1 cours sur 5
  sans dire qu'il a filtré. Un modèle pressé conclut que c'est tout.
- **« Quand est mon examen ? »** — les deux trouvent la date,
  mais **dans un PDF** du cours : aucune réponse d'outil ne porte cette date.
  Comme la recherche n'indexe pas les PDF, ils ont choisi le PDF au hasard.
- **« Le dernier TD du cours ? »** — Claude propose deux fichiers TD différents
  selon le groupe. Le fichier le plus récemment modifié est autre. La
  question est ambiguë (groupe, semaine) et **aucun champ ne permet de
  trancher** : ni groupe, ni « semaine en cours ».

Deux coûts visibles côté client : Claude a dû appeler un méta-outil
`search(namespace)` pour trouver le bon outil parmi 27, et les deux modèles ont
enchaîné 3 à 7 appels pour des questions simples.

## Où vit quoi

```
tools/mcp-audit.py        CLI : --service, --intents, --baseline
benchmark/harness.py      client MCP stdio + client école + métriques
benchmark/services/*.py   un fichier par plateforme (moodle aujourd'hui)
benchmark/run.py          mêmes questions sur les binaires de release
```

Ajouter Brightspace, Omnivox ou Canvas = un fichier dans
`benchmark/services/` (copier `_template.py`), pas un second harnais.
Détails : `benchmark/README.md`.
