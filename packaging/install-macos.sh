#!/bin/bash
# Invoked by the DMG's installer app (possibly with administrator approval).
set -euo pipefail
source_app="$1"
destination=/Applications/student-mcp.app

if [ ! -x "$source_app/Contents/MacOS/student-mcp" ] || [ ! -x "$source_app/Contents/MacOS/student-mcp-desktop" ]; then
  echo 'The installer is missing its application payload. Download the DMG again.' >&2
  exit 1
fi
if /usr/bin/pgrep -x student-mcp >/dev/null; then
  echo 'Quit student-mcp from its menu-bar icon, then run the installer again.' >&2
  exit 1
fi
if [ -L "$destination" ]; then
  echo 'Applications/student-mcp.app is a symbolic link; move it before installing.' >&2
  exit 1
fi
if [ -e "$destination" ] && [ ! -f "$destination/Contents/MacOS/student-mcp" ]; then
  echo 'Applications/student-mcp.app is not a student-mcp bundle; move it before installing.' >&2
  exit 1
fi

# Stage a complete bundle on the same volume before touching the installed app.
staging="$(/usr/bin/mktemp -d /Applications/.student-mcp-install.XXXXXX)"
cleanup() {
  if [ -e "$staging/previous.app" ] && [ ! -e "$destination" ]; then
    echo "The previous app is preserved at $staging/previous.app; restore it manually." >&2
  else
    /bin/rm -rf "$staging"
  fi
}
trap cleanup EXIT
/usr/bin/ditto "$source_app" "$staging/student-mcp.app"
if [ -e "$destination" ]; then
  /bin/mv "$destination" "$staging/previous.app"
fi
if ! /bin/mv "$staging/student-mcp.app" "$destination"; then
  if [ -e "$staging/previous.app" ]; then
    /bin/mv "$staging/previous.app" "$destination"
  fi
  exit 1
fi
# The old bundle is removed with staging; ~/.student-mcp* is never touched.
