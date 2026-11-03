//! Explicit, single-attempt writes. Confirmation policy belongs to the MCP
//! client; annotations are metadata, not an authorization enforcement mechanism.
use super::{reads, require_course_id, OmnivoxClient};
use serde_json::{json, Value};

fn text(max: usize, description: &str) -> Value {
    json!({"type":"string","minLength":1,"maxLength":max,"description":description})
}

fn guid_schema(description: &str) -> Value {
    json!({"type":"string","pattern":"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$","description":description})
}

fn tool(name: &str, description: &str, properties: Value, required: &[&str]) -> Value {
    json!({"name":format!("omnivox_{name}"),"description":format!("{description} Student write. Client approval depends on client settings. Never retry automatically after an uncertain outcome."),
        "inputSchema":{"type":"object","properties":properties,"required":required},
        "annotations":{"readOnlyHint":false,"destructiveHint":true,"idempotentHint":false,"openWorldHint":true}})
}

pub(super) fn tools() -> Vec<Value> {
    let message = guid_schema(
        "MIO message Id from omnivox_mio_messages, omnivox_mio_search or omnivox_mio_message.",
    );
    let sent = json!({"type":"boolean","description":"MioEnvoi from the selected message. Required: do not guess whether this is the sent or received copy."});
    let category = json!({"type":"string","maxLength":200,"description":"Custom category NAME (NomCategorie), not a SEARCH_FOLDER_* system ID. Empty string removes categorization (returns to the normal mailbox)."});
    let category_id = json!({"type":"integer","minimum":1,"maximum":2147483647,"description":"Numeric IdCategorie of a CUSTOM MIO category, from omnivox_mio_folders. System folders cannot be renamed/deleted."});
    vec![
        tool("mio_send", "Send a plain-text MIO, reply, reply-all or forward to explicit recipient OIDs. Use mio_people/contacts/message to select them. Reply/forward requires reply_to_id; recipients, subject and body are always explicit, with no automatic quoting, signature or extra recipients. No attachments in this version.", json!({
            "recipient_ids":{"type":"array","items":guid_schema("Recipient OID."),"minItems":1,"maxItems":100,"uniqueItems":true},
            "subject":text(500,"Subject as plain text; include Re:/Fw: yourself when appropriate."),
            "message":text(50000,"Plain-text message, HTML-escaped before sending. Encoded HTML must fit the site's 50,000-character limit."),
            "action":{"type":"string","enum":["new","reply","reply_all","forward"],"default":"new"},
            "reply_to_id":message,
            "hide_recipients":{"type":"boolean","default":false,"description":"Hide recipients from one another (CacheDestinataire)."}
        }), &["recipient_ids","subject","message"]),
        tool("mio_move", "Categorize one MIO message, or remove its custom categorization with an empty category. This is not the trash action.", json!({"message_id":message,"category":category,"is_sent":sent}), &["message_id","category","is_sent"]),
        tool("mio_flags", "Set BOTH the starred flag and the re-read reminder for one message. Supply both values to preserve the intended state; reread is not the same as changing the Unread property.", json!({"message_id":message,"is_sent":sent,"flagged":{"type":"boolean"},"reread":{"type":"boolean"}}), &["message_id","is_sent","flagged","reread"]),
        tool("mio_delete", "Move one MIO message to the trash. Does NOT permanently delete it.", json!({"message_id":message,"is_sent":sent}), &["message_id","is_sent"]),
        tool("mio_delete_permanently", "PERMANENTLY DELETE one MIO message. Irreversible; this is distinct from moving to trash.", json!({"message_id":message,"is_sent":sent}), &["message_id","is_sent"]),
        tool("mio_restore", "Restore one MIO message from the trash.", json!({"message_id":message,"is_sent":sent}), &["message_id","is_sent"]),
        tool("mio_mark_read", "Mark one MIO message as read. The official client also clears its re-read indicator. Does not support marking Unread=true.", json!({"message_id":message,"is_sent":sent}), &["message_id","is_sent"]),
        tool("mio_folder_create", "Create a custom MIO category/folder.", json!({"name":text(200,"New custom category name, not a SEARCH_FOLDER_* system ID.")}), &["name"]),
        tool("mio_folder_rename", "Rename a custom MIO category/folder.", json!({"category_id":category_id,"name":text(200,"New category name.")}), &["category_id","name"]),
        tool("mio_folder_delete", "Delete a custom MIO category/folder. Explicitly choose transfer_category for affected messages; an empty string selects no custom category. Never use this to delete system folders.", json!({"category_id":category_id,"transfer_category":category}), &["category_id","transfer_category"]),
        tool("notification_dismiss", "Dismiss one dismissible dashboard item. Copy service_id and item_ids from a single omnivox_notifications entry (IdService and ListeIDQuoiDeNeuf).", json!({"service_id":text(200,"IdService from the selected notification."),"item_ids":text(4000,"Exact ListeIDQuoiDeNeuf string from that notification, not an invented list.")}), &["service_id","item_ids"]),
        tool("assignment_mark_consulted", "Mark an assignment consulted without depositing a file. This only records consultation; it does NOT submit or complete the assignment.", json!({"course_id":text(64,"Course ID from omnivox_my_courses."),"assignment_id":guid_schema("IDTravail from omnivox_assignments."),"term":reads::term_schema()}), &["course_id","assignment_id"]),
    ]
}

