#!/usr/bin/env python3
"""Live MCP quality audit for student-mcp.

Replays real student intents against the stdio MCP server, then compares what
the tools return with what the school API actually holds. It measures the three
failure modes we keep hitting: payloads too big to read, data the tool hides,
and raw school errors leaking into answers.

Usage:
  python3 tools/mcp-audit.py                       # audit target/release/student-mcp
  python3 tools/mcp-audit.py --bin target/debug/student-mcp
  python3 tools/mcp-audit.py --baseline ~/.student-mcp.eval/baseline.json
  python3 tools/mcp-audit.py --write-baseline
  python3 tools/mcp-audit.py --offline             # no connected account

Exit code 1 when a high/blocker finding appears, so it can gate a change.
Standard library only. The school token is read from the student-mcp config and
never printed. Reports default to ~/.student-mcp.eval/ (outside the repository).
"""
from __future__ import annotations

import argparse
import json
import os
import re
import subprocess
import sys
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


def load_account(path: Path) -> dict[str, Any] | None:
    try:
        data = json.loads(path.read_text())
    except Exception:
        return None
    for account in data.get("accounts") or []:
        moodle = account.get("moodle") or {}
        if moodle.get("url") and moodle.get("token"):
            return {"id": account.get("id"), "url": moodle["url"], "token": moodle["token"]}
    return None


def default_config() -> Path:
    """XDG location first, then the pre-XDG file, so the audit works either way."""
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

    def __init__(self, binary: Path):
        self.binary = binary

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


