use crate::catalog::Catalog;
use crate::config::{Settings, VERSION};
use crate::moodle::{request_mobile_token, MoodleClient};
use serde_json::{json, Value};

pub type ToolResult = Result<Value, String>;

const LEGACY_PROTOCOL: &str = "2025-03-26";
const MODERN_PROTOCOL: &str = "2026-07-28";
const LEGACY_PROTOCOLS: &[&str] = &["2024-11-05", "2025-03-26", "2025-06-18", "2025-11-25"];

pub fn is_protocol_date(version: &str) -> bool {
    let bytes = version.as_bytes();
    bytes.len() == 10
        && bytes[4] == b'-'
        && bytes[7] == b'-'
        && bytes
            .iter()
            .enumerate()
            .all(|(index, byte)| index == 4 || index == 7 || byte.is_ascii_digit())
}

/// Protocol revisions from 2026-07-28 on are stateless: no initialize, no session.
pub fn is_modern_version(version: &str) -> bool {
    is_protocol_date(version) && version >= MODERN_PROTOCOL
}

fn meta_protocol(message: &Value) -> Option<&str> {
    // JSON Pointer escapes `/` as `~1`. The key really contains a slash.
    message
        .pointer("/params/_meta/io.modelcontextprotocol~1protocolVersion")
        .and_then(Value::as_str)
}

pub fn allows_stateless(message: &Value, protocol_header: Option<&str>) -> bool {
    meta_protocol(message)
        .or(protocol_header)
        .is_some_and(is_modern_version)
}

fn negotiated_legacy(requested: Option<&str>) -> &'static str {
    LEGACY_PROTOCOLS
        .iter()
        .copied()
        .find(|version| Some(*version) == requested)
        .unwrap_or(LEGACY_PROTOCOL)
}

fn unsupported_protocol(id: &Value, requested: &str) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": {
            "code": -32022,
            "message": "Unsupported protocol version",
            "data": {
                "supported": [MODERN_PROTOCOL, "2025-11-25", "2025-06-18", LEGACY_PROTOCOL, "2024-11-05"],
                "requested": requested
            }
        }
    })
}

pub fn header_mismatch(id: &Value, message: &str) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": { "code": -32020, "message": message }
    })
}

/// Tool list for the current settings: always student_status, plus enabled services.
pub fn tool_definitions(settings: &Settings) -> Vec<Value> {
    let mut available = settings.clone();
    for service in ["moodle", "omnivox", "brightspace"] {
        if let Some((id, _)) = settings.account_list(service).first() {
            let selected = settings.for_account(service, id);
            match service {
                "moodle" => available.moodle = selected.moodle,
                "omnivox" => available.omnivox = selected.omnivox,
                "brightspace" => available.brightspace = selected.brightspace,
                _ => {}
            }
        }
    }
    let settings = &available;
    definitions_for_services(
        settings.moodle().is_some(),
        settings.omnivox().is_some(),
        settings.brightspace().is_some(),
    )
}

/// Local UI catalogue, also available before an account is connected.
pub fn service_tool_definitions(service: &str) -> Option<Vec<Value>> {
    let (moodle, omnivox, brightspace) = match service {
        "moodle" => (true, false, false),
        "omnivox" => (false, true, false),
        "brightspace" => (false, false, true),
        _ => return None,
    };
    Some(
        definitions_for_services(moodle, omnivox, brightspace)
            .into_iter()
            .filter(|tool| tool["name"] != "student_status")
            .collect(),
    )
}

