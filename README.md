# student-mcp

A small **Rust binary** (a window + a local MCP server) that connects student
services to AI assistants: **Moodle**, **Omnivox**, and **Brightspace /
myCourses**, using the same school login you already have.

- 🪟 **Minimal popup window**: click a service, type your credentials, that's it (no web UI).
- 🔌 **MCP server**: stdio for local clients, stable HTTPS `/mcp/<token>` link for web AI connectors through srv.us, with automatic reconnect.
- 🔐 Status quo: the Moodle token stays out of the URL, and passwords are never written to disk.
- 📦 Install: one command, one binary with an embedded SSH client; GitHub Actions builds 6 platforms. No extra account, SSH installation, domain or network configuration needed.

## Install

macOS / Linux:

```bash
curl -fsSL https://raw.githubusercontent.com/CloudDown/student-mcp/main/install.sh | bash
```

Windows (PowerShell):

```powershell
irm https://raw.githubusercontent.com/CloudDown/student-mcp/main/install.ps1 | iex
```

The window shows course platforms (**Moodle**, **Omnivox**, **Brightspace**). Click a logo to connect.
Successful connections
return to the home screen with a green outline around the connected logo.
Credentials stay in `~/.student-mcp.json`.

**Multiple accounts, one connector:** open a platform and click **+ Add account**
to connect another account. Use the account selector to manage its credentials,
permissions or disconnection independently. No institution name is required;
accounts are distinguished by their site address and an automatic identifier.
Existing single-account configurations continue to work.

The same MCP link exposes all connected accounts. Adding or removing an account
does not change that link or its token. Restarting the app or changing its local
port keeps the same hostname. `student_status` lists the account IDs; service tools accept
`account_id`, required when that platform has multiple accounts. With just one
account on a platform, existing calls work without specifying an ID.

Click a configured service to manage it: its connection is checked in the
background (with **Retry** to check again), and you can change permissions,
replace credentials or disconnect. A green outline means credentials are saved,
not that the remote server is currently online.

Connect a service, then copy the HTTPS link below the logos into your AI's MCP
connector settings. The app creates the link automatically in the background.
Click it to view errors, retry, or turn public access off. The copy button is
enabled only while a public tunnel is running; local access is in link settings.
Local and public link switches are saved across restarts. Turning off the local
link also stops the public tunnel; re-enabling it respects the saved public setting.

**Runs in the background:** closing the window hides it in the Windows/Linux
notification area or macOS menu bar, keeping the MCP server and public link
running. Use the icon's **Open student-mcp** or **Quit student-mcp** menu.
Quitting stops the tunnel. On Linux, a StatusNotifier/AppIndicator-compatible
tray host is required (some GNOME sessions need an extension); without a tray,
the window closes normally and quits rather than leaving an unreachable app.
For reliable hide/restore, Linux uses X11/XWayland when `DISPLAY` is available.
On pure Wayland without XWayland, background hiding is disabled and closing quits.
If the Linux tray service disappears, a hidden window is restored.
This does not start the app automatically when logging into the computer.

