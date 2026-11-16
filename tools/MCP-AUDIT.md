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
Le jeton scolaire est lu dans `~/.student-mcp.json` et n'est jamais affiché.

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

## Questions de référence (exécutables)

```bash
python3 tools/mcp-audit.py --intents
```

Chaque ligne est une vraie demande (« le dernier TD de ce cours », « mes notes »,
« quand est mon examen ») et le verdict compare la réponse à ce que l'école
contient réellement. `FAIL` = un assistant doit deviner ou inventer.

Résultat du 2026-11-15, v0.8.4 : **7/9 correctes**, 2 `PARTIAL` :

| demande | verdict | ce qui manque |
|---------|---------|----------------|
| mes cours | PARTIAL | 19 listés, 26 à l'école : les 7 cours terminés sont filtrés sans le dire |
| cette semaine | PASS | la remise à venir, avec date et heure |
| dernier TD de deux cours | PASS | meilleur rang 1 |
| date d'examen | PASS | aucune date coupée par la vue par défaut |
| mes notes | PARTIAL | 1 cours affiché, 5 notés au carnet, sans mention du filtre |
| plan de cours | PASS | |
| corrigé d'un TD | PASS | |
| annonces / notifications | PASS | 14 |

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
