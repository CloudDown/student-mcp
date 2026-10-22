#!/usr/bin/env bash
# Run from a terminal: curl -fsSL https://raw.githubusercontent.com/CloudDown/student-mcp/main/install.sh | bash
set -euo pipefail

for tool in curl unzip mktemp; do
  if ! command -v "$tool" >/dev/null 2>&1; then
    printf '%s is missing on this computer.\n' "$tool" >&2
    exit 1
  fi
done

case "$(uname -s)" in
  Linux) system=linux ;;
  Darwin) system=macos ;;
  *) printf 'This script is for Linux and macOS. On Windows, use PowerShell.\n' >&2; exit 1 ;;
esac
case "$(uname -m)" in
  x86_64|amd64) arch=x64 ;;
  arm64|aarch64) arch=arm64 ;;
  *) printf 'Unsupported architecture: %s\n' "$(uname -m)" >&2; exit 1 ;;
esac

base='https://github.com/CloudDown/student-mcp/releases/latest/download'
file="student-mcp-${system}-${arch}.zip"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

printf 'Downloading student-mcp for %s %s...\n' "$system" "$arch"
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

unzip -q "$tmp/$file" -d "$tmp/app"
binary="$tmp/app/student-mcp"
sidecar="$tmp/app/cloudflared"
license="$tmp/app/LICENSE-cloudflared.txt"
if [ ! -f "$binary" ] || [ ! -f "$sidecar" ] || [ ! -f "$license" ]; then
  printf 'The download does not contain the app and its Cloudflare tunnel component.\n' >&2
  exit 1
fi
chmod +x "$binary"
chmod +x "$sidecar"

if [ "${MOODLE_INSTALL_CHECK_ONLY:-}" = 1 ]; then
  "$binary" --help
  "$sidecar" --version
else
  # Persist the binary, then pop the small window in the background.
  install_dir="$HOME/.student-mcp/bin"
  mkdir -p "$install_dir"
  cp "$binary" "$install_dir/student-mcp"
  cp "$sidecar" "$license" "$install_dir/"
  chmod +x "$install_dir/student-mcp"
  chmod +x "$install_dir/cloudflared"
  nohup "$install_dir/student-mcp" up > "$HOME/.student-mcp/window.log" 2>&1 &
  printf "The student-mcp window is opening. Installed file: %s\n" "$install_dir/student-mcp"
fi
