"""Shared plumbing for every service benchmark.

One MCP stdio client, one HTTP client per school API, and the payload metrics.
A service module (`benchmark/services/<name>.py`) only describes questions and
checks; it never re-implements the transport.
"""
from __future__ import annotations

import json
import os
import re
import subprocess
import time
import urllib.parse
import urllib.request
from pathlib import Path
from typing import Any

# Mirrors session::BROWSER_UA. The school WAF answers 202/challenge without it.
BROWSER_UA = (
    "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) "
    "Chrome/128.0.0.0 Safari/537.36"
)

# Bookkeeping the model also pays for. Useful for diagnosis, not for choosing.
CONTROL_KEYS = {
    "coverage",
    "note",
    "notes",
    "fileurlHint",
    "continuation",
    "warnings",
    "errors",
    "index",
    "cache",
    "expiresInSeconds",
    "expiresInMinutes",
    "documentId",
    "freshness",
    "source",
    "partial",
}

SEVERITY_ORDER = {"blocker": 0, "high": 1, "medium": 2, "info": 3}


def est_tokens(text: str) -> int:
    return len(text) // 4


def load_account(path: Path, service: str = "moodle") -> dict[str, Any] | None:
    """First connected account for one service, credentials only."""
    try:
        data = json.loads(path.read_text())
    except Exception:
        return None
    for account in data.get("accounts") or []:
        settings = account.get(service) or {}
        if settings.get("url") and settings.get("token"):
            return {"id": account.get("id"), "url": settings["url"], "token": settings["token"]}
    return None


def default_config() -> Path:
    """XDG location first, then the pre-XDG file, so the audit works either way."""
    override = os.environ.get("STUDENT_MCP_CONFIG_FILE")
    if override and Path(override).is_file():
        return Path(override)
    candidates = []
    xdg = os.environ.get("XDG_CONFIG_HOME")
    if xdg:
        candidates.append(Path(xdg) / "student-mcp" / "config.json")
    candidates.append(Path.home() / ".config" / "student-mcp" / "config.json")
    candidates.append(Path.home() / ".student-mcp.json")
    for path in candidates:
        if path.is_file():
            return path
    return candidates[0]


class MCP:
    """One stdio MCP session per batch, so each timing is honest."""

    def __init__(self, binary: Path, env: dict[str, str] | None = None):
        self.binary = binary
        self.env = env

    def batch(
        self, calls: list[tuple[str, dict[str, Any]]], timeout: int = 240
    ) -> tuple[list[dict[str, Any]], list[dict[str, Any]]]:
        """Return (tools, one result per call, in order)."""
        messages: list[dict[str, Any]] = [
            {
                "jsonrpc": "2.0",
                "id": 1,
                "method": "initialize",
                "params": {
                    "protocolVersion": "2025-06-18",
                    "capabilities": {},
                    "clientInfo": {"name": "mcp-audit", "version": "1"},
                },
            },
            {"jsonrpc": "2.0", "method": "notifications/initialized"},
            {"jsonrpc": "2.0", "id": 2, "method": "tools/list"},
        ]
        for index, (name, args) in enumerate(calls):
            messages.append(
                {
                    "jsonrpc": "2.0",
                    "id": 100 + index,
                    "method": "tools/call",
                    "params": {"name": name, "arguments": args},
                }
            )
        proc = subprocess.run(
            [str(self.binary)],
            input="\n".join(json.dumps(m) for m in messages) + "\n",
            capture_output=True,
            text=True,
            timeout=timeout,
            env=self.env,
        )
        if proc.returncode != 0:
            raise SystemExit(f"MCP exited {proc.returncode}: {proc.stderr[:400]}")
        tools: list[dict[str, Any]] = []
        results: list[dict[str, Any]] = [
            {"text": "", "is_error": True, "missing": True} for _ in calls
        ]
        for line in proc.stdout.splitlines():
            line = line.strip()
            if not line:
                continue
            message = json.loads(line)
            ident = message.get("id")
            if ident == 2:
                tools = message["result"]["tools"]
            elif isinstance(ident, int) and 100 <= ident < 100 + len(calls):
                result = message.get("result") or {}
                text = "".join(
                    part.get("text", "")
                    for part in result.get("content") or []
                    if isinstance(part, dict)
                )
                results[ident - 100] = {
                    "name": calls[ident - 100][0],
                    "text": text,
                    "types": [p.get("type") for p in result.get("content") or [] if isinstance(p, dict)],
                    "is_error": bool(result.get("isError")),
                    "error": (message.get("error") or {}).get("message"),
                    "missing": False,
                }
        return tools, results


    def one(self, name: str, args: dict[str, Any]) -> tuple[dict[str, Any], float]:
        """One call in its own session, so latency is not shared across a batch."""
        started = time.time()
        _, results = self.batch([(name, args)])
        return results[0], time.time() - started


