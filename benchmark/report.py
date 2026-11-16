"""Rendering and comparison for the audit JSON reports.

Shared by every service: a service only produces rows and findings.
"""
from __future__ import annotations

from typing import Any

from .harness import SEVERITY_ORDER


def render_intents(rows: list[dict[str, Any]]) -> str:
    if not rows:
        return "no intent rows"
    services = {r.get("service") for r in rows if r.get("service")}
    multi = len(services) > 1
    width = max(len(r["question"]) for r in rows)
    head = f"{'#':>2}  " + (f"{'service':9}  " if multi else "") + f"{'question':{width}}  verdict  tool"
    lines = [head]
    for r in rows:
        prefix = f"{r['n']:>2}  " + (f"{r['service']:9}  " if multi else "")
        blank = "    " + (" " * 9 + "  " if multi else "")
        lines.append(f"{prefix}{r['question']:{width}}  {r['verdict']:7}  {r['tool']}")
        lines.append(f"{blank}{'':{width}}  -> {r['evidence']}")
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


