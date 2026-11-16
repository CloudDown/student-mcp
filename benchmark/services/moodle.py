"""Moodle: payload audit and student questions derived from the live account.

Question text is filled at runtime from the connected Moodle (course codes,
file names, assignments, grade items). Brightspace, Omnivox and Canvas get
their own module with the same two entry points (`surface`, `questions`).
"""
from __future__ import annotations

import json
import re
import time
import unicodedata
from pathlib import Path
from typing import Any

from ..harness import (
    MCP,
    School,
    course_files,
    measure,
    parse_mcp_json,
    payload_detail,
    pick_course,
)

NAME = "moodle"
SERVICE = "moodle"


def surface(binary: Path, account: dict[str, Any] | None) -> dict[str, Any]:
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

    # Tool-surface lint: the text a model pays for before it can choose.
    seen: set[str] = set()
    undescribed: list[str] = []
    for tool in tools:
        name = tool.get("name") or "?"
        annotations = tool.get("annotations") or {}
        if "readOnlyHint" not in annotations:
            report["findings"].append(
                {"severity": "medium", "message": f"{name}: no readOnlyHint annotation."}
            )
        if annotations.get("readOnlyHint") is False and "destructiveHint" not in annotations:
            report["findings"].append(
                {"severity": "medium", "message": f"{name}: write tool without destructiveHint."}
            )
        if name in seen:
            report["findings"].append(
                {"severity": "high", "message": f"duplicate tool name {name} in tools/list."}
            )
        seen.add(name)
        description = tool.get("description") or ""
        if len(description) < 60:
            report["findings"].append(
                {
                    "severity": "medium",
                    "message": f"{name}: description is {len(description)} chars, too short to guide a choice.",
                }
            )
        for param, spec in ((tool.get("inputSchema") or {}).get("properties") or {}).items():
            if not (spec or {}).get("description"):
                undescribed.append(f"{name}.{param}")
    if undescribed:
        report["findings"].append(
            {
                "severity": "medium",
                "message": f"{len(undescribed)} parameter(s) without description: "
                + ", ".join(undescribed[:8])
                + ("..." if len(undescribed) > 8 else ""),
            }
        )
    if report["tools_list_bytes"] > 25_000:
        report["findings"].append(
            {
                "severity": "medium",
                "message": f"tools/list is {report['tools_list_bytes']} bytes; every call pays for it.",
            }
        )

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

def _code(course: dict[str, Any]) -> str:
    return (course.get("shortname") or course.get("fullname") or "").split(" ")[0]


def _plain(text: str) -> str:
    text = re.sub(r"<[^>]+>", " ", text or "")
    return re.sub(r"\s+", " ", text).strip()


def _fold(text: str) -> str:
    text = unicodedata.normalize("NFKD", text or "")
    text = "".join(ch for ch in text if not unicodedata.combining(ch))
    return re.sub(r"[^a-z0-9]+", " ", text.lower()).strip()


def _has(haystack: str, *needles: str) -> bool:
    folded = _fold(haystack)
    return all(_fold(n) in folded for n in needles if n)


def _grade_str(item: dict[str, Any]) -> str:
    return str(item.get("gradeformatted") or item.get("grade") or "").strip()


def _weight_val(item: dict[str, Any]) -> float | None:
    raw = item.get("weightraw")
    if isinstance(raw, (int, float)):
        return float(raw)
    text = str(item.get("weightformatted") or "").replace("\xa0", " ").replace(",", ".")
    match = re.search(r"(\d+(?:\.\d+)?)", text)
    return float(match.group(1)) if match else None


def _pick_file(
    files: list[dict[str, Any]],
    pattern: re.Pattern[str],
    *,
    in_module: bool = True,
) -> dict[str, Any] | None:
    hits = [
        f
        for f in files
        if pattern.search(f.get("filename") or "")
        or (in_module and pattern.search(f.get("module") or ""))
    ]
    hits.sort(key=lambda f: f.get("mtime") or 0, reverse=True)
    return hits[0] if hits else None


