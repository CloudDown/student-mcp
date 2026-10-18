#!/usr/bin/env bash
# Run from a terminal: curl -fsSL https://raw.githubusercontent.com/CloudDown/student-mcp/main/install.sh | bash
set -euo pipefail

for tool in curl unzip mktemp; do
  if ! command -v "$tool" >/dev/null 2>&1; then
    printf 'Il manque %s sur cet ordinateur.\n' "$tool" >&2
    exit 1
  fi
done

case "$(uname -s)" in
  Linux) system=linux ;;
  Darwin) system=macos ;;
  *) printf 'Ce script sert à Linux et macOS. Sur Windows, utilisez PowerShell.\n' >&2; exit 1 ;;
esac
case "$(uname -m)" in
  x86_64|amd64) arch=x64 ;;
  arm64|aarch64) arch=arm64 ;;
  *) printf 'Architecture non prise en charge : %s\n' "$(uname -m)" >&2; exit 1 ;;
esac

if [ "${MOODLE_INSTALL_CHECK_ONLY:-}" != 1 ] && [ ! -r /dev/tty ]; then
  printf 'Ouvrez un terminal interactif pour entrer vos identifiants Moodle.\n' >&2
  exit 1
fi

base='https://github.com/CloudDown/student-mcp/releases/latest/download'
file="student-mcp-${system}-${arch}.zip"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

printf 'Téléchargement de Moodle MCP pour %s %s...\n' "$system" "$arch"
curl --fail --location --silent --show-error --retry 2 --proto '=https' --proto-redir '=https' "$base/$file" -o "$tmp/$file"
curl --fail --location --silent --show-error --retry 2 --proto '=https' --proto-redir '=https' "$base/$file.sha256" -o "$tmp/$file.sha256"

read -r expected _ < "$tmp/$file.sha256"
if [ "${#expected}" -ne 64 ]; then
  printf 'La somme de contrôle du téléchargement est invalide.\n' >&2
  exit 1
fi
if command -v sha256sum >/dev/null 2>&1; then
  actual="$(sha256sum "$tmp/$file")"
elif command -v shasum >/dev/null 2>&1; then
  actual="$(shasum -a 256 "$tmp/$file")"
else
  printf 'Aucun outil SHA-256 disponible pour vérifier le téléchargement.\n' >&2
  exit 1
fi
actual="${actual%% *}"
if [ "$expected" != "$actual" ]; then
  printf 'Le téléchargement ne correspond pas à son empreinte SHA-256.\n' >&2
  exit 1
fi

unzip -q "$tmp/$file" -d "$tmp/app"
binary="$tmp/app/student-mcp"
if [ ! -f "$binary" ]; then
  printf 'Le téléchargement ne contient pas le programme attendu.\n' >&2
  exit 1
fi
chmod +x "$binary"

if [ "${MOODLE_INSTALL_CHECK_ONLY:-}" = 1 ]; then
  "$binary" --help
else
  # `curl | bash` consumes stdin; the Moodle wizard must read from the TTY.
  "$binary" install </dev/tty
fi