fn guid(args: &Value, key: &str) -> Result<String, String> {
    let value = reads::required_text(args, key, 36)?;
    let valid = value.len() == 36
        && value.bytes().enumerate().all(|(i, c)| {
            if [8, 13, 18, 23].contains(&i) {
                c == b'-'
            } else {
                c.is_ascii_hexdigit()
            }
        });
    if !valid {
        return Err(format!("Provide {key} as the GUID returned by Omnivox."));
    }
    Ok(value)
}

fn boolean(args: &Value, key: &str, default: Option<bool>) -> Result<bool, String> {
    match args.get(key) {
        None => default.ok_or_else(|| format!("Provide boolean {key}.")),
        Some(value) => value
            .as_bool()
            .ok_or_else(|| format!("Provide boolean {key}.")),
    }
}

fn category(args: &Value, key: &str) -> Result<String, String> {
    let value = args
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("Provide {key} as a category name, or an empty string for none."))?;
    if value.chars().count() > 200
        || value.chars().any(char::is_control)
        || value.trim().starts_with("SEARCH_FOLDER_")
    {
        return Err(format!(
            "Provide {key} as a custom category name, not a system folder ID."
        ));
    }
    Ok(value.trim().into())
}

fn category_id(args: &Value) -> Result<i64, String> {
    args.get("category_id")
        .and_then(Value::as_i64)
        .filter(|id| (1..=i32::MAX as i64).contains(id))
        .ok_or_else(|| "Provide a positive integer category_id for a custom MIO category.".into())
}

fn escape_html(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
        .replace("\r\n", "\n")
        .replace('\r', "\n")
        .replace('\n', "<br />")
}

fn message_text(args: &Value) -> Result<String, String> {
    // Unlike IDs, message text retains whitespace/newlines; never trim the body.
    let text = args
        .get("message")
        .and_then(Value::as_str)
        .ok_or("Provide message as plain text.")?;
    if text.trim().is_empty()
        || text.chars().count() > 50000
        || text
            .chars()
            .any(|c| c.is_control() && !matches!(c, '\r' | '\n' | '\t'))
    {
        return Err("Provide a nonempty plain-text message of at most 50,000 characters.".into());
    }
    let html = escape_html(text);
    if html.chars().count() > 50000 {
        return Err(
            "The HTML-encoded message exceeds Omnivox's 50,000-character limit. Shorten it.".into(),
        );
    }
    Ok(html)
}

struct Write {
    path: &'static str,
    body: Value,
    term: Option<String>,
}

