#!/usr/bin/env python3
"""Live MCP quality audit for student-mcp, per school service.

Two passes over the same stdio server:

  surface   payload size, control-field share, pagination, latency, fidelity
            against the school API, and the tool-definition lint
  intents   real student questions, each graded against what the school holds

Usage:
  python3 tools/mcp-audit.py                      # every connected service
  python3 tools/mcp-audit.py --service moodle
  python3 tools/mcp-audit.py --intents            # student questions only
  python3 tools/mcp-audit.py --bin target/debug/student-mcp
  python3 tools/mcp-audit.py --baseline ~/.student-mcp.eval/baseline.json
  python3 tools/mcp-audit.py --write-baseline

Exit code 1 when a high/blocker finding or a FAIL question remains, so it can
gate a change. Standard library only; the school token is read from the config
and never printed. Reports default to ~/.student-mcp.eval/ (outside the repo).
"""
from __future__ import annotations

import argparse
import json
import os
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))

from benchmark import harness, report as render  # noqa: E402
from benchmark.services import AVAILABLE, PLANNED, load  # noqa: E402


def merge(reports: list[tuple[str, dict]]) -> dict:
    """One report from several services: the first one owns the tool surface."""
    if len(reports) == 1:
        return reports[0][1]
    merged = dict(reports[0][1])
    merged["calls"] = []
    merged["findings"] = []
    for service, part in reports:
        merged["calls"].extend(part.get("calls") or [])
        for finding in part.get("findings") or []:
            merged["findings"].append(
                {**finding, "message": f"[{service}] {finding['message']}"}
            )
    merged["calls"].sort(key=lambda call: call.get("bytes") or 0, reverse=True)
    return merged


def number(rows: list[tuple[str, list[dict]]]) -> list[dict]:
    numbered: list[dict] = []
    for service, service_rows in rows:
        for row in service_rows:
            numbered.append({**row, "n": len(numbered) + 1, "service": service})
    return numbered


def main() -> int:
    parser = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
    parser.add_argument("--bin", default="target/release/student-mcp", help="stdio MCP binary")
    parser.add_argument(
        "--service",
        action="append",
        choices=list(AVAILABLE + PLANNED),
        help="audit one service (repeatable); default: every connected one",
    )
    parser.add_argument("--config", default=None, help="default: the XDG config, else the legacy file")
    parser.add_argument("--baseline", help="earlier JSON report to compare against")
    parser.add_argument("--write-baseline", action="store_true", help="save this run as the baseline")
    parser.add_argument("--json", help="report path (default ~/.student-mcp.eval/last.json)")
    parser.add_argument("--offline", action="store_true", help="skip school API ground truth")
    parser.add_argument("--intents", action="store_true", help="grade student questions only")
    args = parser.parse_args()

    binary = Path(args.bin)
    if not binary.exists() or not os.access(binary, os.X_OK):
        print(f"missing binary: {binary}", file=sys.stderr)
        return 2

    config = Path(args.config) if args.config else harness.default_config()
    requested = args.service or list(AVAILABLE)
    connected: list[tuple[str, object, object]] = []
    for name in requested:
        module = load(name)
        account = None if args.offline else harness.load_account(config, name)
        if account is None and not args.offline and args.service is None:
            continue  # not connected: skip silently in "every service" mode
        if account is None and not args.offline:
            print(f"{name}: no connected account; running without ground truth", file=sys.stderr)
        connected.append((name, module, account))  # type: ignore[arg-type]
    if not connected:
        print("no connected service; connect one in the app or pass --offline", file=sys.stderr)
        return 2

    if args.intents:
        rows = number(
            [(name, module.questions(binary, account)) for name, module, account in connected]
        )
        print(render.render_intents(rows))
        destination = (
            Path(args.json) if args.json else Path.home() / ".student-mcp.eval" / "intents.json"
        )
        destination.parent.mkdir(parents=True, exist_ok=True)
        destination.write_text(json.dumps(rows, indent=2, ensure_ascii=False))
        print(f"\nreport: {destination}")
        return 1 if any(row["verdict"] == "FAIL" for row in rows) else 0

    reports = [(name, module.surface(binary, account)) for name, module, account in connected]
    audit = merge(reports)
    if args.baseline:
        try:
            audit["findings"].extend(
                render.compare(audit, json.loads(Path(args.baseline).read_text()))
            )
        except Exception as exc:
            print(f"baseline ignored: {exc}", file=sys.stderr)

    destination = Path(args.json) if args.json else Path.home() / ".student-mcp.eval" / "last.json"
    destination.parent.mkdir(parents=True, exist_ok=True)
    serialized = json.dumps(audit, indent=2, ensure_ascii=False)
    destination.write_text(serialized)
    if args.write_baseline:
        (destination.parent / "baseline.json").write_text(serialized)

    print(render.render(audit))
    print(f"\nreport: {destination}")
    blocking = [f for f in audit["findings"] if f["severity"] in ("blocker", "high")]
    return 1 if blocking else 0


if __name__ == "__main__":
    sys.exit(main())