def audit(binary: Path, account: dict[str, Any] | None) -> dict[str, Any]:
    mcp = MCP(binary)
    report: dict[str, Any] = {
        "version": None,
        "tools": 0,
        "tools_list_bytes": 0,
        "calls": [],
        "findings": [],
        "heaviest_descriptions": [],
    }

    tools, _ = mcp.batch([])
    report["tools"] = len(tools)
    report["tools_list_bytes"] = len(json.dumps(tools))
    report["heaviest_descriptions"] = sorted(
        ((len(t.get("description") or ""), t["name"]) for t in tools), reverse=True
    )[:5]

    baseline_calls: list[tuple[str, dict[str, Any]]] = [
        ("student_status", {}),
        ("moodle_my_courses", {}),
        ("moodle_deadlines", {"days": 30}),
        ("moodle_grades", {}),
        ("moodle_recent_grades", {}),
    ]
    baseline_results: list[dict[str, Any]] = []
    for name, args in baseline_calls:
        result, seconds = mcp.one(name, args)
        baseline_results.append(result)
        report["calls"].append(measure(result, seconds))
    if baseline_results[0]["text"]:
        try:
            report["version"] = json.loads(baseline_results[0]["text"]).get("version")
        except Exception:
            pass
    courses: list[dict[str, Any]] = []
    if baseline_results[1]["text"]:
        try:
            courses = json.loads(baseline_results[1]["text"]).get("courses") or []
        except Exception:
            pass
    if not courses:
        report["findings"].append(
            {"severity": "blocker", "message": "moodle_my_courses returned no course; audit stopped."}
        )
        return report

    target_course: dict[str, Any] = courses[0]
    module_id: int | None = None
    school: School | None = None
    if account:
        school = School(account)
        try:
            target = pick_course(school, courses, int(time.time()))
            target_course = target["course"]
            if target["modules"]:
                module_id = target["modules"][0].get("id")
            report["target_course"] = {
                "courseid": target_course["id"],
                "name": target_course.get("shortname") or target_course.get("fullname"),
                "school_sections": len(target["sections"]),
                "school_modules": len(target["modules"]),
                "school_files": target["files"],
                "module_ids": [m.get("id") for m in target["modules"][:5]],
            }
        except Exception as exc:
            report["findings"].append(
                {"severity": "info", "message": f"Ground truth unavailable: {type(exc).__name__}: {exc}"}
            )

    calls: list[tuple[str, dict[str, Any]]] = [
        ("moodle_course_content", {"courseid": target_course["id"]}),
        ("moodle_search", {"query": f"{(target_course.get('shortname') or 'TD').split(' ')[0]} TD exercices"}),
        ("moodle_assignments", {}),
    ]
    if module_id:
        calls.append(
            ("moodle_course_content", {"courseid": target_course["id"], "detail": "full", "moduleid": module_id})
        )
    results: list[dict[str, Any]] = []
    for name, args in calls:
        label = name if name != "moodle_course_content" or "detail" not in args else f"{name}(full)"
        result, seconds = mcp.one(name, args)
        result["label"] = label
        results.append(result)
        report["calls"].append(measure(result, seconds))
    overview_text = results[0]["text"]

    if overview_text:
        try:
            data = json.loads(overview_text)
        except Exception:
            data = None
        if isinstance(data, dict):
            coverage = data.get("coverage") or {}
            shown = len(data.get("sections") or data.get("modules") or [])
            total = coverage.get("totalSections")
            report["overview_page"] = {
                "sections_shown": shown,
                "total_sections": total,
                "needs_next_page": bool(data.get("nextPage") or (total and shown < total)),
            }
            expected = (report.get("target_course") or {}).get("school_sections")
            if expected and total and total != expected:
                report["findings"].append(
                    {
                        "severity": "high",
                        "message": f"Overview reports {total} sections; school API has {expected}.",
                    }
                )

    if school is not None and baseline_results[3]["text"]:
        try:
            raw = school.ws("gradereport_overview_get_course_grades")
            graded = [
                g
                for g in raw.get("grades") or []
                if str(g.get("grade") or "").strip() not in ("", "-")
            ]
            grades_text = baseline_results[3]["text"]
            try:
                shown_grades = json.loads(grades_text).get("count", 0)
            except Exception:
                shown_grades = 0
            if len(graded) > shown_grades:
                lower = grades_text.lower()
                disclosed = any(w in lower for w in ("past", "archiv", "not current", "ancien"))
                report["findings"].append(
                    {
                        "severity": "info" if disclosed else "high",
                        "message": (
                            f"moodle_grades shows {shown_grades} of {len(graded)} courses the "
                            "gradebook reports with a grade"
                            + (
                                ""
                                if disclosed
                                else "; the response does not say past courses were filtered"
                            )
                        ),
                    }
                )
        except Exception:
            pass

    for call in report["calls"]:
        if call["bytes"] > 12000 and call["tool"] in ("moodle_search", "moodle_course_content"):
            report["findings"].append(
                {
                    "severity": "high",
                    "message": (
                        f"{call['tool']} returned {call['bytes']} bytes (~{call['tokens']} tokens) "
                        "in one call; the model must read all of it before answering."
                    ),
                }
            )
        if call["seconds"] > 5:
            report["findings"].append(
                {
                    "severity": "medium",
                    "message": f"{call['tool']} took {call['seconds']:.1f}s; a slow tool gets abandoned.",
                }
            )
        if call.get("raw_school_error"):
            report["findings"].append(
                {
                    "severity": "medium",
                    "message": f"{call['tool']} leaks raw Moodle error text (permissions, invalidrecord...).",
                }
            )
        if call["ok"] is False and call["bytes"] < 200:
            report["findings"].append(
                {
                    "severity": "medium",
                    "message": f"{call['tool']} failed: {(call['error'] or '').strip()[:160]}",
                }
            )

    report["findings"] = dedupe(report["findings"])
    report["calls"].sort(key=lambda c: c.get("bytes") or 0, reverse=True)
    return report


def dedupe(findings: list[dict[str, Any]]) -> list[dict[str, Any]]:
    seen: set[str] = set()
    unique: list[dict[str, Any]] = []
    for finding in findings:
        key = finding["severity"] + finding["message"].split(" (")[0]
        if key in seen:
            continue
        seen.add(key)
        unique.append(finding)
    return unique


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


