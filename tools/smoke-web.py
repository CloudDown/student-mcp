#!/usr/bin/env python3
"""Linux browser-UI smoke test using an isolated config and a stub browser."""
import json
import os
from pathlib import Path
import socket
import subprocess
import sys
import tempfile
import time
import urllib.error
import urllib.request

binary = Path(sys.argv[1] if len(sys.argv) > 1 else "target/release/student-mcp").resolve()
with tempfile.TemporaryDirectory(prefix="student-mcp-web-") as tmp:
    root = Path(tmp)
    opener = root / "xdg-open"
    opener.write_text('#!/bin/sh\nexit 0\n')
    opener.chmod(0o700)
    config = root / "settings.json"
    config.write_text(json.dumps({"gateway": {"public_enabled": False, "local_enabled": True},
                                  "moodle": {"url": "https://example.invalid", "token": "fixture", "allow_write": False}}))
    env = dict(os.environ, STUDENT_MCP_CONFIG_FILE=str(config), PATH=f"{root}:{os.environ['PATH']}")
    with socket.socket() as sock:
        sock.bind(("127.0.0.1", 0))
        port = sock.getsockname()[1]
    args = [str(binary), "up", f"--port={port}"]
    process = subprocess.Popen(args, env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    try:
        discovery = config.with_suffix(".ui.json")
        deadline = time.monotonic() + 10
        while not discovery.exists():
            assert process.poll() is None, "server exited"
            assert time.monotonic() < deadline, "startup timeout"
            time.sleep(0.05)
        admin_port, key = json.loads(discovery.read_text())
        assert discovery.stat().st_mode & 0o077 == 0
        base = f"http://127.0.0.1:{admin_port}"

        def request(path, data=None, auth=True, origin=None, address=base):
            headers = {"Content-Type": "application/json"}
            if auth:
                headers["Authorization"] = "Bearer " + key
            if origin:
                headers["Origin"] = origin
            body = json.dumps(data).encode() if data is not None else None
            try:
                response = urllib.request.urlopen(urllib.request.Request(address + path, body, headers), timeout=5)
            except urllib.error.HTTPError as error:
                response = error
            return response.status, response.read()

        assert request("/", auth=False)[0] == 200
        logo_status, logo = request("/assets/moodle.png", auth=False)
        assert logo_status == 200 and logo.startswith(b"\x89PNG\r\n\x1a\n")
        assert request("/state", auth=False)[0] == 401
        assert request("/state", origin="https://untrusted.invalid")[0] == 401
        state = json.loads(request("/state")[1])
        assert len(state["accounts"]) == 1
        assert request("/action", {"action": "permissions", "service": "moodle", "id": "moodle", "allow_write": True})[0] == 200
        assert json.loads(config.read_text())["moodle"]["allow_write"] is True
        assert request("/action", {"action": "verify", "code": "123456"})[0] == 400
        assert request("/action", {"action": "stop"}, auth=False)[0] == 401
        assert request("/state", address=f"http://127.0.0.1:{port}")[0] == 404
        subprocess.run(args, env=env, timeout=5, check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        assert process.poll() is None, "reopen stopped original server"
        status = Path(f"/proc/{process.pid}/status").read_text()
        print(next(line for line in status.splitlines() if line.startswith("VmRSS:")))
        assert request("/action", {"action": "stop"})[0] == 200
        assert process.wait(timeout=5) == 0
        assert not discovery.exists()
        print("Web smoke passed: auth, origin, listener isolation, permissions, reopen, stop")
    finally:
        if process.poll() is None:
            process.terminate()
            process.wait(timeout=5)
