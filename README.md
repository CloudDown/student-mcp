# student-mcp

Un petit **binaire Rust** (une fenêtre + un serveur MCP local) qui connecte les services étudiants aux assistants IA — pour l'instant **Moodle** (`moodle.polymtl.ca`) et **Proxmox**, et plus tard : dossier étudiant, courriel.

- 🪟 **Fenêtre popup minimaliste** : tu cliques un service, tu tapes tes identifiants, c'est tout (pas d'interface web).
- 🔌 **Serveur MCP local** : stdio pour Claude Desktop / OpenCode, HTTP `/mcp/<token>` pour Claude web via Tailscale Funnel.
- 🔐 Status quo : le token Moodle est démasqué en interne, jamais stocké en clair dans l'URL ; les mots de passe ne sont jamais écrits sur disque.
- 📦 Installation : une commande, tout est embarqué (binaire statique), et GitHub Actions compile 6 plateformes.

## Installation

macOS / Linux :

```bash
curl -fsSL https://raw.githubusercontent.com/CloudDown/student-mcp/main/install.sh | bash
```

Windows (PowerShell) :

```powershell
irm https://raw.githubusercontent.com/CloudDown/student-mcp/main/install.ps1 | iex
```

La fenêtre s'ouvre : ajoute **Moodle** (identifiants poly, ou token mobile), **Proxmox** (token API), puis ferme-la — les identifiants restent dans `~/.student-mcp.json`.

## Utiliser les outils

### Claude Desktop / OpenCode / autre MCP local (stdio)

Pointe ton assistant sur le binaire :

```json
{ "student-mcp": { "command": "~/.student-mcp/bin/student-mcp" } }
```

Le mode stdio lit la config à chaque appel : ajoute ou retire un service dans la fenêtre, les outils changent sans redémarrer l'assistant.

### Claude web (connecteur personnalisé)

1. Dans la fenêtre : bouton **« Publier via Tailscale Funnel »**.
2. L'URL ressemble à `https://ton-machine.ts.net/mcp/<token>`.
3. Claude → ⚙️ Connecteurs → *Ajouter un connecteur personnalisé* → colle l'URL.

Le token dans l'URL est le seul secret ; sans lui, la requête recoit un 401. « Retirer l'URL publique » fais `tailscale funnel off`.

## Outils disponibles

Toujours : `student_status`. Dès qu'un service est actif, ses outils apparaissent :
- **Moodle** : `moodle_status`, `moodle_my_courses`, `moodle_course_content`, `moodle_assignments`, `moodle_quizzes`, `moodle_grades`, `moodle_calendar`, `ws_discover`, `ws_schema`, `ws_call` (catalogue Moodle embarqué, ~750 fonctions)
- **Proxmox** : `proxmox_status`, `proxmox_list_vms`, `proxmox_vm_status`, `proxmox_vm_start`, `proxmox_vm_stop`, `proxmox_vm_reboot`

Les écritures (dépôt de fichiers, stop VM…) restent verrouillées tant que tu n'as pas coché « autoriser les modifications » dans la fenêtre.

## Développer

```bash
cargo test          # 9 tests unitaires
cargo run -- up     # fenêtre + HTTP
cargo run           # MCP stdio
cargo build --release  # binaire léger (opt-level=z, lto, strip)
```

Structure : `src/main.rs` (CLI), `gui.rs` (popup egui), `mcp.rs` (JSON-RPC + outils), `http.rs` (serveur MCP HTTP std-lib), `stdio.rs`, `moodle.rs` / `proxmox.rs` (clients), `funnel.rs` (Tailscale), `catalog.rs` (catalogue embarqué), `config.rs` (`~/.student-mcp.json`).

Licence : MIT.
