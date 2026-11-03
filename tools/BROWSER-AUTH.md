# Persistent browser sign-in (experimental)

Brightspace and Omnivox now offer **Connect with browser** on the local
configuration page. Enter your school's HTTPS address, finish the normal
sign-in/MFA in the browser window, and wait for the app to confirm access.
No password or MFA code is passed through the MCP tools.

## Browser and lifecycle

- Uses the default Chromium-based browser (Chrome, Edge, Brave, Helium, …) in a
  private profile. Safari and Firefox cannot drive this session. Override with
  `STUDENT_MCP_BROWSER` set to an absolute executable path.
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
- Node VM tests: Omnivox bridge storage/identity, fragment token, domain scoping,
  and routing reads through the official request worker.
- UI smoke: browser action authentication, invalid services/sites and cancellation.
- Real institutional sign-in/MFA and long-term session duration still need testing
  with the account owner. Windows/macOS browser launches are not validated by the
  Linux tests. A full Omnivox mobile login remains an experimental integration.

```sh
cargo test
node --test tools/test-browser.mjs
# Optional UI test, with Playwright installed in the development environment:
node tools/test-browser-ui.mjs
STUDENT_MCP_BROWSER=/absolute/path/to/chrome cargo test chromium_session_survives -- --ignored
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