class School:
    """Ground truth straight from the school API, with the connector's UA."""

    def __init__(self, account: dict[str, Any]):
        self.base = account["url"].rstrip("/") + "/webservice/rest/server.php"
        self.token = account["token"]

    def ws(self, function: str, **params: Any) -> Any:
        query: dict[str, Any] = {
            "wstoken": self.token,
            "moodlewsrestformat": "json",
            "wsfunction": function,
        }
        query.update(params)
        request = urllib.request.Request(
            self.base + "?" + urllib.parse.urlencode(query), headers={"User-Agent": BROWSER_UA}
        )
        return json.loads(urllib.request.urlopen(request, timeout=60).read())




def payload_detail(text: str) -> dict[str, Any]:
    detail: dict[str, Any] = {"bytes": len(text), "tokens": est_tokens(text)}
    try:
        data = json.loads(text)
    except Exception:
        return detail
    if not isinstance(data, dict):
        return detail
    control = sum(len(json.dumps(v)) for k, v in data.items() if k in CONTROL_KEYS)
    detail["control_share"] = round(control / (len(text) or 1), 2)
    detail["keys"] = sorted(data.keys())
    return detail


def measure(result: dict[str, Any], seconds: float) -> dict[str, Any]:
    text = result["text"]
    return {
        "tool": result.get("label") or result.get("name"),
        "ok": not result["is_error"],
        "error": result.get("error") or (text[:300] if result["is_error"] else None),
        "seconds": round(seconds, 2),
        **payload_detail(text),
        "raw_school_error": '"error": "Moodle error (' in text,
        "paged": "nextPage" in text or "continuation" in text,
    }


def course_contents(school: School, courseid: int) -> list[dict[str, Any]]:
    return school.ws("core_course_get_contents", courseid=courseid) or []


def pick_course(
    school: School, courses: list[dict[str, Any]], now: int
) -> dict[str, Any]:
    """Current-session course with the most files: the realistic worst case."""
    current = [
        c
        for c in courses
        if (c.get("enddate") or 0) >= now - 45 * 86400
        and (c.get("startdate") or 0) <= now + 86400
    ] or courses
    best: dict[str, Any] | None = None
    for course in current[:10]:
        try:
            contents = course_contents(school, course["id"])
        except Exception:
            continue
        modules = [m for s in contents for m in (s.get("modules") or [])]
        files = sum(
            1 for m in modules for c in (m.get("contents") or []) if c.get("type") == "file"
        )
        if best is None or files > best["files"]:
            best = {
                "course": course,
                "sections": contents,
                "modules": modules,
                "files": files,
            }
    if best is None:
        raise RuntimeError("no readable course")
    return best


def course_files(school: "School", courseid: int, pattern: str | None = None) -> list[dict[str, Any]]:
    """Files the school actually exposes, with the module they belong to."""
    files: list[dict[str, Any]] = []
    for section in course_contents(school, courseid):
        for module in section.get("modules") or []:
            for content in module.get("contents") or []:
                if content.get("type") != "file":
                    continue
                name = content.get("filename") or ""
                if pattern and not re.search(pattern, name, re.IGNORECASE):
                    continue
                files.append(
                    {
                        "filename": name,
                        "timemodified": content.get("timemodified") or 0,
                        "moduleid": module.get("id"),
                        "modulename": module.get("name"),
                        "section": section.get("name"),
                    }
                )
    return files


def parse_mcp_json(result: dict[str, Any]) -> Any:
    try:
        return json.loads(result["text"])
    except Exception:
        return None

def tools_surface(binary: str, config: str | None = None) -> dict[str, Any]:
    """Tool count and tools/list size for one binary, without touching the account."""
    env = dict(os.environ)
    if config:
        env["STUDENT_MCP_CONFIG_FILE"] = config
    messages = [
        {"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "surface", "version": "0"}}},
        {"jsonrpc": "2.0", "method": "notifications/initialized"},
        {"jsonrpc": "2.0", "id": 2, "method": "tools/list"},
    ]
    run = subprocess.run(
        [binary],
        input="\n".join(json.dumps(m) for m in messages) + "\n",
        capture_output=True,
        text=True,
        timeout=60,
        env=env,
    )
    tools: list[dict[str, Any]] = []
    for line in run.stdout.splitlines():
        try:
            data = json.loads(line)
        except Exception:
            continue
        if data.get("id") == 2:
            tools = data["result"]["tools"]
    return {
        "count": len(tools),
        "bytes": len(json.dumps(tools)),
        "names": sorted(tool.get("name", "") for tool in tools),
    }
