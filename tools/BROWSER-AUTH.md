# Persistent browser sign-in

Brightspace, Omnivox and Outlook mail transfer offer **Connect with browser**
on the local configuration page. Enter the HTTPS address, finish the normal
sign-in/MFA in the browser window, and wait for the app to confirm access.
No password or MFA code is passed through the MCP tools.

## Browser and lifecycle

- You can keep Safari or Firefox as your default browser and use the local page
  there. School sign-in opens a separate Chromium-based browser with a private
  profile: student-mcp prefers the compatible default browser, then another
  installed Chrome, Edge, Brave, Helium, etc.
- If none is found, **Connect with browser** automatically downloads the official
  stable **Chrome for Testing** from Google. No administrator access or default
  browser change is required. The first download is roughly 150–200 MB (varies by
  platform); allow up to 1.5 GB of disk space for extraction, plus the archive.
  The UI shows preparation, download progress and installation; **Cancel** stops
  the job and removes its temporary download. A stalled network read can take up
  to ten seconds to return before cancellation is observed.
- Managed binaries live next to the configuration, by default
  `~/.student-mcp.browser-runtime/`, separately from account profiles. Completed
  installs are reused. Browser sign-in checks for a newer stable managed version;
  an offline catalogue check falls back to the installed one. MCP requests never
  download or update a browser. Old versions are retained so active processes are
  not disrupted; with student-mcp stopped, the runtime directory can be removed
  to reclaim space (the next sign-in downloads it again).
- Supported download targets: Linux x64/ARM64, macOS Intel/Apple Silicon, and
  Windows x86/x64 (Windows ARM64 uses x64 emulation). Availability depends on
  Google's stable catalogue. Linux still needs Chrome's system libraries and a
  desktop session for interactive sign-in; installing the distribution's Chromium
  package provides its required dependencies. No sandbox restrictions are disabled.
- Override discovery with `STUDENT_MCP_BROWSER` set to an absolute path to a
  Chromium-based executable. An invalid explicit override is reported instead of
  silently downloading a different browser.
- No Node.js, Playwright, extension, or separate gateway is required at runtime.
  Rust uses Chrome DevTools Protocol over a loopback connection.
- Each account has a dedicated browser profile, separate from ordinary browsing.
  Profiles are next to the private config: by default `~/.student-mcp.browsers/`.
  Profile directories are owner-only on Unix; Windows inherits the user's directory
  permissions. They contain authentication secrets and must not be shared.
- The sign-in window closes after validation. MCP tools reopen the profile
  headlessly for the duration of the tool, then close it. A filesystem lock
  serializes processes using the same profile; distinct accounts are isolated.
- Session cookies are checkpointed explicitly because Chromium may otherwise
  discard session-only cookies on exit. Persistent cookies and local storage
  remain in the browser profile. Cookie rotation/deletion is handled by Chromium.
- The browser can follow ordinary SSO navigation when resuming the profile. If
  interaction is required, MCP returns a reconnect message; **Change login →
  Connect with browser** reopens the saved profile. No login window is opened by
  a remote MCP request. No password/MFA fields are automatically filled.
- Sign-in times out after five minutes. **Cancel** discards the pending result;
  it cannot replace the saved account. **Disconnect** removes that account's
  browser data (finish/cancel a running sign-in first). A cancelled or failed
  first sign-in deletes the profile it created; a failed *re-login* keeps the
  existing saved profile intact.

This is persistence, not a permanent token. Institutions still control session
expiration, SSO policy and MFA. A forbidden API request does not establish that
the session expired. Actions are never automatically retried.

## Brightspace

Sign-in is confirmed by `users/whoami` and course enrollments, not by the mere
presence of cookies. The profile remembers the user identifier and refuses an
unexpected account switch; add a new account instead.

This first implementation sends the existing fixed Valence API requests through
the authenticated browser, including bounded file reads and submissions. The
website's CSRF token is used where available. Response transformations remain in
the existing Rust connector. Browser fetch refuses redirects on API/file calls.

An HTTP-only fast path using `/d2l/lp/auth/oauth2/token` was investigated but is
**not implemented**: it is an internal, tenant-dependent endpoint and requires
separate validation. The persistent browser is currently used for each tool.

## Omnivox

The browser opens the college's official mobile login page, using the existing
OVX device identity convention (app version 3.8.9). A minimal Android WebView
bridge supports session storage, auth-code updates, landing-page selection and
device capability callbacks. Biometric authentication is reported unavailable.
Mobile fragment login tokens are handled using the existing connector convention.

