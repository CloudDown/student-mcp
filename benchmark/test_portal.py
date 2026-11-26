"""Offline coverage for the Brightspace/Omnivox benchmark and release runner."""
import contextlib
import datetime as dt
import io
import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

from benchmark import harness, run
from benchmark.services import brightspace, omnivox, portal

ROOT = Path(__file__).resolve().parent


def reference(service):
    return portal.read_reference(ROOT / "references" / f"{service}.example.json", service)


def deadline_case(module):
    ref = reference(module.NAME)
    ref["deadlines"]["verified_utc_day"] = dt.datetime.now(dt.timezone.utc).date().isoformat()
    return ref, module.cases(ref)[-1]


class ScoringTests(unittest.TestCase):
    def test_brightspace_deadlines_distinguish_empty_partial_wrong_scope_and_dates(self):
        _, case = deadline_case(brightspace)
        valid = {"courseid": 42, "type": "assignment", "id": 21, "due": "2026-01-07T15:00:00-05:00"}
        self.assertEqual(case.check({"items": [valid]}, {})[0], "PASS")
        self.assertEqual(case.check({"items": []}, {})[0], "FAIL")
        self.assertEqual(case.check({"items": [], "partial": True}, {})[0], "PARTIAL")
        self.assertEqual(case.check({"items": [valid, valid]}, {})[0], "FAIL")
        self.assertEqual(case.check({"items": [{**valid, "courseid": 99}]}, {})[0], "FAIL")
        self.assertEqual(case.check({"items": [{**valid, "due": "2026-01-08T20:00:00Z"}]}, {})[0], "FAIL")

    def test_omnivox_deadlines_handle_units_and_known_empty_references(self):
        ref, case = deadline_case(omnivox)
        due = ref["deadlines"]["items"][0]["due"]
        row = {"course_id": "MAT101.0001", "kind": "assignment", "id": 21, "due_ms": due // 1000}
        self.assertEqual(case.check({"deadlines": [row]}, {})[0], "PASS")
        ref["deadlines"]["items"] = []
        empty = omnivox.cases(ref)[-1]
        self.assertEqual(empty.check({"deadlines": []}, {})[0], "PASS")
        self.assertEqual(empty.check({"deadlines": [row]}, {})[0], "FAIL")
        ref["deadlines"]["verified_utc_day"] = "2000-01-01"
        self.assertIsNone(omnivox.cases(ref)[-1].check)

    def test_one_pdf_page_cannot_count_as_a_whole_pdf(self):
        case = brightspace.cases(reference("brightspace"))[3]
        coverage = {"mode": "page-window", "firstPage": 1, "lastPage": 1, "totalPages": 5, "allPagesDelivered": False}
        good = {"coverage": coverage, "nextPageFrom": 2, "text": "Fonctions\net dérivées"}
        self.assertEqual(case.check(good, {"image_count": 1})[0], "PASS")
        self.assertEqual(case.check(good, {"image_count": 0})[0], "FAIL")
        self.assertEqual(case.check({**good, "nextPageFrom": None}, {"image_count": 1})[0], "FAIL")
        self.assertEqual(case.check({**good, "text": "autre"}, {"image_count": 1})[0], "PARTIAL")

    def test_documents_instructions_and_grade_match_the_reference(self):
        bs = brightspace.cases(reference("brightspace"))
        self.assertEqual(bs[1].check({"Modules": [{"Topics": [{"Id": 17, "Title": "Guide de révision"}]}]}, {})[0], "PASS")
        self.assertEqual(bs[2].check({"matches": [], "truncated": True}, {})[0], "PARTIAL")
        self.assertEqual(bs[4].check({"folder": {"id": 999, "instructions": "..."}}, {})[0], "FAIL")
        ov = {case.tool: case for case in omnivox.cases(reference("omnivox"))}
        self.assertEqual(ov["omnivox_my_courses"].check({"ListeCours": [{"NoCours": "MAT101", "NoGroupe": "0001", "NomCours": "Mathématiques"}]}, {})[0], "PASS")
        self.assertEqual(ov["omnivox_course_documents"].check({"IdDocCoursDocument": 17, "Titre": "Guide de révision"}, {})[0], "PASS")
        self.assertEqual(ov["omnivox_assignment_detail"].check({"Enonce": "<p>Résoudre les exercices 1 à 4.</p><p>Justifier chaque réponse.</p>"}, {})[0], "PASS")
        self.assertEqual(ov["omnivox_assignment_detail"].check({"Enonce": "Résoudre les exercices 1 à 4."}, {})[0], "FAIL")
        self.assertEqual(ov["omnivox_grades"].check([{"Titre": "Quiz 1", "Note": 8840}], {})[0], "PASS")
        self.assertEqual(ov["omnivox_grades"].check([{"Titre": "Quiz 1", "Note": 88.4}, {"Titre": "Quiz 2", "Note": 8840}], {})[0], "FAIL")

    def test_omnivox_term_is_forwarded_to_term_dependent_reads(self):
        ref = reference("omnivox")
        cases = {case.tool: case for case in omnivox.cases(ref)}
        for tool in (
            "omnivox_my_courses", "omnivox_course_documents", "omnivox_file",
            "omnivox_assignment_detail", "omnivox_grades",
        ):
            self.assertEqual(cases[tool].args.get("term"), ref["term"])
        self.assertNotIn("term", cases["omnivox_deadlines"].args)
        self.assertEqual(cases["omnivox_terms"].args, {})
        ref.pop("term")
        self.assertNotIn("omnivox_terms", {case.tool for case in omnivox.cases(ref)})
        self.assertEqual(
            [case.args.get("term") for case in omnivox.cases(ref) if case.tool != "omnivox_deadlines"],
            [None] * 5,
        )

    def test_omnivox_current_term_must_match_the_reference(self):
        ref = reference("omnivox")
        case = next(case for case in omnivox.cases(ref) if case.tool == "omnivox_terms")
        good = {"current_term": ref["term"], "available_terms": {"AnSessionDisponible": [ref["term"]]}}
        self.assertEqual(case.check(good, {})[0], "PASS")
        self.assertEqual(case.check({**good, "current_term": "20262"}, {})[0], "FAIL")
        self.assertEqual(case.check({"current_term": ref["term"], "available_terms": {}}, {})[0], "FAIL")

    def test_invalid_omnivox_term_is_rejected(self):
        ref = reference("omnivox")
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "reference.json"
            for term in ("2026", "202633", 20263):
                path.write_text(json.dumps({**ref, "term": term}))
                with self.assertRaises(ValueError):
                    portal.read_reference(path, "omnivox")

    def test_generic_reference_checks_follow_paths_rows_and_text(self):
        spec = {
            "question": "« Combien de documents dans ce cours ? »",
            "tool": "omnivox_documents_summary",
            "args": {"term": "20263"},
            "expect": [
                {"path": "ListeSommaire", "match": {"NoCours": "MAT101", "NoGroupe": "0001"},
                 "values": {"NbDocumentsDisponibles": 17}},
                {"path": "ListeSommaire", "min_count": 1},
                {"path": "ListeSommaire", "count": 1},
                {"path": "ListeTravaux", "empty": True},
            ],
            "text": ["ListeSommaire"],
        }
        case = portal.spec_cases("omnivox", [spec])[0]
        data = {"ListeSommaire": [{"NoCours": "MAT101", "NoGroupe": "0001", "NbDocumentsDisponibles": 17}],
                "ListeTravaux": []}
        result = {"text": '{"ListeSommaire":[]}'}
        self.assertEqual(case.check(data, result)[0], "PASS")
        self.assertEqual(case.check({"ListeSommaire": [], "ListeTravaux": []}, result)[0], "FAIL")
        self.assertEqual(case.check({**data, "ListeSommaire": data["ListeSommaire"] * 2}, result)[0], "FAIL")
        self.assertEqual(case.check({**data, "ListeTravaux": [{}]}, result)[0], "FAIL")
        self.assertEqual(case.check({**data, "ListeSommaire": [{**data["ListeSommaire"][0], "NbDocumentsDisponibles": 16}]}, result)[0], "FAIL")
        self.assertEqual(case.check(data, {"text": "absent"})[0], "FAIL")

    def test_generic_checks_are_validated_and_appended_to_cases(self):
        ref = reference("omnivox")
        ref["checks"] = [{
            "question": "« Combien de cours ? »",
            "tool": "omnivox_status",
            "args": {},
            "expect": [{"path": "", "values": {"connected": True}}],
        }]
        self.assertEqual(omnivox.cases(ref)[-1].tool, "omnivox_status")
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "reference.json"
            for broken in (
                {**ref["checks"][0], "tool": "moodle_search"},
                {**ref["checks"][0], "args": {"account_id": "omnivox"}},
                {**ref["checks"][0], "expect": []},
                {**ref["checks"][0], "expect": [{"path": "", "empty": False}]},
                {**ref["checks"][0], "expect": [{"path": "", "min_count": 0}]},
                {**ref["checks"][0], "expect": [{"path": "", "values": {}}]},
                {**ref["checks"][0], "expect": [{"path": "", "unknown": True}]},
                {**ref["checks"][0], "text": [1]},
            ):
                path.write_text(json.dumps({**ref, "checks": [broken]}))
                with self.assertRaises(ValueError):
                    portal.read_reference(path, "omnivox")

    def test_mcp_transport_preserves_pdf_metadata_and_counts_nonempty_images(self):
        content = [{"type": "text", "text": '{"coverage":{}}'},
                   {"type": "text", "text": "Page 1"},
                   {"type": "image", "data": "YQ=="},
                   {"type": "image", "data": ""}]
        line = json.dumps({"id": 100, "result": {"content": content}})
        proc = subprocess.CompletedProcess([], 0, stdout=line + "\n", stderr="")
        with patch.object(harness.subprocess, "run", return_value=proc):
            result, _ = harness.MCP(Path("not-called")).one("brightspace_file", {})
        self.assertEqual(json.loads(result["first_text"]), {"coverage": {}})
        self.assertEqual(result["image_count"], 1)
        self.assertEqual(result["wire_bytes"], len(line.encode("utf-8")))


class RunnerTests(unittest.TestCase):
    def test_reference_required_and_non_readonly_tools_never_called(self):
        binary = Path("not-called")
        with patch.dict(os.environ, {portal.REFERENCE_ENV: ""}), patch.object(harness.MCP, "batch") as batch:
            self.assertEqual(brightspace.questions(binary, None)[0]["verdict"], "SKIP")
            batch.assert_not_called()
        path = ROOT / "references" / "brightspace.example.json"
        tools = [{"name": "brightspace_my_courses", "annotations": {"readOnlyHint": False}}]
        with (
            patch.dict(os.environ, {portal.REFERENCE_ENV: str(path)}),
            patch.object(harness.MCP, "batch", return_value=(tools, [])),
            patch.object(harness.MCP, "one") as one,
        ):
            rows = brightspace.questions(binary, {"id": "brightspace"})
            self.assertEqual(rows[0]["verdict"], "FAIL")
            one.assert_not_called()

    def test_brightspace_follows_course_bookmark_and_keeps_selected_account(self):
        path = ROOT / "references" / "brightspace.example.json"
        schema = {"properties": {"account_id": {"type": "string"}}}
        tools = [{"name": "brightspace_my_courses", "annotations": {"readOnlyHint": True}, "inputSchema": schema}]
        pages = [
            {"Items": [], "PagingInfo": {"HasMoreItems": True, "Bookmark": "page2"}},
            {"Items": [{"OrgUnit": {"Id": 42, "Name": "Mathématiques"}}]},
        ]
        with (
            patch.dict(os.environ, {portal.REFERENCE_ENV: str(path)}),
            patch.object(harness.MCP, "batch", return_value=(tools, [])),
            patch.object(harness.MCP, "one", side_effect=[({"structured": page, "wire_bytes": 80}, 0.25) for page in pages]) as one,
        ):
            rows = brightspace.questions(Path("not-called"), {"id": "brightspace"})
        self.assertEqual(rows[0]["verdict"], "PASS")
        self.assertEqual(rows[0]["seconds"], 0.5)
        self.assertEqual(rows[0]["wire_bytes"], 160)
        self.assertEqual(one.call_args.args[1], {"account_id": "brightspace", "bookmark": "page2"})

    def test_browser_snapshot_is_private_and_does_not_follow_profile_symlinks(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            source = root / "live" / "config.json"
            profile_id = "a" * 32
            profile = source.parent / "browsers" / profile_id
            profile.mkdir(parents=True)
            (profile / "profile.lock").touch()
            (profile / "session.json").write_text("{}")
            (profile / "DevToolsActivePort").write_text("stale")
            (profile / "sensitive-link").symlink_to(source)
            source.write_text(json.dumps({
                "brightspace": {"url": "https://example.invalid", "token": f"browser-profile:{profile_id}"},
                "moodle": {"url": "https://unrelated.invalid", "token": "not-copied"},
            }))
            before = source.read_bytes()
            path = ROOT / "references" / "brightspace.example.json"
            with patch.dict(os.environ, {"STUDENT_MCP_CONFIG_FILE": str(source)}):
                with run.portal_config("brightspace", path, source) as (private, frozen, account_id):
                    self.assertEqual(account_id, "brightspace")
                    self.assertEqual(private.stat().st_mode & 0o777, 0o600)
                    self.assertEqual(frozen.stat().st_mode & 0o777, 0o600)
                    self.assertNotIn("moodle", json.loads(private.read_text()))
                    copy = private.parent / "browsers" / profile_id
                    self.assertEqual(list(copy.iterdir()), [copy / "session.json"])
                self.assertFalse(private.parent.exists())
            self.assertEqual(source.read_bytes(), before)
            self.assertTrue((profile / "DevToolsActivePort").exists())

    def test_release_cli_calls_existing_audit_with_a_frozen_reference(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            source = root / "config.json"
            source.write_text(json.dumps({
                "omnivox": {"url": "https://example.invalid", "token": "test-only"},
            }))
            argv = [
                "run.py", "--service", "omnivox", "--reference", str(ROOT / "references/omnivox.example.json"),
            ]

            def audit(command, *, env, **_kwargs):
                self.assertEqual(command[command.index("--service") + 1], "omnivox")
                self.assertNotEqual(Path(env["STUDENT_MCP_CONFIG_FILE"]), source)
                self.assertEqual(portal.read_reference(Path(env[portal.REFERENCE_ENV]), "omnivox")["account_id"], "omnivox")
                Path(command[-1]).write_text(json.dumps([{
                    "n": 1, "service": "omnivox", "question": "Cours", "verdict": "PASS",
                    "evidence": "Conforme", "tool": "omnivox_my_courses", "bytes": 12,
                }]))
                return subprocess.CompletedProcess(command, 0, stdout="", stderr="")

            with (
                patch.object(sys, "argv", argv),
                patch.object(run, "RESULTS", root / "results"),
                patch.object(run, "binary_for", return_value=source),
                patch.object(run, "source_bytes", return_value=1),
                patch.object(harness, "tools_surface", return_value={"count": 1, "bytes": 10}),
                patch.dict(os.environ, {"STUDENT_MCP_CONFIG_FILE": str(source)}),
                patch.object(run.subprocess, "run", side_effect=audit),
                contextlib.redirect_stdout(io.StringIO()) as out,
            ):
                self.assertEqual(run.main(), 0)
            self.assertIn("1 PASS", out.getvalue())
            self.assertTrue((root / "results/omnivox/current.json").is_file())
            self.assertFalse((root / "results/current.json").exists())

    def test_audit_runs_a_readonly_question_over_stdio_without_school_access(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            server = root / "fake-mcp"
            server.write_text(f"#!{sys.executable}\n" + """\
import json, sys
for line in sys.stdin:
    message = json.loads(line)
    if 'id' not in message:
        continue
    if message['method'] == 'tools/list':
        result = {'tools': [{'name': 'omnivox_my_courses', 'annotations': {'readOnlyHint': True}}]}
    elif message['method'] == 'tools/call':
        assert message['params']['name'] == 'omnivox_my_courses'
        courses = {'ListeCours': [{'NoCours': 'MAT101', 'NoGroupe': '0001', 'NomCours': 'Mathématiques'}]}
        result = {'content': [{'type': 'text', 'text': json.dumps(courses)}]}
    else:
        result = {}
    print(json.dumps({'id': message['id'], 'result': result}))
""", encoding="utf-8")
            server.chmod(0o700)
            config = root / "config.json"
            config.write_text(json.dumps({"omnivox": {"url": "https://example.invalid", "token": "test-only"}}))
            ref = root / "reference.json"
            sample = reference("omnivox")
            ref.write_text(json.dumps({key: sample[key] for key in ("service", "account_id", "course")}))
            output = root / "report.json"
            result = subprocess.run(
                [sys.executable, "-B", str(run.AUDIT), "--service", "omnivox", "--intents",
                 "--config", str(config), "--bin", str(server), "--json", str(output)],
                env={**os.environ, portal.REFERENCE_ENV: str(ref),
                     "STUDENT_MCP_CONFIG_FILE": str(config), "STUDENT_MCP_BENCH_ACCOUNT_ID": "omnivox"},
                capture_output=True, text=True, timeout=20,
            )
            self.assertEqual(result.returncode, 0, result.stderr)
            rows = json.loads(output.read_text())
            self.assertEqual([row["verdict"] for row in rows], ["PASS", *(["SKIP"] * 5)])
            self.assertGreater(rows[0]["wire_bytes"], rows[0]["bytes"])


if __name__ == "__main__":
    unittest.main()