def intents(binary: Path, account: dict[str, Any] | None) -> list[dict[str, Any]]:
    """Twenty real student questions, graded against the school API.

    Every row is a question an assistant would receive. PASS means the tools carry
    enough correct data for a right answer, PARTIAL means a plausible answer would
    be incomplete, FAIL means the model must guess or invent, SKIP means this
    account does not hold the data the question needs.
    """
    mcp = MCP(binary)
    school = School(account) if account else None
    rows: list[dict[str, Any]] = []

    def row(question: str, verdict: str, evidence: str, tool: str = "-", payload: int = 0) -> None:
        rows.append(
            {
                "n": len(rows) + 1,
                "question": question,
                "verdict": verdict,
                "evidence": evidence,
                "tool": tool,
                "bytes": payload,
            }
        )

    def call(name: str, args: dict[str, Any]) -> tuple[dict[str, Any], Any, float]:
        result, seconds = mcp.one(name, args)
        return result, parse_mcp_json(result), seconds

    # --- context -----------------------------------------------------------
    result, data, _ = call("moodle_my_courses", {})
    courses = (data or {}).get("courses") or []
    by_code: dict[str, dict[str, Any]] = {}
    for course in courses:
        code = (course.get("shortname") or "").split(" ")[0]
        if code:
            by_code.setdefault(code, course)
    now = int(time.time())

    # 1 — courses ------------------------------------------------------------
    evidence = f"{len(courses)} cours listés"
    verdict = "PASS" if courses else "FAIL"
    if school:
        try:
            info = school.ws("core_webservice_get_site_info")
            truth = school.ws("core_enrol_get_users_courses", userid=info["userid"])
            evidence += f", école {len(truth)}"
            if len(truth) != len(courses):
                disclosed = any(
                    word in result["text"].lower()
                    for word in ("past", "ended", "excluded", "archiv", "ancien")
                )
                verdict = "PASS" if disclosed else "PARTIAL"
                evidence += " | filtre annoncé" if disclosed else " | filtre silencieux"
        except Exception as exc:
            evidence += f" (vérité indisponible: {type(exc).__name__})"
    row("« Quels cours j'ai cette session ? »", verdict, evidence, "moodle_my_courses", len(result["text"]))

    # 2 — due in the next 7 days --------------------------------------------
    result, data, _ = call("moodle_deadlines", {"days": 7})
    items = (data or {}).get("deadlines") or []
    names = [item.get("name") for item in items if item.get("name")]
    evidence = f"{len(names)} élément(s)" + (f": {names[0]}" if names else "")
    verdict = "PASS"
    if school:
        try:
            events = school.ws(
                "core_calendar_get_action_events_by_timesort",
                limitnum=50,
                timesortfrom=now,
                timesortto=now + 7 * 86400,
            ).get("events") or []
            missing = [e.get("name") for e in events if e.get("name") not in " ".join(names)]
            evidence += f" | école {len(events)}"
            if missing:
                verdict = "FAIL"
                evidence += f" | manquants: {missing[:2]}"
        except Exception as exc:
            evidence += f" (vérité indisponible: {type(exc).__name__})"
    row("« Qu'est-ce que je dois remettre cette semaine ? »", verdict, evidence, "moodle_deadlines", len(result["text"]))

    # 3 — what is already overdue -------------------------------------------
    result, data, _ = call("moodle_deadlines", {"days": 14, "include_expired": True})
    expired = [
        item
        for item in (data or {}).get("deadlines") or []
        if (item.get("time") or 0) and (item.get("time") or 0) < now
    ]
    row(
        "« Qu'est-ce qui est déjà en retard ? »",
        "PASS" if expired else "SKIP",
        f"{len(expired)} élément(s) échu(s) dans la fenêtre" if expired else "aucun échu dans la fenêtre",
        "moodle_deadlines include_expired",
        len(result["text"]),
    )

    if not school:
        for question, tool in (
            ("« Le dernier TD du cours ? »", "moodle_search"),
            ("« Donne-moi le corrigé du TD3 »", "moodle_search"),
            ("« Trouve le plan de cours »", "moodle_search"),
        ):
            row(question, "SKIP", "vérité terrain indisponible (--offline)", tool)
        return rows

    # 4 — discovery of a recent exercise sheet, per course -------------------
    audited = 0
    best = 0
    best_code = ""
    best_recent = 0
    payload = 0
    for code, course in list(by_code.items())[:3]:
        try:
            sheets = course_files(school, course["id"], r"\bTD\b|TD\d|_TD")
        except Exception:
            continue
        recent = [f for f in sheets if f["timemodified"] >= now - 180 * 86400]
        if not recent:
            continue
        audited += 1
        result, data, _ = call("moodle_search", {"query": f"{code} TD"})
        found = [r.get("filename") for r in (data or {}).get("results") or []]
        ranks = [found.index(f["filename"]) + 1 for f in recent if f["filename"] in found]
        rank = min(ranks) if ranks else 0
        if rank and (not best or rank < best):
            best, best_code, best_recent, payload = rank, code, len(recent), len(result["text"])
    if audited:
        verdict = "PASS" if best and best <= 3 else ("PARTIAL" if best and best <= 8 else "FAIL")
        row(
            "« C'est quoi le dernier TD de mon cours ? »",
            verdict,
            f"{audited} cours inspecté(s), meilleur rang {best or 'absent'}"
            + (f" ({best_code}, {best_recent} TD récents)" if best_code else ""),
            "moodle_search",
            payload,
        )
    else:
        row("« C'est quoi le dernier TD de mon cours ? »", "SKIP", "aucun TD daté récent détecté", "moodle_search")

    # 5 — a correction, by type --------------------------------------------
    emitted = False
    for course in list(by_code.values())[:3]:
        try:
            corrections = course_files(school, course["id"], r"corrig|correction")
        except Exception:
            continue
        if not corrections:
            continue
        result, data, _ = call("moodle_search", {"query": "corrigé exercices"})
        found = [r.get("filename") for r in (data or {}).get("results") or []]
        hit = next((f for f in corrections if f["filename"] in found), None)
        row(
            "« Donne-moi le corrigé des exercices »",
            "PASS" if hit else "FAIL",
            f"{len(corrections)} corrigé(s) dans ce cours | trouvé: {hit['filename'] if hit else 'aucun'}",
            "moodle_search",
            len(result["text"]),
        )
        emitted = True
        break
    if not emitted:
        row("« Donne-moi le corrigé des exercices »", "SKIP", "aucun corrigé dans les cours inspectés", "moodle_search")

    # 6 — course outline ----------------------------------------------------
    emitted = False
    for course in list(by_code.values())[:3]:
        try:
            candidates = course_files(school, course["id"], r"plan|syllabus|entente")
        except Exception:
            continue
        if not candidates:
            continue
        result, data, _ = call("moodle_search", {"query": "plan de cours", "courseid": course["id"]})
        found = [r.get("filename") for r in (data or {}).get("results") or []]
        outline = next((f for f in candidates if f["filename"] in found), None)
        row(
            "« Trouve le plan de cours »",
            "PASS" if outline else "FAIL",
            f"{len(candidates)} candidat(s) dans ce cours | trouvé: {outline['filename'] if outline else 'aucun'}",
            "moodle_search",
            len(result["text"]),
        )
        emitted = True
        break
    if not emitted:
        row("« Trouve le plan de cours »", "SKIP", "aucun plan de cours dans les cours inspectés", "moodle_search")

    # --- the heaviest course: reading, pagination, files -------------------
    target = courses[0]
    try:
        best_files = -1
        for course in courses[:6]:
            count = len(course_files(school, course["id"]))
            if count > best_files:
                target, best_files = course, count
        contents = school.ws("core_course_get_contents", courseid=target["id"]) or []
        school_sections = len(contents)
        modules = [m for s in contents for m in (s.get("modules") or [])]
    except Exception:
        school_sections, modules = 0, []

    # 7 — overview page -----------------------------------------------------
    result, data, _ = call("moodle_course_content", {"courseid": target["id"]})
    sections = (data or {}).get("sections") or []
    total = ((data or {}).get("coverage") or {}).get("totalSections")
    has_next = bool((data or {}).get("nextPage"))
    verdict = "PASS" if sections and (not total or total <= len(sections) or has_next) else "FAIL"
    row(
        "« Montre-moi le contenu du cours »",
        verdict,
        f"{len(sections)} section(s) sur {total or '?'} | nextPage {'oui' if has_next else 'non'}"
        + (" | pas de nextPage alors que tout n'est pas là" if total and total > len(sections) and not has_next else ""),
        "moodle_course_content",
        len(result["text"]),
    )

    # 8 — full instructions -------------------------------------------------
    module_id = next((m.get("id") for m in modules if m.get("description")), None) or (modules[0].get("id") if modules else None)
    full_text = ""
    if module_id:
        result, data, _ = call(
            "moodle_course_content",
            {"courseid": target["id"], "detail": "full", "moduleid": module_id},
        )
        full_text = result["text"]
        verdict = "PASS" if len(full_text) > 400 and not result["is_error"] else "PARTIAL"
        row(
            "« Donne-moi les consignes complètes de l'activité »",
            verdict,
            f"{len(full_text)} octets de texte complet, erreur={result['is_error']}",
            "moodle_course_content detail=full",
            len(full_text),
        )

    # 9 — read a PDF as text ------------------------------------------------
    pdf = None
    for module in modules:
        for content in module.get("contents") or []:
            if (content.get("mimetype") or "").startswith("application/pdf"):
                pdf = content
                break
        if pdf:
            break
    if pdf:
        result, _, seconds = call("moodle_file", {"fileurl": pdf["fileurl"], "format": "text"})
        text = result["text"]
        has_text = any(ch.isalpha() for ch in text) and len(text) > 200
        row(
            "« Lis-moi ce PDF et résume-le »",
            "PASS" if has_text and not result["is_error"] else "FAIL",
            f"{len(text)} octets extraits en {seconds:.1f}s, erreur={result['is_error']}",
            "moodle_file format=text",
            len(text),
        )
        # 10 — same PDF, visual pages
        result, _, seconds = call(
            "moodle_file", {"fileurl": pdf["fileurl"], "format": "images", "page_from": 1, "page_to": 1}
        )
        visual = "image" in (result.get("types") or [])
        row(
            "« Montre-moi la figure de la première page »",
            "PASS" if visual else "FAIL",
            f"blocs image={visual}, {seconds:.1f}s, erreur={result['is_error']}",
            "moodle_file format=images",
            len(result["text"]),
        )
    else:
        row("« Lis-moi ce PDF et résume-le »", "SKIP", "aucun PDF dans le cours audité", "moodle_file")
        row("« Montre-moi la figure de la première page »", "SKIP", "aucun PDF dans le cours audité", "moodle_file")

    # 11 — grades overview --------------------------------------------------
    result, data, _ = call("moodle_grades", {})
    shown = len((data or {}).get("grades") or [])
    graded = []
    try:
        raw = school.ws("gradereport_overview_get_course_grades").get("grades") or []
        graded = [g for g in raw if str(g.get("grade") or "").strip() not in ("", "-")]
    except Exception:
        pass
    if graded:
        disclosed = any(word in result["text"].lower() for word in ("past", "archiv", "not current", "ancien"))
        verdict = "PASS" if shown >= len(graded) else ("PARTIAL" if disclosed else "FAIL")
        row(
            "« Quelles notes j'ai eues ? »",
            verdict,
            f"{shown} cours affiché(s), {len(graded)} noté(s) au carnet" + ("" if disclosed else ", filtre non annoncé"),
            "moodle_grades",
            len(result["text"]),
        )
    else:
        row("« Quelles notes j'ai eues ? »", "SKIP", "carnet sans note publiée", "moodle_grades", len(result["text"]))

    # 12 — one course, item level ------------------------------------------
    course_id = graded[0]["courseid"] if graded else (courses[0]["id"] if courses else 0)
    code = next((c.get("shortname", "").split(" ")[0] for c in courses if c.get("id") == course_id), "")
    result, data, _ = call("moodle_grades", {"courseid": course_id})
    count = len((data or {}).get("grades") or [])
    row(
        f"« J'ai combien dans mon cours {code} ? »" if code else "« J'ai combien dans ce cours ? »",
        "PASS" if count else ("SKIP" if not graded else "PARTIAL"),
        f"{count} élément(s) noté(s) détaillés",
        "moodle_grades courseid",
        len(result["text"]),
    )

    # 13 — weights honesty --------------------------------------------------
    weights_disclosed = '"weightsAvailable"' in result["text"] or '"weightsSource"' in result["text"]
    row(
        "« Quelle est la pondération du final ? »",
        "PASS" if weights_disclosed else "PARTIAL",
        "la réponse dit si les poids sont disponibles" if weights_disclosed else "aucun champ de pondération dans la réponse",
        "moodle_grades",
        len(result["text"]),
    )

    # 14 — announcements ----------------------------------------------------
    result, data, _ = call("moodle_announcements", {"limit": 5})
    count = len((data or {}).get("announcements") or data or [])
    row(
        "« Quelles annonces le prof a postées ? »",
        "PASS" if count else "PARTIAL",
        f"{count} annonce(s) lisible(s)",
        "moodle_announcements",
        len(result["text"]),
    )

    # 15 — forum thread -----------------------------------------------------
    result, data, _ = call("moodle_forums", {"courseids": [target["id"]]})
    forums = (data or {}).get("forums") or []
    if forums:
        result, data, _ = call(
            "moodle_forum_discussions", {"forumid": forums[0]["id"], "limit": 3}
        )
        discussions = (data or {}).get("discussions") or []
        row(
            "« Quelles discussions sur le forum du cours ? »",
            "PASS" if not result["is_error"] else "FAIL",
            f"{len(forums)} forum(s), {len(discussions)} discussion(s) lue(s)",
            "moodle_forums → moodle_forum_discussions",
            len(result["text"]),
        )
    else:
        row("« Quelles discussions sur le forum du cours ? »", "SKIP", "aucun forum dans ce cours", "moodle_forums")

    # 16 — private messages -------------------------------------------------
    result, data, _ = call("moodle_messages", {"limit": 5})
    conversations = (data or {}).get("conversations") or []
    row(
        "« J'ai des messages non lus ? »",
        "PASS" if not result["is_error"] else "FAIL",
        f"{len(conversations)} conversation(s)", 
        "moodle_messages",
        len(result["text"]),
    )

    # 17 — a date that lives in a description ------------------------------
    months = "janvier|février|mars|avril|mai|juin|juillet|août|septembre|octobre|novembre|décembre"
    date_re = re.compile(r"\d{1,2}(?:er)?\s+(?:" + months + ")", re.I)
    dated = next(
        (
            (m.get("id"), m.get("name"), date_re.search(m.get("description") or "").group(0))
            for m in modules
            if date_re.search(m.get("description") or "")
        ),
        None,
    )
    if dated:
        module_id, name, date = dated
        result, _, _ = call(
            "moodle_course_content",
            {"courseid": target["id"], "detail": "full", "moduleid": module_id},
        )
        row(
            "« Quand est mon examen ? »",
            "PASS" if date in result["text"] else "FAIL",
            f"activité « {name} »: date « {date} » {'transmise' if date in result['text'] else 'perdue'}",
            "moodle_course_content detail=full",
            len(result["text"]),
        )
    else:
        row("« Quand est mon examen ? »", "SKIP", "aucune date dans les descriptions du cours", "moodle_course_content")

    # 18 — timetable --------------------------------------------------------
    result, data, _ = call("moodle_calendar", {})
    events = (data or {}).get("events") or []
    row(
        "« Mon horaire de la semaine »",
        "PASS" if not result["is_error"] else "FAIL",
        f"{len(events)} événement(s) à venir",
        "moodle_calendar",
        len(result["text"]),
    )

    # 19 — completion -------------------------------------------------------
    result, data, _ = call("moodle_completion", {"courseid": target["id"]})
    activities = (data or {}).get("activities") or []
    tracked = None
    try:
        tracked = len(
            school.ws("core_completion_get_activities_completion_status", courseid=target["id"]).get(
                "statuses"
            )
            or []
        )
    except Exception:
        pass
    if not result["is_error"] and not activities and tracked == 0:
        verdict, evidence = "SKIP", "achèvement non activé dans ce cours (école: 0 aussi)"
    elif result["is_error"]:
        verdict, evidence = "FAIL", "erreur de l'outil"
    else:
        verdict, evidence = ("PASS", f"{len(activities)} activité(s) suivie(s)") if activities else ("PARTIAL", "liste vide sans explication")
    row("« Où j'en suis dans ce cours ? »", verdict, evidence, "moodle_completion", len(result["text"]))

    # 20 — a service this account does not hold ----------------------------
    result, _ = mcp.one("omnivox_deadlines", {"days": 14})
    honest = result["is_error"] and len(result["text"]) < 400
    row(
        "« Et sur Omnivox, mes échéances ? »",
        "PASS" if honest else ("SKIP" if not result["is_error"] else "PARTIAL"),
        "refus explicite" if honest else ("service connecté" if not result["is_error"] else "erreur confuse"),
        "omnivox_deadlines",
        len(result["text"]),
    )
    return rows


