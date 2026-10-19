# student-mcp

A small **Rust binary** (a window + a local MCP server) that connects student
services to AI assistants — for now **Moodle** (`moodle.polymtl.ca`) and
**Proxmox**, later: student record, email.

- 🪟 **Minimal popup window**: click a service, type your credentials, that's it (no web UI).
- 🔌 **Local MCP server**: stdio for Claude Desktop / OpenCode, HTTP `/mcp/<token>` for Claude web through Tailscale Funnel.
- 🔐 Status quo: the Moodle token stays out of the URL, and passwords are never written to disk.
- 📦 Install: one command, everything is bundled (static binary), and GitHub Actions builds 6 platforms.

## Install

macOS / Linux:

```bash
curl -fsSL https://raw.githubusercontent.com/CloudDown/student-mcp/main/install.sh | bash
```

Windows (PowerShell):

```powershell
irm https://raw.githubusercontent.com/CloudDown/student-mcp/main/install.ps1 | iex
```

The window opens: add **Moodle** (Poly credentials, or a mobile token),
**Proxmox** (API token), then close it — credentials stay in
`~/.student-mcp.json`.

## Using the tools

### Local MCP client (stdio) — Claude Desktop, OpenCode, …

Point your assistant at the binary:

```json
{ "student-mcp": { "command": "~/.student-mcp/bin/student-mcp" } }
```

stdio mode reads the config on every call: add or remove a service in the
window and the tools change without restarting the assistant.

### Claude web (custom connector)

1. In the window, click **“View the link for AI tools”** → **“Create my public link”**.
2. The URL looks like `https://your-machine.ts.net/mcp/<token>`.
3. Claude → ⚙️ Settings → Connectors → *Add a custom connector* → paste the URL.
   Warnings about OAuth are expected; choose **“Continue anyway”**.

The token in the URL is the only secret; without it a request gets a 401.
**“Remove the public link”** runs `tailscale funnel off`.

## Available tools

Always: `student_status`. As soon as a service is connected, its tools appear:

- **Moodle**: `moodle_status`, `moodle_my_courses`, `moodle_course_content`, `moodle_assignments`, `moodle_quizzes`, `moodle_grades`, `moodle_calendar`, `ws_discover`, `ws_schema`, `ws_call` (embedded Moodle catalogue, ~750 functions)
- **Proxmox**: `proxmox_status`, `proxmox_list_vms`, `proxmox_vm_status`, `proxmox_vm_start`, `proxmox_vm_stop`, `proxmox_vm_reboot`

Writes (dropping files, stopping a VM…) stay locked until you tick **“Allow changes”** in the window.

## Development

```bash
cargo test             # unit tests
cargo run -- up        # window + HTTP
cargo run              # MCP stdio
cargo build --release  # small binary (opt-level=z, lto, strip)
```

Layout: `src/main.rs` (CLI), `gui.rs` (egui popup), `mcp.rs` (JSON-RPC + tools),
`http.rs` (std-only MCP HTTP server), `stdio.rs`, `moodle.rs` / `proxmox.rs`
(clients), `funnel.rs` (Tailscale), `catalog.rs` (embedded catalogue),
`config.rs` (`~/.student-mcp.json`).

License: MIT.
