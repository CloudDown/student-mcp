# student-mcp

A small **Rust binary** (a window + a local MCP server) that connects student
services to AI assistants: **Moodle**, **Omnivox** (through a local
[omnivox-mcp](https://github.com/Beat-YT/omnivox-mcp) gateway), and **Brightspace /
myCourses** (with an institution-authorized OAuth token).

- 🪟 **Minimal popup window**: click a service, type your credentials, that's it (no web UI).
- 🔌 **MCP server**: stdio for local clients, automatic HTTPS `/mcp/<token>` link for web AI connectors through Cloudflare Quick Tunnels.
- 🔐 Status quo: the Moodle token stays out of the URL, and passwords are never written to disk.
- 📦 Install: one command, app + verified `cloudflared` component bundled; GitHub Actions builds 6 platforms. No Cloudflare account, domain, Tailscale or network configuration needed.

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

Click a configured service to manage it: its connection is checked in the
background (with **Retry** to check again), and you can change permissions,
replace credentials or disconnect. A green outline means credentials are saved,
not that the remote server is currently online.

Connect a service, then copy the HTTPS link below the logos into your AI's MCP
connector settings. The app creates the link automatically in the background.
Click it to view errors, retry, or turn public access off. The copy button is
enabled only while a public tunnel is running; local access is in link settings.

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

Omnivox is read-only: Skytech has no public student API, so this app talks to a
local [omnivox-mcp](https://github.com/Beat-YT/omnivox-mcp) gateway. Log in there
(MFA included), then enter its address and access key — not your Omnivox
password.

Brightspace / myCourses (McGill and other D2L schools) is read-only: enter the
institution's site address and an approved OAuth access token, not your school
password. Tokens expire and must currently be replaced manually; browser OAuth
and automatic refresh are not implemented. This does not guarantee that McGill
or another institution allows student API access. See
[the guide](docs/GUIDE.md#quebec-integration-targets) for remaining targets.

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
2. Copy the complete `https://random-name.trycloudflare.com/mcp/<token>` link.
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
passes through Cloudflare, not a project-hosted relay. Closing the app or using
**“Remove the public link”** terminates its tunnel, without changing any other
app's tunnels or existing Tailscale routes. Disconnecting the last service also
stops the tunnel. The app must remain running (its window can be hidden in the
tray) and the computer awake and online.

**Temporary, best-effort link:** every new tunnel gets a different hostname;
update the connector in your AI after restarting it. Quick Tunnels are intended
for testing/development, have no uptime guarantee, support at most 200 concurrent
requests, and do not support SSE. Our MCP server uses JSON responses, not SSE.
Actual AI-client compatibility still needs validation in that client; not every
AI or subscription supports custom MCP connectors.

## Available tools

Always: `student_status`. As soon as a service is connected, its tools appear:

- **Moodle**: `moodle_status`, `moodle_my_courses`, `moodle_course_content`, `moodle_assignments`, `moodle_quizzes`, `moodle_grades`, `moodle_calendar`, `ws_discover`, `ws_schema`, `ws_call` (embedded Moodle catalogue, ~750 functions)
- **Omnivox** (read only, via omnivox-mcp): `omnivox_status`, `omnivox_my_courses`, `omnivox_course_documents`, `omnivox_assignments`, `omnivox_grades`, `omnivox_schedule`, `omnivox_calendar`
- **Brightspace** (read only): `brightspace_status`, `brightspace_my_courses` (paged), `brightspace_course_content`, `brightspace_grades`

Writes stay locked until you tick **“Allow changes”** in the window.

## Development

```bash
cargo test             # unit tests
cargo run -- up        # window + HTTP
cargo run              # MCP stdio
cargo build --release  # small binary (opt-level=z, lto, strip)
```

For local development, fetch the sidecar beside the executable before `cargo run -- up`:

```bash
python3 scripts/fetch-cloudflared.py --label linux-x64 --output target/debug
```

Use your platform label (`linux-arm64`, `macos-x64`, `macos-arm64`,
`windows-x64`, `windows-arm64`) as appropriate. An explicit
`STUDENT_MCP_CLOUDFLARED` path can override the sidecar for development.
Upstream has no Windows ARM64 binary; that package uses the x64 sidecar via
Windows 11 ARM's x64 emulation. Native Mac/Windows validation is still required.

Offline packaging tests: `python3 -m unittest discover -s scripts -p 'test_*.py'`.
To test the real public MCP endpoint (isolated configuration, no student accounts):

```bash
STUDENT_MCP_CLOUDFLARED="$PWD/target/debug/cloudflared" \
  cargo test --locked public_mcp_smoke -- --ignored --nocapture
```

Release packaging pins the sidecar version and upstream SHA-256 digests in
`scripts/fetch-cloudflared.py`, includes its Apache license, and verifies the
sidecar can run on each CI platform. Update those pins when updating cloudflared.

Layout: `src/main.rs` (CLI), `gui.rs` (egui popup), `mcp.rs` (JSON-RPC + tools),
`http.rs` (std-only MCP HTTP server), `stdio.rs`, `moodle.rs` / `omnivox.rs` / `brightspace.rs`
(clients), `funnel.rs` (legacy Tailscale helpers, unused by the app), `catalog.rs` (embedded catalogue),
`tunnel.rs` (managed Quick Tunnel child process), `config.rs` (`~/.student-mcp.json`).
`background.rs` owns the tray icon and hide/reopen/quit lifecycle. Linux uses
StatusNotifier over D-Bus without GTK; Windows/macOS use native tray menus.

License: MIT.
