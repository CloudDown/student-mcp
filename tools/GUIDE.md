# student-mcp guide

A local browser page plus a Rust MCP server, with a built-in srv.us tunnel client.

## Installation

Download from the [latest release](https://github.com/CloudDown/student-mcp/releases/latest):

- **Windows:** double-click `student-mcp-setup.exe`,
  install, and leave **Open student-mcp** selected. Reopen from the Start menu;
  uninstall through Windows Settings → Apps.
- **macOS:** open `student-mcp.dmg`, double-click
  **Install student-mcp**, then **Install and open**. Reopen from Applications.
  To remove it, stop it from its browser page and move the app to the Trash.
- One download per desktop OS: Intel/AMD and ARM are handled automatically.
- Quit any running copy before upgrading. Removing/updating the application
  does not delete saved accounts or tunnel identity files in your home folder.

The installers are not yet publisher-signed/notarized; OS security checks may
warn or block them. Do not disable those checks. See
[release maintenance](#release-maintenance) for build and signing requirements.
Linux and advanced users can still use `tools/packaging/install.sh` / `tools/packaging/install.ps1`, as described
in the [README](../README.md#install). No ZIPs are published.

## 1. What you see when it opens

Your browser opens a local page with accounts, login, verification, and MCP link
settings. Its separate loopback listener is never forwarded through the tunnel.
For Claude web, wait for a public HTTPS link; a localhost link only works locally.

### Close the tab, keep the app running

Closing the tab leaves the connector running. A small tray icon (notification
area / status bar) stays visible while the app is up: click it or use
**Open student-mcp** to reopen the page, and **Quit student-mcp** to stop the
server and tunnel. You can also stop from the page with **Arrêter student-mcp**.
Launch student-mcp again if the tray is gone and you need the page.

## 2. Connecting a service

**Moodle.** Two authentication options.

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
and the two session cookie **values** from your browser after SSO:
`d2lSessionVal` and `d2lSecureSessionVal` (separate fields; values only). The
**⋯** help control explains how to copy them from DevTools. An
institution-authorized OAuth access token can go under **Advanced options**
instead (without `Bearer`). The app verifies your profile and course
enrollments. Read tools cover courses, content, grades, assignments (dropbox +
due dates), quizzes, calendar, aggregated deadlines and course news.
`brightspace_file` reads documents using `courseid` and a file topic's `Id`
from `brightspace_course_content` as `topicid`. PDFs are returned as original
resources/download links by default; `format=text` supports page selection and
`find`, while `format=images` handles scans and diagrams. The 12 MB download
limit also applies here. External links and redirected downloads are refused so
session cookies never leave the configured site.
`brightspace_assignment_submit` posts a file to a dropbox folder when **Allow
assistant changes** is on for that account. Provide `courseid` + `folderid`, and
exactly one of `path` (file on the machine running student-mcp),
`content_base64` + `filename`, or `text`. Write tools advertise
`readOnlyHint: false` so MCP clients can require confirmation; the app toggle is
a second gate. Course lists retain `PagingInfo`; supply `bookmark` to request
the next page. Prefer `brightspace_deadlines` for upcoming work. Sessions expire
and must be replaced via **Change login**. The connector uses LP API 1.49 and LE
API 1.82.

Saved accounts have check, reconnect and disconnect actions. Moodle and
Brightspace write permissions can be changed without re-entering credentials.
Omnivox remains read-only.

### Additional Brightspace workflows

Brightspace now exposes 55 tools. Beyond the original course/file/assignment
tools, the connector supports discussion forum/topic/post reading, thread/reply
creation, deletion and read status; course-local TOC title/URL search; native
content due items, recent/bookmarked topics and completion counts; own final
grades and item feedback; alerts and course update counters; groups/sections
and visible participants; checklist/survey definitions; full announcements and
dismiss/unhide actions; course pin/unpin; profile/locale reads; notification
channels, subscriptions and subscribe/unsubscribe.

New group-membership discovery requires LP API 1.64 and checks support first.
Joining/leaving groups targets only the connected user and may be refused by
institutional permissions. Notification carrier/message type IDs are GUIDs
from the channel/subscription results, not numeric course IDs. Discussion
creation currently accepts plain text only, not attachments or anonymous/group
topics. Search does not search PDF bodies. Paging metadata is preserved; request
subsequent bookmarks/pages rather than treating the first page as exhaustive.

All writes require **Allow assistant changes**. Client confirmation depends on
the client's settings, not just MCP annotations. If an action times out or
returns an unexpected response, inspect its actual state before retrying.
These additions have local HTTP-fixture tests, not live institutional validation.
Optional modules (locker, awards, ePortfolio, CPD) are not implemented yet;
see [coverage and remaining work](BRIGHTSPACE-COVERAGE.md).

### Quebec integration targets

Target order: Moodle → Omnivox → Brightspace → monPortail/Brio.
Only implemented connectors are displayed, without empty slots.

**Status (v0.6.0):** Moodle and Brightspace are the stable student connectors.
Omnivox remains experimental: login/MFA may still fail on real college portals.

- **Moodle** also covers installations such as StudiUM; it is not a separate
  platform. Each institution must enable the required mobile/Web Service access.
- **Omnivox (Léa; Mio messaging is not implemented)** signs in with the student's college login
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
- **Brightspace** uses the Valence API with session cookies pasted from the
  browser after SSO (OAuth tokens still work). Tools cover content, grades,
  dropbox assignments (including student submit when writes are enabled),
  quizzes, calendar, deadlines and news. Tested with local HTTP fixtures; live
  validation still depends on a real institutional account.
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
  - macOS native install: `/Applications/student-mcp.app/Contents/MacOS/student-mcp`
  - Linux / terminal install on macOS: `~/.student-mcp/bin/student-mcp`
  - Windows: `%LOCALAPPDATA%\student-mcp\student-mcp.exe`
- Use the main binary, not `student-mcp-desktop`: the desktop launcher always
   opens the browser page, while the main binary starts MCP stdio without arguments.
- The tool list follows the window: add or remove a service and the tools appear
  or disappear on the next call (the config is re-read every time).
- Tool naming, parameters, annotations and result shape follow
  [MCP tool conventions](MCP-CONVENTIONS.md). Restart student-mcp after upgrades
  so assistants reload the live `tools/list` catalogue.

## 4. Automatic public link for web AI

1. Install the app; its single binary includes the SSH tunnel client.
2. Connect a service. A free, stable srv.us tunnel starts automatically:
   no additional account, domain, SSH installation or network setup.
3. Copy the **complete** `https://<stable-id>.srv.us/mcp/<token>` link
   into your AI's **MCP connector settings**, not a chat message. In Claude,
   choose "Continue anyway" if it warns about OAuth.
4. Keep the app running and the computer awake and online; you can close its
   browser tab. To stop this public tunnel, turn off the public link and apply
   settings, or use **Arrêter student-mcp** on the page.
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

SSH host verification is trust on first use: the first key is accepted without
independent authentication, and later changes are refused. The tunnel key does
not authenticate visitors; the secret MCP token does. School credentials must
remain local, but tool results are disclosed to the AI provider and the relay.
See [srv.us documentation](https://docs.srv.us/).

The token protects the URL: without it you get a 401. You can rotate the token
by quitting the app, deleting `gateway.token` in `~/.student-mcp.json`, then
restarting. Copy the new full connector URL; its hostname stays the same.

## 5. Commands

```
student-mcp            # MCP stdio (local AI assistant)
student-mcp up         # browser page + local HTTP server
student-mcp up --port=9000
student-mcp install    # reopens the page (used by the installer)
student-mcp version
```

## 6. Troubleshooting

- **The browser does not open**: use the local URL printed on launch, or use stdio
  on a headless machine.
- **Port already taken**: an existing instance is reopened; for another process
  occupying that port, choose a different port with `--port=`.
- **Moodle rejects the school login**: SSO or 2FA blocks `/login/token.php` →
  switch to the **Mobile token** mode.
- **Moodle did not return / provide a token**: school WAFs (AWS on
  moodle.polymtl.ca) challenge ureq's default User-Agent with HTTP 202 and an
  empty HTML body. All Moodle HTTP clients must send `session::BROWSER_UA`.
  Do not treat that 202 as a missing mobile token.
- **Brightspace asks for SSO cookies**: the school sends you through Microsoft
  or another identity provider. Sign in in your browser, copy the **values** of
  `d2lSessionVal` and `d2lSecureSessionVal`, and paste each in its field (see
  the **⋯** help).
- **Omnivox asks for identity verification**: enter the emailed 6-digit code in
  the app. Use **Send a new code** if needed. If the session expires, sign in
  again to start a new verification.
- **The assistant cannot open a Moodle PDF / stops mid-chapter**: Moodle file
  URLs need the web service token, which is never given to the model. Use
  `moodle_file` with the `fileurl` from `moodle_course_content`; do not fetch
  that URL with a generic HTTP tool. The default `format=file` returns the original
  PDF resource and a download link when HTTP sharing is available, without running
  local PDF tools or duplicating the document as text/images. Above 6 MB, HTTP
  clients receive the download link; stdio still receives the original file.

  If your client cannot consume PDF resources, use `format=text` with
  `page_from`/`page_to` (20,000 characters maximum), or `format=images` for scans
  and diagrams (2 pages by default, at most 8). There is no local OCR: the LLM
  interprets the PDF or the requested images. `find` searches a section/phrase
  in the text layer. Page/search arguments without a format default to text.
  Follow `nextPageFrom` when `truncated` is true, or reduce the page range when
  `textTruncated` is true. Downloads are cached for 60 seconds per account.

  Poppler (`pdftotext`, `pdftoppm`, `pdfinfo`, `pdftocairo`) is needed only for
  requested extraction/rendering. Search results also offer original-file and
  text links. Localhost links cannot be opened by a remote browser
  AI; use inline resources or enable the public link.
- **The shared file link says it expired**: links are capability URLs valid for
  up to three hours (4 files / 24 MiB maximum). Call `moodle_file` again to mint a fresh one. Anyone with the
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

## Release maintenance

Run checks from the repository root:

```sh
cargo test --locked
python3 -B -m unittest discover -s tools -p 'test_*.py'
```

The optional tunnel test opens real public tunnels with synthetic requests and
isolated credentials; it never uses saved school accounts:

```sh
cargo test --locked public_mcp_smoke -- --ignored --nocapture
```

`.github/workflows/release.yml` builds x64/ARM64 binaries, checks native installers
and publishes tags beginning with `v`. `tools/stage-release.py` stages Linux
binaries with SHA-256 sidecars; `tools/package-desktop.py` creates the universal
Windows installer and macOS disk image from both architecture payloads.
Installer sources and terminal install scripts are in `tools/packaging/`.

Packaging requires Python 3.11+, Inno Setup on Windows, or Xcode Command Line
Tools on macOS. For example:

```sh
python3 tools/package-desktop.py macos \
  --x64-binary target/x86_64-apple-darwin/release/student-mcp \
  --x64-launcher target/x86_64-apple-darwin/release/student-mcp-desktop \
  --arm64-binary target/aarch64-apple-darwin/release/student-mcp \
  --arm64-launcher target/aarch64-apple-darwin/release/student-mcp-desktop
```

For Windows, use the `windows` subcommand and corresponding MSVC `.exe` paths.
Output goes to `release/`. The Windows C runtime is statically linked; macOS
uses system libraries. No compiler or extra runtime is installed on users' PCs.

SHA-256 sidecars check integrity, not publisher identity. Public releases still
need Authenticode signatures on Windows and Developer ID signing, notarization
and stapling on macOS; current macOS ad-hoc signatures do not provide these.
Keep signing credentials outside the repository and generate checksums after
signing. Test browser-downloaded artifacts on clean native Windows/macOS systems
for first launch, upgrades, account preservation and uninstall. Automated checks
do not prove institutional login or actual AI-client interoperability.
