<h1 align="center">student-mcp</h1>
<p align="center">
  <img src="src/resources/assets/app-icon.png" width="72" alt="student-mcp tray icon">
</p>
<p align="center">Moodle, Omnivox and Brightspace in your MCP assistant.</p>

<p align="center"><img src="tools/screens/home.png" width="100%" alt="Local configuration page with Moodle connected and a demo AI connection link" /></p>

## Install

### Windows

Download **[student-mcp-setup.exe](https://github.com/CloudDown/student-mcp/releases/latest/download/student-mcp-setup.exe)**.

### macOS

Download **[student-mcp.dmg](https://github.com/CloudDown/student-mcp/releases/latest/download/student-mcp.dmg)**.

### Linux

```sh
curl -fsSL https://raw.githubusercontent.com/CloudDown/student-mcp/main/tools/packaging/install.sh | bash
```

The installer adds student-mcp to your application launcher. The raw
`student-mcp-linux-x64` / `student-mcp-linux-arm64` binaries are also available
from the release page, but do not install a launcher by themselves.

Windows/macOS builds are not yet publisher-signed. [Details](tools/GUIDE.md#installation).

The local page shows the installed version and update-check status at the top left,
opposite the language button. When a newer GitHub release is available, an update
button takes its place. Click it to download and verify
the update automatically. Linux updates in place and restarts; Windows/macOS open
the official installer to finish installation. Connected accounts are preserved.

<p align="center"><img src="tools/screens/omnivox.png" width="100%" alt="Omnivox browser sign-in on the local configuration page" /></p>

<table>
  <tr>
    <td width="33%" align="center"><img src="src/resources/assets/moodle-icon.png" width="48" alt="Moodle logo" /><br /><b>Moodle</b><br /><sub>Courses, deadlines, grades &amp; PDFs</sub></td>
    <td width="33%" align="center"><img src="src/resources/assets/omnivox-icon.png" width="48" alt="Omnivox logo" /><br /><b>Omnivox</b><br /><sub>Léa &amp; MIO · Courses, grades, documents, calendar and messaging</sub></td>
    <td width="33%" align="center"><img src="src/resources/assets/brightspace-icon.png" width="48" alt="Brightspace logo" /><br /><b>Brightspace</b><br /><sub>myCourses · Courses, content &amp; grades</sub></td>
  </tr>
</table>

## Omnivox is experimental

Léa and MIO work on some colleges and fail on others when the mobile page
changes. The local app shows an **Experimental** badge. Tool replies include the
same warning. If Omnivox returns a login page or unexpected HTML instead of
data, student-mcp says the college page may have changed — use the official
Omnivox site or app, then reconnect. Do not treat Omnivox answers as final
without checking the portal.

Sign in on the local page → copy the link → add it to your assistant's **MCP connectors**.
Keep the computer online. Closing the tab leaves a tray icon; quit from there or from the page.

Safari and Firefox can remain your default browser. School sign-in uses an installed
Chromium-based browser, or downloads a dedicated Chrome for Testing automatically
on first use. [Browser setup](tools/BROWSER-AUTH.md).

**v0.6.7:** Adds in-app update checks and verified downloads, plus application icons in desktop installers and the Linux application launcher. Includes curated Omnivox tools for Léa and MIO, improved document delivery and connection management, and more complete Moodle deadline reporting. Availability can vary by college and account permissions.

<p align="center"><a href="tools/GUIDE.md">Docs</a> · <a href="tools/GUIDE.md#4-automatic-public-link-for-web-ai">Privacy &amp; relay</a> · MIT code; <a href="src/services/moodle/catalog/LICENSE.md">GPL-3.0+ Moodle catalogue</a></p>