fn request(name: &str, args: &Value) -> Result<Option<Write>, String> {
    let (path, body) = match name {
        "omnivox_mio_send" => {
            let recipients = args
                .get("recipient_ids")
                .and_then(Value::as_array)
                .filter(|ids| !ids.is_empty() && ids.len() <= 100)
                .ok_or("Provide recipient_ids as an array of 1 to 100 recipient OIDs.")?;
            let mut ids = Vec::new();
            let mut seen = std::collections::HashSet::new();
            for value in recipients {
                let id = guid(&json!({"recipient":value}), "recipient")?;
                if !seen.insert(id.to_ascii_lowercase()) {
                    return Err("recipient_ids contains a duplicate OID.".into());
                }
                ids.push(id);
            }
            let action = match args.get("action") {
                None => "new",
                Some(Value::String(action)) => action.as_str(),
                _ => return Err("Provide action as new, reply, reply_all or forward.".into()),
            };
            let (code, reply) = match action {
                "new" => {
                    if args.get("reply_to_id").is_some() {
                        return Err(
                            "reply_to_id requires action=reply, reply_all or forward.".into()
                        );
                    }
                    ("0", "undefined".into())
                }
                "reply" => ("1", guid(args, "reply_to_id")?),
                "forward" => ("2", guid(args, "reply_to_id")?),
                "reply_all" => ("3", guid(args, "reply_to_id")?),
                _ => return Err("Provide action as new, reply, reply_all or forward.".into()),
            };
            let subject = escape_html(&reads::required_text(args, "subject", 500)?);
            (
                "Mio/SendMessage",
                json!({"IDMessage":Value::Null,"To":ids.join(","),"Sujet":subject,"Message":message_text(args)?,"Attachements":[],"IDMessageReply":reply,"derniereAction":code,"CacheDestinataire":boolean(args,"hide_recipients",Some(false))?}),
            )
        }
        "omnivox_mio_move" => (
            "Mio/CategoriseMessage",
            json!({"oidMessage":guid(args,"message_id")?,"nomCategorie":category(args,"category")?,"isEnvoyeur":boolean(args,"is_sent",None)?}),
        ),
        "omnivox_mio_flags" => (
            "Mio/SetIndicateursMessage",
            json!({"Id":guid(args,"message_id")?,"indicateurs":{"Drapeau":boolean(args,"flagged",None)?,"ARelire":boolean(args,"reread",None)?},"IsEnvoi":boolean(args,"is_sent",None)?}),
        ),
        "omnivox_mio_delete" | "omnivox_mio_delete_permanently" => (
            "Mio/DeleteMessage",
            json!({"Id":guid(args,"message_id")?,"Envoie":boolean(args,"is_sent",None)?,"SupprimerPermanent":name=="omnivox_mio_delete_permanently"}),
        ),
        "omnivox_mio_restore" => (
            "Mio/RestoreMessage",
            json!({"Id":guid(args,"message_id")?,"Envoie":boolean(args,"is_sent",None)?}),
        ),
        "omnivox_mio_mark_read" => (
            "Mio/SetMessageLu",
            json!({"Id":guid(args,"message_id")?,"IsEnvoi":boolean(args,"is_sent",None)?}),
        ),
        "omnivox_mio_folder_create" | "omnivox_mio_folder_rename" => {
            let name_value = category(args, "name")?;
            if name_value.is_empty() {
                return Err("Provide a nonempty custom category name.".into());
            }
            if name == "omnivox_mio_folder_create" {
                ("Mio/AjoutCategorie", json!({"nomCategorie":name_value}))
            } else {
                (
                    "Mio/RenommerCategorie",
                    json!({"idCategorie":category_id(args)?,"nouveauNomCategorie":name_value}),
                )
            }
        }
        "omnivox_mio_folder_delete" => (
            "Mio/SupprimerCategorie",
            json!({"idCategorie":category_id(args)?,"categorieTransfer":category(args,"transfer_category")?}),
        ),
        "omnivox_notification_dismiss" => (
            "App/AddItemCache",
            json!({"idService":reads::required_text(args,"service_id",200)?,"listeIDQuoiDeNeuf":reads::required_text(args,"item_ids",4000)?}),
        ),
        "omnivox_assignment_mark_consulted" => {
            return Ok(Some(Write {
                path: "LeaEtudiant/GetTravauxDetailModel",
                body: json!({"AnSession":"","idClasse":require_course_id(args)?,"idTravail":guid(args,"assignment_id")?,"isAddConsultation":true}),
                term: Some(reads::term(args)?),
            }))
        }
        _ => return Ok(None),
    };
    Ok(Some(Write {
        path,
        body,
        term: None,
    }))
}

