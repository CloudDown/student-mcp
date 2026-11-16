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
    """Real student questions, checked against what the school actually holds.

    Each row answers: could a student get this right through the MCP alone, and
    what did it cost? Failure here means an assistant must guess or invent.
    """
    mcp = MCP(binary)
    school = School(account) if account else None
    rows: list[dict[str, Any]] = []

    def row(question: str, verdict: str, evidence: str, tool: str = "-", payload: int = 0) -> None:
        rows.append(
            {"question": question, "verdict": verdict, "evidence": evidence, "tool": tool, "bytes": payload}
        )

    # 1. Which courses am I enrolled in?
    result, seconds = mcp.one("moodle_my_courses", {})
    data = parse_mcp_json(result) or {}
    listed = data.get("courses") or []
    evidence = f"{len(listed)} cours listés"
    verdict = "PASS" if listed else "FAIL"
    if school:
        try:
            truth = school.ws("core_enrol_get_users_courses", userid=school.ws("core_webservice_get_site_info")["userid"])
            evidence += f", école {len(truth)}"
            if len(truth) != len(listed):
                verdict = "PARTIAL"
        except Exception as exc:
            evidence += f" (vérité indisponible: {type(exc).__name__})"
    row("« Quels cours j'ai cette session ? »", verdict, evidence, "moodle_my_courses", len(result["text"]))

    # 2. What is due in the next 7 days?
    result, seconds = mcp.one("moodle_deadlines", {"days": 7})
    data = parse_mcp_json(result) or {}
    names = [d.get("name") for d in data.get("deadlines") or [] if d.get("name")]
    evidence = f"{len(names)} échéance(s): " + "; ".join(names[:3])
    verdict = "PASS"
    if school:
        try:
            now = int(time.time())
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

    if not school:
        return rows

    courses = listed or []
    by_code: dict[str, dict[str, Any]] = {}
    for course in courses:
        code = (course.get("shortname") or "").split(" ")[0]
        if code:
            by_code[code] = course

    # 3. Current-session TD files: can one be seen without reading every row?
    for code in ("MTH0102", "PHS0102", "CHE0501"):
        course = by_code.get(code)
        if not course:
            continue
        try:
            tds = course_files(school, course["id"], r"\bTD\b|TD\d|_TD")
        except Exception:
            continue
        recent = [f for f in tds if f["timemodified"] >= int(time.time()) - 180 * 86400]
        if not recent:
            continue
        newest = max(recent, key=lambda f: f["timemodified"])
        result, _ = mcp.one("moodle_search", {"query": f"{code} TD"})
        data = parse_mcp_json(result) or {}
        found = [r.get("filename") for r in data.get("results") or []]
        ranks = [found.index(f["filename"]) + 1 for f in recent if f["filename"] in found]
        best = min(ranks) if ranks else 0
        verdict = "PASS" if best and best <= 3 else ("PARTIAL" if best and best <= 8 else "FAIL")
        row(
            f"« C'est quoi le dernier TD de {code} ? »",
            verdict,
            f"{len(recent)} TD de la session, {len(found)} résultats"
            f" | meilleur rang {best or 'absent'}"
            f" | le plus récent: {newest['filename']} ({time.strftime('%Y-%m-%d', time.localtime(newest['timemodified']))})",
            "moodle_search",
            len(result["text"]),
        )

    # 4. A date that only lives inside a long description: does it survive?
    course = by_code.get("PHS0102") or (courses[0] if courses else None)
    if course:
        months = "janvier|février|mars|avril|mai|juin|juillet|août|septembre|octobre|novembre|décembre"
        date_re = re.compile(r"\d{1,2}(?:er)?\s+(?:" + months + r")", re.I)
        overview, _ = mcp.one("moodle_course_content", {"courseid": course["id"]})
        data = parse_mcp_json(overview) or {}
        preview: dict[int, dict[str, Any]] = {}
        for section in data.get("sections") or []:
            for module in section.get("modules") or []:
                preview[module.get("id")] = module
        leaked = None
        try:
            for section in course_contents(school, course["id"]):
                for module in section.get("modules") or []:
                    description = module.get("description") or ""
                    match = date_re.search(description)
                    if not match:
                        continue
                    shown = preview.get(module.get("id"))
                    if shown is None:
                        continue
                    if not date_re.search(shown.get("description") or ""):
                        leaked = (module.get("name"), match.group(0), shown.get("descriptionTruncated"))
                        break
                if leaked:
                    break
        except Exception:
            leaked = None
        if leaked:
            name, date, truncated = leaked
            row(
                f"« Quand est mon examen de {course.get('shortname', '').split(' ')[0]} ? »",
                "FAIL",
                f"activité « {name} »: l'école donne « {date} », la vue par défaut la coupe"
                f" (descriptionTruncated={truncated}) — il faut detail=full",
                "moodle_course_content",
                len(overview["text"]),
            )
        else:
            row(
                f"« Quand est mon examen de {course.get('shortname', '').split(' ')[0]} ? »",
                "PASS",
                "aucune date présente dans une description n'est coupée par la vue par défaut",
                "moodle_course_content",
                len(overview["text"]),
            )

    # 5. A grade the student knows.
    result, _ = mcp.one("moodle_grades", {})
    data = parse_mcp_json(result) or {}
    grades = data.get("grades") or []
    try:
        raw = school.ws("gradereport_overview_get_course_grades").get("grades") or []
        truth = [
            g for g in raw if str(g.get("grade") or "").strip() not in ("", "-")
        ]
    except Exception:
        truth = []
    shown_names = " ".join(json.dumps(g) for g in grades)
    covered = [g for g in truth if str(g.get("rawgrade") or "")[:4] in shown_names.replace(".", ",")]
    verdict = "PASS" if truth and len(covered) == len(truth) else ("PARTIAL" if grades else "FAIL")
    row(
        "« Quelles notes j'ai eues ? »",
        verdict,
        f"MCP {len(grades)} cours | carnet {len(truth)} cours notés | non annoncé si filtré",
        "moodle_grades",
        len(result["text"]),
    )

    # 6. A specific document by name, in a specific course.
    course = by_code.get("CHE0501")
    if course:
        expected = course_files(school, course["id"], r"plan|syllabus|entente")
        result, _ = mcp.one("moodle_search", {"query": f"plan de cours {course['shortname'].split(' ')[0]}"})
        data = parse_mcp_json(result) or {}
        found = [r.get("filename") for r in data.get("results") or []]
        hit = next((f for f in expected if f["filename"] in found), None)
        verdict = "PASS" if hit else ("FAIL" if expected else "SKIP")
        row(
            "« Trouve le plan de cours de CHE0501 »",
            verdict,
            f"école: {len(expected)} candidat(s) ({', '.join(f['filename'] for f in expected[:2])})"
            f" | trouvé: {hit['filename'] if hit else 'aucun'}",
            "moodle_search",
            len(result["text"]),
        )

    # 7. Follow-up work: the correction of an exercise.
    course = by_code.get("MTH0102")
    if course:
        expected = course_files(school, course["id"], r"corrig|correction|solution")
        td3 = [f for f in expected if re.search(r"TD\s*3", f["filename"], re.I)]
        result, _ = mcp.one("moodle_search", {"query": "corrigé TD3 MTH0102"})
        data = parse_mcp_json(result) or {}
        found = [r.get("filename") for r in data.get("results") or []]
        hit = next((f for f in (td3 or expected) if f["filename"] in found), None)
        verdict = "PASS" if hit else ("FAIL" if (td3 or expected) else "SKIP")
        row(
            "« Trouve le corrigé du TD3 de MTH0102 »",
            verdict,
            f"école: {len(td3)} corrigé(s) TD3, {len(expected)} corrigés | trouvé: {hit['filename'] if hit else 'aucun'}",
            "moodle_search",
            len(result["text"]),
        )

    # 8. Anything new for me?
    result, _ = mcp.one("moodle_notifications", {})
    data = parse_mcp_json(result) or {}
    count = data.get("count") or len(data.get("notifications") or [])
    row(
        "« J'ai des nouvelles annonces ou notifications ? »",
        "PASS" if result["text"] and not result["is_error"] else "FAIL",
        f"count={count}",
        "moodle_notifications",
        len(result["text"]),
    )
    return rows


def render_intents(rows: list[dict[str, Any]]) -> str:
    if not rows:
        return "no intent rows"
    width = max(len(r["question"]) for r in rows)
    lines = [f"{'question':{width}}  verdict  tool"]
    for r in rows:
        lines.append(f"{r['question']:{width}}  {r['verdict']:7}  {r['tool']}")
        lines.append(f"{'':{width}}  -> {r['evidence']}")
    passed = sum(1 for r in rows if r["verdict"] == "PASS")
    lines.append("")
    lines.append(f"{passed}/{len(rows)} demandes étudiantes obtiennent une réponse correcte via le MCP")
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
    parser.add_argument("--config", default=str(Path.home() / ".student-mcp.json"))
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
    account = None if args.offline else load_account(Path(args.config))
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