fn definitions_for_services(moodle: bool, omnivox: bool, brightspace: bool) -> Vec<Value> {
    let mut tools = vec![json!({
        "name": "student_status",
        "description": "Show which student services this connector exposes and the accounts in use.",
        "annotations": { "readOnlyHint": true }
    })];
    if moodle {
        let moodle = json!([
            { "name": "moodle_status", "description": "Check the Moodle connection and show which site and account this token belongs to.", "annotations": { "readOnlyHint": true } },
            { "name": "moodle_my_courses", "description": "List enrolled courses as a compact summary (id, names, dates). Hidden and ended courses are omitted unless include_hidden is true.", "inputSchema": { "type": "object", "properties": { "include_hidden": { "type": "boolean", "description": "Include hidden and ended courses." } } }, "annotations": { "readOnlyHint": true } },
            { "name": "moodle_search", "description": "Search the student's courses and materials. Prefer this to locate a TD, quiz or file. Pass courseid to search one course; a course code in the query (MATH101) does the same. Matches TD5 to names like 'TD Chapitre 5'.", "inputSchema": { "type": "object", "properties": { "query": { "type": "string" }, "courseid": { "type": "integer", "minimum": 1, "description": "Limit the search to one course." }, "page": { "type": "integer", "minimum": 0 } }, "required": ["query"] }, "annotations": { "readOnlyHint": true } },
            { "name": "moodle_course_content", "description": "Compact section tree of a course: section names, activities, and fileurl values. Optional query keeps matching modules (e.g. TD5). Pass fileurl to moodle_file; do not fetch it with a generic HTTP tool.", "inputSchema": { "type": "object", "properties": { "courseid": { "type": "integer", "minimum": 1 }, "query": { "type": "string", "description": "Keep modules whose name or filename matches, such as TD5 or chapitre 5." } }, "required": ["courseid"] }, "annotations": { "readOnlyHint": true } },
            { "name": "moodle_resources", "description": "Flat list of pages, files, URLs, folders and books, without section names. Prefer moodle_course_content to find a TD or week.", "inputSchema": { "type": "object", "properties": { "courseid": { "type": "integer", "minimum": 1 } }, "required": ["courseid"] }, "annotations": { "readOnlyHint": true } },
            { "name": "moodle_file", "description": "Read a course document using fileurl. PDF: original file by default; format=text for selected pages or format=images for diagrams/scans. Office/OpenDocument: extracted text, tables, slides and sheet cell values in bounded text blocks; page_from/page_to select blocks, not physical pages. Supports docx/pptx/xlsx/xls/odt/odp/ods; legacy doc/ppt need local antiword/catppt. Charts/images/equations may be missing from Office text: export to PDF for visual analysis. find searches extracted text. Follow nextPageFrom when truncated. Never send the Moodle token to another tool.", "inputSchema": { "type": "object", "properties": { "fileurl": { "type": "string" }, "format": { "type": "string", "enum": ["file", "text", "images"], "description": "PDF representation. Office files return extracted text; images requires PDF." }, "find": { "type": "string", "description": "Phrase to find in extracted text." }, "page_from": { "type": "integer", "minimum": 1 }, "page_to": { "type": "integer", "minimum": 1 } }, "required": ["fileurl"] }, "annotations": { "readOnlyHint": true } },
            { "name": "moodle_assignments", "description": "List visible assignments; optionally filter to specific course IDs.", "inputSchema": { "type": "object", "properties": { "courseids": { "type": "array", "items": { "type": "integer" } } } }, "annotations": { "readOnlyHint": true } },
            { "name": "moodle_assignment_status", "description": "Submission and grading status for one assignment, including uploaded files.", "inputSchema": { "type": "object", "properties": { "assignid": { "type": "integer", "minimum": 1 } }, "required": ["assignid"] }, "annotations": { "readOnlyHint": true } },
            { "name": "moodle_quizzes", "description": "List visible quizzes; optionally filter to specific course IDs.", "inputSchema": { "type": "object", "properties": { "courseids": { "type": "array", "items": { "type": "integer" } } } }, "annotations": { "readOnlyHint": true } },
            { "name": "moodle_quiz_status", "description": "Attempts, best grade and access info for one quiz.", "inputSchema": { "type": "object", "properties": { "quizid": { "type": "integer", "minimum": 1 } }, "required": ["quizid"] }, "annotations": { "readOnlyHint": true } },
            { "name": "moodle_grades", "description": "Grades the student can act on. Without courseid: current courses that have a grade. With courseid: compact items for that course (name, grade, max), not the raw Moodle usergrades blob.", "inputSchema": { "type": "object", "properties": { "courseid": { "type": "integer", "minimum": 1 } } }, "annotations": { "readOnlyHint": true } },
            { "name": "moodle_calendar", "description": "Get the upcoming Moodle calendar view; optionally filter to one course.", "inputSchema": { "type": "object", "properties": { "courseid": { "type": "integer", "minimum": 1 } } }, "annotations": { "readOnlyHint": true } },
            { "name": "moodle_deadlines", "description": "Upcoming deadlines from assignments, quizzes and the Moodle calendar, sorted by date. Prefer this for due work.", "inputSchema": { "type": "object", "properties": { "days": { "type": "integer", "minimum": 1, "maximum": 365, "description": "How many days ahead to look (default 60)." }, "courseids": { "type": "array", "items": { "type": "integer" } } } }, "annotations": { "readOnlyHint": true } },
            { "name": "moodle_announcements", "description": "Recent posts from course announcement forums (news). Plain text, newest first.", "inputSchema": { "type": "object", "properties": { "courseids": { "type": "array", "items": { "type": "integer" } }, "limit": { "type": "integer", "minimum": 1, "maximum": 50 } } }, "annotations": { "readOnlyHint": true } },
            { "name": "moodle_forums", "description": "List forums in courses (news and discussion boards).", "inputSchema": { "type": "object", "properties": { "courseids": { "type": "array", "items": { "type": "integer" } } } }, "annotations": { "readOnlyHint": true } },
            { "name": "moodle_forum_discussions", "description": "List recent discussions in one forum.", "inputSchema": { "type": "object", "properties": { "forumid": { "type": "integer", "minimum": 1 }, "limit": { "type": "integer", "minimum": 1, "maximum": 50 } }, "required": ["forumid"] }, "annotations": { "readOnlyHint": true } },
            { "name": "moodle_forum_posts", "description": "Read all posts in a forum discussion. Use discussionid from moodle_announcements or moodle_forum_discussions.", "inputSchema": { "type": "object", "properties": { "discussionid": { "type": "integer", "minimum": 1 } }, "required": ["discussionid"] }, "annotations": { "readOnlyHint": true } },
            { "name": "moodle_notifications", "description": "Unread notification count and recent Moodle popup notifications.", "inputSchema": { "type": "object", "properties": { "limit": { "type": "integer", "minimum": 1, "maximum": 50 } } }, "annotations": { "readOnlyHint": true } },
            { "name": "moodle_messages", "description": "Private message conversations and unread counts.", "inputSchema": { "type": "object", "properties": { "limit": { "type": "integer", "minimum": 1, "maximum": 50 } } }, "annotations": { "readOnlyHint": true } },
            { "name": "moodle_conversation_messages", "description": "Messages inside one private conversation.", "inputSchema": { "type": "object", "properties": { "conversationid": { "type": "integer", "minimum": 1 }, "limit": { "type": "integer", "minimum": 1, "maximum": 100 } }, "required": ["conversationid"] }, "annotations": { "readOnlyHint": true } },
            { "name": "moodle_completion", "description": "Activity completion status for a course.", "inputSchema": { "type": "object", "properties": { "courseid": { "type": "integer", "minimum": 1 } }, "required": ["courseid"] }, "annotations": { "readOnlyHint": true } },
            { "name": "ws_discover", "description": "Search Moodle Web Service functions this token can call, with description and access type.", "inputSchema": { "type": "object", "properties": { "query": { "type": "string" }, "limit": { "type": "integer", "minimum": 1, "maximum": 100 } } }, "annotations": { "readOnlyHint": true } },
            { "name": "ws_schema", "description": "Show bundled Moodle catalogue metadata for one Web Service function by exact name (parameters, read/write type). Does not prove the connected token can call it; use ws_discover or site_info for availability.", "inputSchema": { "type": "object", "properties": { "name": { "type": "string" } }, "required": ["name"] }, "annotations": { "readOnlyHint": true } },
            { "name": "ws_call", "description": "Call a Moodle Web Service function by exact name. Prefer dedicated moodle_* tools first. Find candidates with ws_discover. This function may change school data; confirm write actions with the user before calling. Moodle still enforces the token's permissions.", "inputSchema": crate::catalog::ws_call_schema(), "annotations": { "readOnlyHint": false } }
        ]);
        tools.extend(moodle.as_array().cloned().unwrap_or_default());
        tools.extend([
            json!({"name": "moodle_recent_grades", "description": "Latest dated assessment grades across enrolled current courses, newest first. Not course totals. Omitted undated grades and per-course errors are reported; dates are grading timestamps, not publication dates.",
                "inputSchema": {"type": "object", "properties": {
                    "courseids": {"type": "array", "maxItems": 30, "items": {"type": "integer", "minimum": 1}},
                    "limit": {"type": "integer", "minimum": 1, "maximum": 50}}},
                "annotations": {"readOnlyHint": true}}),
            json!({"name": "moodle_course_updates", "description": "Activity changes in a course since a Unix timestamp: files, configuration, grades, forum posts and other changes reported by Moodle. Returns names and links; not a complete audit log.",
                "inputSchema": {"type": "object", "properties": {
                    "courseid": {"type": "integer", "minimum": 1},
                    "since": {"type": "integer", "minimum": 0, "description": "Past Unix timestamp in seconds."}}, "required": ["courseid", "since"]},
                "annotations": {"readOnlyHint": true}}),
        ]);
    }
    if omnivox {
        tools.extend(crate::omnivox::tools());
    }
    if brightspace {
        tools.extend(crate::brightspace::tools());
    }
    // inputSchema is required by MCP even when a tool takes no arguments.
    // A strict client can reject the entire tools/list result if one is missing.
    for tool in &mut tools {
        tool.as_object_mut()
            .expect("tool definitions are objects")
            .entry("inputSchema")
            .or_insert_with(|| json!({ "type": "object", "properties": {} }));
        if tool["name"] != "student_status" {
            tool["inputSchema"]["properties"]["account_id"] = json!({
                "type": "string",
                "description": "Account ID from student_status. Required when this platform has multiple connected accounts."
            });
        }
    }
    tools
}