def _pick_graded(items: list[dict[str, Any]]) -> dict[str, Any] | None:
    for item in items:
        grade = _grade_str(item)
        if item.get("itemname") and grade not in ("", "-", "None"):
            return item
    return None


def _pick_weight(items: list[dict[str, Any]]) -> dict[str, Any] | None:
    best: dict[str, Any] | None = None
    best_w = -1.0
    for item in items:
        if not item.get("itemname"):
            continue
        weight = _weight_val(item)
        if weight is not None and weight > best_w:
            best, best_w = item, weight
    return best


def _course_total(items: list[dict[str, Any]]) -> dict[str, Any] | None:
    for item in items:
        if item.get("itemtype") == "course" and _grade_str(item) not in ("", "-", "None"):
            return item
    return None


PLAN_RE = re.compile(r"plan[\s._-]*de[\s._-]*cours|syllabus|entente", re.I)
TD_RE = re.compile(r"(?:^|[\s._-])td[\s._-]*\d+", re.I)
CORRIGE_RE = re.compile(r"corrig|correction|solutionnaire|(?:^|[\s._-])sol(?:ution)?[\s._-]", re.I)
FORM_RE = re.compile(r"formulaire|feuille[s]?[\s._-]*de[\s._-]*formule", re.I)
GUIDE_RE = re.compile(r"guide.{0,24}cp\s*2|objectifs.{0,12}cp\s*2|r[eé]vision[\s._-]*cp\s*2", re.I)
SKIP_FILE_RE = re.compile(r"^index\.html$", re.I)


def inspect_account(school: School, now: int) -> dict[str, Any]:
    """Live snapshot of the connected Moodle. Nothing here is hardcoded."""
    info = school.ws("core_webservice_get_site_info")
    userid = info["userid"]
    enrolled = school.ws("core_enrol_get_users_courses", userid=userid) or []
    current = [
        course
        for course in enrolled
        if (course.get("enddate") or 0) >= now - 14 * 86400
        and (course.get("startdate") or 0) <= now + 86400
    ] or enrolled

    try:
        raw_assign = school.ws("mod_assign_get_assignments") or {}
    except Exception:
        raw_assign = {}
    assign_map = {
        course.get("id"): course.get("assignments") or []
        for course in raw_assign.get("courses") or []
    }

    try:
        raw_quiz = school.ws("mod_quiz_get_quizzes_by_courses") or {}
    except Exception:
        raw_quiz = {}
    quiz_map: dict[int, list[dict[str, Any]]] = {}
    for quiz in raw_quiz.get("quizzes") or []:
        quiz_map.setdefault(quiz.get("course"), []).append(quiz)

    details: list[dict[str, Any]] = []
    for course in current:
        courseid = course["id"]
        try:
            contents = school.ws("core_course_get_contents", courseid=courseid) or []
        except Exception:
            contents = []
        files: list[dict[str, Any]] = []
        forums: list[dict[str, Any]] = []
        modules: list[dict[str, Any]] = []
        for section in contents:
            section_name = _plain(section.get("name") or "")
            for module in section.get("modules") or []:
                modules.append(module)
                if module.get("modname") == "forum":
                    forums.append(
                        {
                            "id": module.get("instance") or module.get("id"),
                            "name": module.get("name") or "",
                            "courseid": courseid,
                        }
                    )
                for content in module.get("contents") or []:
                    if content.get("type") != "file":
                        continue
                    name = content.get("filename") or ""
                    if SKIP_FILE_RE.match(name):
                        continue
                    files.append(
                        {
                            "filename": name,
                            "fileurl": content.get("fileurl"),
                            "mtime": content.get("timemodified") or 0,
                            "mime": content.get("mimetype") or "",
                            "module": module.get("name") or "",
                            "moduleid": module.get("id"),
                            "section": section_name,
                        }
                    )
        try:
            report = school.ws(
                "gradereport_user_get_grade_items", courseid=courseid, userid=userid
            )
            items = ((report.get("usergrades") or [{}])[0].get("gradeitems") or [])
        except Exception:
            items = []
        details.append(
            {
                "id": courseid,
                "code": _code(course),
                "short": course.get("shortname") or "",
                "files": files,
                "modules": modules,
                "forums": forums,
                "n_sections": len(contents),
                "assignments": assign_map.get(courseid, []),
                "quizzes": quiz_map.get(courseid, []),
                "grade_items": items,
            }
        )

    academic = [
        row for row in details if len(row["files"]) >= 5 or len(row["modules"]) >= 10
    ]
    heaviest = max(academic or details, key=lambda row: len(row["files"]), default=None)
    upcoming: list[dict[str, Any]] = []
    overdue: list[dict[str, Any]] = []
    for row in details:
        for assignment in row["assignments"]:
            due = assignment.get("duedate") or 0
            packed = {**assignment, "code": row["code"], "courseid": row["id"]}
            if due > now:
                upcoming.append(packed)
            elif due:
                overdue.append(packed)
    upcoming.sort(key=lambda item: item.get("duedate") or 0)
    return {
        "enrolled_count": len(enrolled),
        "details": details,
        "academic": academic,
        "heaviest": heaviest,
        "upcoming": upcoming,
        "overdue": overdue,
    }


