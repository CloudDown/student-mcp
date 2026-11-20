#!/usr/bin/env python3
"""Run the same student questions against several releases and compare scores.

Each version is the real released binary (downloaded from GitHub), not a local
build, so the comparison measures what a student actually installed. The
`current` version is the local release build.

    python3 benchmark/run.py                     # v0.8.0 … v0.8.4 + current
    python3 benchmark/run.py --versions v0.8.3,current
    python3 benchmark/run.py --update            # rewrite benchmark/SCORES.md

The questions come from `python3 tools/mcp-audit.py --intents`, which grades
against the school API (Moodle) or a private official-portal reference
(Brightspace/Omnivox). This script only orchestrates.
"""
from __future__ import annotations

import argparse
import json
import os
import platform
import re
import shutil
import stat
import statistics
import subprocess
import sys
import tempfile
from contextlib import contextmanager
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))

from benchmark import harness, report  # noqa: E402
from benchmark.services import AVAILABLE  # noqa: E402
from benchmark.services.portal import REFERENCE_ENV, read_reference  # noqa: E402

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


def copy_browser_profile(source: Path, dest: Path, account: dict) -> None:
    """Copy the selected profile while the app's own lock excludes Chrome."""
    import fcntl

    profile_id = account["token"].removeprefix("browser-profile:")
    if not re.fullmatch(r"[0-9a-fA-F]{32}", profile_id):
        raise ValueError("Identifiant de profil navigateur invalide.")
    override = os.environ.get("STUDENT_MCP_CONFIG_FILE") or os.environ.get("MOODLE_CONFIG_FILE")
    if override and Path(override).expanduser().resolve() == source:
        root = source.parent / "browsers"
    else:
        xdg_data = Path(os.environ.get("XDG_DATA_HOME", ""))
        root = (xdg_data if xdg_data.is_absolute() else Path.home() / ".local/share") / "student-mcp" / "browsers"
    profile = root / profile_id

    def ignore(_path, names):
        return {
            name for name in names
            if name in {"profile.lock", "DevToolsActivePort"}
            or name.startswith("Singleton")
            or (Path(_path) / name).is_symlink()
        }

    try:
        with (profile / "profile.lock").open("rb") as lock:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
            if not (profile / "session.json").is_file():
                raise OSError
            shutil.copytree(profile, dest / "browsers" / profile_id, ignore=ignore)
    except BlockingIOError:
        raise ValueError("Profil navigateur occupé : termine la connexion ou l'appel en cours, puis relance.") from None
    except OSError:
        raise ValueError("Profil navigateur introuvable ou impossible à copier.") from None


@contextmanager
def portal_config(service: str, reference: Path, source: Path):
    """Give benchmarked releases only the selected account, in a private directory."""
    ref = read_reference(reference, service)
    account = harness.load_account(source, service, account_id=ref["account_id"])
    if account is None:
        raise ValueError(f"Aucun compte {service} activé ne correspond à la référence.")
    original = json.loads(source.read_text(encoding="utf-8"))
    ident = account["id"]
    if ident == service:
        connection = dict(original[service], allow_write=False)
        config = {service: connection}
    else:
        selected = next(row for row in original["accounts"] if row.get("id") == ident and row.get("service") == service)
        connection = dict(selected[service], allow_write=False)
        config = {"accounts": [{"id": ident, "service": service, service: connection}]}
    with tempfile.TemporaryDirectory(prefix="student-mcp-bench-") as directory:
        private = Path(directory) / "config.json"
        with os.fdopen(os.open(private, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600), "w", encoding="utf-8") as out:
            json.dump(config, out)
        if account["token"].startswith("browser-profile:"):
            copy_browser_profile(source, private.parent, account)
        # Freeze the exact same reference for all versions, even if the original is edited.
        frozen = private.with_name("reference.json")
        with os.fdopen(os.open(frozen, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600), "w", encoding="utf-8") as out:
            json.dump(ref, out)
        yield private, frozen, ident


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