/// Dispatch one tool call; always returns a JSON-RPC tool result value.
pub fn dispatch_tool(settings: &Settings, name: &str, args: &Value) -> Value {
    let service = if name.starts_with("moodle_") || name.starts_with("ws_") {
        Some("moodle")
    } else if name.starts_with("omnivox_") {
        Some("omnivox")
    } else if name.starts_with("brightspace_") {
        Some("brightspace")
    } else {
        None
    };
    let mut selected = None;
    if let Some(service) = service {
        let accounts = settings.account_list(service);
        let requested = args.get("account_id").and_then(Value::as_str);
        let id =
            match (args.get("account_id"), requested) {
                (Some(_), Some(id)) if accounts.iter().any(|(known, _)| known == id) => {
                    Some(id.to_string())
                }
                (Some(_), _) => return tool_error(
                    "Unknown account_id for this platform. Call student_status to list accounts.",
                ),
                (None, _) if accounts.len() > 1 => return tool_error(
                    "Several accounts are connected. Call student_status and provide account_id.",
                ),
                _ => accounts.first().map(|(id, _)| id.clone()),
            };
        if let Some(id) = id {
            selected = Some(settings.for_account(service, &id));
        }
    }
    let settings = selected.as_ref().unwrap_or(settings);
    let outcome = match name {
        "student_status" => student_status(settings),
        "omnivox_status"
        | "omnivox_my_courses"
        | "omnivox_course_documents"
        | "omnivox_assignments"
        | "omnivox_grades"
        | "omnivox_schedule"
        | "omnivox_calendar" => omnivox_tool(settings, name, args),
        name if name.starts_with("brightspace_") => brightspace_tool(settings, name, args),
        "moodle_status" => moodle_tool(settings, |config| {
            let client = MoodleClient::new(config);
            let info = client.site_info()?;
            Ok(json!({
                "sitename": info.get("sitename"),
                "username": info.get("username"),
                "userid": info.get("userid"),
                "release": info.get("release"),
                "availableFunctions": info.get("functions").and_then(Value::as_array).map(|list| list.len()).unwrap_or(0)
            }))
        }),
        "moodle_my_courses" => moodle_tool(settings, |config| {
            let include_hidden = args
                .get("include_hidden")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            MoodleClient::new(config).my_courses(include_hidden)
        }),
        "moodle_search" => moodle_tool(settings, |config| {
            let query = args
                .get("query")
                .and_then(Value::as_str)
                .ok_or("Provide query.")?;
            let page = args.get("page").and_then(json_i64).unwrap_or(0);
            let courseid = match args.get("courseid") {
                None | Some(Value::Null) => None,
                Some(value) => {
                    Some(json_positive_i64(value).ok_or("Provide an integer courseid > 0.")?)
                }
            };
            MoodleClient::new(config).search(query, page, courseid)
        }),
        "moodle_course_content" => moodle_tool(settings, |config| {
            let query = args
                .get("query")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|query| !query.is_empty());
            MoodleClient::new(config).course_content(require_integer(args, "courseid")?, query)
        }),
        "moodle_resources" => moodle_tool(settings, |config| {
            MoodleClient::new(config).resources(require_integer(args, "courseid")?)
        }),
        "moodle_file" => moodle_tool(settings, |config| {
            let fileurl = args
                .get("fileurl")
                .and_then(Value::as_str)
                .ok_or("Provide fileurl from moodle_course_content.")?;
            let query = file_query(args)?;
            MoodleClient::new(config).download_file(fileurl, &query)
        }),
        "moodle_assignments" => moodle_tool(settings, |config| {
            let client = MoodleClient::new(config);
            client.call(
                "mod_assign_get_assignments",
                &json!({ "courseids": args.get("courseids").cloned().unwrap_or(Value::Null) }),
            )
        }),
        "moodle_assignment_status" => moodle_tool(settings, |config| {
            MoodleClient::new(config).assignment_status(require_integer(args, "assignid")?)
        }),
        "moodle_quizzes" => moodle_tool(settings, |config| {
            let client = MoodleClient::new(config);
            client.call(
                "mod_quiz_get_quizzes_by_courses",
                &json!({ "courseids": args.get("courseids").cloned().unwrap_or(Value::Null) }),
            )
        }),
        "moodle_quiz_status" => moodle_tool(settings, |config| {
            MoodleClient::new(config).quiz_status(require_integer(args, "quizid")?)
        }),
        "moodle_grades" => moodle_tool(settings, |config| {
            let client = MoodleClient::new(config);
            match args.get("courseid") {
                None | Some(Value::Null) => client.grades_overview(),
                Some(value) => {
                    let courseid =
                        json_positive_i64(value).ok_or("Provide an integer courseid > 0.")?;
                    client.grades_for_course(courseid)
                }
            }
        }),
        "moodle_recent_grades" => moodle_tool(settings, |config| {
            let courseids = optional_i64_list(args.get("courseids"))?;
            let limit = args
                .get("limit")
                .and_then(json_i64)
                .unwrap_or(20)
                .clamp(1, 50) as usize;
            MoodleClient::new(config).recent_grades(courseids, limit)
        }),
        "moodle_course_updates" => moodle_tool(settings, |config| {
            let since = args
                .get("since")
                .and_then(json_i64)
                .ok_or("Provide since as a Unix timestamp in seconds.")?;
            MoodleClient::new(config).course_updates(require_integer(args, "courseid")?, since)
        }),
        "moodle_calendar" => moodle_tool(settings, |config| {
            let client = MoodleClient::new(config);
            let payload = match args.get("courseid") {
                None => json!({}),
                Some(value) => json!({
                    "courseid": json_positive_i64(value).ok_or("Provide an integer courseid > 0.")?
                }),
            };
            client.call("core_calendar_get_calendar_upcoming_view", &payload)
        }),
        "moodle_deadlines" => moodle_tool(settings, |config| {
            let days = args.get("days").and_then(json_i64).unwrap_or(60);
            let courseids = optional_i64_list(args.get("courseids"))?;
            MoodleClient::new(config).deadlines(days, courseids)
        }),
        "moodle_announcements" => moodle_tool(settings, |config| {
            let limit = args
                .get("limit")
                .and_then(json_i64)
                .unwrap_or(20)
                .clamp(1, 50) as usize;
            let courseids = optional_i64_list(args.get("courseids"))?;
            MoodleClient::new(config).announcements(courseids, limit)
        }),
        "moodle_forums" => moodle_tool(settings, |config| {
            let courseids = optional_i64_list(args.get("courseids"))?;
            MoodleClient::new(config).forums(courseids)
        }),
        "moodle_forum_discussions" => moodle_tool(settings, |config| {
            let limit = args
                .get("limit")
                .and_then(json_i64)
                .unwrap_or(20)
                .clamp(1, 50) as usize;
            MoodleClient::new(config).forum_discussions(require_integer(args, "forumid")?, limit)
        }),
        "moodle_forum_posts" => moodle_tool(settings, |config| {
            MoodleClient::new(config).forum_posts(require_integer(args, "discussionid")?)
        }),
        "moodle_notifications" => moodle_tool(settings, |config| {
            let limit = args
                .get("limit")
                .and_then(json_i64)
                .unwrap_or(20)
                .clamp(1, 50) as usize;
            MoodleClient::new(config).notifications(limit)
        }),
        "moodle_messages" => moodle_tool(settings, |config| {
            let limit = args
                .get("limit")
                .and_then(json_i64)
                .unwrap_or(20)
                .clamp(1, 50) as usize;
            MoodleClient::new(config).messages(limit)
        }),
        "moodle_conversation_messages" => moodle_tool(settings, |config| {
            let limit = args
                .get("limit")
                .and_then(json_i64)
                .unwrap_or(30)
                .clamp(1, 100) as usize;
            MoodleClient::new(config)
                .conversation_messages(require_integer(args, "conversationid")?, limit)
        }),
        "moodle_completion" => moodle_tool(settings, |config| {
            MoodleClient::new(config).completion(require_integer(args, "courseid")?)
        }),
        "ws_discover" => moodle_tool(settings, |config| {
            let client = MoodleClient::new(config);
            let info = client.site_info()?;
            let available: Vec<String> = info
                .get("functions")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .filter_map(|fn_entry| {
                    fn_entry
                        .get("name")
                        .and_then(Value::as_str)
                        .map(str::to_string)
                })
                .collect();
            let catalog = Catalog::load();
            let limit = args
                .get("limit")
                .and_then(json_i64)
                .unwrap_or(20)
                .clamp(1, 100) as usize;
            let query = args.get("query").and_then(Value::as_str).unwrap_or("");
            let authorized: std::collections::HashSet<&str> =
                available.iter().map(String::as_str).collect();
            let results: Vec<Value> = catalog.search(query, catalog.len()).into_iter()
                .filter(|entry| authorized.contains(entry.name.as_str()))
                .take(limit)
                .map(|entry| json!({ "name": entry.name, "description": entry.description, "write": entry.write }))
                .collect();
            Ok(json!({ "totalAvailable": available.len(), "results": results }))
        }),
        "ws_schema" => moodle_tool(settings, |_config| {
            let name = args
                .get("name")
                .and_then(Value::as_str)
                .ok_or("Provide name.")?;
            let catalog = Catalog::load();
            let entry = catalog.get(name).ok_or(format!(
                "Function {name} does not exist in the Moodle catalogue."
            ))?;
            Ok(json!({
                "name": entry.name,
                "description": entry.description,
                "type": if entry.write { "write" } else { "read" },
                "parameters": entry.parameters,
                "availableOnToken": "unknown",
                "note": "Bundled catalogue metadata only; it may differ from your site's Moodle version and does not prove this token can call the function. Use ws_discover for token-available names. Optional/nullable flags are included; unresolved structures have a note. Confirm writes with the user before calling.",
                "catalogRelease": catalog.release()
            }))
        }),
        "ws_call" => moodle_tool(settings, |config| {
            let name = args
                .get("name")
                .and_then(Value::as_str)
                .ok_or("Provide name.")?;
            let client = MoodleClient::new(config);
            let payload = args.get("args").cloned().unwrap_or_else(|| json!({}));
            let info = client.site_info()?;
            let authorized: std::collections::HashSet<String> = info
                .get("functions")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|fn_entry| {
                    fn_entry
                        .get("name")
                        .and_then(Value::as_str)
                        .map(str::to_string)
                })
                .collect();
            if !authorized.contains(name) {
                return Err(format!("Function {name} is not available for this token."));
            }
            client.call(name, &payload)
        }),
        other => Err(format!("Unknown tool \"{other}\".")),
    };
    match outcome {
        Ok(data) => {
            if name == "moodle_file" || name == "brightspace_file" {
                let mut content = crate::moodle::mcp_content_for_file(data);
                if name == "brightspace_file" {
                    for item in &mut content {
                        if let Some(uri) = item.pointer_mut("/resource/uri") {
                            if let Some(value) = uri.as_str() {
                                *uri = json!(value.replacen("moodle://", "brightspace://", 1));
                            }
                        }
                    }
                }
                return json!({ "content": content, "isError": false });
            }
            // Keep pretty text for human/LLM clients and structuredContent for
            // machines. File tools stay content-only (binary/text blocks).
            json!({
                "content": [{
                    "type": "text",
                    "text": serde_json::to_string_pretty(&data).unwrap_or_default()
                }],
                "structuredContent": data,
                "isError": false
            })
        }
        Err(message) => {
            json!({ "content": [{ "type": "text", "text": message }], "isError": true })
        }
    }
}

