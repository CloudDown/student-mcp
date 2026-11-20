"""Shared read-only checks against independently verified portal data."""
from __future__ import annotations

import json
import os
import re
import subprocess
from dataclasses import dataclass
from datetime import date, datetime, timezone
from html.parser import HTMLParser
from pathlib import Path
from typing import Callable

from .. import harness

REFERENCE_ENV = "STUDENT_MCP_BENCH_REFERENCE"
Verdict = tuple[str, str]


@dataclass
class Case:
    question: str
    tool: str
    args: dict
    check: Callable[[object, dict], Verdict] | None
    skip: str = "Aucune donnée de référence pour ce cas."
    follow: Callable[[object], dict | None] | None = None


def read_reference(path: Path, service: str) -> dict:
    try:
        if path.stat().st_size > 1024 * 1024:
            raise ValueError
        ref = json.loads(path.read_text(encoding="utf-8"))
        account = ref["account_id"]
        course = ref["course"]
        require(ref["service"] == service and nonblank(account))
        require(has_title(course) and valid_id(service, course["id"], course=True))
        document = ref.get("document")
        if document is not None:
            require(has_title(document) and valid_id(service, document["id"]))
            require(all(key not in document or nonblank(document[key]) for key in ("query", "title_field")))
            pages = document.get("pdf_pages")
            require(pages is None or positive_int(pages))
            require("page1_text" not in document or (pages and nonblank(document["page1_text"])))
        assignment = ref.get("assignment")
        if assignment is not None:
            require(valid_id(service, assignment["id"]) and nonblank(assignment["instructions"]))
            if service == "omnivox":
                require(nonblank(assignment["instructions_field"]))
        grade = ref.get("grade")
        if grade is not None:
            require(isinstance(grade["match"], dict) and grade["match"])
            require(isinstance(grade["values"], dict) and grade["values"])
            require(set(grade["match"]).isdisjoint(grade["values"]))
            require(all(type(value) in (str, int, float, bool)
                        for value in (*grade["match"].values(), *grade["values"].values())))
        deadlines = ref.get("deadlines")
        if deadlines is not None:
            require(type(deadlines["days"]) is int and 1 <= deadlines["days"] <= 365)
            date.fromisoformat(deadlines["verified_utc_day"])
            require(isinstance(deadlines["items"], list))
            keys = set()
            for item in deadlines["items"]:
                require(item["kind"] in (("assignment", "quiz", "calendar") if service == "brightspace" else ("assignment",)))
                require(valid_id(service, item["id"]))
                require(instant(service, item["due"]) is not None)
                key = (item["kind"], item["id"])
                require(key not in keys)
                keys.add(key)
    except (AttributeError, KeyError, TypeError, ValueError, OSError):
        raise ValueError(f"Référence {service} invalide : voir le format dans benchmark/README.md.") from None
    return ref


def require(condition: object):
    if not condition:
        raise ValueError


def positive_int(value: object) -> bool:
    return type(value) is int and value > 0


def nonblank(value: object) -> bool:
    return isinstance(value, str) and bool(value.strip())


def has_title(row: dict) -> bool:
    return isinstance(row, dict) and nonblank(row.get("title"))


def valid_id(service: str, value: object, *, course: bool = False) -> bool:
    if service == "brightspace":
        return positive_int(value)
    if not isinstance(value, str) or not value or len(value.encode("utf-8")) > (64 if course else 200):
        return False
    if value != value.strip() or any(ord(char) < 32 for char in value):
        return False
    return not course or all(value.partition("."))


def objects(value: object):
    if isinstance(value, dict):
        yield value
        for child in value.values():
            yield from objects(child)
    elif isinstance(value, list):
        for child in value:
            yield from objects(child)


class PlainText(HTMLParser):
    breaks = {"br", "p", "div", "li", "td", "tr"}

    def __init__(self):
        super().__init__()
        self.parts: list[str] = []

    def handle_data(self, data: str):
        self.parts.append(data)

    def handle_starttag(self, tag: str, attrs: list):
        if tag in self.breaks:
            self.parts.append(" ")

    def handle_endtag(self, tag: str):
        if tag in self.breaks:
            self.parts.append(" ")


def plain(text: str) -> str:
    parser = PlainText()
    parser.feed(text)
    parser.close()
    return " ".join("".join(parser.parts).split())


def found(ok: bool, subject: str) -> Verdict:
    return ("PASS", f"{subject} conforme à la référence.") if ok else ("FAIL", f"{subject} absent ou différent de la référence.")


def document_check(rows, ref: dict, id_key: str, title_keys: tuple[str, ...]) -> Verdict:
    ident = ref["id"]
    matched = [
        row for row in rows
        if (row.get(id_key) == ident and type(row.get(id_key)) is type(ident))
        or (isinstance(ident, str) and positive_int(row.get(id_key)) and str(row[id_key]) == ident)
    ]
    if any(row.get(key) == ref["title"] for row in matched for key in title_keys):
        return "PASS", "Titre et identifiant du document conformes."
    if matched:
        return "PARTIAL", "Identifiant retrouvé, mais titre absent ou différent."
    return "FAIL", "Identifiant du document introuvable."


