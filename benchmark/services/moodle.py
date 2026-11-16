"""Moodle: the twenty student questions and the payload audit.

Everything here is Moodle-specific: ids, `core_*` ground truth and the wording
of a Moodle course. Brightspace, Omnivox and Canvas get their own module with the
same two entry points (`surface`, `questions`).
"""
from __future__ import annotations

import json
import re
import time
from pathlib import Path
from typing import Any

from ..harness import (
    MCP,
    School,
    course_files,
    measure,
    parse_mcp_json,
    payload_detail,
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


def questions(binary: Path, account: dict[str, Any] | None) -> list[dict[str, Any]]:
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

    # --- 21..50 : second batch -------------------------------------------
    tools, _ = mcp.batch([])
    tool_names = {tool.get("name") for tool in tools}
    schemas = {tool.get("name"): tool.get("inputSchema") or {} for tool in tools}
    annotations = {tool.get("name"): (tool.get("annotations") or {}) for tool in tools}

    def add(question: str, tool: str, verdict: str, evidence: str, payload: int = 0) -> None:
        row(question, verdict, evidence, tool, payload)

    # one assignments call feeds four questions (it fans out to Moodle statuses)
    result, data, _ = call("moodle_assignments", {})
    items = (data or {}).get("assignments") or []
    upcoming = sorted(
        (item for item in items if (item.get("dueDate") or 0) > now), key=lambda i: i["dueDate"]
    )
    if upcoming:
        soonest = upcoming[0]
        has_date = bool(soonest.get("dueDate"))
        add(
            "« Quel est le prochain devoir à rendre ? »",
            "moodle_assignments",
            "PASS" if has_date and soonest.get("title") else "PARTIAL",
            f"{soonest.get('title')} ({soonest.get('course')}), échéance {soonest.get('dueDate')}",
            len(result["text"]),
        )
    else:
        add("« Quel est le prochain devoir à rendre ? »", "moodle_assignments", "SKIP", "aucun devoir à venir")

    first = items[0] if items else None
    if first and first.get("id"):
        result, data, _ = call("moodle_assignment_status", {"assignid": first["id"]})
        status = (data or {}).get("submissionStatus")
        add(
            "« Est-ce que j'ai déjà rendu ce devoir ? »",
            "moodle_assignment_status",
            "PASS" if not result["is_error"] and status is not None else "FAIL",
            f"submissionStatus={status!r}",
            len(result["text"]),
        )
    else:
        add("« Est-ce que j'ai déjà rendu ce devoir ? »", "moodle_assignment_status", "SKIP", "aucun devoir")

    if first is not None:
        cutoff = "cutoffDate" in first
        add(
            "« Quand ferme la remise ? »",
            "moodle_assignments",
            "PASS" if cutoff else "PARTIAL",
            "cutoffDate présent" if cutoff else "aucun champ cutoffDate",
            len(result["text"]),
        )
    else:
        add("« Quand ferme la remise ? »", "moodle_assignments", "SKIP", "aucun devoir dans la réponse")

    result, data, _ = call("moodle_quizzes", {})
    quizzes = (data or {}).get("quizzes") or []
    opened = [q for q in quizzes if (q.get("timeopen") or 0) <= now and (q.get("timeclose") or 0) > now]
    add(
        "« Quels quiz sont ouverts en ce moment ? »",
        "moodle_quizzes",
        "PASS" if not result["is_error"] and quizzes else ("SKIP" if not result["is_error"] else "FAIL"),
        f"{len(opened)} ouvert(s) sur {len(quizzes)}",
        len(result["text"]),
    )
    if quizzes:
        result, data, _ = call("moodle_quiz_status", {"quizid": quizzes[0]["id"]})
        add(
            "« Combien de tentatives il me reste ? »",
            "moodle_quiz_status",
            "PASS" if not result["is_error"] else "FAIL",
            "tentatives lisibles" if not result["is_error"] else result["text"][:80],
            len(result["text"]),
        )
    else:
        add("« Combien de tentatives il me reste ? »", "moodle_quiz_status", "SKIP", "aucun quiz")

    # exact filename search, then the same file through three delivery modes
    if pdf:
        name = pdf.get("filename") or ""
        result, data, _ = call("moodle_search", {"query": name})
        found = [r.get("filename") for r in (data or {}).get("results") or []]
        add(
            "« Trouve-moi ce document précis »",
            "moodle_search",
            "PASS" if name in found else ("PARTIAL" if found else "FAIL"),
            f"rang {found.index(name) + 1 if name in found else 'absent'} sur {len(found)}",
            len(result["text"]),
        )
        result, data, _ = call("moodle_file", {"fileurl": pdf["fileurl"], "find": "page"})
        add(
            "« Cherche un mot dans ce PDF »",
            "moodle_file find",
            "PASS" if not result["is_error"] else "FAIL",
            "recherche texte exécutée" if not result["is_error"] else result["text"][:80],
            len(result["text"]),
        )
        result, data, _ = call("moodle_file", {"fileurl": pdf["fileurl"], "format": "file"})
        has_link = "links" in (data or {}) or "download" in result["text"].lower()
        add(
            "« Donne-moi le lien de téléchargement »",
            "moodle_file format=file",
            "PASS" if has_link and not result["is_error"] else "PARTIAL",
            "lien présent" if has_link else "aucun lien dans la réponse",
            len(result["text"]),
        )
    else:
        for question, tool in (
            ("« Trouve-moi ce document précis »", "moodle_search"),
            ("« Cherche un mot dans ce PDF »", "moodle_file find"),
            ("« Donne-moi le lien de téléchargement »", "moodle_file format=file"),
        ):
            add(question, tool, "SKIP", "aucun PDF dans le cours audité")

    office = None
    for module in modules:
        for content in module.get("contents") or []:
            if (content.get("mimetype") or "").startswith(("application/vnd.openxml", "application/msword")):
                office = content
                break
        if office:
            break
    if office:
        result, data, _ = call("moodle_file", {"fileurl": office["fileurl"], "format": "text"})
        add(
            "« Lis-moi ce document Word ou PowerPoint »",
            "moodle_file office",
            "PASS" if not result["is_error"] and len(result["text"]) > 200 else "PARTIAL",
            f"{len(result['text'])} octets extraits",
            len(result["text"]),
        )
    else:
        add("« Lis-moi ce document Word ou PowerPoint »", "moodle_file office", "SKIP", "aucun document Office")

    result, data, _ = call("moodle_resources", {"courseid": target["id"]})
    resources = (data or {}).get("resources") or []
    with_section = [r for r in resources if r.get("sectionid")]
    add(
        "« Combien de fichiers dans ce cours ? »",
        "moodle_resources",
        "PASS" if not result["is_error"] and resources else ("SKIP" if not result["is_error"] else "FAIL"),
        f"{len(resources)} ressource(s), {len(with_section)} avec section",
        len(result["text"]),
    )

    result, data, _ = call("moodle_course_updates", {"courseid": target["id"], "since": now - 7 * 86400})
    add(
        "« Qu'est-ce qui a changé cette semaine dans le cours ? »",
        "moodle_course_updates",
        "PASS" if not result["is_error"] else "FAIL",
        f"{len((data or {}).get('updates') or [])} changement(s)",
        len(result["text"]),
    )

    if by_code:
        word = (next(iter(by_code.values())).get("fullname") or "").split(" - ")[-1]
        word = word.split()[0] if word else ""
        result, data, _ = call("moodle_search", {"query": word})
        hits = (data or {}).get("results") or []
        add(
            "« Trouve le cours dont le nom contient ce mot »",
            "moodle_search",
            "PASS" if hits else "PARTIAL",
            f"« {word} » → {len(hits)} résultat(s)",
            len(result["text"]),
        )
    else:
        add(
            "« Trouve le cours dont le nom contient ce mot »",
            "moodle_search",
            "SKIP",
            "aucun sigle de cours lisible dans la liste",
        )

    result, data, _ = call("moodle_recent_grades", {"limit": 5})
    recent = (data or {}).get("grades") or []
    add(
        "« Quelles sont mes dernières notes ? »",
        "moodle_recent_grades",
        "PASS" if recent else ("SKIP" if not result["is_error"] else "FAIL"),
        f"{len(recent)} note(s) datée(s)",
        len(result["text"]),
    )

    if graded:
        course_id = graded[0]["courseid"]
        result, data, _ = call("moodle_grades", {"courseid": course_id})
        items = (data or {}).get("grades") or []
        named = [item for item in items if item.get("item") or item.get("name")]
        add(
            "« Quelle note j'ai eue à cet élément ? »",
            "moodle_grades",
            "PASS" if named else ("PARTIAL" if items else "SKIP"),
            f"{len(named)} élément(s) nommé(s)",
            len(result["text"]),
        )
        result, data, _ = call("moodle_grades", {"courseid": course_id})
        has_total = any(k in result["text"] for k in ("total", "weight", "weightsAvailable"))
        add(
            "« Quelle est ma moyenne pondérée ? »",
            "moodle_grades",
            "PASS" if has_total else "PARTIAL",
            "poids ou total exposé" if has_total else "aucune pondération ni explication",
            len(result["text"]),
        )
    else:
        add("« Quelle note j'ai eue à cet élément ? »", "moodle_grades", "SKIP", "aucune note publiée")
        add("« Quelle est ma moyenne pondérée ? »", "moodle_grades", "SKIP", "aucune note publiée")

    result, data, _ = call("moodle_notifications", {"limit": 5})
    add(
        "« Combien de notifications non lues ? »",
        "moodle_notifications",
        "PASS" if not result["is_error"] and "count" in (data or {}) else "FAIL",
        f"count={(data or {}).get('count')}",
        len(result["text"]),
    )

    result, data, _ = call("moodle_announcements", {"limit": 1})
    text_len = sum(len(str(a)) for a in (data or {}).get("announcements") or [])
    add(
        "« Lis la dernière annonce en entier »",
        "moodle_announcements",
        "PASS" if text_len > 100 else ("PARTIAL" if text_len else "SKIP"),
        f"{text_len} caractères de texte",
        len(result["text"]),
    )

    result, data, _ = call("moodle_messages", {"limit": 5})
    conversations = (data or {}).get("conversations") or []
    add(
        "« Y a-t-il un message du prof ? »",
        "moodle_messages",
        "PASS" if not result["is_error"] else "FAIL",
        f"{len(conversations)} conversation(s)",
        len(result["text"]),
    )

    # robustness: the tool must answer, not crash or invent
    result, data, _ = call("moodle_course_content", {"courseid": 999_999_999})
    add(
        "« Ouvre le cours 999999999 »",
        "moodle_course_content",
        "PASS" if result["is_error"] and len(result["text"]) < 400 else "FAIL",
        "erreur claire" if result["is_error"] else "aucune erreur pour un cours inexistant",
        len(result["text"]),
    )

    result, data, _ = call("moodle_search", {"query": "ZZZ9999 inexistant"})
    empty = not ((data or {}).get("results") or [])
    add(
        "« Cherche le cours ZZZ9999 »",
        "moodle_search",
        "PASS" if not result["is_error"] and empty else "PARTIAL",
        "aucun résultat, pas d'invention" if empty else "réponse inattendue",
        len(result["text"]),
    )

    # privacy: no tool may accept another user's id
    leaks = [
        name
        for name, schema in schemas.items()
        if name.startswith(("moodle_", "ws_"))
        and any(param in (schema.get("properties") or {}) for param in ("userid", "user_id", "studentid"))
    ]
    add(
        "« Regarde les notes de mon ami »",
        "surface MCP",
        "PASS" if not leaks else "FAIL",
        "aucun outil n'accepte un identifiant d'élève" if not leaks else f"paramètre utilisateur exposé: {leaks}",
    )

    write_tools = [
        name
        for name, ann in annotations.items()
        if name.startswith(("moodle_", "ws_")) and ann.get("readOnlyHint") is False
    ]
    add(
        "« Dépose mon devoir à ma place »",
        "annotations",
        "PASS" if write_tools else "PARTIAL",
        f"{len(write_tools)} outil(s) marqué(s) comme modifiant les données",
    )

    add(
        "« C'est quoi mon compte connecté ? »",
        "student_status",
        "PASS" if "student_status" in tool_names else "FAIL",
        f"{len(tool_names)} outils, student_status {'présent' if 'student_status' in tool_names else 'absent'}",
    )

    # --- 44..50 : pagination, calendrier, cas limites ---------------------
    result, data, _ = call("moodle_course_content", {"courseid": target["id"]})
    total = ((data or {}).get("coverage") or {}).get("totalSections")
    try:
        truth_sections = len(school.ws("core_course_get_contents", courseid=target["id"]))
    except Exception:
        truth_sections = None
    add(
        "« Combien de sections a ce cours ? »",
        "moodle_course_content",
        "PASS" if total and truth_sections == total else ("PARTIAL" if total else "FAIL"),
        f"{total} annoncée(s), {truth_sections} à l'école",
        len(result["text"]),
    )

    result, data, _ = call("moodle_my_courses", {"include_hidden": True})
    all_courses = (data or {}).get("courses") or []
    try:
        truth_courses = len(school.ws("core_enrol_get_users_courses", userid=school.ws("core_webservice_get_site_info")["userid"]))
    except Exception:
        truth_courses = None
    add(
        "« Montre aussi mes cours terminés »",
        "moodle_my_courses include_hidden",
        "PASS" if truth_courses and len(all_courses) >= truth_courses else "PARTIAL",
        f"{len(all_courses)} cours avec les terminés, {truth_courses} à l'école",
        len(result["text"]),
    )

    result, data, _ = call("moodle_calendar", {"days": 30})
    events = (data or {}).get("events") or []
    add(
        "« Qu'est-ce qui s'en vient dans les 30 jours ? »",
        "moodle_calendar",
        "PASS" if not result["is_error"] and events else ("SKIP" if not result["is_error"] else "FAIL"),
        f"{len(events)} événement(s)",
        len(result["text"]),
    )

    result, data, _ = call("moodle_deadlines", {"days": 1})
    add(
        "« Qu'est-ce qui est dû aujourd'hui ? »",
        "moodle_deadlines days=1",
        "PASS" if not result["is_error"] else "FAIL",
        f"{len((data or {}).get('deadlines') or [])} élément(s) dans la journée",
        len(result["text"]),
    )

    result, data, _ = call("moodle_search", {"query": "ÉLECTRICITÉ"})
    hits = (data or {}).get("results") or []
    add(
        "« Cherche ÉLECTRICITÉ en majuscules »",
        "moodle_search",
        "PASS" if hits else ("PARTIAL" if not result["is_error"] else "FAIL"),
        f"{len(hits)} résultat(s) avec accents et casse",
        len(result["text"]),
    )

    image = None
    for module in modules:
        for content in module.get("contents") or []:
            if (content.get("mimetype") or "").startswith("image/"):
                image = content
                break
        if image:
            break
    if image:
        result, data, _ = call("moodle_file", {"fileurl": image["fileurl"], "format": "text"})
        honest = result["is_error"] and len(result["text"]) < 400
        add(
            "« Lis-moi cette image comme du texte »",
            "moodle_file image",
            "PASS" if honest or not result["is_error"] else "FAIL",
            "limite annoncée" if honest else f"{len(result['text'])} octets",
            len(result["text"]),
        )
    else:
        add("« Lis-moi cette image comme du texte »", "moodle_file image", "SKIP", "aucune image dans le cours")

    try:
        first_section = (school.ws("core_course_get_contents", courseid=target["id"]) or [{}])[0].get("id")
    except Exception:
        first_section = None
    if first_section:
        result, data, _ = call(
            "moodle_course_content",
            {"courseid": target["id"], "detail": "full", "sectionid": first_section},
        )
        add(
            "« Donne-moi le texte complet de la première section »",
            "moodle_course_content detail=full sectionid",
            "PASS" if not result["is_error"] and len(result["text"]) > 200 else "PARTIAL",
            f"{len(result['text'])} octets",
            len(result["text"]),
        )
    else:
        add("« Donne-moi le texte complet de la première section »", "moodle_course_content", "SKIP", "section illisible")

    return rows


