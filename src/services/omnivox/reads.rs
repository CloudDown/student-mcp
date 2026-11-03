//! Curated, read-only mobile endpoints. No caller-supplied paths or write actions.
//! Protocol reference: https://github.com/Beat-YT/omnivox-mcp/tree/main/docs/mobl
use super::{optional_course_id, require_course_id, split_class_id};
use serde_json::{json, Value};

pub(super) fn term_schema() -> Value {
    json!({"type":"string","pattern":"^[0-9]{5}$","description":"Academic term ID from omnivox_terms (YYYYN). Omit for the current term."})
}

fn text(description: &str, max: usize) -> Value {
    json!({"type":"string","minLength":1,"maxLength":max,"description":description})
}

fn integer(description: &str, min: i64, max: i64) -> Value {
    json!({"type":"integer","minimum":min,"maximum":max,"description":description})
}

fn course() -> Value {
    text("Course ID from omnivox_my_courses, e.g. 2434K5EM.1011.", 64)
}

fn tool(name: &str, description: &str, properties: Value, required: &[&str]) -> Value {
    json!({"name":format!("omnivox_{name}"),"description":description,
        "inputSchema":{"type":"object","properties":properties,"required":required}})
}

pub(super) fn tools() -> Vec<Value> {
    let term = term_schema();
    let folder = text(
        "Folder ID from omnivox_mio_folders. Defaults to SEARCH_FOLDER_MioRecu (inbox).",
        200,
    );
    let limit = integer("Maximum items in this page (default 20).", 1, 100);
    let timestamp = integer(
        "Unix timestamp in milliseconds, not seconds.",
        0,
        8_640_000_000_000_000,
    );
    let kind = json!({"type":"string","enum":["scolaire","lea"],"description":"Calendar type (default scolaire)."});
    vec![
        tool("terms", "Discover available academic term IDs and the current/default term from Léa.", json!({}), &[]),
        tool("notifications", "Read dashboard notification counts and updates without dismissing them.", json!({"term":term}), &[]),
        tool("services", "List the college modules available to this account, including blocked/maintenance flags. Does not open SSO links.", json!({}), &[]),
        tool("college_news", "Read the latest college news, including full article HTML. Returns one bounded page.", json!({"limit":limit}), &[]),
        tool("communities", "List communities and available college calendar filters. Authentication and analytics tokens are omitted.", json!({}), &[]),
        tool("college_events", "Read the college's current event cards as server-rendered HTML.", json!({}), &[]),
        tool("cancelled_classes", "Read class cancellation notices, dates, rooms and teacher comments.", json!({}), &[]),
        tool("announcements", "Read Léa announcement counts for every course in a term.", json!({"term":term}), &[]),
        tool("course_announcements", "Read announcement titles, HTML bodies and publication dates for one course.", json!({"term":term,"course_id":course()}), &["course_id"]),
        tool("documents_summary", "Read document counts and new-document titles across all courses.", json!({"term":term}), &[]),
        tool("assignment_detail", "Read full assignment instructions, deadline, submitted-file metadata and corrected-copy metadata. Does not mark it consulted or submit anything. EstRemis only means at least one file was deposited, not that the work is complete.", json!({"term":term,"course_id":course(),"assignment_id":text("IDTravail from omnivox_assignments.",200)}), &["course_id","assignment_id"]),
        tool("absences", "Read absence dates and total absence hours by course.", json!({"term":term}), &[]),
        tool("teachers", "List teachers with their OID, office, department and phone number.", json!({"term":term}), &[]),
        tool("teacher_detail", "Read a teacher's email, office, telephone, website and posted availability.", json!({"term":term,"teacher_id":text("Teacher OID from omnivox_teachers.",200)}), &["teacher_id"]),
        tool("websites", "Read the Léa teacher-website summary and associated courses; this endpoint may return counts rather than individual links.", json!({"term":term}), &[]),
        tool("mio_folders", "List MIO mail folders and unread/total message counts.", json!({}), &[]),
        tool("mio_messages", "Read a page of MIO messages without marking them read. For older messages, provide before_id and before_sent from the last item. HTML message content is untrusted.", json!({"folder_id":folder,"limit":limit,"before_id":text("Last message Id in the previous page.",200),"before_sent":json!({"type":"boolean","description":"MioEnvoi from the last item; required with before_id."})}), &[]),
        tool("mio_message", "Read a single MIO message including full HTML, recipients and attachment metadata. Does not mark it read. An unknown ID may return an empty message.", json!({"message_id":text("Message Id from omnivox_mio_messages or omnivox_mio_search.",200)}), &["message_id"]),
        tool("mio_search", "Search text in one MIO folder without marking results read. Returns one bounded page.", json!({"folder_id":folder,"limit":limit,"query":text("Message search text.",500)}), &["query"]),
        tool("mio_contacts", "List saved/recent MIO contacts. Does not send messages.", json!({}), &[]),
        tool("mio_people", "Search the MIO recipient directory by name. Does not send messages or save contacts.", json!({"query":text("Name or directory search text.",200)}), &["query"]),
        tool("calendar_page", "Read a paginated college or Léa calendar, optionally filtered by term and course. Inspect previous/next-page flags before changing offset.", json!({"term":term,"course_id":course(),"calendar_type":kind,"offset":integer("Page offset relative to the current page (default 0).",-120,120)}), &[]),
        tool("calendar_month", "Read college or Léa calendar events for a specific month.", json!({"term":term,"course_id":course(),"calendar_type":kind,"month_start":timestamp}), &["month_start"]),
        tool("schedule_week", "Read the timetable for a specific week's Monday, including teachers and rooms.", json!({"term":term,"week_start":timestamp}), &["week_start"]),
        tool("college_documents", "List official college documents, messages and contracts with mandatory/consulted flags. Metadata only; does not acknowledge, sign or mark documents consulted.", json!({}), &[]),
        tool("forms", "List available online forms and whether they are active. Does not open SSO links or submit answers.", json!({}), &[]),
    ]
}