def instructions_check(texts, target: str) -> Verdict:
    expected = plain(target)
    if expected and any(isinstance(text, str) and expected in plain(text) for text in texts):
        return "PASS", "Texte de référence complet retrouvé."
    return "FAIL", "La réponse ne contient pas toutes les consignes vérifiées."


def grade_check(data: object, grade: dict) -> Verdict:
    def matches(row: dict, expected: dict) -> bool:
        # JSON booleans must not match numeric 0 or 1.
        return all(row.get(key) == value and isinstance(row.get(key), bool) == isinstance(value, bool)
                   for key, value in expected.items())

    rows = [obj for obj in objects(data) if matches(obj, grade["match"])]
    if len(rows) != 1:
        return "FAIL", "L'évaluation cible est absente ou ambiguë."
    return found(matches(rows[0], grade["values"]), "Valeur de la note")


def pdf_case(tool: str, args: dict, document: dict | None) -> Case:
    def check(data: object, result: dict) -> Verdict:
        total = document["pdf_pages"]
        coverage = data.get("coverage") if isinstance(data, dict) else None
        expected = {
            "mode": "page-window", "firstPage": 1, "lastPage": 1,
            "totalPages": total, "allPagesDelivered": total == 1,
        }
        if not isinstance(coverage, dict) or any(
            coverage.get(key) != value or type(coverage.get(key)) is not type(value)
            for key, value in expected.items()
        ):
            return "FAIL", "La fenêtre PDF est absente ou sa couverture est incorrecte."
        if "nextPageFrom" not in data or data["nextPageFrom"] != (2 if total > 1 else None):
            return "FAIL", "La continuation PDF est incorrecte."
        if result.get("image_count") != 1:
            return "FAIL", "L'image de la première page n'a pas été livrée correctement."
        snippet = document.get("page1_text")
        if snippet and not (
            isinstance(data.get("text"), str) and " ".join(snippet.split()) in " ".join(data["text"].split())
        ):
            return "PARTIAL", "Image reçue, mais le passage de référence manque dans le texte."
        return "PASS", "Page, image et continuation conformes à la référence."

    return Case(
        "« Lis la première page du PDF et dis-moi s’il reste des pages. »",
        tool, {**args, "format": "text", "page_from": 1, "page_to": 1},
        check if document and document.get("pdf_pages") else None,
    )


def instant(service: str, value: object) -> datetime | date | None:
    try:
        if service == "brightspace" and isinstance(value, str):
            if len(value) == 10:
                return date.fromisoformat(value)
            parsed = datetime.fromisoformat(value)
            return parsed.astimezone(timezone.utc) if parsed.tzinfo is not None else None
        if service == "omnivox" and type(value) is int and value > 1_000_000_000:
            return datetime.fromtimestamp(value / 1000 if value >= 10_000_000_000 else value, timezone.utc)
    except (ValueError, OverflowError, OSError):
        pass
    return None


def deadlines_case(service: str, args: dict, course_id: int | str, ref: dict | None) -> Case:
    def check(data: object, _result: dict) -> Verdict:
        brightspace = service == "brightspace"
        rows = data.get("items" if brightspace else "deadlines") if isinstance(data, dict) else None
        if not isinstance(rows, list):
            return "FAIL", "Liste d'échéances absente ou mal formée."
        expected = {(item["kind"], item["id"]): instant(service, item["due"]) for item in ref["items"]}
        actual = {}
        for row in rows:
            if not isinstance(row, dict):
                return "FAIL", "Une échéance est mal formée."
            if row.get("courseid" if brightspace else "course_id") != course_id:
                return "FAIL", "Une échéance n'est pas attribuable au cours demandé."
            kind = row.get("type" if brightspace else "kind")
            ident = row.get("id")
            if not brightspace and positive_int(ident):
                ident = str(ident)
            due = instant(service, row.get("due" if brightspace else "due_ms"))
            if not isinstance(kind, str) or not valid_id(service, ident) or due is None:
                return "FAIL", "Une échéance n'a pas de type, d'identifiant ou de date exploitable."
            key = (kind, ident)
            if key in actual:
                return "FAIL", "Une échéance est dupliquée."
            actual[key] = due
        if any(key not in expected or expected[key] != due for key, due in actual.items()):
            return "FAIL", "Échéance inattendue ou date incorrecte."
        partial = data.get("partial") is True or bool(data.get("errors"))
        if actual != expected or partial:
            if not partial:
                return "FAIL", "Échéances manquantes sans signalement de couverture incomplète."
            return "PARTIAL", "L'outil signale que sa couverture est incomplète."
        return "PASS", "Périmètre du cours et liste d'échéances conformes."

    fresh = ref and date.fromisoformat(ref["verified_utc_day"]) == datetime.now(timezone.utc).date()
    return Case(
        "« Quelles sont les échéances de ce cours uniquement ? »",
        f"{service}_deadlines", {**args, "days": ref["days"]} if ref else args,
        check if fresh else None,
        "Revérifier les échéances aujourd'hui (UTC)." if ref else "Pas de liste d'échéances de référence.",
    )


