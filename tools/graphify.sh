#!/bin/sh
# Rebuild the local source graph without semantic extraction or LLM labeling.
set -eu
cd "$(dirname "$0")/.."

if ! command -v graphify >/dev/null 2>&1; then
    echo 'Install Graphify first: uv tool install graphifyy==0.9.70' >&2
    exit 1
fi

export GRAPHIFY_BACKEND=none
graphify extract . --code-only --cargo --max-workers 2
graphify cluster-only . --no-label