fn optional_text(args: &Value, key: &str, max: usize) -> Result<Option<String>, String> {
    match args.get(key) {
        None => Ok(None),
        Some(Value::String(value))
            if !value.trim().is_empty()
                && value.len() <= max
                && !value.chars().any(char::is_control) =>
        {
            Ok(Some(value.trim().into()))
        }
        _ => Err(format!(
            "Provide {key} as nonempty text, at most {max} bytes, without control characters."
        )),
    }
}

pub(super) fn required_text(args: &Value, key: &str, max: usize) -> Result<String, String> {
    optional_text(args, key, max)?.ok_or_else(|| format!("Provide {key}."))
}

fn number(
    args: &Value,
    key: &str,
    default: Option<i64>,
    min: i64,
    max: i64,
) -> Result<i64, String> {
    let value = match args.get(key) {
        None => default,
        Some(value) => value.as_i64(),
    };
    value
        .filter(|n| (min..=max).contains(n))
        .ok_or_else(|| format!("Provide integer {key} between {min} and {max}."))
}

pub(super) fn term(args: &Value) -> Result<String, String> {
    let term = optional_text(args, "term", 5)?.unwrap_or_default();
    if !term.is_empty() && (term.len() != 5 || !term.bytes().all(|c| c.is_ascii_digit())) {
        return Err("Provide term as a five-digit academic term ID from omnivox_terms.".into());
    }
    Ok(term)
}

fn class_body(args: &Value, term: &str) -> Result<Value, String> {
    let (course, group) = split_class_id(&require_course_id(args)?)?;
    Ok(json!({"AnSession":term,"noCours":course,"noGroupe":group}))
}

