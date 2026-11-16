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

## Questions étudiantes (dérivées du compte)

```bash
python3 tools/mcp-audit.py --intents
```

Les questions ne sont **pas** une liste figée. Au runtime, le harnais lit le
Moodle connecté (cours de la session, fichiers récents, devoirs datés, items
notés, quiz) et pose des demandes précises : « quand est dû *ce* devoir »,
« trouve *ce* fichier », « j'ai combien à *cet* item ». Rien de ça n'est
codé en dur dans le dépôt.

`PASS` = les outils portent assez d'information juste pour répondre ;
`PARTIAL` = une réponse plausible serait incomplète ; `FAIL` = le modèle
doit deviner ; `SKIP` = ce compte n'a pas la donnée.

### Score du 2026-11-16 (compte réel, binaire current)

**44 PASS · 0 PARTIAL · 0 FAIL · 0 SKIP — 100 % des questions évaluables.**

Les outils portent les noms, dates, notes et fichiers demandés. Ça ne dit
pas encore si Claude/GPT *choisissent* le bon outil — seulement que la
donnée est dans la réponse.

L'ancienne batterie générique (50 questions, v0.8.4) restait à 93 % :
recherche trop large (« le corrigé »), `moodle_grades` qui cachait des
cours passés. Elle reste comparable via `benchmark/run.py` jusqu'à ce que
la matrice soit relancée.

Le même passage mesure le coût : 0.8.1 divise le temps par 1,9 (50,9 s → 27,0 s)
et les tokens lus par 2,4 (170 201 → 70 135) sur ces mêmes questions, pendant que
le score monte de 85 à 93 %. Depuis, `tools/list` passe de 3 462 à 6 089 tokens
(descriptions de paramètres) et la source MCP de 724 Ko à 1 015 Ko : c'est le
prix de la compréhension, à surveiller à chaque release.

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
