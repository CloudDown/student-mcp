# Benchmark des MCP

Même questions, même compte, même correcteur — mais des **binaires de release**
différents. Ça mesure ce qu'un étudiant installe réellement, pas ce que le dépôt
compile aujourd'hui.

```bash
python3 benchmark/run.py                      # de v0.6.2 à current
python3 benchmark/run.py --versions v0.8.3,current
python3 benchmark/run.py --update             # réécrit SCORES.md + site/
python3 benchmark/site.py                     # page visuelle (file://benchmark/site/index.html)
python3 benchmark/surfaces.py                 # surface MCP de chaque release
```

## Parcours étudiant (Claude web + ChatGPT web)

C’est **le** test qualité : une question naturelle dans Claude.ai et
ChatGPT.com, MCP branché, réponse finale notée contre Moodle. Le harnais
d’outils (`run.py`) reste le filet de régression ; il n’est pas ce score.

```bash
python3 benchmark/web_student.py    # 8 questions précises + page locale
```

La page (hors git) : `~/.student-mcp.eval/web-student/index.html`.

1. student-mcp allumé, **lien public** copié depuis l’UI.
2. Claude.ai → Customize → Connectors → coller le lien → chat neuf, activer student-mcp.
3. ChatGPT.com → Plugins → Add custom MCP server, sans auth → chat neuf, `@student-mcp`.
4. Une conversation **neuve** par question. Coller seulement la question, pas le nom d’outil.
5. Coller les deux réponses dans la page : PASS / PARTIAL / FAIL tout de suite.

`student.py` (Luna/Haiku via API) est un proxy pas cher. Ce n’est **pas** le
verdict étudiant, et un HTTP 403 ne compte pas comme un run.

Les versions testées sont celles qui changent vraiment le MCP : depuis 0.6.2 les
27 noms d'outils sont identiques, mais leurs définitions passent de 13,8 Ko à
24,4 Ko et leur comportement change. Les releases qui partagent une surface
identique (0.6.5/0.6.7/0.6.8 = 0.6.6, 0.7.0 = 0.6.9, 0.8.2 = 0.8.1) sont
sautées. `surfaces.py` régénère cette liste.

Les binaires sont téléchargés depuis GitHub (`gh release download`), mis en
cache dans `benchmark/.bin/` (ignoré par git), et chaque exécution reçoit une
**copie privée** de la configuration (0600, supprimée à la fin) : aucune release
ne peut réécrire la vraie config, et l'ancien format `~/.student-mcp.json` reste
lisible via `STUDENT_MCP_CONFIG_FILE`.

Résultat historique Moodle : `SCORES.md`. Les questions principales sont
générées depuis le compte dans `services/moodle.py` (la matrice publiée en
compte 50). Les **trois** questions de `scenarios.example.json` servent juste à
essayer `student.py` : elles ne limitent ni la matrice ni les huit questions web.

Ce que la matrice mesure, au-delà du score :

| colonne | sens |
|---|---|
| temps total / médian | somme des 50 appels, et l'appel typique — un outil lent est un outil abandonné |
| tokens/question, tokens totaux | octets de réponse ÷ 4 : ce qu'un modèle doit lire pour ces 50 questions |
| tools/list | ce que coûte la liste d'outils avant le premier appel |
| binaire | taille installée par l'étudiant |
| src MCP | `mcp.rs` + `src/services/` au tag : le poids du code qu'on maintient |

Un gain de score qui double les tokens ou la latence n'est pas un gain : les
trois colonnes se lisent ensemble.

## Structure

```
benchmark/
  harness.py            client MCP stdio, client HTTP école, métriques de charge
  report.py             rendu texte et comparaison à une baseline
  services/
    __init__.py         registre AVAILABLE + load()
    moodle.py           50 questions + audit de charge utile
    brightspace.py      7 cas de référence
    omnivox.py          7 cas de référence
    portal.py           vérifications communes pour ces deux plateformes
    _template.py        squelette pour un futur service
  references/           deux exemples fictifs de références portail
  run.py                releases × services → matrice et SCORES.md
  web_student.py        questions précises → page Claude.ai / ChatGPT.com
  student.py            proxy API (pas le verdict web)
  scenarios.example.json trois exemples de saisie pour student.py (proxy API)
  site.py               page unique file:// (questions, réponses, métriques)
  site/index.html       généré, hors git (réponses scolaires)
  results/              JSON brut par version (hors git)
  .bin/                 binaires téléchargés (hors git)
```

## Brightspace et Omnivox

Le même harnais vérifie les inscriptions, la découverte d'un document, la
lecture d'une page de PDF et sa continuation, les consignes, les notes et les
échéances d'un seul cours. Brightspace teste aussi sa recherche par titre. Les
verdicts portent sur les réponses d'outils, pas sur une réponse Claude/ChatGPT.

1. Copie le [modèle Brightspace](references/brightspace.example.json) ou
   [Omnivox](references/omnivox.example.json) **hors du dépôt**. Remplace les
   valeurs fictives par des données vérifiées **dans le portail officiel** ou
   dans ses réponses réseau de lecture, jamais dans les résultats du MCP testé.
   N'y mets ni cookies, ni jetons, ni identifiants de connexion.
