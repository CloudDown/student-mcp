# Guide student-mcp

Petite fenêtre + serveur MCP local. Tout-en-un, aucun framework web.

## 1. Ce que tu vois à l'ouverture

```
student-mcp  v0.2.0
● Serveur MCP local : 127.0.0.1:8420
Connecteur local (Claude Desktop, OpenCode…):
  http://127.0.0.1:8420/mcp/<token>            [Copier]
Services
  [M   Moodle  — Actif / « Cours, devoirs, notes… »]
  [P   Proxmox — Actif / « VMs et containers » ]
  [D   Dossier étudiant — Bientôt]
  [@   Courriel Poly  — Bientôt]
Lien public (optionnel)
  [Vérifier Tailscale] ou [Publier via Tailscale Funnel]
```

## 2. Connecter un service

**Moodle.** Deux méthodes.
1. *Identifiant Poly* : prenom.nom + mot de passe → le binaire appelle `/login/token.php` avec le service mobile `moodle_mobile_app` ; seul le **token** est sauvegardé, jamais le mot de passe.
2. *Token existant* : colle un token mobile (plus sûr si tu as une double auth).

**Proxmox.** Datacenter → Permissions → API Tokens → crée un token (par ex. `etudiant@pam!mcp`), copie son *secret*, entre `https://ton-pve:8006`, l'ID du token et le secret.

Si le serveur a un certificat autosigné, l'app refusera la connexion (HTTPS strict) : mets un vrai certificat ou fais signer le PVE.

## 3. Assister IA local

- **Claude Desktop / OpenCode / beaucoup d'autres** : indique le binaire en commande stdio :
  - macOS / Linux : `~/.student-mcp/bin/student-mcp`
  - Windows : `%LOCALAPPDATA%\student-mcp\student-mcp.exe`
- La liste des outils suit la fenêtre : ajoute/enlève un service, les outils apparaissent/disparaissent au prochain appel (config relue à chaque commande).

## 4. Lien public pour le web Claude

1. Installe et connecte **Tailscale** (une fois) : `tailscale login`.
2. Dans la fenêtre → **Publier via Tailscale Funnel** ; si le daemon Linux demande sudo, la fenêtre affiche la commande exacte (`sudo tailscale funnel --bg 8420`).
3. Colle `https://ton-machine.ts.net/mcp/<token>` dans Claude (connecteur personnalisé).
4. Pour couper : **Retirer l'URL publique** ou `tailscale funnel off`.

Le token protège l'URL : sans lui 401. Tu peux recharger le token en supprimant `gateway.token` dans `~/.student-mcp.json`.

## 5. Commandes

```
student-mcp            # MCP stdio (assistant IA local)
student-mcp up         # fenêtre + serveur HTTP local
student-mcp up --port=9000
student-mcp install    # réouvre la fenêtre (posée par l'installateur)
student-mcp version
```

## 6. Dépannage

- **La fenêtre ne s'ouvre pas (SSH)** : la fenêtre a besoin d'un écran ; sur un serveur headless, utilise uniquement le mode stdio.
- **Port déjà pris** : `up` essaie automatiquement 8420→8429 ; force-en un avec `--port=`.
- **Moodle refuse le login SSO** : la double auth bloque `/login/token.php` → passe par la méthode token existant.
- **Le Funnel ne part pas** : Tailscale installé ? connecté ? (`tailscale status`) puis `sudo tailscale funnel --bg 8420`.
- **Certificat PVE invalide** : l'app refuse en HTTPS strict ; corrige le certificat du serveur.