fn confirmed(name: &str, body: &Value, response: Value) -> Result<Value, String> {
    // Never treat an arbitrary truthy object (e.g. {Error: ...}) as success.
    let has_error = response
        .get("NaviguerPage")
        .and_then(Value::as_str)
        .is_some_and(|s| !s.is_empty())
        || response.get("IsPeriodeArret").and_then(Value::as_bool) == Some(true)
        || response.get("IsAvailable").and_then(Value::as_bool) == Some(false)
        || crate::services::write_response::explicit_failure(&response);
    if has_error {
        return Err("Omnivox returned an error or blocked this write. Check its state before retrying; no automatic retry was made.".into());
    }
    let accepted = response == Value::Bool(true)
        || response == json!(1)
        || response
            .as_str()
            .is_some_and(|s| matches!(s.trim().to_ascii_lowercase().as_str(), "ok" | "true"))
        || ["success", "Success"]
            .iter()
            .any(|key| response.get(key).and_then(Value::as_bool) == Some(true));
    let consulted = name == "omnivox_assignment_mark_consulted"
        && response
            .pointer("/Travail/IDTravail")
            .and_then(Value::as_str)
            .zip(body.get("idTravail").and_then(Value::as_str))
            .is_some_and(|(actual, expected)| actual.eq_ignore_ascii_case(expected));
    if !accepted && !consulted {
        return Err("Omnivox did not explicitly confirm the write. It may already have been applied: inspect the message, folder, notification or assignment before deciding whether to retry. No automatic retry was made.".into());
    }
    Ok(json!({"operation":name,"confirmed":true,"response":response}))
}

pub(super) fn dispatch(
    client: &OmnivoxClient,
    name: &str,
    args: &Value,
) -> Option<Result<Value, String>> {
    let mut request = match request(name, args) {
        Ok(Some(request)) => request,
        Ok(None) => return None,
        Err(error) => return Some(Err(error)),
    };
    Some((|| {
        if let Some(term) = &request.term {
            request.body["AnSession"] = json!(client.resolve_term(term)?);
        }
        let value = client.mobl_write(request.path, request.body.clone())?;
        confirmed(name, &request.body, value)
    })())
}

#[cfg(test)]
mod tests {
    use super::*;
    const MESSAGE: &str = "11111111-2222-3333-4444-555555555555";
    const RECIPIENT: &str = "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee";
    type Captured = Vec<(String, Value, String)>;

    fn args() -> Value {
        json!({"recipient_ids":[RECIPIENT],"subject":"A < B & C","message":"  Hello <script>\n& goodbye  ",
            "message_id":MESSAGE,"assignment_id":MESSAGE,"course_id":"course.group","term":"20263",
            "category":"Projects","name":"Projects","category_id":7,"transfer_category":"",
            "is_sent":false,"flagged":true,"reread":false,"service_id":"mio","item_ids":"123"})
    }

    #[test]
    fn writes_have_explicit_parameters_and_client_policy_annotations() {
        let mut names = std::collections::HashSet::new();
        for tool in tools() {
            let name = tool["name"].as_str().unwrap();
            assert!(names.insert(name.to_string()));
            assert_eq!(tool["annotations"]["readOnlyHint"], false);
            assert_eq!(tool["annotations"]["destructiveHint"], true);
            assert_eq!(tool["annotations"]["idempotentHint"], false);
            assert!(request(name, &args()).unwrap().is_some());
            for key in tool["inputSchema"]["required"].as_array().unwrap() {
                let mut missing = args();
                missing
                    .as_object_mut()
                    .unwrap()
                    .remove(key.as_str().unwrap());
                assert!(
                    request(name, &missing).is_err(),
                    "{name} must require {key}"
                );
            }
        }
        assert_eq!(names.len(), 12);
        assert_eq!(super::super::tools().len(), 46);
    }

    #[test]
    fn message_sending_escapes_html_and_preserves_explicit_recipients_and_reply_mode() {
        let request = request("omnivox_mio_send", &args()).unwrap().unwrap();
        assert_eq!(request.path, "Mio/SendMessage");
        assert_eq!(request.body["To"], RECIPIENT);
        assert_eq!(request.body["Sujet"], "A &lt; B &amp; C");
        assert_eq!(
            request.body["Message"],
            "  Hello &lt;script&gt;<br />&amp; goodbye  "
        );
        assert_eq!(request.body["Attachements"], json!([]));
        assert_eq!(request.body["IDMessageReply"], "undefined");
        assert_eq!(request.body["CacheDestinataire"], false);
        for (action, code) in [("reply", "1"), ("reply_all", "3"), ("forward", "2")] {
            let mut args = args();
            args["action"] = json!(action);
            assert!(super::request("omnivox_mio_send", &args).is_err());
            args["reply_to_id"] = json!(MESSAGE);
            args["hide_recipients"] = json!(true);
            let request = super::request("omnivox_mio_send", &args).unwrap().unwrap();
            assert_eq!(request.body["IDMessageReply"], MESSAGE);
            assert_eq!(request.body["derniereAction"], code);
            assert_eq!(request.body["To"], RECIPIENT);
            assert_eq!(request.body["CacheDestinataire"], true);
        }
    }

