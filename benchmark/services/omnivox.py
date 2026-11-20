"""Omnivox / Léa student workflows, graded against an official-portal reference."""
from . import portal

NAME = "omnivox"


def cases(ref: dict) -> list[portal.Case]:
    course = ref["course"]
    document = ref.get("document")
    assignment = ref.get("assignment")
    grade = ref.get("grade")
    args = {"course_id": course["id"]}

    def course_check(data, _result):
        if isinstance(data, dict):
            rows = data.get("ListeCours", data.get("courses"))
            if isinstance(rows, list):
                for row in rows:
                    if not isinstance(row, dict):
                        continue
                    ident = row.get("id") or row.get("course_id")
                    if not ident and isinstance(row.get("NoCours"), str) and isinstance(row.get("NoGroupe"), str):
                        ident = f"{row['NoCours'].strip()}.{row['NoGroupe'].strip()}"
                    if ident == course["id"]:
                        title = row.get("NomCours") or row.get("Titre") or row.get("name")
                        return portal.found(title == course["title"], "Nom du cours")
        return "FAIL", "Cours introuvable dans les inscriptions."

    def document_check(data, _result):
        title = document.get("title_field")
        return portal.document_check(
            portal.objects(data), document, "IdDocCoursDocument",
            (title,) if title else ("Titre", "NomFichier", "NomFichierOriginal", "NomDocument"),
        )

    def instructions(data, _result):
        field = assignment["instructions_field"]
        return portal.instructions_check((row.get(field) for row in portal.objects(data)), assignment["instructions"])

    return [
        portal.Case("« Ce cours est-il dans mes inscriptions ? »", "omnivox_my_courses", {}, course_check),
        portal.Case("« Où est ce document dans le contenu du cours ? »", "omnivox_course_documents",
                    args, document_check if document else None),
        portal.pdf_case("omnivox_file", {**args, "file_id": document["id"]} if document else args, document),
        portal.Case("« Quelles sont les consignes de ce devoir ? »", "omnivox_assignment_detail",
                    {**args, "assignment_id": assignment["id"]} if assignment else args,
                    instructions if assignment else None),
        portal.Case("« Quelle est ma note à cette évaluation ? »", "omnivox_grades", args,
                    (lambda data, _: portal.grade_check(data, grade)) if grade else None),
        portal.deadlines_case(NAME, {"course_ids": [course["id"]]}, course["id"], ref.get("deadlines")),
    ]


def questions(binary, account):
    return portal.run(binary, account, NAME, cases)


def surface(binary, account):
    return portal.run(binary, account, NAME, cases, surface=True)