def questions(binary: Path, account: dict[str, Any] | None) -> list[dict[str, Any]]:
    """Student questions derived from the connected Moodle, graded against it.

    Question text is filled at runtime from live names (course, file, assignment,
    grade item). Nothing in this function hardcodes an account.
    """
    mcp = MCP(binary)
    school = School(account) if account else None
    rows: list[dict[str, Any]] = []
    last = {"seconds": 0.0}

    def row(question: str, verdict: str, evidence: str, tool: str = "-", payload: int = 0) -> None:
        rows.append(
            {
                "n": len(rows) + 1,
                "question": question,
                "verdict": verdict,
                "evidence": evidence,
                "tool": tool,
                "bytes": payload,
                "seconds": round(last.get("seconds", 0.0), 3),
            }
        )

    def call(name: str, args: dict[str, Any]) -> tuple[dict[str, Any], Any]:
        result, seconds = mcp.one(name, args)
        last["seconds"] = seconds
        return result, parse_mcp_json(result)

    def ask(
        question: str,
        tool: str,
        args: dict[str, Any],
        grade,
    ) -> None:
        result, data = call(tool, args)
        verdict, evidence = grade(result, data)
        row(question, verdict, evidence, tool, len(result["text"]))

    if not school:
        row("« Quels cours j'ai cette session ? »", "SKIP", "vérité terrain indisponible", "moodle_my_courses")
        return rows

    now = int(time.time())
    facts = inspect_account(school, now)
    academic = facts["academic"]
    heaviest = facts["heaviest"]
    codes = [row["code"] for row in academic if row["code"]]

    # 1. current session — names, not a headcount vs past courses
    result, data = call("moodle_my_courses", {})
    text = result["text"]
    missing = [code for code in codes if not _has(text, code)]
    row(
        "« Quels cours j'ai cette session ? »",
        "FAIL" if missing else "PASS",
        f"{len((data or {}).get('courses') or [])} listés, session {codes}"
        + (f", manquants {missing}" if missing else ""),
        "moodle_my_courses",
        len(text),
    )

    # 2–3. the actual next hand-in, and anything already late
    soon = facts["upcoming"][0] if facts["upcoming"] else None
    if soon:
        name = _plain(soon.get("name") or "")
        due = time.strftime("%Y-%m-%d", time.localtime(soon["duedate"]))
        ask(
            f"« Quand est dû {name} de {soon['code']} ? »",
            "moodle_deadlines",
            {"days": 21},
            lambda result, data, name=name, due=due, due_ts=str(soon["duedate"]): (
                (
                    "PASS"
                    if _has(result["text"], name) and (due in result["text"] or due_ts in result["text"])
                    else ("PARTIAL" if _has(result["text"], name) else "FAIL")
                ),
                f"école {due} | {'trouvé' if _has(result['text'], name) else 'absent de deadlines'}",
            ),
        )
        ask(
            f"« Est-ce que j'ai déjà rendu {name} ? »",
            "moodle_assignment_status",
            {"assignid": soon["id"]},
            lambda result, data: (
                "PASS" if not result["is_error"] and data is not None else "FAIL",
                f"submissionStatus={(data or {}).get('submissionStatus')!r}"
                if isinstance(data, dict)
                else result["text"][:80],
            ),
        )
        cutoff = soon.get("cutoffdate") or 0
        ask(
            f"« Quand ferme la remise de {name} ? »",
            "moodle_assignments",
            {},
            lambda result, data, name=name, cutoff=cutoff: (
                (
                    "PASS"
                    if _has(result["text"], name)
                    and (not cutoff or str(cutoff) in result["text"] or "cutoff" in result["text"].lower())
                    else ("PARTIAL" if _has(result["text"], name) else "FAIL")
                ),
                "cutoff présent" if "cutoff" in result["text"].lower() else "pas de cutoff dans la réponse",
            ),
        )
    else:
        row("« Quel est le prochain devoir à rendre ? »", "SKIP", "aucune échéance future dans le compte", "moodle_deadlines")

    if facts["overdue"]:
        names = [_plain(item.get("name") or "") for item in facts["overdue"][:3]]
        ask(
            "« Qu'est-ce qui est déjà en retard ? »",
            "moodle_deadlines",
            {"days": 21, "include_expired": True},
            lambda result, data, names=names: (
                "PASS" if any(_has(result["text"], name) for name in names if name) else "FAIL",
                f"école {names} | {'trouvé' if any(_has(result['text'], n) for n in names if n) else 'absent'}",
            ),
        )

    # per current academic course: plan, last TD, a named corrigé, a fresh file, a grade, a weight
    used_filenames: set[str] = set()
    for course in academic[:4]:
        code = course["code"]
        files = course["files"]
        plan = _pick_file(files, PLAN_RE)
        td = _pick_file(files, TD_RE, in_module=False)
        corrigé = _pick_file(files, CORRIGE_RE)
        recent = next(
            (
                f
                for f in sorted(files, key=lambda item: item["mtime"], reverse=True)
                if f["mtime"] >= now - 21 * 86400 and f["filename"] not in used_filenames
            ),
            None,
        )
        graded = _pick_graded(course["grade_items"])
        weighted = _pick_weight(course["grade_items"])
        total = _course_total(course["grade_items"])

        if plan:
            used_filenames.add(plan["filename"])
            ask(
                f"« Trouve le plan de cours de {code} »",
                "moodle_search",
                {"query": f"{code} plan de cours", "courseid": course["id"]},
                lambda result, data, name=plan["filename"]: (
                    "PASS" if any(_has(n, name) or _has(name, n) for n in _names(data)) or _has(result["text"], name) else "FAIL",
                    f"école {name} | résultats {_names(data)[:3]}",
                ),
            )
        if td:
            used_filenames.add(td["filename"])
            ask(
                f"« C'est quoi le dernier TD de {code} ? »",
                "moodle_search",
                {"query": f"{code} TD", "courseid": course["id"]},
                lambda result, data, name=td["filename"], stamped=time.strftime("%Y-%m-%d", time.localtime(td["mtime"])): (
                    "PASS" if _has(result["text"], name) or any(_has(n, name) or _has(name, n) for n in _names(data)) else "FAIL",
                    f"école {name} ({stamped})",
                ),
            )
        if corrigé and corrigé["filename"] not in used_filenames:
            used_filenames.add(corrigé["filename"])
            ask(
                f"« Trouve {corrigé['filename']} »",
                "moodle_search",
                {"query": corrigé["filename"], "courseid": course["id"]},
                lambda result, data, name=corrigé["filename"]: (
                    "PASS" if _has(result["text"], name) or any(_has(n, name) or _has(name, n) for n in _names(data)) else "FAIL",
                    f"{'trouvé' if _has(result['text'], name) else 'absent'} parmi {len(_names(data))} hit(s)",
                ),
            )
        if recent and recent["filename"] not in used_filenames:
            used_filenames.add(recent["filename"])
            ask(
                f"« Trouve {recent['filename']} dans {code} »",
                "moodle_search",
                {"query": recent["filename"], "courseid": course["id"]},
                lambda result, data, name=recent["filename"], stamped=time.strftime("%Y-%m-%d", time.localtime(recent["mtime"])): (
                    "PASS" if _has(result["text"], name) or any(_has(n, name) or _has(name, n) for n in _names(data)) else "FAIL",
                    f"déposé {stamped}",
                ),
            )
        if graded:
            item_name = graded["itemname"]
            expected = _grade_str(graded)
            ask(
                f"« J'ai combien à {item_name} de {code} ? »",
                "moodle_grades",
                {"courseid": course["id"]},
                lambda result, data, item_name=item_name, expected=expected: (
                    (
                        "PASS"
                        if _has(result["text"], item_name) and _has(result["text"], expected.split()[0])
                        else ("PARTIAL" if _has(result["text"], item_name) else "FAIL")
                    ),
                    f"école {expected} | {'item vu' if _has(result['text'], item_name) else 'item absent'}",
                ),
            )
        if weighted and (not graded or weighted.get("itemname") != graded.get("itemname")):
            item_name = weighted["itemname"]
            weight = _weight_val(weighted)
            ask(
                f"« Combien pèse {item_name} dans {code} ? »",
                "moodle_grades",
                {"courseid": course["id"]},
                lambda result, data, item_name=item_name, weight=weight: (
                    (
                        "PASS"
                        if _has(result["text"], item_name)
                        and (weight is None or _has(result["text"], str(int(weight)) if weight == int(weight) else str(weight).replace(".", ",")))
                        else ("PARTIAL" if _has(result["text"], item_name) else "FAIL")
                    ),
                    f"école {weighted.get('weightformatted') or weight} | {'vu' if _has(result['text'], item_name) else 'absent'}",
                ),
            )
        if total and _grade_str(total) not in ("", "-"):
            expected = _grade_str(total)
            ask(
                f"« Quelle est ma moyenne en {code} ? »",
                "moodle_grades",
                {"courseid": course["id"]},
                lambda result, data, expected=expected: (
                    "PASS" if _has(result["text"], expected.split()[0]) else "PARTIAL",
                    f"école {expected}",
                ),
            )

    # heaviest course: pagination, instructions, a real PDF
    if heaviest:
        code = heaviest["code"]
        ask(
            f"« Montre-moi le contenu de {code} »",
            "moodle_course_content",
            {"courseid": heaviest["id"]},
            lambda result, data, n=heaviest["n_sections"]: (
                (
                    "PASS"
                    if (data or {}).get("nextPage") or ((data or {}).get("coverage") or {}).get("totalSections") == n
                    or len((data or {}).get("sections") or []) == n
                    else "PARTIAL"
                ),
                f"{len((data or {}).get('sections') or [])} section(s) / {n} école, nextPage={bool((data or {}).get('nextPage'))}",
            ),
        )
        ask(
            f"« Combien de sections a {code} ? »",
            "moodle_course_content",
            {"courseid": heaviest["id"]},
            lambda result, data, n=heaviest["n_sections"]: (
                "PASS" if str(n) in result["text"] or ((data or {}).get("coverage") or {}).get("totalSections") == n else "FAIL",
                f"école {n}, annoncé {((data or {}).get('coverage') or {}).get('totalSections')}",
            ),
        )
        labelled = next(
            (m for m in heaviest["modules"] if (m.get("description") or "") and m.get("name")),
            heaviest["modules"][0] if heaviest["modules"] else None,
        )
        if labelled and labelled.get("id"):
            ask(
                f"« Donne-moi les consignes de {labelled.get('name')} dans {code} »",
                "moodle_course_content",
                {"courseid": heaviest["id"], "detail": "full", "moduleid": labelled["id"]},
                lambda result, data: (
                    "PASS" if not result["is_error"] and len(result["text"]) > 200 else "PARTIAL",
                    f"{len(result['text'])} octets, erreur={result['is_error']}",
                ),
            )
        pdf = next((f for f in heaviest["files"] if (f.get("mime") or "").startswith("application/pdf") and f.get("fileurl")), None)
        if pdf:
            ask(
                f"« Lis-moi {pdf['filename']} »",
                "moodle_file",
                {"fileurl": pdf["fileurl"], "format": "text"},
                lambda result, data: (
                    "PASS" if not result["is_error"] and sum(ch.isalpha() for ch in result["text"]) > 80 else "FAIL",
                    f"{len(result['text'])} octets extraits, erreur={result['is_error']}",
                ),
            )
            ask(
                f"« Montre-moi la première page de {pdf['filename']} »",
                "moodle_file",
                {"fileurl": pdf["fileurl"], "format": "images", "page_from": 1, "page_to": 1},
                lambda result, data: (
                    "PASS" if "image" in (result.get("types") or []) else "FAIL",
                    f"types={result.get('types')}",
                ),
            )
            ask(
                f"« Donne-moi le lien de téléchargement de {pdf['filename']} »",
                "moodle_file",
                {"fileurl": pdf["fileurl"], "format": "file"},
                lambda result, data: (
                    "PASS" if "download" in result["text"].lower() or "links" in result["text"] else "PARTIAL",
                    "lien présent" if "download" in result["text"].lower() else "pas de lien",
                ),
            )

        form = _pick_file(heaviest["files"], FORM_RE, in_module=False)
        if form:
            ask(
                f"« Trouve le formulaire d'examen de {code} »",
                "moodle_search",
                {"query": f"{code} formulaire", "courseid": heaviest["id"]},
                lambda result, data, name=form["filename"]: (
                    "PASS" if _has(result["text"], name) else "FAIL",
                    f"école {name}",
                ),
            )
        guide = _pick_file(
            heaviest["files"], re.compile(r"guide.{0,40}cp\s*2", re.I), in_module=False
        ) or _pick_file(heaviest["files"], GUIDE_RE, in_module=False)
        if guide:
            ask(
                f"« Trouve le guide d'étude du CP2 de {code} »",
                "moodle_search",
                {"query": f"{code} guide CP2", "courseid": heaviest["id"]},
                lambda result, data, name=guide["filename"]: (
                    "PASS" if _has(result["text"], name) else "FAIL",
                    f"école {name}",
                ),
            )
        office = next(
            (
                f
                for f in heaviest["files"]
                if (f.get("mime") or "").startswith(("application/vnd.openxml", "application/msword", "application/vnd.ms-"))
                and f.get("fileurl")
            ),
            None,
        )
        if office:
            ask(
                f"« Lis-moi {office['filename']} »",
                "moodle_file",
                {"fileurl": office["fileurl"], "format": "text"},
                lambda result, data: (
                    "PASS" if not result["is_error"] and len(result["text"]) > 200 else "PARTIAL",
                    f"{len(result['text'])} octets",
                ),
            )
        if heaviest["forums"]:
            forum = heaviest["forums"][0]
            ask(
                f"« Quelles discussions sur le forum {forum['name']} de {code} ? »",
                "moodle_forum_discussions",
                {"forumid": forum["id"], "limit": 5},
                lambda result, data: (
                    "PASS" if not result["is_error"] else "FAIL",
                    f"{len((data or {}).get('discussions') or [])} discussion(s)",
                ),
            )
        ask(
            f"« Qu'est-ce qui a changé cette semaine dans {code} ? »",
            "moodle_course_updates",
            {"courseid": heaviest["id"], "since": now - 7 * 86400},
            lambda result, data: (
                "PASS" if not result["is_error"] else "FAIL",
                f"{len((data or {}).get('updates') or [])} changement(s)",
            ),
        )

    quiz_course = next((row for row in academic if row["quizzes"]), None)
    if quiz_course:
        names = [q.get("name") or "" for q in quiz_course["quizzes"] if q.get("name")]
        ask(
            f"« Quels tests formatifs il y a dans {quiz_course['code']} ? »",
            "moodle_quizzes",
            {},
            lambda result, data, names=names: (
                "PASS" if sum(1 for n in names if _has(result["text"], n)) >= min(3, len(names)) else "PARTIAL",
                f"école {len(names)} quiz, aperçu {names[:2]}",
            ),
        )
        first = quiz_course["quizzes"][0]
        ask(
            f"« Combien de tentatives il me reste au {first.get('name')} ? »",
            "moodle_quiz_status",
            {"quizid": first["id"]},
            lambda result, data: (
                "PASS" if not result["is_error"] else "FAIL",
                "lisible" if not result["is_error"] else result["text"][:80],
            ),
        )

    ask(
        "« Quelles annonces le prof a postées ? »",
        "moodle_announcements",
        {"limit": 5},
        lambda result, data: (
            "PASS" if not result["is_error"] else "FAIL",
            f"{len((data or {}).get('announcements') or [])} annonce(s)",
        ),
    )
    ask(
        "« Qu'est-ce qui s'en vient dans les 30 jours ? »",
        "moodle_calendar",
        {"days": 30},
        lambda result, data, soon=soon: (
            (
                "PASS"
                if not result["is_error"]
                and (not soon or _has(result["text"], _plain(soon.get("name") or "")))
                else ("PARTIAL" if not result["is_error"] else "FAIL")
            ),
            f"{len((data or {}).get('events') or [])} événement(s)",
        ),
    )
    ask(
        "« Quelles sont mes dernières notes ? »",
        "moodle_recent_grades",
        {"limit": 5},
        lambda result, data: (
            "PASS" if (data or {}).get("grades") or not result["is_error"] else "FAIL",
            f"{len((data or {}).get('grades') or [])} note(s)",
        ),
    )
    ask(
        "« J'ai des messages non lus ? »",
        "moodle_messages",
        {"limit": 5},
        lambda result, data: (
            "PASS" if not result["is_error"] else "FAIL",
            f"{len((data or {}).get('conversations') or [])} conversation(s)",
        ),
    )

    result, _ = mcp.one("omnivox_deadlines", {"days": 14})
    last["seconds"] = 0.0
    honest = result["is_error"] and len(result["text"]) < 400
    row(
        "« Et sur Omnivox, mes échéances ? »",
        "PASS" if honest else ("SKIP" if not result["is_error"] else "PARTIAL"),
        "refus explicite" if honest else ("service connecté" if not result["is_error"] else "erreur confuse"),
        "omnivox_deadlines",
        len(result["text"]),
    )

    tools, _ = mcp.batch([])
    tool_names = {tool.get("name") for tool in tools}
    schemas = {tool.get("name"): tool.get("inputSchema") or {} for tool in tools}
    annotations = {tool.get("name"): (tool.get("annotations") or {}) for tool in tools}
    row(
        "« C'est quoi mon compte connecté ? »",
        "PASS" if "student_status" in tool_names else "FAIL",
        f"student_status {'présent' if 'student_status' in tool_names else 'absent'}",
        "student_status",
    )
    leaks = [
        name
        for name, schema in schemas.items()
        if name.startswith(("moodle_", "ws_"))
        and any(param in (schema.get("properties") or {}) for param in ("userid", "user_id", "studentid"))
    ]
    row(
        "« Regarde les notes de mon ami »",
        "PASS" if not leaks else "FAIL",
        "aucun outil n'accepte un identifiant d'élève" if not leaks else f"paramètre exposé: {leaks}",
        "surface MCP",
    )
    write_tools = [
        name
        for name, ann in annotations.items()
        if name.startswith(("moodle_", "ws_")) and ann.get("readOnlyHint") is False
    ]
    row(
        "« Dépose mon devoir à ma place »",
        "PASS" if write_tools else "PARTIAL",
        f"{len(write_tools)} outil(s) d'écriture",
        "annotations",
    )

    return rows


def _names(data: Any) -> list[str]:
    return [item.get("filename") or item.get("name") or "" for item in (data or {}).get("results") or []]
