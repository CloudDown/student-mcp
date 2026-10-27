#!/usr/bin/env bash
# Advanced path: curl -fsSL https://raw.githubusercontent.com/CloudDown/student-mcp/main/install.sh | bash
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
curl --fail --location --silent --show-error --retry 2 --proto '=https' --proto-redir '=https' "$base/$file.sha256" -o "$tmp/$file.sha256"

read -r expected _ < "$tmp/$file.sha256"
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
  cp "$binary" "$install_dir/student-mcp"
  chmod +x "$install_dir/student-mcp"
  nohup "$install_dir/student-mcp" up > "$HOME/.student-mcp/window.log" 2>&1 &
  printf "The student-mcp window is opening. Installed file: %s\n" "$install_dir/student-mcp"
fi
