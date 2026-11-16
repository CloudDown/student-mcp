"""One module per school service.

Each module exposes exactly two entry points and nothing else is expected:

    NAME = "moodle"
    def surface(binary: Path, account: dict | None) -> dict
        # payload, fidelity, latency and tool-surface findings
    def questions(binary: Path, account: dict | None) -> list[dict]
        # student questions graded PASS / PARTIAL / FAIL / SKIP

Adding Brightspace, Omnivox or Canvas means one new file in this directory plus
one line in `AVAILABLE`, not a second harness. Copy `_template.py`.
"""

from importlib import import_module

AVAILABLE = ("moodle", "brightspace", "omnivox", "canvas")


def load(name: str):
    if name not in AVAILABLE:
        raise SystemExit(f"unknown service {name!r}; known: {', '.join(AVAILABLE)}")
    return import_module(f"benchmark.services.{name}")