After login, the connector verifies access to Léa. Read requests use the site's
`Skytech.Commun.Utils.HttpRequestWorker.PostJSON`, rather than exporting a web
session and assuming `/Mobl/` accepts it. Device identity and mobile storage are
retained with the profile. The [46 Omnivox tools](OMNIVOX-TOOLS.md) reuse this
session. Binary file downloads use same-origin GET with the profile's mobile
identity; redirects are refused and responses are capped at 12 MB. The whole
native app bridge is not implemented.

Write requests deliberately do **not** use `PostJSON`, `PostJSONWithRetry` or
the app's `SendMessage` wrapper: the inspected client can replay a request on
an expired challenge or a send timeout. The dedicated write transport reuses
`X-K`/`X-T` values already produced by the site's session code, plus the mobile
identity, in one same-origin fetch with `redirect: manual`. It does not invent
tokens or evaluate returned challenge code. An `X-KE` expiry, redirect, timeout
or unconfirmed response returns an error; inspect the action state before
deciding whether to retry. A returned `X-AuthHash` is persisted through the
existing native bridge. Client confirmation policies remain in the MCP client.

**Other connection methods** still provides manual login, the existing gateway,
and Brightspace cookie/OAuth input. Moodle keeps its mobile token flow.

## Verification and limitations

- Local Rust fixture with real Chromium: restart persistence, cookie rotation,
  cookie deletion, CSRF, binary downloads, size bounds, origin restrictions,
  account-identity mismatch, profile cleanup and no replay of writes.
- Installer fixtures: official download URL/platform selection, incomplete
  downloads, cancellation, progress, bounded extraction, path traversal rejection,
  macOS framework symlinks and publication only after extraction. Real macOS and
  Windows installation still require validation on those systems.
- Node VM tests: Omnivox bridge storage/identity, fragment token, domain scoping,
  and routing reads through the official request worker.
- UI smoke: browser action authentication, invalid services/sites and cancellation.
- Real institutional sign-in/MFA and long-term session duration still need testing
  with the account owner. Windows/macOS browser launches are not validated by the
  Linux tests. A full Omnivox mobile login still depends on the live college page.

```sh
cargo test
node --test tools/test-browser.mjs
# Optional UI test, with Playwright installed in the development environment:
node tools/test-browser-ui.mjs
STUDENT_MCP_BROWSER=/absolute/path/to/chrome cargo test chromium_session_survives -- --ignored
# Optional real browser download (uses disposable test storage):
cargo test managed_browser_download_and_launch -- --ignored
python3 tools/smoke-web.py target/debug/student-mcp
```

## Research sources

- [Playwright persistent browser contexts](https://playwright.dev/docs/api/class-browsertype#browser-type-launch-persistent-context)
- [General-Mudkip: persistent Brightspace sign-in](https://github.com/General-Mudkip/d2l-mcp-server/blob/main/src/auth.ts)
- [RohanMuppa: session-cookie token renewal](https://github.com/RohanMuppa/brightspace-mcp-server/blob/main/src/auth/token-mint.ts)
- [D2L official OAuth registration](https://docs.valence.desire2learn.com/basic/oauth2.html)
- [Beat-YT Omnivox browser lifecycle](https://github.com/Beat-YT/omnivox-mcp/blob/main/src/omnivox-api/puppet/BrowserManager.ts)
- [Beat-YT mobile bridge protocol](https://github.com/Beat-YT/omnivox-mcp/blob/main/docs/native-bridge.md)

The mobile bridge is a small independent implementation of the observed protocol;
no upstream app bundle or third-party service is downloaded at runtime.
# Moodle mobile SSO

All services now start with **Connect with browser**. For Moodle, enter the
school URL (including the installation path, if any), then complete the school's
SSO/MFA. The connector opens `admin/tool/mobile/launch.php` with
`service=moodle_mobile_app` and a fresh random passport. It intercepts the
`moodlemobile://token=…` response locally, checks Moodle's MD5 site/passport
binding, decodes the token, and verifies `core_webservice_get_site_info` before
saving it. The optional private token is discarded. No protocol handler is
registered and the token is never passed to the OS or the AI.

Existing Moodle tokens remain usable without reconnecting. If mobile browser
login is disabled by the school, **Other connection methods** retains password
login. No password is saved. The temporary Moodle browser profile is removed
after sign-in; Brightspace and Omnivox retain their per-account profiles.

Target live checks: `https://studium.umontreal.ca` and
`https://moodle.polymtl.ca`. Protocol tests use synthetic responses; they do not
prove that either school has enabled this flow. An account holder must complete
SSO/MFA on each site, check a course list, restart, and check it again. Never put
tokens, credentials, or screenshots of authenticated personal data in reports.
