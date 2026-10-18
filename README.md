<h1 align="center">Student MCP</h1>
<p align="center">Let OpenCode or Claude Desktop read your Moodle courses, assignments, deadlines and grades. One command, no Node.js.</p>

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

## Usage

1. The wizard asks for your Moodle URL (Enter keeps `https://moodle.polymtl.ca`; any Moodle works), your Moodle login, then **OpenCode** or **Claude Desktop**.
2. Restart the app and ask: *"Check my Moodle connection"*.
3. Then ask about your courses, deadlines or grades.

Your password is only used to request a token and is not stored. Write operations are off by default.

<p align="center"><a href="docs/GUIDE.md">Docs (FR)</a> · MIT (catalog GPL) · Not an official Polytechnique Montréal tool.</p>