def run_version(
    version: str, binary: Path, config: Path, *, service: str = "moodle",
    reference: Path | None = None, account_id: str | None = None,
) -> dict:
    results_dir = RESULTS if service == "moodle" else RESULTS / service
    results_dir.mkdir(parents=True, exist_ok=True)
    output = results_dir / f"{version}.json"
    output.unlink(missing_ok=True)  # a stale report must never mask a failure
    metrics = results_dir / f"{version}.metrics.json"
    metrics.unlink(missing_ok=True)
    env = dict(os.environ)
    env["STUDENT_MCP_CONFIG_FILE"] = str(config)
    if reference:
        env[REFERENCE_ENV] = str(reference)
    if account_id:
        env["STUDENT_MCP_BENCH_ACCOUNT_ID"] = account_id
    print(f"  {version}: {binary.name}, questions in flight…")
    result = subprocess.run(
        [
            sys.executable,
            str(AUDIT),
            "--intents",
            "--service",
            service,
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
            "error": (result.stderr or result.stdout).strip()[:300] if service == "moodle" else "Aucun rapport produit par l'audit.",
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
    if service != "moodle":
        counts["wire_bytes_total"] = sum(int(row.get("wire_bytes") or 0) for row in rows)
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
    metrics.write_text(json.dumps(summary, indent=2))
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


def write_scores(results: list[dict], config_source: Path, service: str = "moodle") -> None:
    n_questions = 0
    for entry in results:
        counts = entry.get("counts") or {}
        n_questions = max(n_questions, len(counts.get("rows") or []), counts.get("graded", 0) + counts.get("SKIP", 0))
    n_label = f"{n_questions} questions" if n_questions else "questions"
    lines = [
        "# MCP benchmark across releases" if service == "moodle" else f"# MCP benchmark — {service}",
        "",
        f"{n_label} étudiantes identiques, même compte, même correcteur "
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
        "**Colonnes.** *temps total* = somme des appels d'outils ; *temps médian* = "
        "l'appel typique ; *tokens/question* = médiane des octets de réponse divisée par 4 ; "
        f"*tokens totaux* = tout ce qu'un modèle lirait pour ces {n_questions or 'N'} questions ; "
        "*tools/list* = ce que coûte la liste d'outils avant le premier appel ; "
        "*src MCP* = `mcp.rs` + `src/services/` au tag.",
        "",
        "Les `PARTIAL` et `FAIL` comptent : une release peut gagner en vitesse et perdre "
        "une réponse correcte. Les `SKIP` sont des données absentes du compte.",
        "",
        "Régénérer : `python3 benchmark/run.py --update`." if service == "moodle" else
        f"Régénérer avec `benchmark/run.py --service {service} --reference … --update`.",
    ]
    scores = "SCORES.md" if service == "moodle" else f"SCORES-{service}.md"
    (ROOT / "benchmark" / scores).write_text("\n".join(lines) + "\n")
    print(f"\nwrote benchmark/{scores} (config: {config_source.name})")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--versions", help="comma-separated releases (portal default: current)")
    parser.add_argument("--service", choices=AVAILABLE, default="moodle")
    parser.add_argument("--reference", type=Path, help="private official-portal reference (Brightspace/Omnivox)")
    parser.add_argument("--update", action="store_true", help="rewrite the selected service's score table")
    args = parser.parse_args()
    portal = args.service != "moodle"
    if portal != bool(args.reference):
        parser.error("--reference est requise pour Brightspace/Omnivox et non utilisée pour Moodle.")
    versions = [part.strip() for part in (args.versions or ("current" if portal else ",".join(DEFAULT_VERSIONS))).split(",") if part.strip()]
    if not versions:
        parser.error("au moins une version est requise.")
    results: list[dict] = []
    if portal:
        source = harness.default_config().resolve()
        try:
            with portal_config(args.service, args.reference.resolve(), source) as (config, reference, ident):
                for version in versions:
                    results.append(run_version(version, binary_for(version), config, service=args.service,
                                               reference=reference, account_id=ident))
        except (ValueError, OSError, subprocess.TimeoutExpired) as exc:
            print(str(exc) if isinstance(exc, ValueError) else "Le benchmark n'a pas pu terminer.", file=sys.stderr)
            return 2
    else:
        source, config = config_copy()
        print(f"config: {source} (copie privée, supprimée à la fin)")
        try:
            for version in versions:
                results.append(run_version(version, binary_for(version), config))
        finally:
            config.unlink(missing_ok=True)

    print()
    print(matrix(results))
    if portal:
        for entry in results:
            if counts := entry.get("counts"):
                print(f"\n{args.service} — {entry['version']}")
                print(report.render_intents(counts["rows"]))
    if portal and any(not (entry.get("counts") or {}).get("graded") for entry in results):
        print("Au moins une version n'a pas de score exploitable.", file=sys.stderr)
        return 2
    if args.update:
        write_scores(results, source, args.service)
        if not portal:
            from benchmark import site as site_mod  # noqa: E402
            site_mod.main()
    if portal and results[-1]["counts"]["FAIL"]:
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
