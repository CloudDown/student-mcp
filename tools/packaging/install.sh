#!/usr/bin/env bash
# Advanced path: curl -fsSL https://raw.githubusercontent.com/CloudDown/student-mcp/main/tools/packaging/install.sh | bash
# Downloads the same release as the Windows/macOS installers, without choosing a ZIP.
set -euo pipefail

for tool in curl mktemp; do
  if ! command -v "$tool" >/dev/null 2>&1; then
    printf '%s is missing on this computer.\n' "$tool" >&2
    exit 1
  fi
done

case "$(uname -s)" in
  Linux) system=linux ;;
  Darwin) system=macos ;;
  *) printf 'This script is for Linux and macOS. On Windows, use the setup.exe or PowerShell.\n' >&2; exit 1 ;;
esac

base='https://github.com/CloudDown/student-mcp/releases/latest/download'
tmp="$(mktemp -d)"
mounted=
cleanup() {
  if [ -n "$mounted" ]; then
    if ! /usr/bin/hdiutil detach "$mounted" >/dev/null 2>&1; then
      printf 'Could not unmount %s; temporary files kept at %s.\n' "$mounted" "$tmp" >&2
      return
    fi
  fi
  rm -rf "$tmp"
}
trap cleanup EXIT

if [ "$system" = linux ]; then
  case "$(uname -m)" in
    x86_64|amd64) arch=x64 ;;
    arm64|aarch64) arch=arm64 ;;
    *) printf 'Unsupported architecture: %s\n' "$(uname -m)" >&2; exit 1 ;;
  esac
  file="student-mcp-linux-${arch}"
else
  if ! command -v /usr/bin/hdiutil >/dev/null 2>&1; then
    printf 'hdiutil is missing on this computer.\n' >&2
    exit 1
  fi
  file=student-mcp.dmg
fi

printf 'Downloading %s...\n' "$file"
curl --fail --location --silent --show-error --retry 2 --proto '=https' --proto-redir '=https' "$base/$file" -o "$tmp/$file"
curl --fail --location --silent --show-error --retry 2 --proto '=https' --proto-redir '=https' "$base/SHA256SUMS.txt" -o "$tmp/SHA256SUMS.txt"

expected="$(awk -v filename="$file" '$2 == filename { print $1 }' "$tmp/SHA256SUMS.txt")"
if [ "${#expected}" -ne 64 ]; then
  printf 'The download checksum is invalid.\n' >&2
  exit 1
fi
if command -v sha256sum >/dev/null 2>&1; then
  actual="$(sha256sum "$tmp/$file")"
elif command -v shasum >/dev/null 2>&1; then
  actual="$(shasum -a 256 "$tmp/$file")"
else
  printf 'No SHA-256 tool available to verify the download.\n' >&2
  exit 1
fi
actual="${actual%% *}"
if [ "$expected" != "$actual" ]; then
  printf 'The download does not match its SHA-256 checksum.\n' >&2
  exit 1
fi

if [ "$system" = macos ]; then
  mounted="$tmp/mnt"
  mkdir -p "$mounted"
  /usr/bin/hdiutil attach "$tmp/$file" -nobrowse -readonly -mountpoint "$mounted" >/dev/null
  binary="$mounted/Install student-mcp.app/Contents/Resources/student-mcp.app/Contents/MacOS/student-mcp"
else
  binary="$tmp/$file"
fi
if [ ! -f "$binary" ]; then
  printf 'The download does not contain the student-mcp app.\n' >&2
  exit 1
fi
if [ "$system" = linux ]; then
  chmod +x "$binary"
fi

# Detect a wrong architecture or incompatible system loader before replacing
# any installed app. Never install compilers/runtimes or alter system packages.
if ! "$binary" --version > "$tmp/preflight.log" 2>&1; then
  cat "$tmp/preflight.log" >&2
  printf 'This build cannot run on this system. The existing installation was not changed.\n' >&2
  exit 1
fi

if [ "${MOODLE_INSTALL_CHECK_ONLY:-}" = 1 ]; then
  "$binary" --help
