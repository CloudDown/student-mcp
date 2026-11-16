#!/usr/bin/env python3
"""Run the same student questions against several releases and compare scores.

Each version is the real released binary (downloaded from GitHub), not a local
build, so the comparison measures what a student actually installed. The
`current` version is the local release build.

    python3 benchmark/run.py                     # v0.8.0 … v0.8.4 + current
    python3 benchmark/run.py --versions v0.8.3,current
    python3 benchmark/run.py --update            # rewrite benchmark/SCORES.md

The questions come from `python3 tools/mcp-audit.py --intents`, which grades
every answer against the school API. This script only orchestrates: no second
question set, no second grader.
"""
from __future__ import annotations

import argparse
import json
import os
import platform
import shutil
import stat
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
AUDIT = ROOT / "tools" / "mcp-audit.py"
BIN_DIR = ROOT / "benchmark" / ".bin"
RESULTS = ROOT / "benchmark" / "results"
# One version per distinct MCP definition. `benchmark/surfaces.py` shows that
# every release since 0.6.2 exposes the same 27 tool names, but the definitions
# grow from 13.8 KB to 24.4 KB and the behaviour changes; versions sharing an
# identical surface are skipped.
DEFAULT_VERSIONS = [
    "v0.6.2",
    "v0.6.4",
    "v0.6.6",
    "v0.6.9",
    "v0.8.0",
    "v0.8.1",
    "v0.8.3",
    "v0.8.4",
    "current",
]


def release_asset() -> str:
    machine = platform.machine().lower()
    return "student-mcp-linux-arm64" if machine in ("aarch64", "arm64") else "student-mcp-linux-x64"


def binary_for(version: str) -> Path:
    if version == "current":
        binary = ROOT / "target" / "release" / "student-mcp"
        if not binary.exists():
            raise SystemExit("build the local release first: cargo build --release")
        return binary
    BIN_DIR.mkdir(parents=True, exist_ok=True)
    target = BIN_DIR / version
    if target.exists():
        return target
    asset = release_asset()
    print(f"  downloading {asset} from {version}")
    result = subprocess.run(
        [
            "gh",
            "release",
            "download",
            version,
            "--pattern",
            asset,
            "--output",
            str(target),
            "--clobber",
        ],
        cwd=ROOT,
        capture_output=True,
        text=True,
    )
    if result.returncode != 0:
        raise SystemExit(f"cannot download {version}: {result.stderr.strip()[:200]}")
    target.chmod(target.stat().st_mode | stat.S_IXUSR)
    return target


def config_copy() -> tuple[Path, Path]:
    """A private copy of the live config, so no release can rewrite the real one."""
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
        raise SystemExit("no connected account found; connect a service in the app first")
    handle, path = tempfile.mkstemp(prefix="student-mcp-bench-", suffix=".json")
    os.close(handle)
    shutil.copyfile(source, path)
    os.chmod(path, 0o600)
    return source, Path(path)


def run_version(version: str, binary: Path, config: Path) -> dict:
    output = RESULTS / f"{version}.json"
    output.unlink(missing_ok=True)  # a stale report must never mask a failure
    env = dict(os.environ)
    env["STUDENT_MCP_CONFIG_FILE"] = str(config)
    print(f"  {version}: {binary.name}, questions in flight…")
    result = subprocess.run(
        [
            sys.executable,
            str(AUDIT),
            "--intents",
            "--bin",
            str(binary),
            "--config",
            str(config),
            "--json",
            str(output),
        ],
        cwd=ROOT,
        env=env,
        capture_output=True,
        text=True,
        timeout=900,
    )
    if not output.exists():
        return {
            "version": version,
            "error": (result.stderr or result.stdout).strip()[:300],
        }
    rows = json.loads(output.read_text())
    counts = {key: sum(1 for r in rows if r["verdict"] == key) for key in ("PASS", "PARTIAL", "FAIL", "SKIP")}
    graded = counts["PASS"] + counts["PARTIAL"] + counts["FAIL"]
    counts["graded"] = graded
    counts["score"] = round(100 * counts["PASS"] / graded) if graded else 0
    counts["rows"] = rows
    return {"version": version, "counts": counts}


def matrix(results: list[dict]) -> str:
    lines = [f"{'version':10} {'PASS':>4} {'PART':>4} {'FAIL':>4} {'SKIP':>4} {'score':>6}  gains/losses"]
    previous = None
    for entry in results:
        version = entry["version"]
        counts = entry.get("counts")
        if not counts:
            lines.append(f"{version:10} {'-':>4} {'-':>4} {'-':>4} {'-':>4} {'-':>6}  {entry.get('error', 'failed')[:40]}")
            continue
        delta = ""
        if previous:
            # Compare by question text, never by row number: an older release
            # can emit a different number of rows and the numbers shift.
            before = {r["question"]: r["verdict"] for r in previous["counts"]["rows"]}
            gained = [
                r["n"]
                for r in counts["rows"]
                if r["verdict"] == "PASS" and before.get(r["question"]) not in (None, "PASS")
            ]
            lost = [
                r["n"]
                for r in counts["rows"]
                if r["verdict"] != "PASS" and before.get(r["question"]) == "PASS"
            ]
            if gained:
                delta += f" +{','.join(map(str, gained))}"
            if lost:
                delta += f" -{','.join(map(str, lost))}"
        lines.append(
            f"{version:10} {counts['PASS']:4} {counts['PARTIAL']:4} {counts['FAIL']:4} "
            f"{counts['SKIP']:4} {counts['score']:5}%  {delta.strip()}"
        )
        previous = entry
    return "\n".join(lines)


def write_scores(results: list[dict], config_source: Path) -> None:
    lines = [
        "# MCP benchmark across releases",
        "",
        "Same 20 student questions, same account, same grader "
        "(`tools/mcp-audit.py --intents`). Each row is a released binary.",
        "",
        "| release | PASS | PARTIAL | FAIL | SKIP | score |",
        "|---|---|---|---|---|---|",
    ]
    for entry in results:
        counts = entry.get("counts")
        if not counts:
            lines.append(f"| {entry['version']} | – | – | – | – | échec |")
            continue
        lines.append(
            f"| {entry['version']} | {counts['PASS']} | {counts['PARTIAL']} | "
            f"{counts['FAIL']} | {counts['SKIP']} | **{counts['score']} %** |"
        )
    lines += [
        "",
        "Les colonnes `PARTIAL` et `FAIL` comptent : une release peut gagner en",
        "rapidité et perdre une réponse correcte. Les `SKIP` sont des données absentes",
        "du compte, pas des erreurs.",
        "",
        "Régénérer : `python3 benchmark/run.py --update`.",
    ]
    (ROOT / "benchmark" / "SCORES.md").write_text("\n".join(lines) + "\n")
    print(f"\nwrote benchmark/SCORES.md (account: {config_source.name})")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--versions", default=",".join(DEFAULT_VERSIONS))
    parser.add_argument("--update", action="store_true", help="rewrite benchmark/SCORES.md")
    args = parser.parse_args()
    versions = [part.strip() for part in args.versions.split(",") if part.strip()]
    RESULTS.mkdir(parents=True, exist_ok=True)

    source, config = config_copy()
    print(f"config: {source} (copie privée, supprimée à la fin)")
    results: list[dict] = []
    try:
        for version in versions:
            binary = binary_for(version)
            results.append(run_version(version, binary, config))
    finally:
        config.unlink(missing_ok=True)

    print()
    print(matrix(results))
    if args.update:
        write_scores(results, source)
    return 0


if __name__ == "__main__":
    sys.exit(main())