    #[test]
    fn deletion_direction_and_permanence_are_never_implicit() {
        let trash = request("omnivox_mio_delete", &args()).unwrap().unwrap();
        assert_eq!(
            trash.body,
            json!({"Id":MESSAGE,"Envoie":false,"SupprimerPermanent":false})
        );
        let permanent = request("omnivox_mio_delete_permanently", &args())
            .unwrap()
            .unwrap();
        assert_eq!(permanent.body["SupprimerPermanent"], true);
        let flags = request("omnivox_mio_flags", &args()).unwrap().unwrap();
        assert_eq!(
            flags.body,
            json!({"Id":MESSAGE,"IsEnvoi":false,"indicateurs":{"Drapeau":true,"ARelire":false}})
        );
        let consulted = request("omnivox_assignment_mark_consulted", &args())
            .unwrap()
            .unwrap();
        assert_eq!(consulted.body["isAddConsultation"], true);
        assert_eq!(consulted.term.as_deref(), Some("20263"));
    }

    #[test]
    fn malformed_writes_are_rejected_before_transport() {
        for (name, key, value) in [
            ("omnivox_mio_delete", "message_id", json!("not-a-guid")),
            ("omnivox_mio_delete", "is_sent", json!("false")),
            ("omnivox_mio_flags", "flagged", Value::Null),
            ("omnivox_mio_send", "recipient_ids", json!([])),
            (
                "omnivox_mio_send",
                "recipient_ids",
                json!([RECIPIENT, RECIPIENT.to_uppercase()]),
            ),
            ("omnivox_mio_send", "message", json!("&".repeat(11000))),
            ("omnivox_mio_send", "message", json!("a\u{0000}b")),
            ("omnivox_mio_send", "subject", json!("")),
            ("omnivox_mio_send", "reply_to_id", json!(MESSAGE)),
            (
                "omnivox_mio_folder_create",
                "name",
                json!("SEARCH_FOLDER_MioRecu"),
            ),
            ("omnivox_mio_folder_rename", "category_id", json!(0)),
            ("omnivox_mio_folder_rename", "category_id", json!("7")),
            (
                "omnivox_mio_move",
                "category",
                json!("SEARCH_FOLDER_MioSupprimer"),
            ),
            (
                "omnivox_assignment_mark_consulted",
                "term",
                json!("invalid"),
            ),
            (
                "omnivox_assignment_mark_consulted",
                "course_id",
                json!("../class"),
            ),
        ] {
            let mut args = args();
            args[key] = value;
            assert!(
                request(name, &args).is_err(),
                "{name} accepted {key}: {args}"
            );
        }
    }

    #[test]
    fn only_explicit_success_is_reported_as_confirmed() {
        for response in [
            Value::Null,
            json!(false),
            json!(0),
            json!({}),
            json!({"Error":"Denied"}),
            json!({"HasError":true,"Success":true}),
            json!({"Success":false}),
            json!({"IsPeriodeArret":true}),
            json!({"IsAvailable":false}),
            json!({"NaviguerPage":"ErrorPage.Any"}),
        ] {
            assert!(confirmed("omnivox_mio_send", &json!({}), response).is_err());
        }
        for response in [json!(true), json!(1), json!("OK"), json!({"Success":true})] {
            assert_eq!(
                confirmed("omnivox_mio_send", &json!({}), response).unwrap()["confirmed"],
                true
            );
        }
        let body = json!({"idTravail":MESSAGE});
        assert!(confirmed(
            "omnivox_assignment_mark_consulted",
            &body,
            json!({"Travail":{"IDTravail":RECIPIENT}})
        )
        .is_err());
        assert!(confirmed(
            "omnivox_assignment_mark_consulted",
            &body,
            json!({"Travail":{"IDTravail":MESSAGE}})
        )
        .is_ok());
    }