2. `account_id` est l'identifiant du compte activé dans student-mcp
   (`student_status`, sans collecte de données scolaires). Utilise le même
   compte dans le portail. `course` est obligatoire ; supprime les sections
   facultatives non vérifiables pour qu'elles donnent `SKIP`. Pour Omnivox,
   renseigne `term` (le trimestre à cinq chiffres, sinon les outils existants
   gardent leur route gateway et peuvent répondre vide sur une connexion
   collège directe), puis complète `instructions_field` avec le vrai champ des
   consignes ; pour les notes, `match` identifie une évaluation de façon unique
   et `values` liste les champs qu'on compare sur cette même évaluation. Les
   noms de champs des exemples sont illustratifs : utilise ceux de ton portail.
   Respecte l'unité du connecteur (p. ex. `8840` pour `88,40` en x100 sur
   Omnivox).
3. Pour `deadlines`, vérifie la **liste complète** du cours pour la fenêtre
   indiquée et renseigne la date UTC du relevé. Une liste `[]` signifie
   « vérifiée vide », pas « pas vérifiée ». Brightspace inclut devoirs, quiz et
   échéances du calendrier ; Omnivox seulement les remises Léa. Lance la
   comparaison rapidement après le relevé, hors d'un début ou d'une fin
   d'échéance. Si la date UTC a changé, le cas est `SKIP`.

```bash
python3 benchmark/run.py --service brightspace --reference ~/brightspace-reference.json
python3 benchmark/run.py --service omnivox --reference ~/omnivox-reference.json --versions current
# Comparer une release compatible au binaire local déjà compilé :
python3 benchmark/run.py --service brightspace --reference ~/brightspace-reference.json --versions vX.Y.Z,current
```

Les cas fixes couvrent les parcours principaux. Pour vérifier d'autres
lectures sans ajouter de code, la référence accepte une liste `checks` :

```json
{
  "checks": [{
    "question": "« Combien de documents dans ce cours ? »",
    "tool": "omnivox_documents_summary",
    "args": {"term": "20263"},
    "expect": [{
      "path": "ListeSommaire",
      "match": {"NoCours": "MAT101", "NoGroupe": "0001"},
      "values": {"NbDocumentsDisponibles": 17}
    }]
  }]
}
```

`path` suit les clés et index (`HoraireDex.Cours`, `0.Titre`). `match`
sélectionne une seule ligne d'une liste, `values` compare des champs exacts,
`contains` cherche un texte, `empty` exige une liste vide, `min_count` un
minimum d'éléments et `count` un nombre exact. `text` vérifie des sous-chaînes
dans la réponse brute.
Chaque outil nommé est appelé seulement si sa définition déclare
`readOnlyHint: true` ; aucun outil d'écriture n'est exécuté.

Le harnais utilise une copie privée du seul compte sélectionné et fige la
référence pour toutes les versions. Sur Linux, il copie un éventuel profil
Chromium sous le verrou de l'application ; aucune session n'est exportée
vers le correcteur. Ferme les appels ou connexions actifs et utilise un
navigateur Chromium installé (ou son chemin dans `STUDENT_MCP_BROWSER`).
La configuration et le profil d'origine ne sont pas modifiés. Seuls les outils
déclarés en lecture seule et nommés dans les cas sont appelés.

Les verdicts affichent leur raison. Une échéance hors cours, manquante sans
avertissement ou mal datée échoue ; une couverture explicitement incomplète
est notée `PARTIAL`. Les rapports ignorés par Git sont séparés par service
dans `results/` et enregistrent temps, octets de texte et taille sur le fil, sans
stocker les réponses scolaires intégrales. `--update` écrit une table de
scores séparée, sans toucher à la matrice ni à la page Moodle. Le code de
sortie vaut `1` si la dernière version demandée contient un `FAIL`, `2` si
une version ne produit aucun score exploitable. Sans référence réelle
exécutée, il n'existe pas encore de score de terrain pour ces services.

## Ajouter un service

Un fichier, deux fonctions, une ligne :

```python
# benchmark/services/omnivox.py
NAME = "omnivox"
def surface(binary, account) -> dict: ...      # charge utile, fidélité, latence
def questions(binary, account) -> list[dict]: ...  # questions notées
```

Puis `AVAILABLE` dans `services/__init__.py`. Le CLI, le benchmark et la table de
scores le prennent automatiquement — il n'y a pas de second harnais à écrire.

Règles qui gardent la comparaison honnête :

- la vérité terrain vient de l'école, via ses API ou un relevé indépendant du
  portail ; un client HTTP direct doit utiliser le **même User-Agent** que le
  connecteur (un WAF répond une page vide au UA par défaut) ;
- une donnée absente du compte donne `SKIP`, pas `FAIL` ;
- jamais d'écriture testée sur un compte réel ;
- une question qui passe de PASS à FAIL est une régression bloquante, même si le
  score global monte.

Canvas n'a pas encore de batterie. La logique spécifique va dans son module ;
le transport MCP, le rendu et la comparaison restent communs.
