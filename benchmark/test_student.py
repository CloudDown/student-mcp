"""Offline regression tests for the student runner's failure reporting."""
import io
import json
import os
import tempfile
import unittest
import urllib.error
from contextlib import redirect_stderr, redirect_stdout
from pathlib import Path
from unittest.mock import patch

from benchmark import student


class StudentFailureTests(unittest.TestCase):
    def test_gateway_error_reports_known_category_without_secret(self):
        error = urllib.error.HTTPError(
            "https://example.invalid", 403, "Forbidden", {},
            io.BytesIO(b'{"error":{"type":"permission_error","message":"private"}}'),
        )
        self.assertEqual(student.safe_api_error(error, "fake-key"), "permission_error")

    def test_gateway_error_discards_unknown_and_sensitive_fields(self):
        error = urllib.error.HTTPError(
            "https://example.invalid", 403, "Forbidden", {},
            io.BytesIO(b'{"error":{"code":"fake-key","message":"fake-key"}}'),
        )
        self.assertIsNone(student.safe_api_error(error, "fake-key"))

    def test_forbidden_stops_campaign_and_returns_failure(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            scenarios = root / "scenarios.json"
            scenarios.write_text(json.dumps([{"question": "test"}]))
            binary = root / "binary"
            binary.touch()
            private_config = root / "config"
            private_config.touch()
            output = root / "result.json"
            tool = {"name": "moodle_search", "annotations": {"readOnlyHint": True}}
            argv = [
                "student.py", "--scenarios", str(scenarios), "--bin", str(binary),
                "--repeat", "3", "--output", str(output),
            ]
            with (
                patch.dict(os.environ, {"OPENCODE_API_KEY": "fake-test-key"}),
                patch("sys.argv", argv),
                patch.object(student, "config_copy", return_value=(root / "source", private_config)),
                patch.object(student.MCP, "batch", return_value=([tool], [])),
                patch.object(student, "run_turns", side_effect=student.ApiError(403)) as run,
                redirect_stderr(io.StringIO()) as err,
                redirect_stdout(io.StringIO()) as out,
            ):
                self.assertEqual(student.main(), 1)
            self.assertEqual(run.call_count, 1)
            self.assertIn("HTTP 403", err.getvalue())
            self.assertIn("0/1 completed", out.getvalue())
            rows = json.loads(output.read_text())["results"]
            self.assertEqual(len(rows), 1)
            self.assertFalse(rows[0]["completed"])
            self.assertNotIn("fake-test-key", output.read_text())
            self.assertFalse(private_config.exists())


if __name__ == "__main__":
    unittest.main()