def render_intents(rows: list[dict[str, Any]]) -> str:
    if not rows:
        return "no intent rows"
    width = max(len(r["question"]) for r in rows)
    lines = [f"{'#':>2}  {'question':{width}}  verdict  tool"]
    for r in rows:
        lines.append(f"{r['n']:>2}  {r['question']:{width}}  {r['verdict']:7}  {r['tool']}")
        lines.append(f"{'':>2}  {'':{width}}  -> {r['evidence']}")
    passed = sum(1 for r in rows if r["verdict"] == "PASS")
    partial = sum(1 for r in rows if r["verdict"] == "PARTIAL")
    failed = sum(1 for r in rows if r["verdict"] == "FAIL")
    skipped = sum(1 for r in rows if r["verdict"] == "SKIP")
    graded = passed + partial + failed
    score = (100 * passed / graded) if graded else 0
    lines.append("")
    lines.append(
        f"Score: {passed} PASS · {partial} PARTIAL · {failed} FAIL · {skipped} SKIP"
        f" — {score:.0f}% des questions évaluables ({passed}/{graded})"
    )
    return "\n".join(lines)


def compare(report: dict[str, Any], baseline: dict[str, Any]) -> list[dict[str, Any]]:
    regressions: list[dict[str, Any]] = []
    before = {c["tool"]: c for c in baseline.get("calls") or []}
    for call in report["calls"]:
        old = before.get(call["tool"])
        if not old:
            continue
        if call["bytes"] > old["bytes"] * 1.25 and call["bytes"] - old["bytes"] > 2000:
            regressions.append(
                {
                    "severity": "high",
                    "message": (
                        f"{call['tool']} payload grew {old['bytes']} -> {call['bytes']} bytes "
                        "against the baseline."
                    ),
                }
            )
    old_bytes = baseline.get("tools_list_bytes")
    if old_bytes and report["tools_list_bytes"] > old_bytes * 1.15:
        regressions.append(
            {
                "severity": "medium",
                "message": (
                    f"tools/list grew {old_bytes} -> {report['tools_list_bytes']} bytes "
                    f"({baseline.get('tools')} -> {report['tools']} tools)."
                ),
            }
        )
    return regressions


