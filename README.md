<h1 align="center">student-mcp</h1>

<p align="center">Connect your school services to Claude, Grok and other MCP assistants.</p>

<p align="center">
  <img src="assets/moodle-icon.png" alt="Moodle" width="72">
  &nbsp;&nbsp;
  <img src="assets/omnivox-icon.png" alt="Omnivox" width="72">
  &nbsp;&nbsp;
  <img src="assets/brightspace-icon.png" alt="Brightspace" width="72">
</p>

<p align="center">One small app. No terminal setup. Your credentials stay on your computer.</p>

## Install

Open the [latest release](https://github.com/CloudDown/student-mcp/releases/latest).

| Platform | Download | What to do |
|---|---|---|
| Windows | `student-mcp-windows-<x64\|arm64>-setup.exe` | Double-click, install, keep **Open student-mcp** selected |
| macOS | `student-mcp-macos-<x64\|arm64>.dmg` | Open the DMG, double-click **Install student-mcp**, confirm |
| Linux | `student-mcp-linux-<x64\|arm64>.zip` | Use the advanced terminal install below |

Choose **arm64** for Apple Silicon or Windows ARM, and **x64** for Intel / AMD.
The app opens automatically after installation. No Rust, Python, SSH client,
Cloudflare account or domain is required.

> Windows SmartScreen and macOS Gatekeeper may warn because public signing and
> notarization are not active yet. Do not disable system security protections.

### Linux / advanced install

```bash
curl -fsSL https://raw.githubusercontent.com/CloudDown/student-mcp/main/install.sh | bash
```

Windows PowerShell alternative:

```powershell
irm https://raw.githubusercontent.com/CloudDown/student-mcp/main/install.ps1 | iex
```

## Connect

1. Open the app.
2. Click **Moodle**, **Omnivox** or **Brightspace**.
3. Sign in with your normal school account.
4. Copy the HTTPS link shown below the service icons.
5. Paste it into your AI assistant's **MCP connector** settings.

That is all. Keep the app open; closing its window only hides it in the tray or
menu bar. Use **Quit student-mcp** to stop the app and public link.

<table>
  <tr>
    <td width="33%" align="center"><img src="assets/moodle-icon.png" alt="Moodle" width="96"><br><sub>Moodle</sub></td>
    <td width="33%" align="center"><img src="assets/omnivox-icon.png" alt="Omnivox" width="96"><br><sub>Omnivox</sub></td>
    <td width="33%" align="center"><img src="assets/brightspace-icon.png" alt="Brightspace" width="96"><br><sub>Brightspace</sub></td>
  </tr>
</table>

## What it can do

- **Moodle** — courses, deadlines, messages, assignments, quizzes, grades,
  calendars, forums and PDF reading.
- **Omnivox / Léa / Mio** — courses, documents, assignments, grades, schedule
  and calendar. Read-only. Email verification is supported when required.
- **Brightspace / myCourses** — courses, content and visible grades. Read-only.

Only connected services appear in the assistant. Multiple accounts per service
are supported.

## Privacy and security

- Passwords and email security codes are never saved.
- School sessions and tokens stay in the local configuration.
- The public link is a secret: anyone who has it can use the enabled tools.
- The HTTPS tunnel is provided by [srv.us](https://docs.srv.us/), which can
  technically read traffic. Use local MCP/stdio if that is not acceptable.
- Downloaded release files include SHA-256 checksums.

## Local MCP

For Claude Desktop, OpenCode and other local clients, use the main binary in
stdio mode instead of the desktop launcher:

```json
{ "student-mcp": { "command": "/path/to/student-mcp" } }
```

The GUI and local MCP server read the same configuration, so adding a service
in the app updates the available tools automatically.

## Documentation

- [User guide](docs/GUIDE.md) — setup, services, MCP clients and troubleshooting
- [Desktop distribution](docs/DISTRIBUTION.md) — packaging, signing and release validation
- [srv.us validation](docs/SRVUS-EXPERIMENT.md) — tunnel design and security limits

## Development

```bash
cargo test
cargo run -- up
python3 -m unittest discover -s scripts -p 'test_*.py'
```

License: MIT.
