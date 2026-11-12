# student-mcp guide

A local browser page plus a Rust MCP server, with a built-in srv.us tunnel client.

## Installation

Install from the [latest release](https://github.com/CloudDown/student-mcp/releases/latest):

- **Windows:** download `student-mcp-setup.exe`, double-click it, and leave
  **Open student-mcp** selected. It supports x64 and ARM64. Reopen from the Start
  menu; uninstall through Windows Settings → Apps. The optional PowerShell
  `tools/packaging/install.ps1` downloads and runs the same setup file.
- **macOS:** open `student-mcp.dmg`, double-click
  **Install student-mcp**, then **Install and open**. Reopen from Applications.
  If macOS blocks opening, follow the [macOS security steps](#macos-security-warning)
  below; the authorization is available after the first blocked opening attempt.
  To remove it, stop it from its browser page and move the app to the Trash.
- Windows and macOS choose Intel/AMD or ARM automatically.
- Quit any running copy before upgrading. Removing/updating the application
  does not delete saved accounts or tunnel identity files in your home folder.

The installers are not yet publisher-signed/notarized; OS security checks may
warn or block them. Do not disable those checks. See
[release maintenance](#release-maintenance) for build and signing requirements.
Linux uses `tools/packaging/install.sh`, as described in the
[README](../README.md#install). It adds student-mcp and its icon to the user's
application menu. Downloading the raw binary alone does not create a launcher.
No ZIPs are published.

### macOS security warning

The current download is not signed with an Apple Developer ID or notarized by
Apple. macOS may therefore block the installer or the installed app. Only proceed
if you trust the download from the [official GitHub release](https://github.com/CloudDown/student-mcp/releases/latest).

1. Open the DMG and try opening **Install student-mcp**. Dismiss the warning
   about an unidentified developer or Apple being unable to check the app.
2. Open **System Settings → Privacy & Security**, scroll to **Security**, and
   click **Open Anyway** for **Install student-mcp**.
3. Confirm **Open**, then select **Install and open** in the installer.
4. If the installed **student-mcp** app is blocked by the same warning, try
   opening it from Applications and repeat the authorization for that app.

On a French-language Mac, the path is **Réglages Système → Confidentialité et
sécurité → Ouvrir quand même**. This adds an exception for the selected app;
it does not require disabling Gatekeeper or removing quarantine attributes.
If the button is unavailable on a school-managed Mac, contact its administrator.
If macOS reports malware, damage, or that the app will harm your computer, stop
rather than override the warning. See [Apple's instructions](https://support.apple.com/en-us/102445).

## 1. What you see when it opens

Your browser opens a local page with accounts, login, verification, and MCP link
settings. Its separate loopback listener is never forwarded through the tunnel.
For Claude web, wait for a public HTTPS link; a localhost link only works locally.
**Set up an assistant** under **Your link** opens the assistant guides; see
[Local AI assistant](#3-local-ai-assistant).

### Close the tab, keep the app running

Closing the tab leaves the connector running. A small tray icon (notification
area / status bar) stays visible while the app is up: click it or use
**Open student-mcp** to reopen the page, and **Quit student-mcp** to stop the
server and tunnel. You can also stop from the page with **Arrêter student-mcp**.
Launch student-mcp again if the tray is gone and you need the page.
At the top right of the window, turn on **Launch at startup** to start automatically when you
sign in after boot, without opening a browser tab. The public link reconnects
automatically when configured. Turn it off to remove the startup entry.

## 2. Connecting a service

Type your school in the search field: the matching service opens
with its address already filled in. The four tiles stay available when you
already know which platform your school uses.

**Canvas.** Same browser sign-in as Brightspace. Enter the school Canvas
address (ZoneCours HEC: `https://zonecours2.hec.ca`) and **Connect with
browser**. student-mcp stores a private browser profile, not a personal
access token (student tokens expire and are against Instructure policy).
The 24 read-only tools cover courses and syllabus, modules and module items,
pages, full announcements and assignment instructions, rubrics, own submissions
and feedback/history, grades, files, upcoming events, Classic Quizzes and own
quiz status, discussions and replies, inbox conversations and the to-do feed.
Reading a conversation explicitly preserves its unread state.

Lists return `next_page`: pass it back as `page` with the same other arguments
until it is null. `per_page` defaults to 50 (maximum 100); Canvas can return fewer
items and still have another page. Modules and discussion entries have separate
item/reply tools so embedded previews are not mistaken for complete contents.
Page bodies, syllabus and instructions retain the HTML supplied by Canvas.
Announcements use Canvas's default recent two-week window; the calendar and
to-do tools expose Canvas's dashboard feeds, not a complete historical archive.
New Quizzes can appear as assignments: use `canvas_assignment_status` for their
visible submission results. Permissions and locked content remain enforced by
the school. These tools have local HTTP/MCP and browser-bridge fixture coverage;
institution-specific behavior still needs validation with a connected account.

**Brightspace / Omnivox: browser sign-in.** Enter your school's
address and choose **Connect with browser**. An installed Chromium-based browser
(Chrome, Edge, Brave, Helium, …) opens the official sign-in page. If none is found,
student-mcp asks permission before downloading a dedicated Chrome for Testing,
with download progress and cancellation. You can keep Safari or Firefox as your
default browser; no manual browser installation is needed on supported systems.
Complete login and MFA in the separate sign-in window. A dedicated per-account
profile retains the session across restarts. If interaction is needed later,
choose **Edit sign-in → Connect with browser**. The account is saved only after
course access is verified. **Manual sign-in** opens the manual
methods described below. See [browser setup, persistence and test status](BROWSER-AUTH.md).

**Moodle.** Enter your school username and password. The app calls
`/login/token.php` with the `moodle_mobile_app` service; only the **token**
is saved, never the password. If your school uses Microsoft SSO or 2FA, this
mode can be refused. New accounts use Polytechnique Montréal's Moodle site;
existing accounts keep their configured site.

**Omnivox (Léa / Mio).** Enter the Omnivox address
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

Omnivox now exposes **46 tools** (34 reads and 12 writes), including MIO messages/search,
announcements, absences, teacher contacts, notifications, college news/forms,
calendar pagination, weekly schedules and file reading. See
[Omnivox tools](OMNIVOX-TOOLS.md) for the complete list and ID conventions.
Browser-profile connections use the same tools; see
[persistent browser sign-in](BROWSER-AUTH.md).
Writes include sending/replying/forwarding MIO, message and folder deletion,
categorization, flags, restoring messages, marking messages read, dismissing
notifications and marking assignments consulted. They advertise
`readOnlyHint: false` and `destructiveHint: true`; approval/automatic execution
is controlled by the MCP client's settings, not an extra Omnivox confirmation
parameter. **Assignment upload/removal is not implemented yet**: the observed
official hand-in flow opens a separate CVIR web page, not a documented mobile
upload endpoint. Do not confuse recording consultation with handing in work.

**Brightspace / myCourses.** Enter your institution's Brightspace site address
and either an OAuth access token or a session cookie header copied after SSO
(`d2lSessionVal=…; d2lSecureSessionVal=…`). The app verifies your profile and
course enrollments. Read tools cover courses, content, grades, assignments (dropbox +
due dates), quizzes, calendar, aggregated deadlines and course news.
`brightspace_file` reads documents using `courseid` and a file topic's `Id`
from `brightspace_course_content` as `topicid`. PDFs are returned as original
resources/download links by default; `format=text` supports page selection and
`find`, while `format=images` handles scans and diagrams. The 12 MB download
limit also applies here. External links and redirected downloads are refused so
session cookies never leave the configured site.
`brightspace_assignment_submit` posts a file to a dropbox folder. Provide `courseid` + `folderid`, and
exactly one of `path` (file on the machine running student-mcp),
`content_base64` + `filename`, or `text`. Write tools advertise
`readOnlyHint: false` so MCP clients can require confirmation. Course lists retain `PagingInfo`; supply `bookmark` to request
the next page. Prefer `brightspace_deadlines` for upcoming work. Sessions expire
and must be replaced via **Edit sign-in**. The connector uses LP API 1.49 and LE
API 1.82.

Saved accounts have check, reconnect and disconnect actions. The switch beside
each name enables that account for assistants without deleting the login; a
disabled account is omitted from `student_status` and from platform tools.
Several accounts may share one platform and one MCP link. When more than one
account of a platform is enabled, those tools require `account_id` (`enum` of
the live ids). `initialize` instructions list the enabled accounts (name,
platform, host, id). Connected services expose their full tool list. Write
tools keep `readOnlyHint: false` and `destructiveHint: true` so Claude or
ChatGPT can ask before changing school data. Older `tools.advanced` /
`tools.allow_changes` / `allow_write` fields are ignored on load and omitted on
the next save. Credentials and the public link stay in place. An older
`disabled_services` entry is expanded on load into per-account disables.

Connection checks and manual sign-ins run in a background worker. The local
status page, link settings and Quit remain available while the school responds.
Only one connection operation runs at a time.

Browser-backed tools keep one browser for all requests within the tool and
close it afterwards. Concurrent calls for the same profile wait up to two
minutes for its OS lock, without launching competing browsers. Cancelling a
browser sign-in also interrupts its lock wait.

Document links last 24 hours by default while the app is running (`link_ttl_hours` in `~/.student-mcp.json`, 1–168 hours). The shared
store is bounded to 128 links and 24 MiB; when full, new files are attached
inline rather than evicting links that were already issued.

`omnivox_deadlines` and `canvas_deadlines` match the Moodle/Brightspace due-work
tools: a sorted window (default 60 days), `partial`/`errors` when a course
source fails, and no mixing of course IDs across accounts. Omnivox fetches each
course's assignment list because the summary has titles only; college-calendar
holidays stay on `omnivox_calendar`. Canvas merges assignments, Classic Quizzes
and upcoming-events, and de-duplicates assignment calendar rows.

`moodle_deadlines` reports `partial` and per-source `errors` when some sources
fail. `include_expired=true` returns items already due inside the same day
window (for example, this week's past items). Action events are paged (up to 500 per call); a remaining cursor is returned
as `actionEventsNextAfterEventId` for `ws_call` with
`core_calendar_get_action_events_by_timesort`. The upcoming-calendar source
remains subject to the school's own calendar window.

### Finding Moodle sources and reading all instructions

`moodle_search` searches course/activity names, filenames, section text and full
activity descriptions, not just the shortened previews. Matches include their
course/section/activity IDs, an excerpt and the fields that matched. Without a
course name or `courseid`, it searches visible courses in windows of 8, with at
most 2 concurrent school requests. A matching course name no longer substitutes
for a missing document result: Moodle's own search is tried when needed.

Follow the returned `continuation` arguments to finish the current search. Results
are capped at 40 per window; `offset` retrieves the rest. `page` selects the next
course window in the default `source=auto` mode. Use `source=global` to search
Moodle's upstream index instead; in that mode `page` is the upstream result page.
`partial`/`errors` report failed sources. No match is not proof of absence when
coverage is incomplete. PDF contents are **not searched**, and no PDF text cache
is consulted: `pdfTextSearched=false` says so. Read a PDF with `moodle_file`
first if you need to search inside it.

`moodle_course_content` defaults to a bounded navigation overview: at most six
sections per page, eight activities per section and six files per activity, with
`moduleCount`/`fileCount` and `truncated` flags when anything is omitted. Follow
`nextPage` for the other sections. Each section has a direct `url`
(`course/view.php?id=…&section=…`). Each file carries `timemodified` when Moodle
reports it.

Subsections (`mod_subsection`, Moodle 4.3+) appear as `subsections` under their
parent activity; their activities are searchable and readable like normal ones.
The response also includes the student's **groups**
(`core_group_get_course_user_groups`) and the course groups. `group_only=true`
filters with Moodle's own `uservisible` and standard availability
`group`/`grouping` conditions: it never infers a group from section names. Group
data can be unavailable or partial; the response reports that instead of guessing. Description previews and section summaries explicitly report
truncation. To read all textual instructions, use `detail=full`, optionally with
`moduleid` or `sectionid`, then follow `nextPage` until null. This returns up to 4
blocks of 8,000 characters per
call. Omit selectors to read the course's available metadata text. Attachments
`detail=full` can take `query` directly: it returns the complete text of the
matching activities/sections in the same call, with `nextPage` pagination.

`moodle_resources` adds the real `sectionid` and section name to each page,
file, URL, folder and book, including resources inside a subsection. With
`courseid`, `moodle_grades` reports Moodle item weights (from grade items or the
user report table), `weightsSource` and the course total; if Moodle reports no
weights, `weightsAvailable=false` and no weight is invented.

still require the appropriate file tool and its page windows. Recognized absolute
HTML links are listed but not fetched; `richContentOmitted` flags media, layout or
references that require the school page or source files.

The account's course list and metadata are indexed locally for 15 minutes.
`refresh=true` on these tools bypasses the index; failed refreshes do not return
stale data as current. Changing credentials/account changes the index namespace.
Metadata persists beside the configuration in `~/.student-mcp.index/` (or the
configuration path with an `.index` extension). It contains course text and file
references, not downloaded PDF/Office bytes or login credentials. It is private
to the local user, is not part of git and can be removed while the app is closed.
The 15-minute lifetime is a freshness limit, not an erasure deadline. On index
writes, snapshots older than one day are reclaimed; at most 16 account snapshots
of 24 MiB each are retained. Old credential namespaces can be evicted early.

The index is built as tools are used: no background full-school crawl, external
LLM, graph viewer or new MCP tool is required. `coverage` describes what a response
delivered or searched, not what the model understood or saw in another chat.

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

Client confirmation depends on the client's settings, not just MCP annotations;
non-Claude clients may not prompt before a write. If an action times out or
returns an unexpected response, inspect its actual state before retrying.
These additions have local HTTP-fixture tests, not live institutional validation.
Optional modules (locker, awards, ePortfolio, CPD) are not implemented yet;
see [coverage and remaining work](BRIGHTSPACE-COVERAGE.md).

### Quebec integration targets

Target order: Moodle → Omnivox → Brightspace → monPortail/Brio.
Only implemented connectors are displayed, without empty slots.

**Status (v0.6.0):** Moodle and Brightspace are the student connectors in
regular use. Omnivox login/MFA can still fail on some college portals.

- **Moodle** also covers installations such as StudiUM; it is not a separate
  platform. Each institution must enable the required mobile/Web Service access.
- **Omnivox (Léa; Mio messaging is not implemented)** signs in with the student's college login
  through Omnivox's mobile login page and reads the `/Mobl/` JSON endpoints.
  A web session from `/Login/Account/Login` is rejected by `/Mobl/`, so the
  connector keeps the same mobile identity (user agent, device id, AuthHash)
  for the whole session. The email-code verification API is supported directly.
  Other portal variants may not be supported. Login and email-code form loading are checked against the live
  Maisonneuve MFA API; code validation, mobile-session establishment and
  persistence are covered by local HTTP fixtures. End-to-end verification with a
  real email code still depends on the college account.
- **Brightspace** uses the Valence API with session cookies pasted from the
  browser after SSO (OAuth tokens still work). Tools cover content, grades,
  dropbox assignments (including student submit; approval depends on the MCP
  client),
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
- **Set up an assistant** separates **Web** from **On this computer**. The web
  view retains short guides for Grok, Claude, ChatGPT and Gemini. The local view
  scans known configuration locations, application directories and executable paths
  without running an assistant or modifying files. Choose a detected app, then
  **Confirm setup**: no command or JSON needs to be copied. A configured app
  shows **Remove setup**, which deletes only `student-mcp` and keeps the other
  servers and settings, after creating a backup.
- Automatic local setup supports Claude Desktop, Claude Code, Codex / ChatGPT
  Desktop, Gemini CLI, OpenCode (V2), Cursor, Grok Build, Oh My Pi, GitHub Copilot /
  VS Code, Windsurf, Pi, Qwen Code, Kiro, Amp, Crush and Devin CLI.
  It merges only `student-mcp` into the native JSON, JSONC or TOML
  configuration, keeps other settings and MCP servers, and backs up existing files.
  OpenCode uses `mcp.servers` and retains JSONC comments; Codex and Grok Build use
  `mcp_servers` in TOML, with comments preserved. VS Code targets the current
  Copilot global configuration (`~/.copilot/mcp-config.json`); AI features must be enabled.
  Oh My Pi targets its default profile unless a profile/directory override is inherited.
  `CODEX_HOME`, `COPILOT_HOME`, and the documented OMP directory overrides are respected.
  A saved setting is not a live connection check: the result tells you whether to
  restart the assistant or reload MCP; OpenCode applies settings automatically.
  No school credential or MCP bearer token is written.
- The additional local agents are drawn from [Omarchy's current agent catalogue](https://github.com/basecamp/omarchy/blob/quattro/bin/omarchy-default-agent),
  but configured using each client's own MCP format: [Pi](https://pi.dev/docs/latest/mcp),
  [Qwen Code](https://qwenlm.github.io/qwen-code-docs/en/users/features/mcp/),
  [Kiro](https://kiro.dev/docs/mcp/configuration/), [Amp](https://ampcode.com/docs/customize/mcp),
  [Crush](https://github.com/charmbracelet/crush/blob/main/docs/config/README.md),
  and [Devin CLI](https://docs.devin.ai/cli/extensibility/mcp/configuration).
  Amp uses the literal `amp.mcpServers` key (JSONC comments preserved), Crush uses
  `mcp` in its still-supported config-directory `crush.json`, never its machine-owned
  state file, and the other new clients use `mcpServers`. Crush's `crushrc` is not
  executed or edited and retains precedence over JSON. Existing approval policies
  stay unchanged; no Omarchy permission-bypass flags are copied.
  Detection also checks bounded mise installation directories without invoking mise
  or executing Omarchy's lazy installers. A lazy launcher alone is not an installed app.
  Grok, ChatGPT, Claude on the web and Gemini on the web use the public HTTPS link
  instead; the same pages show the official steps and a copy button.
- Automatic setup refuses invalid native configuration, invalid MCP containers,
  symlinks and unreadable files. Writes are serialized, backed up and atomic;
  private file permissions are retained. Nothing is written until an app is selected
  and **Confirm setup** is pressed. The scan is bounded and may miss portable apps
  or custom profiles not visible to the student-mcp process.
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
student-mcp settings-url  # prints the running settings URL
student-mcp version
```

Incoming MCP requests are appended to `~/.student-mcp.mcp.log` (mode 0600,
rotated at 1 MiB). Each line has the time, method, **redacted** path, status and
user-agent; headers, bodies and connector tokens are never written. Set
`STUDENT_MCP_TEST_LOG` only in tests to redirect unit-test writes.

`up` prints the settings URL on startup and in `~/.student-mcp/window.log`
(user-only permissions) even when the browser cannot open. `settings-url` reads
that discovery file and checks the running instance; it changes no settings.
The Linux installer also links the command into `~/.local/bin` and says what to
add to `PATH` when that directory is missing from it. An unrelated existing
command there is preserved.

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
  or another identity provider. Sign in in your browser and paste
  `d2lSessionVal=…; d2lSecureSessionVal=…`, or use an OAuth access token.
- **Omnivox asks for identity verification**: enter the emailed 6-digit code in
  the app. Use **Send a new code** if needed. If the session expires, sign in
  again to start a new verification.
- **The assistant cannot open a Moodle PDF / stops mid-chapter**: Moodle file
  URLs need the web service token, which is never given to the model. Use
  `moodle_file` with the `fileurl` from `moodle_course_content`; do not fetch
  that URL with a generic HTTP tool. The default `format=file` returns the original
  file (PDF, Office or other) and a download link when HTTP sharing is available,
  without conversion or extraction. Above 6 MiB, sharing-enabled clients receive
  the download link; without HTTP sharing, the original is still attached.

  To **study** a PDF, use `format=text` or `format=images`: both send the
  extracted text labeled by page **and a full image of every page in the window**,
  including vector diagrams on text-heavy pages. Default: 4 pages; maximum: 8;
  text: 20,000 characters per call.

  Moodle book chapters (`pluginfile.php/…/mod_book/chapter/…/index.html`) are
  supported: the HTML source is returned as the original file or as text blocks
  for `format=text`. This is the same authenticated download as other files, not
  a login failure. Images and links inside the chapter are not rendered or
  fetched; open the Moodle page for its visual layout.

  If any image cannot be delivered, the call
  fails explicitly: request a smaller window or the original. No local OCR.
  `find` searches the text layer and selects a matching window. Page/search
  arguments without a format imply study mode. Follow `nextPageFrom` when
  `truncated` is true; reduce the window when `textTruncated` is true.

  Every file response supplies `documentId`. Reuse it as `document_id` with the
  same tool and account for later searches/windows, without school file IDs or
  another download. The in-memory cache lasts up to 5 minutes (4 originals /
  24 MiB total; early eviction possible). PDF inspection/text extraction and
  Office conversion are prepared once per cached document, on first study.
  Reload via the original school IDs after expiry or app restart.

  Word/PowerPoint/OpenDocument documents and slides convert to PDF locally on
  Linux when system LibreOffice, Bubblewrap and `prlimit` are available. Conversion
  uses a fresh macro-disabled profile, no host home/environment/network, a
  30-second deadline and bounded resources. There is no unsandboxed fallback:
  without conversion, results contain text blocks and explicit `visualLimitations`.
  Spreadsheets stay sheet/row/cell text (no formula execution); TXT/CSV stay text.

  Poppler (`pdftotext`, `pdftoppm`, `pdfinfo`) is needed only for study.
  For an original download link request `format=file`. Localhost links cannot be opened by a remote
  browser AI; use inline resources or enable the public link.
- **The shared file link says it expired**: links are capability URLs valid for
  the configured lifetime (24 hours by default; 128 files / 24 MiB maximum). Call `moodle_file` again to mint a fresh one. Anyone with the
  link can read that one file until it expires; the Moodle token is never in
  the link.
- **Gemini web says the MCP server could not be reached**: Gemini's custom
  connectors are currently limited to eligible US accounts/regions. The same link
  keeps working in Claude, ChatGPT and Grok. Confirm the failure in
  `~/.student-mcp.mcp.log` (method, path, status, user-agent; tokens are redacted)
  before changing the connector URL.
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

## Developer checks and MCP Inspector

These tools run on the developer machine or in GitHub Actions only. They are
not application dependencies and are not included in the release installers.

### Rust checks

Install once:

```sh
rustup component add clippy rustfmt
cargo install cargo-deny --version 0.20.2 --locked
```

Run from the repository root:

```sh
cargo fmt --all -- --check
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo deny --locked check
```

Use `cargo fmt --all` to apply formatting. CI runs these checks on pushes and
pull requests. `deny.toml` checks RustSec advisories, allowed licenses and
dependency sources across all platforms; duplicate versions are warnings.
Review advisory findings rather than adding blanket ignores. Its license check
covers Cargo packages, not bundled browser downloads or external assets.

### MCP Inspector

Requires Node **22.23.4+, 24.22.1+, or 26.12.1+** on those supported major lines.
Install the tested Inspector version on your development machine:

```sh
npm install --global @modelcontextprotocol/inspector@2.14.0
```

From the repository root, build once before connecting, then open the Inspector:

```sh
cargo build --locked --bin student-mcp
mcp-inspector --config tools/inspector.json
```

Select `student-mcp`, connect, and use **Tools** to inspect schemas and invoke
tools. The config launches the local source build over stdio. It uses your normal
student-mcp accounts; `STUDENT_MCP_CONFIG_FILE` selects a separate configuration
when passed to the server with Inspector's `-e` option. Keep credentials out of
`tools/inspector.json`.

For a CLI-only protocol/schema check, without invoking school tools:

```sh
mcp-inspector --cli --config tools/inspector.json --server student-mcp --method tools/list --strict
```

## Local code graph

[Graphify](https://github.com/Graphify-Labs/graphify) is an optional developer tool
for exploring connections between the Rust backend, browser bridges and tooling.
From the repository root (Python 3.10+ and `uv`):

```sh
uv tool install graphifyy==0.9.70
sh tools/graphify.sh
graphify query "browser profile login"
graphify explain "token_login_agent"
```

The script parses code locally with tree-sitter, inspects the Cargo workspace,
and generates `graphify-out/graph.json`, `GRAPH_REPORT.md` and `graph.html`.
Open `graphify-out/graph.html` in a browser to explore the graph. Re-run the script
after source edits or branch changes; extraction reuses its local cache.
The commands above were validated with Graphify 0.9.70.

The build uses no LLM or API key. `.graphifyignore` excludes the vendored Moodle
catalogue and images; Graphify also respects `.gitignore`. Generated files stay
local and are not shipped with the app. `AGENTS.md` gives coding assistants the
query workflow. Graph edges are navigation hints: confirm behavior in source,
especially Rust macros, dynamic calls and Rust/JavaScript boundaries.

## Release maintenance

### Optional publisher signatures

Configure these **GitHub repository Actions secrets** (Settings → Secrets and
variables → Actions). Never commit certificates, passwords or private keys.
Without them, CI and packaging pass with unsigned Windows / ad-hoc macOS builds.
When credentials are supplied, signing or notarization failures fail the build;
there is no silent unsigned fallback. Use workflow dispatch to test packaging
without publishing a release. Only a `v*` tag publishes a release.

**Windows:** obtain an Authenticode code-signing certificate with its private key
in an importable PFX from your certificate provider. Supply
`WINDOWS_CERTIFICATE_BASE64` (base64 of the PFX) and
`WINDOWS_CERTIFICATE_PASSWORD`. CI imports it into a temporary runner's user
certificate store, signs both architectures and the setup file with SHA-256
and a timestamp, verifies the signatures, then deletes the certificate/key.
Hardware-only or cloud/HSM certificates need a provider-specific signing
integration; a public `.cer` alone cannot sign. A signature does not immediately
guarantee Windows SmartScreen reputation.

**macOS:** enrol in the Apple Developer Program, create a **Developer ID
Application** certificate and export it with its private key from Keychain as
`.p12`. Supply `MACOS_CERTIFICATE_BASE64`, `MACOS_CERTIFICATE_PASSWORD` and
`MACOS_SIGNING_IDENTITY` (full `Developer ID Application: Name (TEAMID)` string).
For notarization also provide `APPLE_ID`, `APPLE_TEAM_ID` and
`APPLE_APP_PASSWORD` (an app-specific password from your Apple account).
CI uses a temporary keychain, signs binaries and nested apps inside out with
hardened runtime/timestamps, notarizes and staples the installed app, installer,
and DMG, then deletes the keychain. Checksums are generated **after** signing and
stapling. PR jobs never import signing secrets.

Missing secrets are supported; partially supplied or invalid credentials are
errors. Test the signed downloads on clean native systems before publishing.

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