/// Existing tools retain their gateway tool mapping unless an explicit term is
/// requested. Extended tools use the gateway's documented /Mobl/ proxy as well
/// as direct browser/mobile sessions; no unimplemented gateway tool names.
pub(super) fn request(name: &str, args: &Value) -> Result<Option<(&'static str, Value)>, String> {
    let legacy = matches!(
        name,
        "omnivox_my_courses"
            | "omnivox_course_documents"
            | "omnivox_assignments"
            | "omnivox_grades"
            | "omnivox_schedule"
    );
    if legacy && args.get("term").is_none() {
        return Ok(None);
    }
    // Do not validate unrelated tools' arguments here.
    if !legacy && !tools().iter().any(|tool| tool["name"] == name) {
        return Ok(None);
    }
    let term = term(args)?;
    let upper = json!({"AnSession":term});
    let lower = json!({"anSession":term});
    let (path, body) = match name {
        "omnivox_terms" | "omnivox_my_courses" => ("LeaCommun/GetDefaultModel", upper),
        "omnivox_course_documents" => (
            "LeaEtudiant/GetDocumentsListeModel",
            json!({"AnSession":term,"idClasse":require_course_id(args)?}),
        ),
        "omnivox_assignments" => match optional_course_id(args)? {
            Some(id) => (
                "LeaEtudiant/GetTravauxListeModel",
                json!({"AnSession":term,"idClasse":id}),
            ),
            None => ("LeaEtudiant/GetTravauxSommaireModel", upper),
        },
        "omnivox_grades" => match optional_course_id(args)? {
            Some(_) => ("LeaEtudiant/GetNotesDetailModel", class_body(args, &term)?),
            None => ("LeaEtudiant/GetNotesSommaireModel", upper),
        },
        "omnivox_schedule" => ("Horaire/GetHoraireModel", upper),
        "omnivox_notifications" => (
            "App/Notification",
            json!({"anSession":term,"isFirstLoadQD9":true}),
        ),
        "omnivox_services" => (
            "App/GetOffreService",
            json!({"DerniereOuverture":0,"DernierHash":"","Force":true,"EnvoieStatistique":false,"HasPreviousOffreService":false}),
        ),
        "omnivox_college_news" => (
            "College/ListeActualite",
            json!({"NombreItem":number(args,"limit",Some(20),1,100)?,"ReferenceNouvelle":Value::Null}),
        ),
        "omnivox_communities" => (
            "College/GetInfoNouvellesAffichees",
            json!({"DateHeureModificationData":0,"HasCalendrier":false,"HasCommunaute":false,"loadCours":false,"force":true}),
        ),
        "omnivox_college_events" => ("College/EvenementsWeb_GetFirstLoad", json!({})),
        "omnivox_cancelled_classes" => ("CoursAnnule/GetCoursAnnuleModel", json!({})),
        "omnivox_announcements" => ("LeaEtudiant/GetCommuniquesSommaireModel", upper),
        "omnivox_course_announcements" => (
            "LeaEtudiant/GetCommuniquesListeModel",
            class_body(args, &term)?,
        ),
        "omnivox_documents_summary" => ("LeaEtudiant/GetDocumentsSommaireModel", upper),
        "omnivox_assignment_detail" => (
            "LeaEtudiant/GetTravauxDetailModel",
            json!({"AnSession":term,"idClasse":require_course_id(args)?,"idTravail":required_text(args,"assignment_id",200)?,"isAddConsultation":false}),
        ),
        "omnivox_absences" => ("LeaEtudiant/GetAbsencesSommaireModel", lower),
        "omnivox_teachers" => ("LeaEtudiant/GetEnseignantsSommaireModel", lower),
        "omnivox_teacher_detail" => (
            "LeaEtudiant/GetEnseignantsDetailModel",
            json!({"anSession":term,"oidProfesseur":required_text(args,"teacher_id",200)?}),
        ),
        "omnivox_websites" => ("LeaEtudiant/GetSitesWebSommaireModel", lower),
        "omnivox_mio_folders" => ("Mio/GetListeFoldersModel", json!({})),
        "omnivox_mio_messages" => {
            let folder = optional_text(args, "folder_id", 200)?
                .unwrap_or_else(|| "SEARCH_FOLDER_MioRecu".into());
            let limit = number(args, "limit", Some(20), 1, 100)?;
            if let Some(before) = optional_text(args, "before_id", 200)? {
                let sent = args.get("before_sent").and_then(Value::as_bool).ok_or(
                    "Provide boolean before_sent (MioEnvoi from the last message) with before_id.",
                )?;
                (
                    "Mio/GetMessages",
                    json!({"IdFolder":folder,"DernierOidMessage":before,"IndicateurDernierMioEnvoi":sent,"NbMessages":limit}),
                )
            } else {
                if args.get("before_sent").is_some() {
                    return Err("before_sent requires before_id.".into());
                }
                (
                    "Mio/GetLatestMessages",
                    json!({"IdFolder":folder,"NbMessages":limit,"DateHeureDerniereUpdate":0,"NombreItemDernierUpdate":0,"Force":true,"UtilisationQuota":0}),
                )
            }
        }
        "omnivox_mio_message" => (
            "Mio/GetMessage",
            json!({"IdMessage":required_text(args,"message_id",200)?}),
        ),
        "omnivox_mio_search" => (
            "Mio/SearchMessages",
            json!({"IdFolder":optional_text(args,"folder_id",200)?.unwrap_or_else(|| "SEARCH_FOLDER_MioRecu".into()),"NbMessages":number(args,"limit",Some(20),1,100)?,"SearchText":required_text(args,"query",500)?}),
        ),
        "omnivox_mio_contacts" => ("Mio/GetListeContacts", json!({})),
        "omnivox_mio_people" => (
            "Mio/RechercheIndividu",
            json!({"TexteRecherche":required_text(args,"query",200)?}),
        ),
        "omnivox_calendar_page" | "omnivox_calendar_month" => {
            let kind =
                optional_text(args, "calendar_type", 20)?.unwrap_or_else(|| "scolaire".into());
            if !matches!(kind.as_str(), "scolaire" | "lea") {
                return Err("Provide calendar_type as scolaire or lea.".into());
            }
            let mut body = json!({"anSession":term,"coursGroupe":optional_course_id(args)?.unwrap_or_default(),"typeCalendrier":kind,"filtre":"choix_tous"});
            if name == "omnivox_calendar_month" {
                body["moisCourant"] =
                    json!(number(args, "month_start", None, 0, 8_640_000_000_000_000)?);
                ("Calendrier/GetCalendrierMoisModel", body)
            } else {
                body["decalagePagination"] = json!(number(args, "offset", Some(0), -120, 120)?);
                ("Calendrier/GetCalendrierModel", body)
            }
        }
        "omnivox_schedule_week" => (
            "Horaire/GetHoraireSemaineModel",
            json!({"AnSession":term,"TimeStampDebutSemaine":number(args,"week_start",None,0,8_640_000_000_000_000)?}),
        ),
        "omnivox_college_documents" => ("DocMessageCollege/GetDefaultModel", json!({})),
        "omnivox_forms" => ("Formulaire/GetFormulaireModel", json!({})),
        _ => return Ok(None),
    };
    Ok(Some((path, body)))
}

