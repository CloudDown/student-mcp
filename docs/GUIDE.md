# student-mcp guide

A small window plus a local MCP server, with a built-in srv.us tunnel client.

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
signs in through the **mobile** Omnivox page (the one the official app uses),
keeps only the session cookies, and reads courses, documents, assignments,
grades, schedule and calendar from the mobile JSON endpoints. If Omnivox asks
for **identity verification**, the app shows a **Security code (6 digits)**
field. Enter the code received by email and click **Verify and connect**.
**Send a new code** requests another code in the same session; use the most
recent one. An invalid code can be retried, while an expired session requires
signing in again. **Cancel verification** discards the pending session. Neither
the password nor the code is saved, and existing accounts are replaced only
after the new connection is verified. The app also saves a random mobile device
id so the same mobile session can be reopened after a restart.
**Advanced options** also supports a local
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
- **Omnivox (including Léa/Mio)** signs in with the student's college login
  through Omnivox's mobile login page and reads the `/Mobl/` JSON endpoints.
  A web session from `/Login/Account/Login` is rejected by `/Mobl/`, so the
  connector keeps the same mobile identity (user agent, device id, AuthHash)
  for the whole session. The email-code verification API is supported directly.
  Other portal variants may need the local
  [omnivox-mcp](https://github.com/Beat-YT/omnivox-mcp) gateway under advanced
  options. Login and email-code form loading are checked against the live
  Maisonneuve MFA API; code validation, mobile-session establishment and
  persistence are covered by local HTTP fixtures. End-to-end verification with a
  real email code still depends on the college account.
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

1. Install the app; its single binary includes the SSH tunnel client.
2. Connect a service. A free, stable srv.us tunnel starts automatically:
   no additional account, domain, SSH installation or network setup.
3. Copy the **complete** `https://<stable-id>.srv.us/mcp/<token>` link
   into your AI's **MCP connector settings**, not a chat message. In Claude,
   choose "Continue anyway" if it warns about OAuth.
4. Keep the app running and the computer awake and online; you can close its
   window into the tray. To stop this public tunnel, turn off **Public link** in
   link settings, or use **Quit student-mcp** in the tray menu.
   Existing Tailscale configuration and other apps are left untouched.

The tunnel exposes only the app's current loopback HTTP port. The app reconnects
automatically after a network interruption and stops all tunnel tasks on quit or
when public access is disabled. **srv.us terminates HTTPS: its operator can
technically read/alter exchanges and see/reuse the secret link.** No project-hosted
relay is used. Anyone holding the complete secret link can use your enabled
tools, so do not share it or include it in screenshots. Prefer local stdio if
you do not accept this third-party trust.

**The hostname stays the same across app restarts and port changes.** The app
generates a dedicated SSH key in `~/.student-mcp.srvus-key`; back it up privately.
Deleting that file means a new hostname. The key is not your school login or MCP
token. `~/.student-mcp.srvus-host` records the relay's host key on first use;
unexpected changes stop public access instead of reconnecting to a different key.
These paths follow any custom configuration file location. A corrupted identity
is left unchanged and reported rather than silently resetting your address.

Replace an old Cloudflare connector URL **once** with this new srv.us URL.
There is no automatic fallback to temporary hostnames. srv.us is best-effort:
there is no uptime or two-million-user capacity guarantee. Networks blocking
outbound TCP port 22 prevent the tunnel from connecting. The current MCP server
answers with JSON rather than SSE. Compatibility with your actual AI client and
its connector/subscription rules must still be checked.

See [srv.us](https://docs.srv.us/) and [security and validation](SRVUS-EXPERIMENT.md).

The token protects the URL: without it you get a 401. You can rotate the token
by quitting the app, deleting `gateway.token` in `~/.student-mcp.json`, then
restarting. Copy the new full connector URL; its hostname stays the same.

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
- **Moodle did not return / provide a token**: school WAFs (AWS on
  moodle.polymtl.ca) challenge ureq's default User-Agent with HTTP 202 and an
  empty HTML body. All Moodle HTTP clients must send `session::BROWSER_UA`.
  Do not treat that 202 as a missing mobile token.
- **Brightspace asks for SSO cookies**: the school sends you through Microsoft
  or another identity provider. Sign in in your browser, copy `d2lSessionVal`
  and `d2lSecureSessionVal`, and paste them under **Advanced options**.
- **Omnivox asks for identity verification**: enter the emailed 6-digit code in
  the app. Use **Send a new code** if needed. If the session expires, sign in
  again to start a new verification.
- **The assistant cannot open a Moodle PDF / stops mid-chapter**: Moodle file
  URLs need the web service token, which is never given to the model. Use
  `moodle_file` with the `fileurl` from `moodle_course_content`; do not fetch
  that URL with a generic HTTP tool. `moodle_file` now returns:
  - `find` results (`matchedPage`, `matches`) when you pass a section such as
    `"3.3"` or a phrase;
  - extracted text, page images, and `totalPages` / `truncated` /
    `nextPageFrom` for the requested window (up to 24 pages, plus a short tail,
    so a 30-page chapter arrives whole);
  - `links.reader` (full HTML reader with every page), `links.download` (direct
    PDF), `links.text` (plain text), and `links.parts` (one URL per page window).

  Links exist in `student-mcp up` mode (local link and/or public tunnel); stdio
  clients receive the inline blob, text and images instead. PDFs larger than
  6 MB are not inlined as base64 when links exist: use `links.download`. Open `links.reader`
  when a chapter is long, or call `moodle_file` again with
  `page_from=nextPageFrom`. Do not invent missing pages. `links.localOnly` is
  true when no public tunnel is active: a browser AI cannot reach that address,
  so rely on the inline text/images or turn the public link on. Install Poppler
  (`pdftotext`, `pdftoppm`, `pdfinfo`, `pdftocairo`) for PDF rendering/slicing,
  and Tesseract for scanned PDFs without a text layer.
- **The shared file link says it expired**: links are capability URLs valid for
  three hours. Call `moodle_file` again to mint a fresh one. Anyone with the
  link can read that one file until it expires; the Moodle token is never in
  the link.
- **Claude cannot register with the sign-in service** (`ofid_…` reference):
  Claude started an OAuth flow, but this server deliberately has no OAuth. All
  `/.well-known/oauth-*` paths return 404. Delete the connector, re-add it with
  the **complete** `/mcp/<token>` link, and choose **Continue anyway**. If your
  Claude version offers request headers, use `Authorization: Bearer <token>`.
- **No public link**: connect a service first, then open link settings for the
  error. Network failures retry automatically. Check access to `srv.us` on TCP
  port 22. Identity file errors require checking folder permissions/restoring
  your backup; they do not silently regenerate an existing key.
- **srv.us server identity changed**: public access stops. Verify the change
  with the provider before touching `~/.student-mcp.srvus-host`; do not simply
  delete it to bypass the warning.
- **Claude reports HTTP 502 or an expired link**: keep the app open. A freshly
  generated hostname may take time to resolve: first wait and retry the same
  link. The app reconnects automatically, keeping that address. Firewalls or
  srv.us outages can also block it. A stable URL does not make an offline app
  reachable.
