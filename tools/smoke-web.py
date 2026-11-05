#!/usr/bin/env python3
"""Linux browser-UI smoke test using an isolated config and a stub browser."""
import json
from concurrent.futures import ThreadPoolExecutor
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import os
from pathlib import Path
import re
import socket
import subprocess
import sys
import tempfile
import threading
import time
import urllib.error
import urllib.request

binary = Path(sys.argv[1] if len(sys.argv) > 1 else "target/release/student-mcp").resolve()
with tempfile.TemporaryDirectory(prefix="student-mcp-web-") as tmp:
    root = Path(tmp)
    opener = root / "xdg-open"
    opened = root / 'browser-opened'
    opener.write_text('#!/bin/sh\nprintf x >> "$STUDENT_MCP_BROWSER_MARKER"\n')
    opener.chmod(0o700)
    config = root / "settings.json"
    school_started = threading.Event()
    school_release = threading.Event()

    class School(BaseHTTPRequestHandler):
        def log_message(self, *args):
            pass

        def do_POST(self):
            self.rfile.read(int(self.headers.get("Content-Length", "0")))
            school_started.set()
            school_release.wait(10)
            body = json.dumps({"userid": 1, "username": "Student", "sitename": "Local school"}).encode()
            try:
                self.send_response(200)
                self.send_header("Content-Type", "application/json")
                self.send_header("Content-Length", str(len(body)))
                self.end_headers()
                self.wfile.write(body)
            except (BrokenPipeError, ConnectionResetError):
                pass

    school = ThreadingHTTPServer(("127.0.0.1", 0), School)
    threading.Thread(target=school.serve_forever, daemon=True).start()
    school_url = f"http://127.0.0.1:{school.server_port}"
    config.write_text(json.dumps({"gateway": {"public_enabled": False, "local_enabled": True},
                                  "moodle": {"url": school_url, "token": "fixture", "allow_write": False}}))
    env = dict(os.environ, STUDENT_MCP_CONFIG_FILE=str(config),
               STUDENT_MCP_BROWSER_MARKER=str(opened), XDG_CONFIG_HOME=str(root / '.config'),
               STUDENT_MCP_BROWSER=str(root / 'missing-chrome'),
               PATH=f"{root}:{os.environ['PATH']}")
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

        status, page = request("/", auth=False)
        assert status == 200
        assert b"Allow assistant changes" not in page
        assert b'data-action="permissions"' not in page
        assert b'L\xc3\xa9a / Mio \xc2\xb7 Read only' not in page
        assert b'Experimental' not in page
        assert b'myCourses \xc2\xb7 Read only' not in page
        home = page.split(b'function home(){', 1)[1].split(b'function header(', 1)[0]
        manage = page.split(b'function manageScreenContent(){', 1)[1].split(b'function serviceScreen(){', 1)[0]
        links = page.split(b'function linkScreen(){', 1)[1].split(b'function publicMessage(', 1)[0]
        assert b'Launch at startup' in home and b'name="autostart"' in home
        assert b'name="advanced"' not in home and b'Allow changes' not in manage
        assert b'data-screen="mail"' in home and b"t('Mail transfer')" in home
        assert b"mail-diagram" in page and b"School mail" in page and b"Personal mail" in page
        assert b"Connect to Outlook" in page and b"Forwarding on" in page
        assert b"Your school blocks automatic forwarding." in page
        assert b"Ton \xc3\xa9cole bloque le transfert automatique" in page
        assert b"Transfert actif" in page and b"Mail de l" in page
        logo_outlook = request("/assets/outlook.png", auth=False)
        assert logo_outlook[0] == 200 and logo_outlook[1].startswith(b"\x89PNG\r\n\x1a\n")
        assert b'name="autostart"' not in links
        assert manage.index(b'class="status-row"') < manage.index(b'data-add') < manage.index(b'class="manage-actions"')
        assert b'accountChoices()' in manage and b'account-list' not in manage
        assert b'.startup-row .switch:checked{background:var(--blue)' in page
        translations = page.split(b'const toolDescriptions={', 1)[1].split(b'\n};', 1)[0]
        translated_tool_names = set(re.findall(
            rb"^ (\w+):\['[^']{20,}','[^']{20,}'\]", translations, re.M
        ))
        assert b"canvas" in home and b"ZoneCours" in page
        logo_status, logo = request("/assets/moodle.png", auth=False)
        assert logo_status == 200 and logo.startswith(b"\x89PNG\r\n\x1a\n")
        canvas_status, canvas = request("/assets/canvas.png", auth=False)
        assert canvas_status == 200 and canvas.startswith(b"\x89PNG\r\n\x1a\n")
        icon_status, icon = request("/assets/app-icon.png", auth=False)
        assert icon_status == 200 and icon.startswith(b"\x89PNG\r\n\x1a\n")
        assert b'<link rel="icon" type="image/png" href="/assets/app-icon.png">' in page
        assert request("/state", auth=False)[0] == 401
        assert request("/update", auth=False)[0] == 401
        assert request("/update", origin="https://example.com")[0] == 401
        update_status, update_body = request("/update")
        assert update_status == 200
        assert isinstance(json.loads(update_body)["available"], bool)
        assert request("/action", {"action": "update"}, auth=False)[0] == 401
        assert request("/tools/moodle", auth=False)[0] == 401
        assert request("/tools/unknown")[0] == 404
        for service in ("moodle", "omnivox", "brightspace"):
            status, body = request(f"/tools/{service}")
            assert status == 200
            tools = json.loads(body)["tools"]
            assert tools
            assert all(tool["name"].startswith(service + "_") or
                       (service == "moodle" and tool["name"].startswith("ws_"))
                       for tool in tools)
            assert {tool["name"].encode() for tool in tools} <= translated_tool_names, service
        assert request("/state", origin="https://untrusted.invalid")[0] == 401
        state = json.loads(request("/state")[1])
        assert len(state["accounts"]) == 1
        assert state["accounts"][0]["enabled"] is True
        saved_token = json.loads(config.read_text())["moodle"]["token"]
        status, body = request("/action", {"action": "service_enabled", "service": "moodle",
                                             "id": "moodle", "enabled": False})
        assert status == 200 and json.loads(body)["enabled"] is False
        disabled_state = json.loads(request("/state")[1])
        assert disabled_state["accounts"][0]["enabled"] is False
        assert json.loads(config.read_text())["moodle"]["token"] == saved_token
        connector_path = "/mcp/" + disabled_state["local_link"].rsplit("/", 1)[-1]
        status, body = request(connector_path, {"jsonrpc": "2.0", "id": 1, "method": "tools/list",
                                                "params": {"_meta": {
                                                    "io.modelcontextprotocol/protocolVersion": "2026-07-28"}}},
                               auth=False, address=f"http://127.0.0.1:{port}")
        assert status == 200
        assert [tool["name"] for tool in json.loads(body)["result"]["tools"]] == ["student_status"]
        status, body = request("/action", {"action": "service_enabled", "service": "moodle",
                                             "id": "moodle", "enabled": True})
        assert status == 200 and json.loads(body)["enabled"] is True
        assert json.loads(request("/state")[1])["accounts"][0]["enabled"] is True
        assert json.loads(config.read_text())["moodle"]["token"] == saved_token
        enabled_state = json.loads(request("/state")[1])
        assert "advanced" not in enabled_state
        assert "allow_changes" not in enabled_state["accounts"][0]
        status, body = request(connector_path, {"jsonrpc": "2.0", "id": 2, "method": "tools/list",
                                                "params": {"_meta": {
                                                    "io.modelcontextprotocol/protocolVersion": "2026-07-28"}}},
                               auth=False, address=f"http://127.0.0.1:{port}")
        names = [tool["name"] for tool in json.loads(body)["result"]["tools"]]
        assert names[0] == "student_status"
        assert "moodle_search" in names and "moodle_deadlines" in names
        assert "ws_call" in names and "ws_discover" in names
        status, body = request("/action", {"action": "allow_changes", "service": "moodle",
                                             "id": "moodle", "enabled": True})
        assert status == 400
        assert json.loads(config.read_text())["moodle"]["token"] == saved_token
        status, body = request("/action", {"action": "advanced_tools", "enabled": True})
        assert status == 400
        assert json.loads(config.read_text())["moodle"]["token"] == saved_token
        assert state["autostart"] is False
        assert request("/action", {"action": "autostart", "enabled": True}, auth=False)[0] == 401
        status, body = request("/action", {"action": "autostart", "enabled": True})
        assert status == 200 and "message" not in json.loads(body)
        entry = root / '.config' / 'autostart' / 'student-mcp.desktop'
        assert 'up --background' in entry.read_text()
        assert json.loads(request("/state")[1])["autostart"] is True
        assert request("/action", {"action": "autostart", "enabled": False})[0] == 200
        assert not entry.exists()
        assert json.loads(request("/state")[1])["autostart"] is False
        assert request("/action", {"action": "permissions", "service": "moodle", "id": "moodle", "allow_write": True})[0] == 400
        assert "allow_write" not in json.loads(request("/state")[1])["accounts"][0]
        assert request("/action", {"action": "verify", "code": "123456"})[0] == 400
        assert b'id="browser-connect-form"' in page
        assert b'Connect with browser' in page
        assert b'Other connection methods' in page
        assert b'Advanced options' not in page
        assert b'function advancedFields' not in page
        assert b'gateway_url' not in page
        assert request("/action", {"action": "browser_connect", "service": "brightspace", "url": "https://school.example"}, auth=False)[0] == 401
        assert request("/action", {"action": "browser_connect", "service": "moodle", "url": "https://school.example"})[0] == 400
        assert request("/action", {"action": "browser_connect", "service": "omnivox", "url": "https://other.example"})[0] == 400
        assert request("/action", {"action": "browser_connect", "service": "brightspace", "url": "https://school.example"})[0] == 400
        assert request("/action", {"action": "cancel"})[0] == 200
        assert json.loads(request("/state")[1])["browser_login"] is None
        assert request("/action", {"action": "stop"}, auth=False)[0] == 401
        assert request("/state", address=f"http://127.0.0.1:{port}")[0] == 404
        subprocess.run(args, env=env, timeout=5, check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        assert process.poll() is None, "reopen stopped original server"
        deadline = time.monotonic() + 5
        while not opened.exists() or len(opened.read_text()) < 2:
            assert time.monotonic() < deadline, "browser reopen did not complete"
            time.sleep(0.05)
        before = opened.read_text()
        subprocess.run([str(binary), "up", f"--port={port}", "--background"],
                       env=env, timeout=5, check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        assert opened.read_text() == before, "background startup reopened the browser"
        status = Path(f"/proc/{process.pid}/status").read_text()
        print(next(line for line in status.splitlines() if line.startswith("VmRSS:")))
        with ThreadPoolExecutor(max_workers=1) as pool:
            check = pool.submit(request, "/action", {"action": "check", "service": "moodle", "id": "moodle"})
            try:
                assert school_started.wait(3), "check never reached the fixture"
                start = time.monotonic()
                assert request("/state")[0] == 200
                assert time.monotonic() - start < 2, "slow school blocked status"
                assert request("/action", {"action": "disconnect", "service": "moodle", "id": "moodle"})[0] == 400
                assert request("/action", {"action": "links", "local": True, "public": False})[0] == 200
            finally:
                school_release.set()
            code, result = check.result(timeout=5)
            assert code == 200 and "Student" in json.loads(result)["message"], (code, result)

            school_started.clear()
            school_release.clear()
            check = pool.submit(request, "/action", {"action": "check", "service": "moodle", "id": "moodle"})
            try:
                assert school_started.wait(3)
                assert request("/action", {"action": "stop"})[0] == 200
                assert process.wait(timeout=3) == 0, "slow school blocked shutdown"
            finally:
                school_release.set()
        assert not discovery.exists()
        print("Web smoke passed: auth, autostart, listener isolation, reopen, slow-school responsiveness and stop")
    finally:
        school_release.set()
        school.shutdown()
        school.server_close()
        if process.poll() is None:
            process.terminate()
            process.wait(timeout=5)
