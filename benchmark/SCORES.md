# MCP benchmark across releases

Same 20 student questions, same account, same grader (`tools/mcp-audit.py --intents`). Each row is a released binary.

| release | PASS | PARTIAL | FAIL | SKIP | score |
|---|---|---|---|---|---|
| v0.6.2 | 35 | 3 | 3 | 9 | **85 %** |
| v0.6.4 | 35 | 3 | 3 | 9 | **85 %** |
| v0.6.6 | 35 | 3 | 3 | 9 | **85 %** |
| v0.6.9 | 35 | 3 | 3 | 9 | **85 %** |
| v0.8.0 | 35 | 3 | 3 | 9 | **85 %** |
| v0.8.1 | 41 | 1 | 2 | 6 | **93 %** |
| v0.8.3 | 42 | 1 | 2 | 5 | **93 %** |
| v0.8.4 | 42 | 1 | 2 | 5 | **93 %** |
| current | 42 | 1 | 2 | 5 | **93 %** |

Les colonnes `PARTIAL` et `FAIL` comptent : une release peut gagner en
rapidité et perdre une réponse correcte. Les `SKIP` sont des données absentes
du compte, pas des erreurs.

Régénérer : `python3 benchmark/run.py --update`.