/// Some read models also carry login/analytics credentials or preauthenticated
/// portal handoffs. They are unnecessary to answer student questions.
pub(super) fn without_credentials(mut value: Value) -> Value {
    fn clean(value: &mut Value) {
        match value {
            Value::Object(map) => {
                map.retain(|key, _| {
                    !matches!(
                        key.to_ascii_lowercase().as_str(),
                        "authhash"
                            | "jsonwebtoken"
                            | "infosautologincvir"
                            | "sid"
                            | "tksencrypte"
                            | "nomcookietks"
                            | "tokenredirection"
                            | "logintokenauthentification"
                            | "urlservice"
                            | "infoclassedistance"
                    )
                });
                for value in map.values_mut() {
                    clean(value);
                }
            }
            Value::Array(items) => {
                for item in items {
                    clean(item);
                }
            }
            _ => {}
        }
    }
    clean(&mut value);
    value
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_extended_tool_has_a_fixed_read_endpoint_and_valid_schema() {
        let args = json!({"term":"20263","course_id":"2434K5EM.1011","assignment_id":"work","teacher_id":"teacher","message_id":"message","query":"exam","month_start":1793480400000_i64,"week_start":1793570400000_i64});
        let mut names = std::collections::HashSet::new();
        for tool in tools() {
            let name = tool["name"].as_str().unwrap();
            assert!(names.insert(name.to_string()), "duplicate {name}");
            assert_eq!(tool["inputSchema"]["type"], "object");
            for required in tool["inputSchema"]["required"].as_array().unwrap() {
                assert!(tool["inputSchema"]["properties"]
                    .get(required.as_str().unwrap())
                    .is_some());
                let mut missing = args.clone();
                missing
                    .as_object_mut()
                    .unwrap()
                    .remove(required.as_str().unwrap());
                assert!(
                    request(name, &missing).is_err(),
                    "{name} must require {required}"
                );
            }
            let (path, _) = request(name, &args).unwrap().unwrap();
            assert!(
                path.starts_with("Mio/Get")
                    || path == "Mio/SearchMessages"
                    || path == "Mio/RechercheIndividu"
                    || path.contains("/Get")
                    || path == "App/Notification"
                    || path == "College/ListeActualite"
                    || path == "College/EvenementsWeb_GetFirstLoad",
                "unexpected endpoint {path}"
            );
        }
        assert_eq!(names.len(), 26);
    }

    #[test]
    fn pagination_and_read_only_flags_match_mobile_contract() {
        let (_, body) = request(
            "omnivox_assignment_detail",
            &json!({"course_id":"course.group","assignment_id":"work"}),
        )
        .unwrap()
        .unwrap();
        assert_eq!(body["isAddConsultation"], false);
        let (path, body) = request(
            "omnivox_mio_messages",
            &json!({"before_id":"last","before_sent":false,"limit":7}),
        )
        .unwrap()
        .unwrap();
        assert_eq!(path, "Mio/GetMessages");
        assert_eq!(body["IndicateurDernierMioEnvoi"], false);
        assert_eq!(body["NbMessages"], 7);
        assert!(request("omnivox_mio_messages", &json!({"before_id":"last"})).is_err());
        assert!(request("omnivox_mio_messages", &json!({"before_sent":false})).is_err());
        let (_, body) = request(
            "omnivox_course_announcements",
            &json!({"course_id":"course.group","term":"20263"}),
        )
        .unwrap()
        .unwrap();
        assert_eq!(
            body,
            json!({"AnSession":"20263","noCours":"course","noGroupe":"group"})
        );
    }

    #[test]
    fn malformed_arguments_never_reach_the_network() {
        for (name, args) in [
            ("omnivox_absences", json!({"term":20263})),
            ("omnivox_absences", json!({"term":"abcd1"})),
            ("omnivox_absences", json!({"term":"202633"})),
            ("omnivox_mio_search", json!({"query":" "})),
            ("omnivox_mio_messages", json!({"limit":0})),
            ("omnivox_mio_messages", json!({"limit":101})),
            ("omnivox_mio_messages", json!({"limit":"20"})),
            ("omnivox_schedule_week", json!({"week_start":-1})),
            ("omnivox_calendar_page", json!({"offset":1.5})),
            ("omnivox_calendar_page", json!({"calendar_type":"unknown"})),
            (
                "omnivox_course_announcements",
                json!({"course_id":"../secret"}),
            ),
        ] {
            assert!(
                request(name, &args).is_err(),
                "accepted invalid arguments for {name}: {args}"
            );
        }
        assert!(request("omnivox_SendMessage", &json!({}))
            .unwrap()
            .is_none());
    }

    #[test]
    fn explicit_terms_extend_existing_tools_without_breaking_gateway_defaults() {
        assert!(request("omnivox_assignments", &json!({}))
            .unwrap()
            .is_none());
        let (path, body) = request(
            "omnivox_grades",
            &json!({"term":"20263","course_id":"course.group"}),
        )
        .unwrap()
        .unwrap();
        assert_eq!(path, "LeaEtudiant/GetNotesDetailModel");
        assert_eq!(body["AnSession"], "20263");
        assert_eq!(body["noCours"], "course");
    }

    #[test]
    fn nested_authentication_material_is_not_returned() {
        let value = without_credentials(
            json!({"Travail":{"Titre":"Exam","InfosAutoLoginCvir":{"SID":"secret"}},"items":[{"AuthHash":"secret","JsonWebToken":"secret","UrlService":"sso-secret","Nom":"Forms"}]}),
        );
        assert_eq!(
            value,
            json!({"Travail":{"Titre":"Exam"},"items":[{"Nom":"Forms"}]})
        );
    }
}
