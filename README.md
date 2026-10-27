<h1 align="center">student-mcp</h1>
<p align="center">Moodle, Omnivox and Brightspace in your MCP assistant.</p>

A small Rust connector configured in your browser: no native GUI runtime,
frontend build toolchain, or hosted configuration service.

## Repository layout

- `src/services/` — one folder per connector: `moodle/`, `omnivox/`, `brightspace/`.
- `src/` — shared application code; embedded interface and icons in `src/resources/`.
- `tools/` — maintenance, tests, installer templates and a single [guide](tools/GUIDE.md).

Installation scripts and templates live in `tools/packaging/`.
`target/` is a generated build cache, not source code; Cargo recreates it when needed.

## Install

### Windows

Download **[student-mcp-setup.exe](https://github.com/CloudDown/student-mcp/releases/latest/download/student-mcp-setup.exe)** → double-click → install and open.
The installer automatically selects Intel / AMD or Windows ARM.

### macOS

Download **[student-mcp.dmg](https://github.com/CloudDown/student-mcp/releases/latest/download/student-mcp.dmg)** → open → double-click **Install student-mcp**.
The same download works on Apple Silicon and Intel. No ZIP or architecture to choose.

### Linux

```sh
curl -fsSL https://raw.githubusercontent.com/CloudDown/student-mcp/main/tools/packaging/install.sh | bash
```

Windows/macOS builds are not yet publisher-signed/notarized and may be blocked.
[Installation details](tools/GUIDE.md#installation).

<table>
  <tr>
    <td width="33%" align="center"><img src="src/resources/assets/moodle-icon.png" width="48" alt="Moodle logo" /><br /><b>Moodle</b><br /><sub>Courses, deadlines, grades &amp; PDFs</sub></td>
    <td width="33%" align="center"><img src="src/resources/assets/omnivox-icon.png" width="48" alt="Omnivox logo" /><br /><b>Omnivox</b><br /><sub>Léa · Grades, documents &amp; schedule</sub></td>
    <td width="33%" align="center"><img src="src/resources/assets/brightspace-icon.png" width="48" alt="Brightspace logo" /><br /><b>Brightspace</b><br /><sub>myCourses · Courses, content &amp; grades</sub></td>
  </tr>
</table>

Sign in → copy the link → add it to your assistant's **MCP connectors**.
Keep the app running and your computer online. Keep the link private.
Closing the browser tab keeps the connector running; launch it again to reopen
settings, or stop it from the page. Configuration stays on a separate local port.

PDFs are sent as original files by default. Text extraction and page images are
available on request; no local OCR is needed. Brightspace is experimental (local
fixtures only); complete Omnivox MFA still needs institutional end-to-end validation.

<p align="center"><a href="tools/GUIDE.md">Docs</a> · <a href="tools/GUIDE.md#4-automatic-public-link-for-web-ai">Privacy &amp; relay</a> · MIT code; <a href="src/services/moodle/catalog/LICENSE.md">GPL-3.0+ Moodle catalogue</a></p>
