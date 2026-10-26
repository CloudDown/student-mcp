# student-mcp guide

A small window plus a local MCP server, with a bundled Cloudflare tunnel component.

## 1. What you see when it opens

```
         [ Moodle ]  [ Omnivox ]  [ Brightspace ]

              <complete link>          [copy]
```

Only logos appear on the home screen (names and states are in tooltips). Click
an unconnected service to enter credentials; after a successful connection you
return home and its logo has a green outline. No unavailable-service tiles are
shown.
After you connect a service, the app automatically creates a public HTTPS link.
Click it for link settings, or use the adjacent button to copy the complete URL.
While the tunnel is starting or unavailable, copying is disabled so a local URL
cannot accidentally be used in a web AI. Local access is still in link settings.

### Close the window, keep the app running

On Windows/Linux, the app has an icon in the notification area; on macOS it is
in the menu bar. Closing the window hides it without stopping the local MCP
server, connected services or current public link. Use **Open student-mcp** in
the icon's menu to restore the window, and **Quit student-mcp** to stop the app
and tunnel completely. No additional startup/login service is installed.

Linux needs a StatusNotifier-compatible tray host (e.g. Plasma or a tray-enabled
panel; some GNOME setups require an AppIndicator extension). If none is
available, closing the window quits normally. If the tray service disappears
while the window is hidden, the app restores its window rather than stranding
you without controls. Link settings describe the currently available behavior.
Linux uses X11/XWayland for hide/restore because the windowing backend cannot
hide Wayland windows. On a pure Wayland session without `DISPLAY`, background
mode is disabled and closing the window quits normally.

## 2. Connecting a service

**Moodle.** Two visible modes.

1. *School login*: firstname.lastname + password → the binary calls
   `/login/token.php` with the `moodle_mobile_app` service; only the **token**
   is saved, never the password. If your school uses Microsoft SSO or 2FA, this
   mode can be refused.
2. *Mobile token*: paste an existing mobile token. This is the reliable option
   when SSO blocks the password flow.

**Omnivox (Léa / Mio).** Enter the college Omnivox address
(`https://cegepxxx.omnivox.ca`), your student number and password. The app
posts the official login form, keeps only the session cookies, and reads
courses, documents, assignments, grades, schedule and calendar from the
mobile JSON endpoints. The password is never stored. If Omnivox then asks for
two-factor authentication, complete it in the portal and retry, or use
**Advanced options** with a local
[omnivox-mcp](https://github.com/Beat-YT/omnivox-mcp) gateway (`http://127.0.0.1:3000`
and `accessKey.txt`).

**Brightspace / myCourses.** Enter your institution's Brightspace site address
and your normal username and password. The app signs in on the school login
page, keeps only the session cookies, and verifies your profile and course
enrollments. This connector only reads your account, courses, content and
visible grades. Course lists retain `PagingInfo`; supply `bookmark` to request
the next page. The password is never stored.

If the school uses Microsoft SSO, the password form is refused. Open Brightspace
in your browser, sign in, then under **Advanced options** paste the
`d2lSessionVal` and `d2lSecureSessionVal` cookies (or an institution-authorized
OAuth token, without `Bearer`). Sessions expire and must be replaced via
**Change login**. The connector uses LP API 1.49 and LE API 1.82.

Once configured, clicking its logo opens a **manage** screen. A live check
runs automatically and can be retried with **Retry**. The saved connection
remains manageable even if the remote service is offline: you can still change
credentials or disconnect. The screen also shows an **Allow changes** switch —
no need to re-enter credentials to change permissions. Omnivox and Brightspace
are always read-only and have no write switch.

### Quebec integration targets

Target order: Moodle → Omnivox → Brightspace → monPortail/Brio.
Only implemented connectors are displayed, without empty slots.

- **Moodle** also covers installations such as StudiUM; it is not a separate
  platform. Each institution must enable the required mobile/Web Service access.
