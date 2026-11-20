"""Brightspace student workflows, graded against an official-portal reference."""
from . import portal

NAME = "brightspace"


def cases(ref: dict) -> list[portal.Case]:
    course = ref["course"]
    document = ref.get("document")
    assignment = ref.get("assignment")
    grade = ref.get("grade")
    args = {"courseid": course["id"]}

    def next_page(data):
        paging = data.get("PagingInfo") if isinstance(data, dict) else None
        if isinstance(paging, dict) and paging.get("HasMoreItems") is True and portal.nonblank(paging.get("Bookmark")):
            return {"bookmark": paging["Bookmark"]}
        return None

    def course_check(data, _result):
        rows = data.get("Items") if isinstance(data, dict) else None
        if not isinstance(rows, list):
            return "FAIL", "Liste d'inscriptions absente ou mal formée."
        for row in rows:
            unit = row.get("OrgUnit") if isinstance(row, dict) else None
            if isinstance(unit, dict) and unit.get("Id") == course["id"]:
                return portal.found(unit.get("Name") == course["title"], "Nom du cours")
        if next_page(data):
            return "PARTIAL", "Cours absent de cette page ; une suite est annoncée."
        return "FAIL", "Cours introuvable sans continuation exploitable."

    def content_check(data, _result):
        topics = (
            topic for row in portal.objects(data) if isinstance(row.get("Topics"), list)
            for topic in row["Topics"] if isinstance(topic, dict)
        )
        return portal.document_check(topics, document, "Id", ("Title",))

    def search_check(data, _result):
        matches = data.get("matches") if isinstance(data, dict) else None
        if not isinstance(matches, list):
            return "FAIL", "Résultats de recherche absents ou mal formés."
        verdict = portal.document_check((row for row in matches if isinstance(row, dict)), document, "id", ("title",))
        if verdict[0] == "FAIL" and data.get("truncated") is True:
            return "PARTIAL", "Document absent des résultats livrés ; la recherche signale une troncature."
        return verdict

    def instructions(data, _result):
        folder = data.get("folder") if isinstance(data, dict) else None
        if not isinstance(folder, dict) or folder.get("id") != assignment["id"]:
            return "FAIL", "La réponse ne correspond pas au devoir demandé."
        return portal.instructions_check([folder.get("instructions")], assignment["instructions"])

    return [
        portal.Case("« Ce cours est-il dans mes inscriptions ? »", "brightspace_my_courses", {}, course_check,
                    follow=next_page),
        portal.Case("« Où est ce document dans le contenu du cours ? »", "brightspace_course_content",
                    args, content_check if document else None),
        portal.Case("« Retrouve ce document par son titre. »", "brightspace_search",
                    {**args, "query": document.get("query", document["title"])} if document else args,
                    search_check if document else None),
        portal.pdf_case("brightspace_file", {**args, "topicid": document["id"]} if document else args, document),
        portal.Case("« Quelles sont les consignes de ce devoir ? »", "brightspace_assignment_status",
                    {**args, "folderid": assignment["id"]} if assignment else args,
                    instructions if assignment else None),
        portal.Case("« Quelle est ma note à cette évaluation ? »", "brightspace_grades", args,
                    (lambda data, _: portal.grade_check(data, grade)) if grade else None),
        portal.deadlines_case(NAME, {"courseids": [course["id"]]}, course["id"], ref.get("deadlines")),
    ]


def questions(binary, account):
    return portal.run(binary, account, NAME, cases)


def surface(binary, account):
    return portal.run(binary, account, NAME, cases, surface=True)
