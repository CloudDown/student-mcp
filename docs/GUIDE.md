# Student MCP

> **English** — Student MCP is a tiny local MCP (Model Context Protocol) server with a white configuration page: log into your student services (Moodle, Proxmox…), copy the connector URL into Claude, done. A public HTTPS link is one click away via Tailscale Funnel. No Node.js needed.
>
> **Install:** macOS/Linux: `curl -fsSL https://raw.githubusercontent.com/CloudDown/student-mcp/main/install.sh | bash` — Windows (PowerShell): `irm https://raw.githubusercontent.com/CloudDown/student-mcp/main/install.ps1 | iex`.
>
> *Not an official Polytechnique Montréal tool.* The rest of this guide is in French.

## Le principe

1. **Installer** — une commande, une exécutable unique (~100 Mo), rien d'autre.
2. **Configurer sur la page** — `student-mcp up` ouvre `http://localhost:8420` : une page blanche, une carte par service. Tu cliques, tu entres tes identifiants, c'est tout.
3. **Connecter Claude** — copie l'URL du connecteur dans *Paramètres → Connecteurs → Ajouter un connecteur personnalisé*.

## Installer

**Mac ou Linux** — colle ceci dans le Terminal :

```sh
curl -fsSL https://raw.githubusercontent.com/CloudDown/student-mcp/main/install.sh | bash
```

**Windows** — colle ceci dans PowerShell :

```powershell
irm https://raw.githubusercontent.com/CloudDown/student-mcp/main/install.ps1 | iex
```

ZIP manuels : [versions Windows, Mac et Linux](https://github.com/CloudDown/student-mcp/releases/latest).

## Services

| Service | État | Authentification |
|---|---|---|
| **Moodle** (PolyMTL ou tout autre) | Actif | Identifiant + mot de passe (token récupéré automatiquement) ou `privatetoken` collé |
| **Proxmox** (laboratoire, club, auto-hébergé) | Actif | Token API `utilisateur@domaine!token` + secret |
| Dossier étudiant / courriel Poly | Plus tard | Aucune API publique : cartes visibles en grisé sur la page |

Le mot de passe Moodle sert **une seule fois** (il n'est enregistré nulle part) ;
seul le token signale ton compte. Le secret Proxmox est stocké dans
`~/.student-mcp.json` (permissions 600). Tout peut être désactivé d'un clic.

## Lien public infini (Tailscale Funnel)

Depuis chez toi, en classe ou sur ton téléphone :

1. Installe Tailscale (une fois) : `tailscale login` (ou l'app Windows/Mac).
2. Clique **« Exposer via Tailscale »** sur la page, ou : `sudo tailscale funnel --bg 8420`.
3. Utilise l'URL **publique** `https://<appareil>.ts.net/mcp/<token>` dans Claude.

Ce que ça implique :

- l'URL contient un **token secret** : sans elle, pas d'accès ; garde-la pour toi ;
- les outils d'écriture (dépôt de fichiers Moodle, démarrage/extinction VM) restent
  **désactivés par défaut** ; tu les actives explicitement sur chaque service ;
- retirer le lien : `tailscale funnel off` ou le bouton sur la page.

## Clients IA locaux (stdio)

Sur le même ordinateur, OpenCode et Claude Desktop n'ont pas besoin d'Internet :

```sh
student-mcp install   # configure OpenCode ou Claude Desktop en mode stdio
```

ou ajoute `student-mcp` (le binaire installé dans `~/.student-mcp/`) comme serveur
MCP local. Le mode stdio lit la même configuration (`~/.student-mcp.json`,
variables d'env `MOODLE_URL`/`MOODLE_TOKEN`, `PROXMOX_URL`/`PROXMOX_TOKEN_ID`/`PROXMOX_TOKEN_SECRET`).

## Outils exposés

`student_status`, puis par service :
`moodle_status`, `moodle_my_courses`, `moodle_course_content`, `moodle_assignments`,
`moodle_quizzes`, `moodle_grades`, `moodle_calendar`, `ws_discover`, `ws_schema`,
`ws_call` · `proxmox_status`, `proxmox_list_vms`, `proxmox_vm_status`,
`proxmox_vm_start`, `proxmox_vm_stop`, `proxmox_vm_reboot`.

Le connecteur n'accède qu'aux données autorisées par ton propre compte. Les
exécutables Windows et Mac ne sont pas signés : ton système peut demander une
confirmation au premier lancement.

Code source sous licence MIT ; catalogue issu de Moodle sous GPL :
[licences](../catalog/LICENSE.md). Projet indépendant de Moodle et de
Polytechnique Montréal.