def render(report: dict[str, Any]) -> str:
    lines = [
        f"student-mcp {report.get('version') or '?'} — {report['tools']} tools, "
        f"tools/list {report['tools_list_bytes']} bytes"
    ]
    target = report.get("target_course")
    if target:
        lines.append(
            f"audited course: {target['name']} — {target['school_sections']} sections, "
            f"{target['school_modules']} activities, {target['school_files']} files"
        )
    page = report.get("overview_page")
    if page:
        lines.append(
            f"overview page: {page['sections_shown']}/{page['total_sections']} sections, "
            f"nextPage {'required' if page['needs_next_page'] else 'not needed'}"
        )
    lines.append("")
    lines.append(f"{'tool':26} {'bytes':>7} {'~tok':>6} {'ctrl':>5} {'paged':>5} {'s':>5}")
    for call in report["calls"]:
        share = "-" if call.get("control_share") is None else f"{call['control_share']:.2f}"
        lines.append(
            f"{call['tool']:26} {call['bytes']:7d} {call['tokens']:6d} {share:>5} "
            f"{'yes' if call.get('paged') else 'no':>5} {call['seconds']:5.1f}"
        )
    lines.append("")
    findings = report.get("findings") or []
    if not findings:
        lines.append("no findings")
    for finding in sorted(findings, key=lambda f: SEVERITY_ORDER.get(f["severity"], 9)):
        lines.append(f"[{finding['severity']:7}] {finding['message']}")
    return "\n".join(lines)


