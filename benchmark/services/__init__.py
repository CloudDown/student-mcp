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

# Implemented services. Adding one means a module here and a line below.
AVAILABLE = ("moodle",)
# Services the harness is designed for; their module does not exist yet.
PLANNED = ("brightspace", "omnivox", "canvas")


def load(name: str):
    if name in PLANNED:
        raise SystemExit(
            f"{name} is planned: copy benchmark/services/_template.py to "
            f"benchmark/services/{name}.py and implement surface() and questions()."
        )
    if name not in AVAILABLE:
        raise SystemExit(f"unknown service {name!r}; known: {', '.join(AVAILABLE + PLANNED)}")
    return import_module(f"benchmark.services.{name}")