fn tool_error(message: &str) -> Value {
    json!({"content": [{"type": "text", "text": message}], "isError": true})
}

fn json_i64(value: &Value) -> Option<i64> {
    match value {
        Value::Number(number) => number.as_i64().or_else(|| {
            let float = number.as_f64()?;
            if float.fract() == 0.0 && float >= i64::MIN as f64 && float <= i64::MAX as f64 {
                Some(float as i64)
            } else {
                None
            }
        }),
        Value::String(text) => text.trim().parse().ok(),
        _ => None,
    }
}

fn json_positive_i64(value: &Value) -> Option<i64> {
    json_i64(value).filter(|value| *value > 0)
}

fn require_integer(args: &Value, key: &str) -> Result<i64, String> {
    args.get(key)
        .and_then(json_positive_i64)
        .ok_or(format!("Provide an integer {key} > 0."))
}

fn file_query(args: &Value) -> Result<crate::moodle::FileQuery, String> {
    use crate::moodle::FileFormat;
    let format = match args.get("format") {
        Some(Value::String(value)) => match value.as_str() {
            "file" => FileFormat::File,
            "text" => FileFormat::Text,
            "images" => FileFormat::Images,
            _ => return Err("format must be file, text or images.".into()),
        },
        None | Some(Value::Null)
            if ["find", "page_from", "page_to"]
                .iter()
                .any(|key| args.get(key).is_some_and(|value| !value.is_null())) =>
        {
            FileFormat::Text
        }
        None | Some(Value::Null) => FileFormat::File,
        _ => return Err("format must be file, text or images.".into()),
    };
    if format == FileFormat::File
        && ["find", "page_from", "page_to"]
            .iter()
            .any(|key| args.get(key).is_some_and(|value| !value.is_null()))
    {
        return Err("Use format=text or images when selecting pages or searching.".into());
    }
    Ok(crate::moodle::FileQuery {
        format,
        page_from: optional_page_number(args.get("page_from"))?,
        page_to: optional_page_number(args.get("page_to"))?,
        find: args
            .get("find")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string),
    })
}

fn optional_page_number(value: Option<&Value>) -> Result<Option<u32>, String> {
    let Some(value) = value else {
        return Ok(None);
    };
    if value.is_null() {
        return Ok(None);
    }
    let number = json_positive_i64(value).ok_or("Provide page_from/page_to as integers >= 1.")?;
    if number > u32::MAX as i64 {
        return Err("page_from/page_to is too large.".into());
    }
    Ok(Some(number as u32))
}

fn optional_i64_list(value: Option<&Value>) -> Result<Option<Vec<i64>>, String> {
    let Some(value) = value else {
        return Ok(None);
    };
    if value.is_null() {
        return Ok(None);
    }
    let Some(items) = value.as_array() else {
        return Err("Provide courseids as an array of integers.".into());
    };
    if items.is_empty() {
        return Ok(None);
    }
    let mut ids = Vec::with_capacity(items.len());
    for item in items {
        let id = json_positive_i64(item)
            .ok_or_else(|| "Provide courseids as an array of integers > 0.".to_string())?;
        ids.push(id);
    }
    Ok(Some(ids))
}

fn student_status(settings: &Settings) -> ToolResult {
    let accounts: Vec<Value> = ["moodle", "omnivox", "brightspace"]
        .into_iter()
        .flat_map(|service| {
            settings
                .account_list(service)
                .into_iter()
                .map(move |(id, url)| {
                    json!({
                        "account_id": id,
                        "service": service,
                        "url": url,
                        "allowWrite": service == "moodle" || service == "brightspace"
                    })
                })
        })
        .collect();
    Ok(json!({
        "version": VERSION,
        "services": settings.services_active(),
        "accounts": accounts,
        "moodleAllowWrite": accounts.iter().any(|account| {
            account.get("service").and_then(Value::as_str) == Some("moodle")
                && account.get("allowWrite").and_then(Value::as_bool) == Some(true)
        }),
        "brightspaceAllowWrite": accounts.iter().any(|account| {
            account.get("service").and_then(Value::as_str) == Some("brightspace")
                && account.get("allowWrite").and_then(Value::as_bool) == Some(true)
        })
    }))
}

fn moodle_tool<F>(settings: &Settings, handler: F) -> ToolResult
where
    F: FnOnce(&crate::config::MoodleConfig) -> Result<Value, String>,
{
    let config = settings
        .moodle()
        .ok_or("Moodle is not configured: connect it in the student-mcp window.")?;
    handler(&config)
}

