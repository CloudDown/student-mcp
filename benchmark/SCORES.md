# MCP benchmark across releases

Same 20 student questions, same account, same grader (`tools/mcp-audit.py --intents`). Each row is a released binary.

| release | PASS | PARTIAL | FAIL | SKIP | score |
|---|---|---|---|---|---|
| v0.8.0 | 12 | 2 | 2 | 4 | **75 %** |
| v0.8.1 | 13 | 1 | 2 | 4 | **81 %** |
| v0.8.2 | 13 | 1 | 2 | 4 | **81 %** |
| v0.8.3 | 14 | 1 | 2 | 3 | **82 %** |
| v0.8.4 | 14 | 1 | 2 | 3 | **82 %** |
| current | 14 | 1 | 2 | 3 | **82 %** |

Les colonnes `PARTIAL` et `FAIL` comptent : une release peut gagner en
rapidité et perdre une réponse correcte. Les `SKIP` sont des données absentes
du compte, pas des erreurs.

Régénérer : `python3 benchmark/run.py --update`.