Omnivox is read-only: enter your college Omnivox address, student number and
password. The app signs in through Omnivox's mobile login, so the saved session
works with the same `/Mobl/` endpoints as the official mobile app. If Omnivox
requests identity verification, enter the **6-digit code received by email** in
the app, then click **Verify and connect**. You can request a new code without
restarting the login. Only the verified session is saved, never the password or
security code. A local [omnivox-mcp](https://github.com/Beat-YT/omnivox-mcp)
gateway remains available under **Advanced options**.

Brightspace / myCourses (McGill and other D2L schools) is read-only: enter the
site address and your normal school username and password. Only the session is
saved. Schools that send you through Microsoft SSO cannot use that password
form; paste `d2lSessionVal` and `d2lSecureSessionVal` from your browser under
**Advanced options**. Tokens expire and must be replaced via **Change login**.
See [the guide](docs/GUIDE.md#quebec-integration-targets) for remaining targets.

## Using the tools

### Local MCP client (stdio) — Claude Desktop, OpenCode, …

Point your assistant at the binary:

```json
{ "student-mcp": { "command": "~/.student-mcp/bin/student-mcp" } }
```

stdio mode reads the config on every call: add or remove a service in the
window and the tools change without restarting the assistant.

### Web AI (MCP connector, e.g. Claude)

1. Install the app and connect a service. Wait for the public link to appear.
2. Copy the complete `https://<stable-id>.srv.us/mcp/<token>` link.
3. Claude → ⚙️ Settings → Connectors → *Add a custom connector* → paste the **complete** URL.
   This server authenticates with that secret link, not OAuth: choose
   **“Continue anyway”** if Claude mentions OAuth.

If Claude says it cannot register with the sign-in service
(`ofid_…` reference), it started an OAuth flow that this server does not
provide. Delete the connector, re-add it with the full `/mcp/<token>` link, and
choose **Continue anyway**. As an alternative, add a request header:
`Authorization: Bearer <token>` (the token is in the link). All discovery
endpoints (`.well-known/oauth-*`) return 404 on purpose so clients fall back to
the secret link.

The token in the URL is the only secret; without it a request gets a 401.
Keep the link private: anyone holding it can use your enabled tools. Traffic
passes through srv.us, not a project-hosted relay. **srv.us terminates HTTPS and
can technically read/alter exchanges and see the secret link.** Use local stdio
instead if you do not accept that trust. Quitting the app or switching off
**Public link** terminates its tunnel, without changing any other
app's tunnels or existing Tailscale routes. The app must remain running (its window can be hidden in the
tray) and the computer awake and online.

**Stable, best-effort link:** a dedicated Ed25519 key is generated automatically
and saved beside the configuration as `~/.student-mcp.srvus-key` (owner-only on
Unix). Back it up privately to keep your hostname after reinstalling or moving
computers. Never share it. The separate `~/.student-mcp.srvus-host` file pins the
relay's SSH identity on first use; changes are refused, not silently accepted.
Configuration overrides place these files beside the overridden config instead.
The app reconnects automatically with bounded backoff after network failures.
Stopping public access or quitting stops all tunnel tasks. Losing the saved key
changes your URL; a corrupt key is left
untouched and reported as an error. Changing the MCP token does not change the
hostname, but requires updating the token part in your AI connector.

**Migrating from Cloudflare:** replace your old `trycloudflare.com` connector URL
once with the new srv.us link. Saved school accounts and the MCP token are kept.
There is no automatic fallback to changing hostnames or to another provider.

srv.us has no uptime or large-scale capacity guarantee. Outbound SSH port 22 must
be allowed by your network. Our MCP server uses JSON responses, not SSE.
Actual AI-client compatibility still needs validation in that client; not every
AI or subscription supports custom MCP connectors.

## Available tools

Always: `student_status`. As soon as a service is connected, its tools appear:

- **Moodle** (student workflow tools + full Web Service escape hatch):
  - Overview: `moodle_status`, `moodle_my_courses`, `moodle_search`, `moodle_deadlines`, `moodle_notifications`, `moodle_messages`, `moodle_conversation_messages`
  - Course work: `moodle_course_content`, `moodle_resources`, `moodle_file`, `moodle_assignments`, `moodle_assignment_status`, `moodle_quizzes`, `moodle_quiz_status`, `moodle_grades`, `moodle_calendar`, `moodle_completion`
  - PDF reading: `moodle_file` accepts `find` (`"3.3"`, a title, a phrase),
    `page_from`, and `page_to`. It returns text, page images, and private
    3-hour links: `links.download` (the PDF), `links.reader` (all pages as
    HTML), `links.text`, and `links.parts` (one URL per page window). Short
    PDFs are returned whole; long ones expose `truncated` / `nextPageFrom`.
  - Forums: `moodle_announcements`, `moodle_forums`, `moodle_forum_discussions`, `moodle_forum_posts`
  - Advanced: `ws_discover`, `ws_schema`, `ws_call` (embedded Moodle catalogue, ~750 functions; any remaining student/admin capability the token allows)
- **Omnivox** (read only): `omnivox_status`, `omnivox_my_courses`, `omnivox_course_documents`, `omnivox_assignments`, `omnivox_grades`, `omnivox_schedule`, `omnivox_calendar`
- **Brightspace** (read only): `brightspace_status`, `brightspace_my_courses` (paged), `brightspace_course_content`, `brightspace_grades`

Writes stay locked until you tick **“Allow changes”** in the window.
`ws_call` arguments cannot override authentication or the selected function;
use JSON objects and arrays for nested Moodle parameters. Moodle API failures
are returned as tool errors, including when Moodle responds with HTTP 200.

## Development

```bash
cargo test             # unit tests
cargo run -- up        # window + HTTP
cargo run              # MCP stdio
cargo build --release  # small binary (opt-level=z, lto, strip)
```

The srv.us SSH client is compiled into the app, including Windows ARM64; there
is no sidecar to download or system SSH command to install. Native Mac/Windows
validation is still required.

Offline packaging tests: `python3 -m unittest discover -s scripts -p 'test_*.py'`.
To test the real public MCP endpoint (isolated configuration, no student accounts):

```bash
cargo test --locked public_mcp_smoke -- --ignored --nocapture
```

Release ZIPs contain the single app binary and have a SHA-256 checksum. Rust
dependencies are locked in `Cargo.lock`. See [srv.us security and validation](docs/SRVUS-EXPERIMENT.md).

Layout: `src/main.rs` (CLI), `gui.rs` (egui popup), `mcp.rs` (JSON-RPC + tools),
`http.rs` (std-only MCP HTTP server), `stdio.rs`, `moodle.rs` / `omnivox.rs` / `brightspace.rs`
(clients), `catalog.rs` (embedded catalogue),
`tunnel.rs` / `tunnel/` (managed srv.us connection and persistent identity), `config.rs` (`~/.student-mcp.json`).
`background.rs` owns the tray icon and hide/reopen/quit lifecycle. Linux uses
StatusNotifier over D-Bus without GTK; Windows/macOS use native tray menus.

License: MIT.