fn omnivox_tool(settings: &Settings, name: &str, args: &Value) -> ToolResult {
    let config = settings
        .omnivox()
        .ok_or("Omnivox is not configured: connect it in the student-mcp window.")?;
    crate::omnivox::dispatch(&crate::omnivox::OmnivoxClient::new(config), name, args)
}

fn brightspace_tool(settings: &Settings, name: &str, args: &Value) -> ToolResult {
    let config = settings
        .brightspace()
        .ok_or("Brightspace is not configured: connect it in the student-mcp window.")?;
    let client = crate::brightspace::BrightspaceClient::new(config);
    match name {
        "brightspace_status" => client.whoami(),
        "brightspace_my_courses" => {
            let bookmark = match args.get("bookmark") {
                None => None,
                Some(Value::String(value)) => Some(value.as_str()),
                _ => return Err("Provide bookmark as a string.".into()),
            };
            client.courses(bookmark)
        }
        "brightspace_course_content" => client.content(require_integer(args, "courseid")?),
        "brightspace_file" => client.download_file(
            require_integer(args, "courseid")?,
            require_integer(args, "topicid")?,
            &file_query(args)?,
        ),
        "brightspace_grades" => client.grades(require_integer(args, "courseid")?),
        "brightspace_assignments" => client.assignments(require_integer(args, "courseid")?),
        "brightspace_assignment_status" => client.assignment_status(
            require_integer(args, "courseid")?,
            require_integer(args, "folderid")?,
        ),
        "brightspace_assignment_submit" => {
            let (filename, bytes) = crate::brightspace::submission_bytes(args)?;
            let comment = args
                .get("comment")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty());
            client.submit_assignment(
                require_integer(args, "courseid")?,
                require_integer(args, "folderid")?,
                &filename,
                &bytes,
                comment,
            )
        }
        "brightspace_quizzes" => client.quizzes(require_integer(args, "courseid")?),
        "brightspace_quiz_status" => client.quiz_status(
            require_integer(args, "courseid")?,
            require_integer(args, "quizid")?,
        ),
        "brightspace_calendar" => {
            let days = args.get("days").and_then(json_i64).unwrap_or(60);
            let courseid = match args.get("courseid") {
                None | Some(Value::Null) => None,
                _ => Some(require_integer(args, "courseid")?),
            };
            client.calendar(courseid, days)
        }
        "brightspace_deadlines" => {
            let days = args.get("days").and_then(json_i64).unwrap_or(60);
            let courseids = optional_i64_list(args.get("courseids"))?;
            client.deadlines(days, courseids)
        }
        "brightspace_news" => client.news(require_integer(args, "courseid")?),
        _ => crate::brightspace::dispatch(&client, name, args),
    }
}

/// Handle a JSON-RPC message; None for notifications (no reply expected).
pub fn handle_message(settings: &Settings, message: &Value) -> Option<Value> {
    handle_message_versioned(settings, message, None)
}

pub fn handle_message_versioned(
    settings: &Settings,
    message: &Value,
    protocol_header: Option<&str>,
) -> Option<Value> {
    let id = message.get("id").cloned().unwrap_or(Value::Null);
    if id.is_null() {
        return None; // notification: keep silent per spec
    }
    let method = message
        .get("method")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let params = message.get("params").cloned().unwrap_or_else(|| json!({}));
    let body_version = meta_protocol(message);
    if let (Some(header), Some(body)) = (protocol_header, body_version) {
        if header != body {
            return Some(header_mismatch(
                &id,
                "MCP-Protocol-Version does not match the protocol version in the request body.",
            ));
        }
    }
    let announced = body_version.or(protocol_header);
    let modern = method != "initialize" && announced.is_some_and(is_modern_version);
    if modern && announced != Some(MODERN_PROTOCOL) {
        return Some(unsupported_protocol(&id, announced.unwrap_or_default()));
    }
    let response = match method {
        "initialize" => json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": {
                "protocolVersion": negotiated_legacy(params.get("protocolVersion").and_then(Value::as_str)),
                "capabilities": { "tools": {} },
                "serverInfo": { "name": "student-mcp", "version": VERSION },
                "instructions": "Call student_status to discover connected accounts. Pass account_id to service tools when multiple accounts use the same platform; never guess which account the user intends. One connector link serves every connected account."
            }
        }),
        "server/discover" => json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": {
                "supportedVersions": [MODERN_PROTOCOL, "2025-11-25", "2025-06-18", LEGACY_PROTOCOL, "2024-11-05"],
                "capabilities": { "tools": {} },
                "_meta": {
                    "io.modelcontextprotocol/serverInfo": { "name": "student-mcp", "version": VERSION }
                },
                "instructions": "Call student_status to discover connected accounts. Pass account_id to service tools when multiple accounts use the same platform; never guess which account the user intends. One connector link serves every connected account.",
                "ttlMs": 60_000,
                "cacheScope": "private"
            }
        }),
        "ping" => json!({ "jsonrpc": "2.0", "id": id, "result": {} }),
        "tools/list" => json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": { "tools": tool_definitions(settings) }
        }),
        "tools/call" => {
            let name = params
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            let arguments = params
                .get("arguments")
                .cloned()
                .unwrap_or_else(|| json!({}));
            json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": dispatch_tool(settings, &name, &arguments)
            })
        }
        "notifications/initialized" | "notifications/cancelled" => return None,
        _ => json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": { "code": -32601, "message": format!("Unknown method: {method}") }
        }),
    };
    let mut response = response;
    if modern {
        if let Some(result) = response.get_mut("result").and_then(Value::as_object_mut) {
            result
                .entry("resultType")
                .or_insert_with(|| json!("complete"));
        }
    }
    Some(response)
}

/// Configuration form flow: activate Moodle credentials, persisting only the token.
pub fn activate_moodle(
    settings: &mut Settings,
    url: &str,
    username: &str,
    password: &str,
    pasted_token: &str,
    allow_write: bool,
) -> Result<String, String> {
    let token = if pasted_token.trim().is_empty() {
        request_mobile_token(url, username, password)?
    } else {
        pasted_token.trim().to_string()
    };
    let base_url = crate::config::normalize_moodle_url(url)?;
    let client = MoodleClient::new(&crate::config::MoodleConfig {
        base_url: base_url.clone(),
        token: token.clone(),
        allow_write: false,
    });
    let info = client.site_info()?;
    let mut updated = settings.clone();
    updated.moodle = Some(crate::config::MoodleSettings {
        url: Some(base_url),
        token: Some(token),
        allow_write: Some(allow_write),
    });
    updated.save_service("moodle").map_err(|error| {
        format!("Cannot save Moodle credentials to the private config file: {error}")
    })?;
    *settings = updated;
    Ok(format!(
        "Connected to {} as {}.",
        info.get("sitename")
            .and_then(Value::as_str)
            .unwrap_or("Moodle"),
        info.get("username").and_then(Value::as_str).unwrap_or("?")
    ))
}

pub fn disable_service(settings: &mut Settings, id: &str) -> Result<(), String> {
    let mut updated = settings.clone();
    updated.disable(id);
    updated.save_service(id).map_err(|error| {
        format!("Cannot save the disconnection. The service is still connected. {error}")
    })?;
    *settings = updated;
    Ok(())
}