- **Omnivox (including Léa/Mio)** signs in with the student's college login and
  reads the `/Mobl/` JSON endpoints. Two-factor and some college portals may
  still need the local [omnivox-mcp](https://github.com/Beat-YT/omnivox-mcp)
  gateway under advanced options. The connector is tested with local HTTP
  fixtures, not a live college account.
- **Brightspace** signs in with the student's school login and reads the Valence
  API with session cookies (OAuth tokens still work). SSO schools need cookies
  pasted from the browser. Tested with local HTTP fixtures; it has not been
  validated against a real institutional account.
- **monPortail/Brio** is not integrated yet. Laval requires institutional SSO/MFA;
  no suitable public student API was found. Current browser-session evidence and
  an authorized test account are needed; old monPortail API examples from 2017
  are not proof of compatibility with Brio in 2026.

References: [D2L API authentication](https://docs.valence.desire2learn.com/basic/oauth2.html),
[McGill myCourses](https://www.mcgill.ca/student-learning-resources/learning-technologies/mycourses-tutorials),
[Omnivox](https://skytech.com/solutions/omnivox),
[Brio and monPortail](https://www.enseigner.ulaval.ca/numerique-et-ia/brio-et-monportail-au-coeur-de-votre-environnement-numerique-denseignement).

## 3. Local AI assistant

- **Claude Desktop / OpenCode / many others**: point the assistant at the binary
  as a stdio command:
  - macOS / Linux: `~/.student-mcp/bin/student-mcp`
  - Windows: `%LOCALAPPDATA%\student-mcp\student-mcp.exe`
- The tool list follows the window: add or remove a service and the tools appear
  or disappear on the next call (the config is re-read every time).

## 4. Automatic public link for web AI

1. Install the app; the installer includes a checksum-verified `cloudflared`.
2. Connect a service. A free Cloudflare Quick Tunnel starts automatically:
   no additional account, domain, sudo command or network setup.
3. Copy the **complete** `https://random-name.trycloudflare.com/mcp/<token>` link
   into your AI's **MCP connector settings**, not a chat message. In Claude,
   choose "Continue anyway" if it warns about OAuth.
4. Keep the app running and the computer awake and online; you can close its
   window into the tray. To stop this public tunnel, use **Remove the public
   link**, disconnect the last service, or **Quit student-mcp** in the tray menu.
   Existing Tailscale configuration and other apps are left untouched.

The tunnel exposes only the app's current loopback HTTP port. The app owns its
child process, reports startup failures and unexpected exits, and lets you retry
from link settings. Cloudflare can see the proxied exchanges; no project-hosted
relay is used. Anyone holding the complete secret link can use your enabled
tools, so do not share it or include it in screenshots.

**The hostname changes whenever a new tunnel is created.** Replace the saved
connector URL after restarting the app/tunnel. Quick Tunnels are for testing and
development, with no uptime guarantee, a 200 in-flight request limit, and no SSE.
The current MCP server answers with JSON rather than SSE. Compatibility with
your actual AI client and its connector/subscription rules must still be checked.

See [Cloudflare Quick Tunnels](https://developers.cloudflare.com/tunnel/get-started/quick-tunnels/).

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
- **Moodle rejects the school login**: SSO or 2FA blocks `/login/token.php` →
  switch to the **Mobile token** mode.
- **Brightspace asks for SSO cookies**: the school sends you through Microsoft
  or another identity provider. Sign in in your browser, copy `d2lSessionVal`
  and `d2lSecureSessionVal`, and paste them under **Advanced options**.
- **Omnivox asks for two-factor**: complete it in the portal, then retry, or
  use the Omnivox MCP gateway under **Advanced options**.
- **The assistant cannot open a Moodle PDF**: Moodle file URLs need the web
  service token, which is never given to the model. Use `moodle_file` with the
  `fileurl` from `moodle_course_content`; do not fetch that URL with a generic
  HTTP tool. The original PDF is attached as an MCP resource; scanned pages are
  also rasterized with `pdftoppm` (up to 16 images). Install Poppler on this
  computer. The AI client must accept MCP file/image content.
- **Claude cannot register with the sign-in service** (`ofid_…` reference):
  Claude started an OAuth flow, but this server deliberately has no OAuth. All
  `/.well-known/oauth-*` paths return 404. Delete the connector, re-add it with
  the **complete** `/mcp/<token>` link, and choose **Continue anyway**. If your
  Claude version offers request headers, use `Authorization: Bearer <token>`.
- **No public link**: connect a service first, then open link settings for the
  error and retry. If `cloudflared` is missing, reinstall from a release that
  bundles it. For source builds, follow the sidecar instructions in the README.
- **Claude reports HTTP 502 or an expired link**: keep the app open. A freshly
  generated hostname may take about a minute to resolve: first wait and retry
  the same link. If the tunnel stopped, retry it in link settings, then replace
  the connector URL with the new one. Firewalls or Cloudflare outages can also
  block it.
