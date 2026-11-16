#!/usr/bin/env python3
"""List the tool surface of every released binary, to pick benchmark versions."""
import hashlib
import json
import os
import platform
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
BIN = ROOT / "benchmark" / ".bin"
BIN.mkdir(parents=True, exist_ok=True)
ASSET = "student-mcp-linux-arm64" if platform.machine() in ("aarch64", "arm64") else "student-mcp-linux-x64"
CONFIG = BIN / "probe-config.json"
source = None
for candidate in (
    Path(os.environ.get("XDG_CONFIG_HOME", "")) / "student-mcp" / "config.json",
    Path.home() / ".config" / "student-mcp" / "config.json",
    Path.home() / ".student-mcp.json",
):
    if candidate.is_file():
        source = candidate
        break
if source is None:
    raise SystemExit("no connected account")
CONFIG.write_bytes(source.read_bytes())
CONFIG.chmod(0o600)

tags = subprocess.run(
    ["gh", "release", "list", "--limit", "30", "--json", "tagName", "--jq", ".[].tagName"],
    cwd=ROOT,
    capture_output=True,
    text=True,
).stdout.split()

for tag in tags:
    target = BIN / tag
    if not target.exists():
        done = subprocess.run(
            ["gh", "release", "download", tag, "--pattern", ASSET, "--output", str(target), "--clobber"],
            cwd=ROOT,
            capture_output=True,
        )
        if done.returncode != 0:
            print(f"{tag:9} no {ASSET}")
            continue
        target.chmod(0o755)
    messages = [
        {"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "probe", "version": "0"}}},
        {"jsonrpc": "2.0", "method": "notifications/initialized"},
        {"jsonrpc": "2.0", "id": 2, "method": "tools/list"},
    ]
    env = dict(os.environ)
    env["STUDENT_MCP_CONFIG_FILE"] = str(CONFIG)
    run = subprocess.run(
        [str(target)], input="\n".join(json.dumps(m) for m in messages) + "\n",
        capture_output=True, text=True, timeout=60, env=env,
    )
    tools = []
    for line in run.stdout.splitlines():
        try:
            data = json.loads(line)
        except Exception:
            continue
        if data.get("id") == 2:
            tools = data["result"]["tools"]
    names = sorted(t["name"] for t in tools)
    digest = hashlib.sha256(json.dumps(names).encode()).hexdigest()[:10]
    print(f"{tag:9} tools={len(names):3} surface={digest} bytes={len(json.dumps(tools))}")