/// Live connectivity check for one configured service, for the "manage" screen.
pub fn test_service(settings: &Settings, id: &str) -> Result<String, String> {
    match id {
        "omnivox" => {
            let config = settings
                .omnivox()
                .ok_or("Omnivox is not connected anymore.")?;
            let college = crate::config::is_omnivox_host(&config.base_url);
            crate::omnivox::OmnivoxClient::new(config).status()?;
            Ok(if college {
                "Omnivox connected (read only).".into()
            } else {
                "Omnivox connected (read only, through the MCP gateway).".into()
            })
        }
        "brightspace" => {
            let config = settings
                .brightspace()
                .ok_or("Brightspace is not connected anymore.")?;
            crate::brightspace::BrightspaceClient::new(config).whoami()?;
            Ok("Brightspace connected.".into())
        }
        "moodle" => {
            let config = settings
                .moodle()
                .ok_or("Moodle is not connected anymore.")?;
            let client = MoodleClient::new(&config);
            let info = client.site_info()?;
            Ok(format!(
                "{} · signed in as {} (Moodle {})",
                info.get("sitename")
                    .and_then(Value::as_str)
                    .unwrap_or("Moodle"),
                info.get("username").and_then(Value::as_str).unwrap_or("?"),
                info.get("release").and_then(Value::as_str).unwrap_or("?")
            ))
        }
        _ => Err("Unknown service.".into()),
    }
}

#[cfg(test)]
mod protocol_tests {
    use super::*;
    use crate::config::MoodleSettings;

    #[test]
    fn multiple_accounts_require_explicit_routing_and_do_not_leak_tokens() {
        let (url, worker) = crate::moodle::tests::server(vec![json!([])]);
        let settings = Settings {
            moodle: Some(MoodleSettings {
                url: Some("https://unused.example".into()),
                token: Some("first-secret".into()),
                allow_write: Some(false),
            }),
            accounts: vec![crate::config::Account {
                id: "second".into(),
                service: "moodle".into(),
                moodle: Some(MoodleSettings {
                    url: Some(url),
                    token: Some("second-secret".into()),
                    allow_write: Some(false),
                }),
                omnivox: None,
                brightspace: None,
            }],
            ..Default::default()
        };
        let status = student_status(&settings).unwrap();
        assert_eq!(status["accounts"].as_array().unwrap().len(), 2);
        assert_eq!(status["moodleAllowWrite"], true);
        assert!(!status.to_string().contains("secret"));
        for args in [
            json!({"courseid": 1}),
            json!({"courseid": 1, "account_id": "missing"}),
        ] {
            assert_eq!(
                dispatch_tool(&settings, "moodle_course_content", &args)["isError"],
                true
            );
        }
        let result = dispatch_tool(
            &settings,
            "moodle_course_content",
            &json!({"courseid": 1, "account_id": "second"}),
        );
        assert_eq!(result["isError"], false);
        let requests = worker.join().unwrap();
        assert!(requests[0].contains("wstoken=second-secret"));
        assert!(!requests[0].contains("first-secret"));
        let mut extra_only = settings;
        extra_only.moodle = None;
        assert!(tool_definitions(&extra_only)
            .iter()
            .any(|t| t["name"] == "moodle_course_content"));
    }

    #[test]
    fn ws_call_checks_token_authorization_even_for_unclassified_functions() {
        let info = json!({"userid":1,"functions":[{"name":"local_example_change_record"}]});
        let (url, worker) = crate::moodle::tests::server(vec![info, json!({"ok":true})]);
        let settings = Settings {
            moodle: Some(MoodleSettings {
                url: Some(url),
                token: Some("fake-token".into()),
                allow_write: Some(false),
            }),
            ..Default::default()
        };
        let reply = dispatch_tool(
            &settings,
            "ws_call",
            &json!({"name": "local_example_change_record"}),
        );
        assert_eq!(reply["isError"], false);
        let requests = worker.join().unwrap();
        assert!(requests[1].contains("wsfunction=local_example_change_record"));
    }

    #[test]
    fn ws_call_allows_writes_for_existing_accounts_and_preserves_valid_calls() {
        let info = json!({"userid": 1, "functions": [
            {"name": "core_course_get_contents"}, {"name": "core_course_delete_courses"}
        ]});
        let (url, worker) = crate::moodle::tests::server(vec![
            info.clone(),
            json!([]),
            info.clone(),
            info,
            json!([]),
        ]);
        let settings = Settings {
            moodle: Some(MoodleSettings {
                url: Some(url.clone()),
                token: Some("fake-token".into()),
                allow_write: Some(false),
            }),
            ..Default::default()
        };
        let write = dispatch_tool(
            &settings,
            "ws_call",
            &json!({"name": "core_course_delete_courses", "args": {"courseids": [1]}}),
        );
        assert_eq!(write["isError"], false);
        let injection = dispatch_tool(
            &settings,
            "ws_call",
            &json!({"name": "core_course_get_contents", "args": {"wsfunction": "core_course_delete_courses"}}),
        );
        assert_eq!(injection["isError"], true);
        assert!(injection.to_string().contains("Reserved Moodle parameter"));
        let read = dispatch_tool(
            &settings,
            "ws_call",
            &json!({"name": "core_course_get_contents", "args": {"courseid": 1, "options": [{"name": "excludemodules", "value": true}]}}),
        );
        assert_eq!(read["isError"], false);
        assert!(read.get("structuredContent").is_some());
        let requests = worker.join().unwrap();
        assert_eq!(requests.len(), 5);
        assert!(requests
            .iter()
            .any(|request| request.contains("wsfunction=core_course_delete_courses")));
        assert!(requests[4].contains("options%5B0%5D%5Bname%5D=excludemodules"));
    }

    #[test]
    fn moodle_file_parses_section_search_and_page_bounds() {
        let query = file_query(&json!({
            "fileurl": "https://moodle.example/pluginfile.php/1/x.pdf",
            "find": "  3.3  ",
            "page_from": "25",
            "page_to": 30
        }))
        .unwrap();
        assert_eq!(query.find.as_deref(), Some("3.3"));
        assert_eq!(query.page_from, Some(25));
        assert_eq!(query.page_to, Some(30));
        let empty = file_query(&json!({"find": "   "})).unwrap();
        assert_eq!(empty.find, None);
        assert!(file_query(&json!({"page_from": 0})).is_err());
        assert!(file_query(&json!({"page_to": -1})).is_err());
    }

    #[test]
    fn moodle_activation_and_disconnect_report_save_failures() {
        crate::config::tests::with_temp_config(|| {
            // A directory at the destination fails on every OS, even as root.
            std::fs::create_dir(crate::config::config_path()).unwrap();
            let mut settings = Settings {
                moodle: Some(MoodleSettings {
                    url: Some("https://old.example".into()),
                    token: Some("old-token".into()),
                    allow_write: Some(true),
                }),
                ..Default::default()
            };
            let original = serde_json::to_value(&settings).unwrap();
            let (url, worker) = crate::moodle::tests::server(vec![json!({"userid": 1})]);
            assert!(
                activate_moodle(&mut settings, &url, "", "", "new-token", false)
                    .unwrap_err()
                    .contains("Cannot save")
            );
            worker.join().unwrap();
            assert_eq!(serde_json::to_value(&settings).unwrap(), original);
            assert!(disable_service(&mut settings, "moodle").is_err());
            assert_eq!(serde_json::to_value(&settings).unwrap(), original);

            std::fs::remove_dir(crate::config::config_path()).unwrap();
            let (url, worker) = crate::moodle::tests::server(vec![json!({"userid": 1})]);
            activate_moodle(&mut settings, &url, "", "", "new-token", false).unwrap();
            worker.join().unwrap();
            assert_eq!(Settings::load().moodle().unwrap().token, "new-token");
            disable_service(&mut settings, "moodle").unwrap();
            assert!(Settings::load().moodle().is_none());
        });
    }

