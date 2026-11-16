"""Template for a new school service.

Copy to `benchmark/services/<name>.py`, implement the two functions, add the name
to `AVAILABLE` in `benchmark/services/__init__.py`. Nothing else changes: the CLI,
the release matrix and the score table pick it up.

Rules that keep the comparison honest:

- Ground truth comes from the school itself, with the same User-Agent the
  connector uses (some school WAFs answer an empty challenge to a default one).
- A question is skipped (`SKIP`) when the account holds no such data; missing
  data is not a tool failure.
- Never test a write on a real account. Read-only questions only.
"""

from __future__ import annotations

from pathlib import Path
from typing import Any

from ..harness import MCP

NAME = "example"


def surface(binary: Path, account: dict[str, Any] | None) -> dict[str, Any]:
    """Payload, fidelity and latency findings. Return an empty report to skip."""
    return {
        "version": None,
        "tools": 0,
        "tools_list_bytes": 0,
        "calls": [],
        "findings": [],
        "heaviest_descriptions": [],
    }


def questions(binary: Path, account: dict[str, Any] | None) -> list[dict[str, Any]]:
    """Student questions, graded against the school API.

    Each row: {"question", "verdict", "evidence", "tool", "bytes"}.
    """
    mcp = MCP(binary)
    rows: list[dict[str, Any]] = []
    result, _ = mcp.one("example_status", {})
    rows.append(
        {
            "question": "« Est-ce que mon compte répond ? »",
            "verdict": "PASS" if not result["is_error"] else "FAIL",
            "evidence": f"{len(result['text'])} octets",
            "tool": "example_status",
            "bytes": len(result["text"]),
        }
    )
    return rows
