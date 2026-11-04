#!/bin/bash
set -euo pipefail
if [[ -z "${MACOS_CERTIFICATE_BASE64:-}" ]]; then exit 0; fi
: "${MACOS_CERTIFICATE_PASSWORD:?Missing certificate password}"
: "${MACOS_SIGNING_IDENTITY:?Missing Developer ID identity}"
keychain="$RUNNER_TEMP/student-signing.keychain-db"
certificate="$RUNNER_TEMP/student-signing.p12"
trap 'rm -f "$certificate"' EXIT
export STUDENT_SIGNING_CERTIFICATE="$certificate"
python3 -c 'import os,base64; open(os.environ["STUDENT_SIGNING_CERTIFICATE"],"wb").write(base64.b64decode(os.environ["MACOS_CERTIFICATE_BASE64"],validate=True))'
password="$(openssl rand -hex 32)"
security create-keychain -p "$password" "$keychain"
security set-keychain-settings -lut 21600 "$keychain"
security unlock-keychain -p "$password" "$keychain"
security import "$certificate" -P "$MACOS_CERTIFICATE_PASSWORD" -A -t cert -f pkcs12 -k "$keychain"
security set-key-partition-list -S apple-tool:,apple:,codesign: -s -k "$password" "$keychain" >/dev/null
security list-keychains -d user -s "$keychain" "$HOME/Library/Keychains/login.keychain-db"
echo "MACOS_SIGNING_IDENTITY=$MACOS_SIGNING_IDENTITY" >> "$GITHUB_ENV"
if [[ -n "${APPLE_ID:-}${APPLE_APP_PASSWORD:-}${APPLE_TEAM_ID:-}" ]]; then
    : "${APPLE_ID:?Missing Apple ID}" "${APPLE_APP_PASSWORD:?Missing app password}" "${APPLE_TEAM_ID:?Missing team ID}"
    xcrun notarytool store-credentials student-mcp --apple-id "$APPLE_ID" --team-id "$APPLE_TEAM_ID" --password "$APPLE_APP_PASSWORD" --keychain "$keychain"
    security default-keychain -d user -s "$keychain"
    echo 'MACOS_NOTARY_PROFILE=student-mcp' >> "$GITHUB_ENV"
fi