    #[test]
    fn connectivity_check_rejects_unconfigured_services_without_a_network_call() {
        let settings = Settings::default();
        assert_eq!(
            test_service(&settings, "moodle").unwrap_err(),
            "Moodle is not connected anymore."
        );
        assert_eq!(
            test_service(&settings, "other").unwrap_err(),
            "Unknown service."
        );
    }

    #[test]
    fn every_advertised_tool_has_an_object_input_schema() {
        let mut settings = Settings::default();
        for enabled in 0..8 {
            settings.brightspace = (enabled & 4 != 0).then(|| crate::config::BrightspaceSettings {
                url: Some("https://school.example".into()),
                token: Some("test-token".into()),
                allow_write: Some(false),
            });
            settings.omnivox = (enabled & 2 != 0).then(|| crate::config::OmnivoxSettings {
                url: Some("http://127.0.0.1:3000".into()),
                token: Some("test-token".into()),
                ..Default::default()
            });
            settings.moodle = (enabled & 1 != 0).then(|| MoodleSettings {
                url: Some("https://moodle.example.org".into()),
                token: Some("test-token".into()),
                ..Default::default()
            });
            let reply = handle_message(
                &settings,
                &json!({
                    "jsonrpc": "2.0", "id": 1, "method": "tools/list"
                }),
            )
            .unwrap();
            let tools = reply["result"]["tools"].as_array().unwrap();
            assert_eq!(
                tools.len(),
                1 + if enabled & 1 != 0 { 26 } else { 0 }
                    + if enabled & 2 != 0 { 7 } else { 0 }
                    + if enabled & 4 != 0 {
                        crate::brightspace::tools().len()
                    } else {
                        0
                    }
            );
            for tool in tools {
                assert_eq!(
                    tool["inputSchema"]["type"], "object",
                    "invalid tool: {}",
                    tool["name"]
                );
                assert!(tool["inputSchema"]["properties"].is_object());
                let name = tool["name"].as_str().unwrap_or("");
                if matches!(
                    name,
                    "moodle_course_content"
                        | "moodle_course_updates"
                        | "moodle_resources"
                        | "moodle_file"
                        | "moodle_search"
                        | "moodle_assignment_status"
                        | "moodle_quiz_status"
                        | "moodle_forum_discussions"
                        | "moodle_forum_posts"
                        | "moodle_conversation_messages"
                        | "moodle_completion"
                        | "ws_schema"
                        | "ws_call"
                        | "brightspace_course_content"
                        | "brightspace_file"
                        | "brightspace_grades"
                        | "brightspace_assignments"
                        | "brightspace_assignment_status"
                        | "brightspace_assignment_submit"
                        | "brightspace_quizzes"
                        | "brightspace_quiz_status"
                        | "brightspace_news"
                        | "omnivox_course_documents"
                ) {
                    assert!(
                        tool["inputSchema"]["required"].is_array(),
                        "{name} is missing required"
                    );
                    assert!(tool["inputSchema"]["properties"].get("required").is_none());
                }
            }
        }
    }

