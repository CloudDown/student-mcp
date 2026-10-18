<h1 align="center">Student MCP</h1>
<p align="center">Installe une fois, connecte tes services étudiants (Moodle, Proxmox…), branche l'URL dans Claude. Un petit binaire, zéro Node.js, page de configuration locale.</p>

## Install

```sh
# macOS / Linux
curl -fsSL https://raw.githubusercontent.com/CloudDown/student-mcp/main/install.sh | bash
```

```powershell
# Windows (PowerShell)
irm https://raw.githubusercontent.com/CloudDown/student-mcp/main/install.ps1 | iex
```

Manual ZIPs: [latest release](https://github.com/CloudDown/student-mcp/releases/latest).

## Configure (la page blanche)

1. À l'installation, la page `http://localhost:8420` s'ouvre : une carte par service.
2. Clique sur **Moodle** (ou **Proxmox**), entre tes identifiants — le mot de passe sert une seule fois, jamais stocké.
3. Copie l'**URL du connecteur** affichée sur la page.
4. Claude → Paramètres → **Connecteurs** → Ajouter un connecteur personnalisé → colle l'URL.
   Tu peux ensuite demander : *« Mes prochains travaux ? »*, *« Mes notes de Calcul ? »*, *« Démarre la VM 101 »*.

## Lien public (Tailscale Funnel)

Pour que Claude (app, téléphone) rejoigne ton MCP depuis n'importe où :

- installe [Tailscale](https://tailscale.com/download) et connecte-toi (`tailscale login`),
- clique **« Exposer via Tailscale »** sur la page (ou `sudo tailscale funnel --bg 8420`).

L'URL publique `https://<ton-appareil>.ts.net/mcp/<token>` est protégée par un token secret : n'importe qui sans l'URL n'y a pas accès ; les écritures (Moodle, Proxmox) restent désactivées par défaut.

## Clients IA locaux (stdio)

OpenCode ou Claude Desktop sur le même appareil ? Mets le binaire directement en serveur MCP local (stdio) :

```sh
# dans ~/.config/opencode/opencode.json (mcp.moodle) :
# "mcp": { "moodle": { "command": ["student-mcp"], } }        (v2)
# ou lance student-mcp install et laisse le faire.
```

<p align="center"><a href="docs/GUIDE.md">Docs (FR)</a> · MIT (catalog GPL) · Not an official Polytechnique Montréal tool.</p>