def main() -> int:
    parser = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
    parser.add_argument("--bin", default="target/release/student-mcp", help="stdio MCP binary")
    parser.add_argument("--config", default=None, help="default: the XDG config, else the legacy file")
    parser.add_argument("--baseline", help="earlier JSON report to compare against")
    parser.add_argument("--write-baseline", action="store_true", help="save this run as the baseline")
    parser.add_argument("--json", help="report path (default ~/.student-mcp.eval/last.json)")
    parser.add_argument("--offline", action="store_true", help="skip school API ground truth")
    parser.add_argument(
        "--intents",
        action="store_true",
        help="ask real student questions and grade the answer against the school API",
    )
    args = parser.parse_args()

    binary = Path(args.bin)
    if not binary.exists() or not os.access(binary, os.X_OK):
        print(f"missing binary: {binary}", file=sys.stderr)
        return 2
    config_path = Path(args.config) if args.config else default_config()
    account = None if args.offline else load_account(config_path)
    if not account and not args.offline:
        print("no connected Moodle account; continuing without ground truth", file=sys.stderr)

    if args.intents:
        rows = intents(binary, account)
        print(render_intents(rows))
        destination = Path(args.json) if args.json else Path.home() / ".student-mcp.eval" / "intents.json"
        destination.parent.mkdir(parents=True, exist_ok=True)
        destination.write_text(json.dumps(rows, indent=2, ensure_ascii=False))
        print(f"\nreport: {destination}")
        return 1 if any(r["verdict"] in ("FAIL",) for r in rows) else 0

    report = audit(binary, account)
    if args.baseline:
        try:
            report["findings"].extend(compare(report, json.loads(Path(args.baseline).read_text())))
        except Exception as exc:
            print(f"baseline ignored: {exc}", file=sys.stderr)

    destination = Path(args.json) if args.json else Path.home() / ".student-mcp.eval" / "last.json"
    destination.parent.mkdir(parents=True, exist_ok=True)
    serialized = json.dumps(report, indent=2, ensure_ascii=False)
    destination.write_text(serialized)
    if args.write_baseline:
        (destination.parent / "baseline.json").write_text(serialized)

    print(render(report))
    print(f"\nreport: {destination}")
    blocking = [f for f in report["findings"] if f["severity"] in ("blocker", "high")]
    return 1 if blocking else 0


if __name__ == "__main__":
    sys.exit(main())
