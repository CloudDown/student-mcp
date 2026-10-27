# Desktop distribution

## Artifacts

`.github/workflows/release.yml` builds on each platform for x64 and arm64. Tags
`v*` publish the artifacts to GitHub Releases; PRs build them without publishing.

| Platform | Main download | Installed application |
| --- | --- | --- |
| Windows | `student-mcp-windows-{x64,arm64}-setup.exe` | `%LOCALAPPDATA%\student-mcp`, Start menu shortcut and uninstaller |
| macOS | `student-mcp-macos-{x64,arm64}.dmg` | `/Applications/student-mcp.app` |
| Linux | `student-mcp-linux-{x64,arm64}.zip` | Via `install.sh` |

All six portable ZIPs and both terminal scripts remain supported. Each ZIP,
installer EXE and DMG has a `.sha256` sidecar. This checks integrity only, not
authenticity when both files come from the same compromised source.

The desktop launcher calls the adjacent `student-mcp` binary with `up`. On
Windows it suppresses the console; on macOS it replaces itself with the GUI
process. The main binary's no-argument stdio interface remains unchanged.

Windows uses Inno Setup (6.7.3, checksum-pinned in CI). Installation is per-user,
adds a Start menu entry, and offers launch at the end (selected by default).
Silent installs do not launch the GUI. The stable AppId supports upgrades; the
existing PowerShell install location is preserved. Uninstall leaves user data.

The DMG contains an AppleScript installer app, with the real app bundled in its
resources. After explicit confirmation it stages and copies the app into
Applications, asking for administrator permission only when needed, then opens
it as the logged-in user. An existing running copy must be quit first. Copying
is staged on the Applications volume, with rollback if the final move fails.
The installer never removes quarantine attributes or disables Gatekeeper.
No login/startup service is installed on either platform. Scripts still use
their existing binary-only installation paths.

## Local packaging

Requires Python 3.11+, the native host build tools, and Inno Setup on Windows
or Xcode Command Line Tools (`osacompile`, `codesign`, `hdiutil`) on macOS:

```bash
cargo build --release --locked
python3 scripts/package-desktop.py --label macos-arm64 \
  --binary target/release/student-mcp \
  --launcher target/release/student-mcp-desktop
```

On Windows use `python`, `--label windows-x64` (or `windows-arm64`), and `.exe`
on both binary paths. `--iscc` can override the compiler location. Build on the
matching architecture or supply binaries for the intended target. Outputs go
to `release/`, never alongside credentials.

## Signing — still required

**Current status:** no Authenticode publisher signature, no Developer ID
signature, no Apple notarization. The macOS package uses ad-hoc signatures for
Apple Silicon code integrity only. Security warnings/blocking are expected;
checksums do not resolve them. No certificate/private key belongs in git.

Before calling this a warning-free public installation path:

1. **Windows:** obtain an Authenticode code-signing identity (certificate-backed
   signing service or suitable hardware-backed key). Sign both executable
   payloads before packaging, and the installer and uninstaller using Inno's
   signing integration. Use SHA-256 plus an RFC 3161 timestamp; verify each
   signature with `signtool verify /pa /all /v`. SmartScreen reputation is
   separate: a valid signature does not guarantee zero warnings immediately.
2. **macOS:** obtain an Apple Developer membership and Developer ID Application
   identity. Replace ad-hoc signing with timestamped Developer ID signatures and
   hardened runtime, signing executable payloads, the real app, then the
   enclosing installer app (inside out). Notarize/staple the installer app and
   real app as appropriate before creating the final DMG; notarize/staple the
   DMG too. Validate using `codesign --verify --deep --strict`, `spctl` and
   `stapler validate`. The installed payload must retain its notarization proof
   after copying out of the installer.
3. Keep signing credentials in protected CI secrets/keychains or an external
   signing service, available only to trusted release jobs, never PR code.
   Generate checksums **after** signing/stapling. A signed-release job should
   fail if signing or notarization fails, not quietly publish an unsigned build.

The signing pipeline is intentionally not activated without real credentials;
the current workflow does not claim publisher verification.

## Validation

Offline: `python3 -m unittest discover -s scripts -p 'test_*.py'`.
CI additionally compiles the native installers, installs/uninstalls Windows
silently, and mounts/verifies the DMG, copies/upgrades the app and runs the
installed CLI on macOS.
These checks do not prove the interactive first-run experience.

Before releasing to students, test downloaded artifacts on clean native
Windows x64/ARM64 and Intel/Apple Silicon Macs:

- download with a browser (so quarantine/SmartScreen applies);
- install by double-click, accept the normal installation confirmation, verify
  the GUI opens without a terminal and can be reopened from Start/Applications;
- verify login, tray/menu-bar close/reopen/quit, and MCP stdio using the installed
  main binary;
- upgrade after quitting, preserve accounts and stable link identity, then
  uninstall without deleting those private files;
- repeat signed first launch while offline after notarization/stapling is added.
