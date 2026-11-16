# Benchmark des MCP

Même questions, même compte, même correcteur — mais des **binaires de release**
différents. Ça mesure ce qu'un étudiant installe réellement, pas ce que le dépôt
compile aujourd'hui.

```bash
python3 benchmark/run.py                      # v0.8.0 … v0.8.4 + current
python3 benchmark/run.py --versions v0.8.3,current
python3 benchmark/run.py --update             # réécrit SCORES.md
```

Les binaires sont téléchargés depuis GitHub (`gh release download`), mis en
cache dans `benchmark/.bin/` (ignoré par git), et chaque exécution reçoit une
**copie privée** de la configuration (0600, supprimée à la fin) : aucune release
ne peut réécrire la vraie config, et l'ancien format `~/.student-mcp.json` reste
lisible via `STUDENT_MCP_CONFIG_FILE`.

Résultat actuel : `SCORES.md`.

## Structure

```
benchmark/
  harness.py            client MCP stdio, client HTTP école, métriques de charge
  report.py             rendu texte et comparaison à une baseline
  services/
    __init__.py         registre AVAILABLE + load()
    moodle.py           20 questions + audit de charge utile
    _template.py        squelette pour Brightspace / Omnivox / Canvas
  run.py                releases × services → matrice et SCORES.md
  results/              JSON brut par version (hors git)
  .bin/                 binaires téléchargés (hors git)
```

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

- la vérité terrain vient de l'école, avec le **même User-Agent** que le
  connecteur (un WAF répond une page vide au UA par défaut) ;
- une donnée absente du compte donne `SKIP`, pas `FAIL` ;
- jamais d'écriture testée sur un compte réel ;
- une question qui passe de PASS à FAIL est une régression bloquante, même si le
  score global monte.

## Pourquoi pas un dossier par service dès maintenant

Moodle est le seul service connecté et le seul dont la vérité terrain est
l'API. Omnivox et Brightspace demandent un client navigateur/session, Canvas une
API différente : chacun aura sa logique dans son fichier, mais le harnais, le
rendu et le benchmark restent uniques. Copier-coller trois harnais serait le
contraire de l'objectif.