def run(binary: Path, account: dict | None, service: str, build_cases: Callable,
        *, surface: bool = False):
    path = os.environ.get(REFERENCE_ENV)
    ref = read_reference(Path(path), service) if account and path else None
    if ref and ref["account_id"] != account["id"]:
        raise ValueError(f"La référence {service} ne correspond pas au compte sélectionné.")
    env = dict(os.environ)
    if account and account.get("config") and account["config"] != harness.default_config().resolve():
        env["STUDENT_MCP_CONFIG_FILE"] = str(account["config"])
    client = harness.MCP(binary, env=env)
    if not surface and not ref:
        return [{
            "question": "Référence portail", "verdict": "SKIP",
            "evidence": "Pas de compte ou de référence indépendante.", "tool": "-", "bytes": 0, "seconds": 0,
        }]
    try:
        tools, _ = client.batch([])
    except (SystemExit, OSError, ValueError, subprocess.TimeoutExpired):
        raise SystemExit("Impossible de lire la liste des outils MCP.") from None
    definitions = {tool["name"]: tool for tool in tools}
    rows, calls, findings = [], [], []
    if not ref:
        findings.append({"severity": "info", "message": f"{service}: sans référence, aucune lecture scolaire."})
    for case in build_cases(ref) if ref else []:
        item = {"question": case.question, "tool": case.tool, "bytes": 0, "seconds": 0}
        spec = definitions.get(case.tool)
        if case.check is None:
            verdict, evidence = "SKIP", case.skip
        elif not spec or (spec.get("annotations") or {}).get("readOnlyHint") is not True:
            verdict, evidence = "FAIL", "Outil absent ou non déclaré en lecture seule ; aucun appel."
        else:
            args = dict(case.args)
            # Older binaries with a single implicit account do not expose this selector.
            if "account_id" in ((spec.get("inputSchema") or {}).get("properties") or {}) or account["id"] != service:
                args["account_id"] = account["id"]
            size = wire = seconds = 0
            seen = set()
            for _ in range(20):
                try:
                    response, elapsed = client.one(case.tool, args)
                except (SystemExit, OSError, ValueError, subprocess.TimeoutExpired):
                    response, elapsed = {"is_error": True}, 0
                seconds += elapsed
                text = response.get("text", "")
                size += len(text.encode("utf-8"))
                wire += response.get("wire_bytes", 0)
                data = harness.parse_mcp_json({"text": response.get("first_text", text)})
                if data is None:
                    data = response.get("structured")
                if response.get("is_error") or response.get("error") or response.get("missing"):
                    verdict, evidence = "FAIL", "L'appel MCP a échoué."
                elif not isinstance(data, (dict, list)):
                    verdict, evidence = "FAIL", "La réponse MCP ne contient pas de données JSON exploitables."
                else:
                    verdict, evidence = case.check(data, response)
                next_args = case.follow(data) if verdict == "PARTIAL" and case.follow else None
                if not next_args:
                    break
                signature = json.dumps(next_args, sort_keys=True)
                if signature in seen:
                    verdict, evidence = "FAIL", "La continuation répète une page."
                    break
                seen.add(signature)
                args.update(next_args)
            else:
                verdict, evidence = "PARTIAL", "Limite du benchmark : 20 pages consultées."
            item.update(bytes=size, wire_bytes=wire, seconds=round(seconds, 3))
            calls.append({
                "tool": case.tool, "ok": verdict == "PASS", "bytes": size, "tokens": size // 4,
                "wire_bytes": wire, "seconds": round(seconds, 3),
                "control_share": None if seen else harness.payload_detail(response.get("first_text", text)).get("control_share"),
                "paged": bool(seen) or isinstance(data, dict) and any(
                    data.get(key) is not None for key in ("nextPage", "nextPageFrom", "continuation")
                ),
            })
        rows.append({**item, "verdict": verdict, "evidence": evidence})
        if verdict in ("FAIL", "PARTIAL"):
            findings.append({
                "severity": "high" if verdict == "FAIL" else "medium",
                "message": f"{case.tool}: {evidence}",
            })
    if not surface:
        return rows
    return {
        "version": None, "tools": len(tools), "tools_list_bytes": len(json.dumps(tools)),
        "calls": calls, "findings": findings,
        "heaviest_descriptions": sorted(
            ((len(tool.get("description") or ""), tool["name"]) for tool in tools), reverse=True
        )[:5],
    }
