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
import statistics
import subprocess
import sys
import tempfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))

from benchmark import harness  # noqa: E402

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


def source_bytes(version: str) -> int:
    """Disk size of the MCP layer itself: mcp.rs plus the service connectors."""
    if version == "current":
        paths = [
            ROOT / "src" / "mcp.rs",
            *(path for path in (ROOT / "src" / "services").rglob("*") if path.is_file()),
        ]
        return sum(path.stat().st_size for path in paths if path.exists())
    listing = subprocess.run(
        ["git", "ls-tree", "-r", "-l", version, "--", "src/mcp.rs", "src/services"],
        cwd=ROOT, capture_output=True, text=True,
    ).stdout
    total = 0
    for line in listing.splitlines():
        parts = line.split()
        if len(parts) >= 4 and parts[1] == "blob":
            total += int(parts[3])
    return total


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
    seconds = [float(r.get("seconds") or 0) for r in rows]
    tokens = [int(r.get("bytes") or 0) // 4 for r in rows]
    counts["seconds_total"] = round(sum(seconds), 1)
    counts["seconds_median"] = round(statistics.median(seconds), 2) if seconds else 0
    counts["seconds_max"] = round(max(seconds), 2) if seconds else 0
    counts["slowest"] = rows[seconds.index(max(seconds))]["question"] if seconds else ""
    counts["tokens_total"] = sum(tokens)
    counts["tokens_median"] = int(statistics.median(tokens)) if tokens else 0
    counts["tokens_max"] = max(tokens) if tokens else 0
    counts["heaviest"] = rows[tokens.index(max(tokens))]["question"] if tokens else ""
    counts["binary_bytes"] = binary.stat().st_size
    counts["source_bytes"] = source_bytes(version)
    try:
        surface = harness.tools_surface(str(binary), str(config))
        counts["tools_count"] = surface["count"]
        counts["tools_bytes"] = surface["bytes"]
    except Exception:
        counts["tools_count"] = 0
        counts["tools_bytes"] = 0
    summary = {key: value for key, value in counts.items() if key != "rows"}
    (RESULTS / f"{version}.metrics.json").write_text(json.dumps(summary, indent=2))
    return {"version": version, "counts": counts}


def matrix(results: list[dict]) -> str:
    head = (
        f"{'version':10} {'score':>6} {'PASS':>4} {'PART':>4} {'FAIL':>4} {'SKIP':>4} "
        f"{'temps':>7} {'méd':>6} {'tok/méd':>8} {'tok/tot':>8} {'tools':>6} {'binaire':>8} {'src MCP':>8}"
    )
    lines = [head]
    previous = None
    for entry in results:
        version = entry["version"]
        counts = entry.get("counts")
        if not counts:
            lines.append(f"{version:10} {'-':>6}  {entry.get('error', 'failed')[:60]}")
            continue
        delta = ""
        if previous:
            # Compare by question text, never by row number: an older release can
            # emit a different number of rows and the numbers shift.
            before = {r["question"]: r["verdict"] for r in previous["counts"]["rows"]}
            gained = [
                r["n"] for r in counts["rows"]
                if r["verdict"] == "PASS" and before.get(r["question"]) not in (None, "PASS")
            ]
            lost = [
                r["n"] for r in counts["rows"]
                if r["verdict"] != "PASS" and before.get(r["question"]) == "PASS"
            ]
            if gained:
                delta += f" +{','.join(map(str, gained))}"
            if lost:
                delta += f" -{','.join(map(str, lost))}"
        lines.append(
            f"{version:10} {counts['score']:5}% {counts['PASS']:4} {counts['PARTIAL']:4} "
            f"{counts['FAIL']:4} {counts['SKIP']:4} {counts['seconds_total']:6.1f}s "
            f"{counts['seconds_median']:5.2f}s {counts['tokens_median']:8} "
            f"{counts['tokens_total']:8} {counts['tools_bytes'] // 4:6} "
            f"{counts['binary_bytes'] / 1e6:7.1f}M {counts['source_bytes'] / 1024:7.0f}K{delta}"
        )
        previous = entry
    return "\n".join(lines)


def write_scores(results: list[dict], config_source: Path) -> None:
    lines = [
        "# MCP benchmark across releases",
        "",
        "50 questions étudiantes identiques, même compte, même correcteur "
        "(`tools/mcp-audit.py --intents`). Chaque ligne est un binaire publié.",
        "",
        "| release | score | PASS | PART | FAIL | SKIP | temps total | temps médian | tokens/question | tokens totaux | tools/list | binaire | src MCP |",
        "|---|---|---|---|---|---|---|---|---|---|---|---|---|",
    ]
    for entry in results:
        counts = entry.get("counts")
        if not counts:
            lines.append(f"| {entry['version']} | – | – | – | – | – | – | – | – | – | – | – | – |")
            continue
        lines.append(
            f"| {entry['version']} | **{counts['score']} %** | {counts['PASS']} | {counts['PARTIAL']} | "
            f"{counts['FAIL']} | {counts['SKIP']} | {counts['seconds_total']:.1f} s | "
            f"{counts['seconds_median']:.2f} s | {counts['tokens_median']} | {counts['tokens_total']} | "
            f"{counts['tools_bytes'] // 4} | {counts['binary_bytes'] / 1e6:.1f} Mo | "
            f"{counts['source_bytes'] / 1024:.0f} Ko |"
        )
    lines += [
        "",
        "**Colonnes.** *temps total* = somme des 50 appels d'outils ; *temps médian* = "
        "l'appel typique ; *tokens/question* = médiane des octets de réponse divisée par 4 ; "
        "*tokens totaux* = tout ce qu'un modèle lirait pour ces 50 questions ; "
        "*tools/list* = ce que coûte la liste d'outils avant le premier appel ; "
        "*src MCP* = `mcp.rs` + `src/services/` au tag.",
        "",
        "Les `PARTIAL` et `FAIL` comptent : une release peut gagner en vitesse et perdre "
        "une réponse correcte. Les `SKIP` sont des données absentes du compte.",
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
