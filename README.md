<h1 align="center">student-mcp</h1>
<p align="center">Moodle, Omnivox and Brightspace in your MCP assistant.</p>

<p align="center"><img src="docs/screens/home.png" width="100%" alt="student-mcp home with school services and a demo AI connection link" /></p>

## Install

### Windows

Download the **[setup.exe](https://github.com/CloudDown/student-mcp/releases/latest)** → double-click → install and open.
Choose **x64** for Intel / AMD, **arm64** for Windows ARM.

### macOS

Download the **[.dmg](https://github.com/CloudDown/student-mcp/releases/latest)** → open → double-click **Install student-mcp**.
Choose **arm64** for Apple Silicon, **x64** for Intel.

### Linux

```sh
curl -fsSL https://raw.githubusercontent.com/CloudDown/student-mcp/main/install.sh | bash
```

Windows/macOS builds are not yet publisher-signed/notarized and may be blocked.
[Installation details](docs/DISTRIBUTION.md).

<p align="center"><img src="docs/screens/moodle.png" width="100%" alt="Moodle sign-in screen with empty fields" /></p>

<table>
  <tr>
    <td width="33%" align="center"><img src="assets/moodle-icon.png" width="48" alt="Moodle logo" /><br /><b>Moodle</b><br /><sub>Courses, deadlines, grades &amp; PDFs</sub></td>
    <td width="33%" align="center"><img src="assets/omnivox-icon.png" width="48" alt="Omnivox logo" /><br /><b>Omnivox</b><br /><sub>Léa / Mio · Grades, documents &amp; schedule</sub></td>
    <td width="33%" align="center"><img src="assets/brightspace-icon.png" width="48" alt="Brightspace logo" /><br /><b>Brightspace</b><br /><sub>myCourses · Courses, content &amp; grades</sub></td>
  </tr>
</table>

Sign in → copy the link → add it to your assistant's **MCP connectors**.
Keep the app running and your computer online. Keep the link private.

<p align="center"><a href="docs/GUIDE.md">Docs</a> · <a href="docs/SRVUS-EXPERIMENT.md#security-boundary">Privacy &amp; relay</a> · MIT</p>
