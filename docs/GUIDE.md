# student-mcp guide

A small window plus a local MCP server. All in one, no web framework.

## 1. What you see when it opens

```
student-mcp  v0.2.0
● Local MCP server: 127.0.0.1:8420
Local connector (Claude Desktop, OpenCode…):
  http://127.0.0.1:8420/mcp/<token>            [Copy link]
Your services
  [M   Moodle  — Connected / "Courses, assignments, grades…"]
  [P   Proxmox — Connected / "VMs and containers"           ]
  [D   Student record — Coming soon]
  [@   Poly email     — Coming soon]
[View the link for AI tools]
```

Services you have already connected show up in green, with a summary and a
**Disconnect** button instead of the credential form.

## 2. Connecting a service

**Moodle.** Two ways.
1. *Poly credentials*: firstname.lastname + password → the binary calls
   `/login/token.php` with the `moodle_mobile_app` service; only the **token**
   is saved, never the password.
2. *Existing token*: paste a mobile token (safer when you use two-factor auth).

**Proxmox.** Datacenter → Permissions → API Tokens → create a token (for
example `student@pam!mcp`), copy its *secret*, then enter
`https://your-pve:8006`, the token ID and the secret.

If the server uses a self-signed certificate the app refuses the connection
(strict HTTPS): use a real certificate or have the PVE certificate signed.

## 3. Local AI assistant

- **Claude Desktop / OpenCode / many others**: point the assistant at the binary
  as a stdio command:
  - macOS / Linux: `~/.student-mcp/bin/student-mcp`
  - Windows: `%LOCALAPPDATA%\student-mcp\student-mcp.exe`
- The tool list follows the window: add or remove a service and the tools appear
  or disappear on the next call (the config is re-read every time).

## 4. Public link for Claude web

1. Install and sign in to **Tailscale** once: `tailscale login`.
2. In the window → **Create my public link**. If the Linux daemon needs sudo,
   the window shows the exact command (`sudo tailscale funnel --bg 8420`).
3. Paste `https://your-machine.ts.net/mcp/<token>` into Claude
   (custom connector) and choose "Continue anyway" when it warns about OAuth.
4. To stop: **Remove the public link**, or run `tailscale funnel off`.

The token protects the URL: without it you get a 401. You can rotate the token
by deleting `gateway.token` in `~/.student-mcp.json`.

## 5. Commands

```
student-mcp            # MCP stdio (local AI assistant)
student-mcp up         # window + local HTTP server
student-mcp up --port=9000
student-mcp install    # reopens the window (used by the installer)
student-mcp version
```

## 6. Troubleshooting

- **The window does not open (SSH)**: the window needs a display; on a headless
  server use the stdio mode only.
- **Port already taken**: `up` tries 8420→8429 automatically; force one with
  `--port=`.
- **Moodle rejects the SSO login**: two-factor auth blocks `/login/token.php` →
  use the existing-token method.
- **The Funnel does not start**: is Tailscale installed? signed in?
  (`tailscale status`) then `sudo tailscale funnel --bg 8420`.
- **Invalid PVE certificate**: the app refuses under strict HTTPS; fix the
  server certificate.