    #[test]
    fn initialize_returns_a_protocol_this_server_speaks() {
        let settings = Settings::default();
        let weird = handle_message(
            &settings,
            &json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "initialize",
                "params": { "protocolVersion": { "injected": true } }
            }),
        )
        .unwrap();
        assert_eq!(weird["result"]["protocolVersion"], "2025-03-26");
        let legacy = handle_message(
            &settings,
            &json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "initialize",
                "params": { "protocolVersion": "2025-11-25" }
            }),
        )
        .unwrap();
        assert_eq!(legacy["result"]["protocolVersion"], "2025-11-25");
        assert!(legacy["result"].get("resultType").is_none());
    }

    #[test]
    fn modern_clients_call_tools_without_a_session_and_get_result_type() {
        let settings = Settings::default();
        let discover = handle_message(
            &settings,
            &json!({
                "jsonrpc": "2.0",
                "id": "discover-1",
                "method": "server/discover",
                "params": { "_meta": { "io.modelcontextprotocol/protocolVersion": "2026-07-28" } }
            }),
        )
        .unwrap();
        assert_eq!(discover["result"]["resultType"], "complete");
        assert!(discover["result"]["supportedVersions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|version| version == "2026-07-28"));
        let call = handle_message(
            &settings,
            &json!({
                "jsonrpc": "2.0",
                "id": 2,
                "method": "tools/call",
                "params": {
                    "name": "student_status",
                    "arguments": {},
                    "_meta": { "io.modelcontextprotocol/protocolVersion": "2026-07-28" }
                }
            }),
        )
        .unwrap();
        assert_eq!(call["result"]["isError"], false);
        assert_eq!(call["result"]["resultType"], "complete");
        let rejected = handle_message(
            &settings,
            &json!({
                "jsonrpc": "2.0",
                "id": 3,
                "method": "tools/list",
                "params": { "_meta": { "io.modelcontextprotocol/protocolVersion": "2026-08-01" } }
            }),
        )
        .unwrap();
        assert_eq!(rejected["error"]["code"], -32022);
        assert!(rejected["error"]["data"]["supported"]
            .as_array()
            .unwrap()
            .iter()
            .any(|version| version == "2026-07-28"));
    }

    #[test]
    fn course_ids_accept_whole_numbers_written_as_strings() {
        let (url, worker) = crate::moodle::tests::server(vec![
            json!({"userid": 42, "functions": []}),
            json!({"usergrades": []}),
        ]);
        let settings = Settings {
            moodle: Some(MoodleSettings {
                url: Some(url),
                token: Some("token".into()),
                allow_write: Some(false),
            }),
            ..Default::default()
        };
        let bad = dispatch_tool(&settings, "moodle_grades", &json!({"courseid": "nope"}));
        assert_eq!(bad["isError"], true);
        let good = dispatch_tool(&settings, "moodle_grades", &json!({"courseid": "15"}));
        assert_eq!(good["isError"], false, "{good}");
        let requests = worker.join().unwrap();
        assert!(
            requests.iter().any(|request| {
                request.contains("courseid=15") && request.contains("userid=42")
            }),
            "{requests:?}"
        );
    }

    #[test]
    fn ui_tool_catalogues_are_separate_and_include_moodle_ws_tools() {
        for service in ["moodle", "omnivox", "brightspace"] {
            let tools = service_tool_definitions(service).unwrap();
            assert!(!tools.is_empty());
            for tool in &tools {
                let name = tool["name"].as_str().unwrap();
                assert!(
                    name.starts_with(&format!("{service}_"))
                        || (service == "moodle" && name.starts_with("ws_")),
                    "unexpected {name} in {service}"
                );
                assert!(tool["description"].is_string());
            }
            if service == "moodle" {
                for name in ["ws_discover", "ws_schema", "ws_call"] {
                    assert!(tools.iter().any(|tool| tool["name"] == name));
                }
            }
        }
        assert!(service_tool_definitions("unknown").is_none());
        assert_eq!(tool_definitions(&Settings::default()).len(), 1);
    }

    #[test]
    fn moodle_student_tools_are_advertised() {
        let settings = Settings {
            moodle: Some(MoodleSettings {
                url: Some("https://moodle.example.org".into()),
                token: Some("token".into()),
                allow_write: Some(false),
            }),
            ..Default::default()
        };
        assert_eq!(
            &tool_definitions(&settings)[1..],
            service_tool_definitions("moodle").unwrap()
        );
        let names: Vec<_> = tool_definitions(&settings)
            .into_iter()
            .filter_map(|tool| tool["name"].as_str().map(str::to_string))
            .collect();
        for required in [
            "moodle_search",
            "moodle_recent_grades",
            "moodle_course_updates",
            "moodle_deadlines",
            "moodle_announcements",
            "moodle_assignment_status",
            "moodle_quiz_status",
            "moodle_forums",
            "moodle_forum_posts",
            "moodle_notifications",
            "moodle_messages",
            "moodle_completion",
            "moodle_resources",
        ] {
            assert!(
                names.iter().any(|name| name == required),
                "missing {required}"
            );
        }
    }

    #[test]
    fn no_argument_tool_can_be_discovered_and_called() {
        let settings = Settings::default();
        let tools = tool_definitions(&settings);
        assert_eq!(
            tools[0]["inputSchema"],
            json!({"type": "object", "properties": {}})
        );
        let reply = handle_message(
            &settings,
            &json!({
                "jsonrpc": "2.0", "id": 2, "method": "tools/call",
                "params": { "name": "student_status", "arguments": {} }
            }),
        )
        .unwrap();
        assert_eq!(reply["result"]["isError"], false);
    }

    #[test]
    fn brightspace_tools_require_a_connection_and_mark_writes() {
        for tool in crate::brightspace::tools() {
            let name = tool["name"].as_str().unwrap();
            assert!(tool["annotations"]["readOnlyHint"].is_boolean());
            let reply = dispatch_tool(&Settings::default(), name, &json!({}));
            assert_eq!(reply["isError"], true);
        }
        assert!(test_service(&Settings::default(), "brightspace").is_err());
    }

    #[test]
    fn omnivox_tools_are_read_only_and_require_a_connection() {
        for tool in crate::omnivox::tools() {
            assert_eq!(tool["annotations"]["readOnlyHint"], true);
            let reply = dispatch_tool(
                &Settings::default(),
                tool["name"].as_str().unwrap(),
                &json!({}),
            );
            assert_eq!(reply["isError"], true);
        }
        assert!(test_service(&Settings::default(), "omnivox").is_err());
    }
}

#[cfg(test)]
mod live_moodle_poly {
    use super::*;
    use crate::config::{MoodleSettings, Settings};
    use serde_json::json;

    fn live_settings() -> Option<Settings> {
        let path = dirs_next_home()?.join(".student-mcp.json");
        let raw = std::fs::read_to_string(path).ok()?;
        let value: serde_json::Value = serde_json::from_str(&raw).ok()?;
        let url = value.pointer("/moodle/url")?.as_str()?.to_string();
        let token = value.pointer("/moodle/token")?.as_str()?.to_string();
        if token.is_empty() || !url.contains("polymtl") {
            return None;
        }
        Some(Settings {
            moodle: Some(MoodleSettings {
                url: Some(url),
                token: Some(token),
                allow_write: Some(false),
            }),
            ..Default::default()
        })
    }

    fn dirs_next_home() -> Option<std::path::PathBuf> {
        std::env::var_os("HOME").map(std::path::PathBuf::from)
    }

    fn text(result: &serde_json::Value) -> String {
        result["content"][0]["text"]
            .as_str()
            .unwrap_or("")
            .to_string()
    }

    #[test]
    #[ignore = "hits the real Polytechnique Moodle with the local student token"]
    fn live_student_tool_matrix() {
        let Some(settings) = live_settings() else {
            eprintln!("skip: no local polymtl moodle config");
            return;
        };
        let status = dispatch_tool(&settings, "moodle_status", &json!({}));
        assert_eq!(status["isError"], false, "{status}");
        let courses = dispatch_tool(&settings, "moodle_my_courses", &json!({}));
        assert_eq!(courses["isError"], false, "{courses}");
        let courseid = serde_json::from_str::<serde_json::Value>(&text(&courses))
            .ok()
            .and_then(|value| value["courses"][0]["id"].as_i64())
            .expect("course id");

        for (name, args) in [
            ("moodle_search", json!({"query": "chimie"})),
            ("moodle_deadlines", json!({"days": 90})),
            ("moodle_notifications", json!({"limit": 5})),
            ("moodle_messages", json!({"limit": 5})),
            ("moodle_announcements", json!({"limit": 5})),
            ("moodle_forums", json!({"courseids": [courseid]})),
            ("moodle_course_content", json!({"courseid": courseid})),
            ("moodle_resources", json!({"courseid": courseid})),
            ("moodle_assignments", json!({"courseids": [courseid]})),
            ("moodle_quizzes", json!({"courseids": [courseid]})),
            ("moodle_grades", json!({})),
            ("moodle_grades", json!({"courseid": courseid})),
            ("moodle_calendar", json!({})),
            ("moodle_completion", json!({"courseid": courseid})),
            ("ws_discover", json!({"query": "forum", "limit": 5})),
        ] {
            let result = dispatch_tool(&settings, name, &args);
            assert_eq!(result["isError"], false, "{name} {args} => {result}");
        }

        let quizzes = dispatch_tool(
            &settings,
            "moodle_quizzes",
            &json!({"courseids": [courseid]}),
        );
        let quizid = serde_json::from_str::<serde_json::Value>(&text(&quizzes))
            .ok()
            .and_then(|value| value["quizzes"][0]["id"].as_i64());
        if let Some(quizid) = quizid {
            let result = dispatch_tool(&settings, "moodle_quiz_status", &json!({"quizid": quizid}));
            assert_eq!(result["isError"], false, "{result}");
        }

        let assigns = dispatch_tool(&settings, "moodle_assignments", &json!({}));
        let assignid = serde_json::from_str::<serde_json::Value>(&text(&assigns))
            .ok()
            .and_then(|value| {
                value["courses"].as_array()?.iter().find_map(|course| {
                    course["assignments"]
                        .as_array()?
                        .first()?
                        .get("id")?
                        .as_i64()
                })
            });
        if let Some(assignid) = assignid {
            let result = dispatch_tool(
                &settings,
                "moodle_assignment_status",
                &json!({"assignid": assignid}),
            );
            assert_eq!(result["isError"], false, "{result}");
        }

        let forums = dispatch_tool(
            &settings,
            "moodle_forums",
            &json!({"courseids": [courseid]}),
        );
        let forumid = serde_json::from_str::<serde_json::Value>(&text(&forums))
            .ok()
            .and_then(|value| value["forums"][0]["id"].as_i64());
        if let Some(forumid) = forumid {
            let discussions = dispatch_tool(
                &settings,
                "moodle_forum_discussions",
                &json!({"forumid": forumid, "limit": 3}),
            );
            assert_eq!(discussions["isError"], false, "{discussions}");
            if let Some(discussionid) =
                serde_json::from_str::<serde_json::Value>(&text(&discussions))
                    .ok()
                    .and_then(|value| value["discussions"][0]["discussionid"].as_i64())
            {
                let posts = dispatch_tool(
                    &settings,
                    "moodle_forum_posts",
                    &json!({"discussionid": discussionid}),
                );
                assert_eq!(posts["isError"], false, "{posts}");
            }
        }

        let messages = dispatch_tool(&settings, "moodle_messages", &json!({"limit": 3}));
        if let Some(conversationid) = serde_json::from_str::<serde_json::Value>(&text(&messages))
            .ok()
            .and_then(|value| value["conversations"][0]["id"].as_i64())
        {
            let result = dispatch_tool(
                &settings,
                "moodle_conversation_messages",
                &json!({"conversationid": conversationid, "limit": 5}),
            );
            assert_eq!(result["isError"], false, "{result}");
        }
    }
}
