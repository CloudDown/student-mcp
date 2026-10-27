# Desktop distribution

## Artifacts

`.github/workflows/release.yml` builds on each platform for x64 and arm64. Tags
`v*` publish the artifacts to GitHub Releases; PRs build them without publishing.

| Platform | Main download | Installed application |
| --- | --- | --- |
| Windows | `student-mcp-setup.exe` | `%LOCALAPPDATA%\student-mcp`, Start menu shortcut and uninstaller |
| macOS | `student-mcp.dmg` | `/Applications/student-mcp.app` |
| Linux | `student-mcp-linux-{x64,arm64}` (raw binary) | Via `install.sh` |

There are no ZIP downloads. Windows/macOS users choose only their OS, not an
architecture. Windows includes both payloads and selects the native one at
installation; macOS uses universal binaries built with `lipo`. Architecture
payloads are internal CI artifacts, not published release assets.

Both terminal scripts remain supported: PowerShell downloads the same setup EXE,
and the shell script extracts the CLI from the same DMG on macOS or downloads a
raw binary on Linux. Each final asset has a `.sha256` sidecar. This checks integrity only, not
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
No login/startup service is installed on either platform. The shell script keeps
its existing binary-only installation path; PowerShell uses the native installer.
The installers contain the application: they do not download and execute a
remote command, and work without an installation-time network connection.

### Only what the app needs

- **Windows:** installs the native-architecture app, its small GUI launcher,
  the Start menu shortcut and uninstaller. The C runtime is statically linked;
  no Visual C++ redistributable is installed. The other architecture stays in
  the setup file, not in the installed directory.
- **macOS:** installs only the app bundle. Its two executables are universal;
  they use system frameworks, not Homebrew libraries. The installer app is not
  copied into Applications. The build targets macOS 11 or later.
- **Linux:** installs only the native binary under `~/.student-mcp/bin`, reusing
  the desktop's graphics libraries. The release reference environment is Ubuntu
  24.04 (x64/ARM64); other distributions need a compatible glibc and working
  X11/Wayland/OpenGL stack. This is not a promise of support for all Linux
  distributions. The script checks that the binary runs before replacing an
  existing installation and reports loader incompatibilities without changing it.

No installer adds Rust, Python, Node.js, a separate SSH client, a package manager,
or a background login service. Linux system packages are not silently modified.

## Local packaging

Requires Python 3.11+, the native host build tools, and Inno Setup on Windows
or Xcode Command Line Tools (`lipo`, `osacompile`, `codesign`, `hdiutil`) on macOS.
Supply binaries built for both targets (CI builds each on its native runner):

```bash
python3 scripts/package-desktop.py macos \
  --x64-binary target/x86_64-apple-darwin/release/student-mcp \
  --x64-launcher target/x86_64-apple-darwin/release/student-mcp-desktop \
  --arm64-binary target/aarch64-apple-darwin/release/student-mcp \
  --arm64-launcher target/aarch64-apple-darwin/release/student-mcp-desktop
```

On Windows use `python`, the `windows` subcommand, and matching x64/ARM64 MSVC
`.exe` paths for all four inputs. `--iscc` can override the compiler location. Outputs go
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
CI additionally compiles the native installers and blocks release until install/
upgrade checks pass on Windows x64/ARM64 and Intel/Apple Silicon Macs. Windows
checks the installed PE architecture, allowed files, CLI execution and uninstall;
macOS verifies universal slices, system-only shared-library dependencies, bundle
integrity and installed CLI execution. Windows imports are checked for unwanted
Visual C++ runtime DLL dependencies. Both Linux builds run the MCP stdio smoke.
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
