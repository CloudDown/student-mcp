# MCP benchmark across releases

50 questions étudiantes identiques, même compte, même correcteur (`tools/mcp-audit.py --intents`). Chaque ligne est un binaire publié.

| release | score | PASS | PART | FAIL | SKIP | temps total | temps médian | tokens/question | tokens totaux | tools/list | binaire | src MCP |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| v0.6.2 | **85 %** | 35 | 3 | 3 | 9 | 48.6 s | 0.35 s | 175 | 169372 | 3462 | 5.8 Mo | 724 Ko |
| v0.6.4 | **85 %** | 35 | 3 | 3 | 9 | 45.3 s | 0.34 s | 175 | 169372 | 3450 | 5.9 Mo | 722 Ko |
| v0.6.6 | **85 %** | 35 | 3 | 3 | 9 | 43.5 s | 0.34 s | 204 | 169576 | 3943 | 6.2 Mo | 777 Ko |
| v0.6.9 | **85 %** | 35 | 3 | 3 | 9 | 51.9 s | 0.32 s | 204 | 170201 | 4016 | 6.5 Mo | 851 Ko |
| v0.8.0 | **85 %** | 35 | 3 | 3 | 9 | 50.9 s | 0.34 s | 204 | 170201 | 4290 | 7.3 Mo | 869 Ko |
| v0.8.1 | **93 %** | 41 | 1 | 2 | 6 | 27.0 s | 0.24 s | 495 | 70135 | 4884 | 7.4 Mo | 966 Ko |
| v0.8.3 | **93 %** | 42 | 1 | 2 | 5 | 36.3 s | 0.26 s | 495 | 71247 | 5027 | 7.4 Mo | 989 Ko |
| v0.8.4 | **93 %** | 42 | 1 | 2 | 5 | 35.3 s | 0.24 s | 495 | 71869 | 5233 | 7.5 Mo | 1009 Ko |
| current | **93 %** | 42 | 1 | 2 | 5 | 41.5 s | 0.27 s | 495 | 71876 | 6089 | 7.5 Mo | 1015 Ko |

**Colonnes.** *temps total* = somme des 50 appels d'outils ; *temps médian* = l'appel typique ; *tokens/question* = médiane des octets de réponse divisée par 4 ; *tokens totaux* = tout ce qu'un modèle lirait pour ces 50 questions ; *tools/list* = ce que coûte la liste d'outils avant le premier appel ; *src MCP* = `mcp.rs` + `src/services/` au tag.

Les `PARTIAL` et `FAIL` comptent : une release peut gagner en vitesse et perdre une réponse correcte. Les `SKIP` sont des données absentes du compte.

Régénérer : `python3 benchmark/run.py --update`.