    fn server(
        status: u16,
        body: Value,
        college: bool,
    ) -> (crate::config::Settings, std::thread::JoinHandle<Captured>) {
        use tiny_http::{Header, Response, Server};
        let server = Server::http("127.0.0.1:0").unwrap();
        let origin = format!("http://{}", server.server_addr());
        let worker = std::thread::spawn(move || {
            let mut seen = Vec::new();
            let mut first = true;
            while let Some(mut request) = server
                .recv_timeout(std::time::Duration::from_millis(if first {
                    5000
                } else {
                    100
                }))
                .unwrap()
            {
                let mut payload = String::new();
                request.as_reader().read_to_string(&mut payload).unwrap();
                let headers = request
                    .headers()
                    .iter()
                    .map(|h| format!("{}: {}", h.field, h.value))
                    .collect::<Vec<_>>()
                    .join("\n");
                assert_eq!(request.method().as_str(), "POST");
                seen.push((
                    request.url().to_string(),
                    serde_json::from_str(&payload).unwrap(),
                    headers,
                ));
                request
                    .respond(
                        Response::from_string(if first {
                            body.to_string()
                        } else {
                            "true".into()
                        })
                        .with_status_code(if first { status } else { 200 })
                        .with_header(
                            Header::from_bytes("Content-Type", "application/json").unwrap(),
                        )
                        .with_header(
                            Header::from_bytes("Location", "/Mobl/should-not-replay").unwrap(),
                        ),
                    )
                    .unwrap();
                first = false;
            }
            seen
        });
        let settings = crate::config::Settings {
            omnivox: Some(crate::config::OmnivoxSettings {
                url: Some(origin),
                token: Some(
                    if college {
                        "SidLea=private;Mobile_AuthentificationASPX=private"
                    } else {
                        "gateway-key"
                    }
                    .into(),
                ),
                device_id: Some("a".repeat(40)),
                auth_hash: Some("mobile-code".into()),
            }),
            ..Default::default()
        };
        (settings, worker)
    }

    #[test]
    fn every_write_reaches_the_correct_endpoint_once_via_mcp() {
        for college in [false, true] {
            for tool in tools() {
                let name = tool["name"].as_str().unwrap();
                let mut expected = request(name, &args()).unwrap().unwrap();
                if expected.term.is_some() {
                    expected.body["AnSession"] = json!("20263");
                }
                let response = if name == "omnivox_assignment_mark_consulted" {
                    json!({"Travail":{"IDTravail":MESSAGE},"InfosAutoLoginCvir":{"SID":"hidden-secret"}})
                } else {
                    json!(true)
                };
                let (settings, worker) = server(200, response, college);
                let result = crate::mcp::dispatch_tool(&settings, name, &args());
                assert_eq!(result["isError"], false, "{name}: {result}");
                assert_eq!(result["structuredContent"]["confirmed"], true);
                for secret in [
                    "hidden-secret",
                    "mobile-code",
                    "gateway-key",
                    "SidLea=private",
                ] {
                    assert!(!result.to_string().contains(secret));
                }
                let seen = worker.join().unwrap();
                assert_eq!(seen.len(), 1, "write was replayed");
                assert_eq!(seen[0].0, format!("/Mobl/{}", expected.path));
                assert_eq!(seen[0].1, expected.body);
                let headers = seen[0].2.to_ascii_lowercase();
                if college {
                    assert!(headers.contains("x-user-agent-ovx: ovx "));
                    assert!(!headers.contains("authorization: bearer"));
                } else {
                    assert!(headers.contains("authorization: bearer gateway-key"));
                    assert!(!headers.contains("cookie:"));
                }
            }
        }
    }

    #[test]
    fn failures_redirects_and_ambiguous_replies_never_replay_the_write() {
        for (status, body) in [
            (302, json!(true)),
            (307, json!(true)),
            (401, json!(false)),
            (403, json!(false)),
            (500, json!(false)),
            (200, json!({"Error":"refused"})),
            (200, Value::Null),
            (200, json!(false)),
        ] {
            let (settings, worker) = server(status, body, false);
            let result = crate::mcp::dispatch_tool(&settings, "omnivox_mio_delete", &args());
            assert_eq!(result["isError"], true, "{result}");
            assert!(!result.to_string().contains("gateway-key"));
            assert_eq!(worker.join().unwrap().len(), 1, "write was replayed");
        }
    }
}