else
  # Persist the CLI, then open the small window in the background.
  install_dir="$HOME/.student-mcp/bin"
  mkdir -p "$install_dir"
  cp "$binary" "$install_dir/.student-mcp.$$"
  chmod +x "$install_dir/.student-mcp.$$"
  mv -f "$install_dir/.student-mcp.$$" "$install_dir/student-mcp"
  cli_dir="$HOME/.local/bin"
  mkdir -p "$cli_dir"
  if [ -e "$cli_dir/student-mcp" ] || [ -L "$cli_dir/student-mcp" ]; then
    if [ "$(readlink "$cli_dir/student-mcp" 2>/dev/null || true)" != "$install_dir/student-mcp" ]; then
      printf 'Existing %s was preserved. Use %s directly.\n' "$cli_dir/student-mcp" "$install_dir/student-mcp" >&2
    fi
  else
    ln -s "$install_dir/student-mcp" "$cli_dir/student-mcp"
  fi
  case ":$PATH:" in
    *":$cli_dir:"*) ;;
    *) printf 'Add ~/.local/bin to PATH in this terminal and your shell profile:\n  export PATH="$HOME/.local/bin:$PATH"\n' >&2 ;;
  esac
  if [ "$system" = linux ]; then
    # New releases embed the icon; older verified binaries get it from the
    # same commit as this installer (without coupling to the moving main branch).
    data_home="${XDG_DATA_HOME:-$HOME/.local/share}"
    case "$data_home" in /*) ;; *) data_home="$HOME/.local/share" ;; esac
    case "$HOME$data_home" in *$'\n'*|*$'\r'*) printf 'Cannot create a launcher for a path containing a newline.\n' >&2; exit 1 ;; esac
    applications="$data_home/applications"
    icon_dir="$data_home/icons/hicolor/256x256/apps"
    mkdir -p "$applications" "$icon_dir"
    if ! "$install_dir/student-mcp" --app-icon > "$tmp/app-icon.png" 2>/dev/null; then
      icon_base='https://raw.githubusercontent.com/CloudDown/student-mcp/3b24a2035f5730fe0aced08a70ee989138797985/src/resources/assets'
      curl --fail --location --silent --show-error --retry 2 --proto '=https' --proto-redir '=https' "$icon_base/app-icon.png" -o "$tmp/app-icon.png"
      expected_icon='a0e17c386817469405d848dac74d33a92cb47e088911ad1f2f819cd68dc35b62'
      if command -v sha256sum >/dev/null 2>&1; then
        actual_icon="$(sha256sum "$tmp/app-icon.png")"
      else
        actual_icon="$(shasum -a 256 "$tmp/app-icon.png")"
      fi
      if [ "${actual_icon%% *}" != "$expected_icon" ]; then
        printf 'The application icon does not match its SHA-256 checksum.\n' >&2
        exit 1
      fi
    fi
    cp "$tmp/app-icon.png" "$icon_dir/student-mcp.png"
    # Desktop Entry Exec uses double-quoted arguments, not shell quoting.
    exec_path="${install_dir}/student-mcp"
    exec_path="${exec_path//\\/\\\\}"
    exec_path="${exec_path//\"/\\\"}"
    exec_path="${exec_path//\$/\\\$}"
    exec_path="${exec_path//\`/\\\`}"
    exec_path="${exec_path//%/%%}"
    cat > "$applications/student-mcp.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=student-mcp
Comment=Connect your student services to your AI assistant
Exec="$exec_path" up
Icon=student-mcp
Terminal=false
Categories=Education;
EOF
    if command -v update-desktop-database >/dev/null 2>&1; then
      update-desktop-database "$applications" >/dev/null 2>&1 || true
    fi
  fi
  # The local settings URL carries a private UI key. Keep its log user-only.
  (umask 077; touch "$HOME/.student-mcp/window.log"; chmod 600 "$HOME/.student-mcp/window.log")
  nohup "$install_dir/student-mcp" up > "$HOME/.student-mcp/window.log" 2>&1 &
  printf "The student-mcp window is opening. Installed file: %s\n" "$install_dir/student-mcp"
  for attempt in 1 2 3 4 5; do
    if settings_url="$("$install_dir/student-mcp" settings-url 2>/dev/null)"; then
      printf 'Settings: %s\n' "$settings_url"
      break
    fi
    sleep 1
  done
  printf 'Find the settings URL again: "%s" settings-url\nLog: %s\n' "$install_dir/student-mcp" "$HOME/.student-mcp/window.log"
fi
