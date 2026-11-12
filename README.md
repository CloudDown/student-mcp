<h1 align="center">student-mcp</h1>
<p align="center">
  <img src="src/resources/assets/app-icon.png" width="72" alt="student-mcp tray icon">
</p>
<p align="center">Ask your AI about your courses, deadlines, grades and files — without typing your school password into the chat.</p>

## Install

- **Windows** — [student-mcp-setup.exe](https://github.com/CloudDown/student-mcp/releases/latest/download/student-mcp-setup.exe). Keep **Open student-mcp** selected.
- **macOS** — [student-mcp.dmg](https://github.com/CloudDown/student-mcp/releases/latest/download/student-mcp.dmg). Open it, run **Install student-mcp**, then **Install and open**.
- **Linux** — `curl -fsSL https://raw.githubusercontent.com/CloudDown/student-mcp/main/tools/packaging/install.sh | bash`

The builds are unsigned, so Windows or macOS may warn. Do not turn off system protections. On macOS, open the app once, then allow it in **System Settings → Privacy & Security → Open Anyway**. If macOS reports malware or damage, stop.

## 1. Open

<p align="center"><img src="tools/screens/home.png" width="100%" alt="student-mcp 0.8.4 home screen with school search, four learning platforms, link settings and assistant setup" /></p>

A local page opens. Closing the tab leaves a tray icon.

## 2. Connect your school

Pick a platform or search your school, then **Connect with browser** and sign in, MFA included. Only a token or a private browser profile is saved — never your password.

<p align="center"><img src="tools/screens/moodle.png" width="100%" alt="Moodle connection page with school address and Connect with browser" /></p>

Name each account and switch on the ones your assistant may use. Several accounts per platform are fine; one link serves them all.

<p align="center"><img src="tools/screens/profile.png" width="100%" alt="Moodle account named Léa, with assistant access switch and account controls" /></p>

<p align="center"><img src="tools/screens/omnivox.png" width="100%" alt="Omnivox sign-in with Connect with browser" /></p>

## 3. Copy your link

<p align="center"><img src="tools/screens/link.png" width="100%" alt="Local and public link cards with copy buttons; URLs are fictitious examples" /></p>

Copy it and keep this computer awake. The link is a secret: anyone who has it can use your enabled tools. Choose **On this computer** or **From the web**.

## 4. Add your assistant

Paste the complete link into the assistant's MCP connector settings, not into a chat. **Set up an assistant** can also configure a local app for you, with no terminal and no public link.

<p align="center"><img src="tools/screens/assistant.png" width="100%" alt="On this computer assistant setup with detected local apps, app logos, and a separate not-detected section" /></p>

| Assistant | Where |
|---|---|
| Claude | Settings → Connectors → custom connector. OAuth? **Continue anyway**. |
| ChatGPT | Plugins → custom MCP server, no authentication. |
| Grok | [Connectors](https://grok.com/connectors) → New Connector → Custom. |
| Gemini | Settings → Connected Apps → + Custom. Eligible accounts and regions only. |
| OpenCode | MCP settings, or stdio: `student-mcp`. |

## Privacy

School passwords are not stored. The public link is a secret, and the `srv.us` relay can read traffic. Moodle metadata is cached privately for 15 minutes, without file bytes. MCP requests are logged locally with redacted paths.

Guide, changes and maintenance: [tools/GUIDE.md](tools/GUIDE.md) · [CHANGELOG.md](CHANGELOG.md) · MIT code, [GPL-3.0+ Moodle catalogue](src/services/moodle/catalog/LICENSE.md).
