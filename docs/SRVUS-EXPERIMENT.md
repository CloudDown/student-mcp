# Stable public link: srv.us

Status: **integrated as the app's public tunnel; Linux public MCP test passed**.
The SSH client is embedded with russh/Tokio; no system SSH installation,
sidecar, extra account or student configuration is needed. The app reconnects
automatically and preserves its identity independently of the MCP token/port.
Existing Cloudflare connector URLs must be replaced once with the srv.us URL.

## Reproduce

On a machine allowed to reach srv.us over TCP port 22:

```bash
cargo test --locked public_mcp_smoke -- --ignored --nocapture
```

This opt-in test opens real public tunnels. It writes an isolated, account-free
configuration and creates disposable SSH keys; it never loads the normal student
configuration. Only synthetic MCP requests are sent. Temporary keys are deleted
and the tunnel runtime/tasks shut down when the test completes, including on failure.
The local test listener is process-scoped and routing is disabled on cleanup.
Normal `cargo test` does not open these tunnels.

Observed on Linux, 2026-10-27:

- HTTPS MCP `initialize` and `tools/list` work; session headers survive.
- Only `student_status` is exposed, with no connected school accounts.
- Missing and incorrect MCP capabilities receive HTTP 401.
- Disconnecting and reconnecting with the same SSH key returns the same hostname.
- Local tests verify separate identities, corrupt-key preservation, Unix private
  file permissions and refusal of changed host keys.

The embedded client never reads personal SSH configuration or the SSH agent.
The HTTP test disables redirects and validates HTTPS certificates.
SSH host verification is **trust on first use**, including in the app:
the initial key is accepted, but later changes are refused. This does not
independently authenticate the initial SSH host key.

This is not a test from Claude/Grok, native Windows/macOS validation, a load test,
or evidence of a two-million-user capacity or uptime guarantee.

The dedicated Ed25519 seed lives in `~/.student-mcp.srvus-key`; back it up
privately, since deleting it changes the hostname. `~/.student-mcp.srvus-host`
pins the relay's SSH public key. Both paths follow configuration overrides and
are created with mode 0600 on Unix; Windows uses inherited user-profile ACLs.
Existing corrupt identities are not replaced. Pin changes stop the worker,
rather than endlessly retrying or accepting the changed server. There is no
fallback to a different provider or hostname. Only the requested reverse port
is accepted, and every forwarded connection targets the app's loopback port,
never a host chosen by the relay. At most 64 forwarded connections are active.

## Security boundary

For a generated `*.srv.us` address, the path is:

```text
AI client -- HTTPS --> srv.us -- SSH --> local app (loopback HTTP)
```

HTTPS terminates at srv.us, not at the app. Its operator can technically read or
alter MCP requests/responses, see the secret connector token and replay it to
invoke enabled tools. Moving that token to an Authorization header would not
hide it from this relay. The same termination concern applied to the previous
Cloudflare Quick Tunnel; changing providers does not create end-to-end secrecy.

The SSH key identifies a tunnel; it is **not** visitor authentication. The MCP
capability still authenticates visitors. TLS/SSH protect network transport, not
against a malicious or compromised relay. A provider's no-content-logging policy
is a trust statement, not a cryptographic guarantee.

School passwords/session cookies/tokens must remain local and must not be
included in tool results. This probe verifies no school connector behavior or
full absence of credential leaks. Any grades, messages or documents returned
to an AI are necessarily disclosed to that AI provider as well as any TLS
terminator in the path.

## Operational limitations and further hardening

- The link settings disclose the relay's ability to see exchanges and the secret
  link. Students must accept this third-party trust; local stdio remains available.
- Initial SSH host trust is TOFU, not an independently authenticated provider
  pin. Later changes are refused. Verify changes with the operator before any
  manual host-pin reset; never bypass the check blindly.
- Loopback-only binding, high-entropy MCP capabilities, request bounds, no-store
  responses and existing read/write permissions remain in place. Moving a token
  to a header does not make it private from the relay.
- Capability rotation is currently manual: quit, remove `gateway.token` from the
  config, restart and update the AI connector. The hostname remains unchanged.
- Validate native Windows/macOS operation and actual AI-client interoperability.
  Networks blocking outbound port 22 are unsupported; no provider fallback is used.
- Agree on usage/capacity with the operator before any large-scale deployment.
  srv.us documents a single instance and possible contributions or throttling
  if significant use exceeds sponsorship funding.

For stronger privacy, prefer local MCP/stdio when the AI client supports it:
there is no public tunnel or relay, though the AI still receives tool results.
For web clients, TLS passthrough with the private TLS key kept only on the app
could remove passive plaintext access from the relay. Certificate issuance,
hostname control and client trust also need a threat model: passthrough alone
does not stop a hostname operator from attempting certificate reissuance.
An extra encrypted payload layer requires client support and cannot simply be
added to ordinary Claude/Grok MCP connectors. No absolute security guarantee is
claimed.

## Sources

- [srv.us documentation and privacy/hosting notes](https://docs.srv.us/)
- [srv.us backend](https://github.com/xmit-co/srv.us/blob/main/backend/main.go):
  `endpointURLs` derives names from the public key and forwarded port;
  `serveHTTPSConnection` terminates TLS for its own domain/subdomains.

These are published implementation details, not an attestation of the code
actually running on the hosted service.
