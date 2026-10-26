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
    let mut tools = vec![json!({
        "name": "student_status",
        "description": "Show which student services this connector exposes and the accounts in use.",
        "annotations": { "readOnlyHint": true }
    })];
    if settings.moodle().is_some() {
        let moodle = json!([
            { "name": "moodle_status", "description": "Check the Moodle connection and show which site and account this token belongs to.", "annotations": { "readOnlyHint": true } },
            { "name": "moodle_my_courses", "description": "List the courses in which the connected student is enrolled.", "annotations": { "readOnlyHint": true } },
            { "name": "moodle_course_content", "description": "Get the sections, activities and resources in a course. To read a PDF or other file, pass its fileurl to moodle_file; do not fetch fileurl with a generic HTTP tool.", "inputSchema": { "type": "object", "properties": { "courseid": { "type": "integer", "minimum": 1 } }, "required": ["courseid"] }, "annotations": { "readOnlyHint": true } },
            { "name": "moodle_file", "description": "Download a Moodle course file with the connected account. PDFs are returned as the original file plus page images. Read the attached PDF for the exercises. Pass fileurl from moodle_course_content. Never send the Moodle token to another tool.", "inputSchema": { "type": "object", "properties": { "fileurl": { "type": "string" } }, "required": ["fileurl"] }, "annotations": { "readOnlyHint": true } },
            { "name": "moodle_assignments", "description": "List visible assignments; optionally filter to specific course IDs.", "inputSchema": { "type": "object", "properties": { "courseids": { "type": "array", "items": { "type": "integer" } } } }, "annotations": { "readOnlyHint": true } },
            { "name": "moodle_quizzes", "description": "List visible quizzes; optionally filter to specific course IDs.", "inputSchema": { "type": "object", "properties": { "courseids": { "type": "array", "items": { "type": "integer" } } } }, "annotations": { "readOnlyHint": true } },
            { "name": "moodle_grades", "description": "Get the connected student's grade items for a course.", "inputSchema": { "type": "object", "properties": { "courseid": { "type": "integer", "minimum": 1 } }, "required": ["courseid"] }, "annotations": { "readOnlyHint": true } },
            { "name": "moodle_calendar", "description": "Get the upcoming Moodle calendar view; optionally filter to one course.", "inputSchema": { "type": "object", "properties": { "courseid": { "type": "integer", "minimum": 1 } } }, "annotations": { "readOnlyHint": true } },
            { "name": "ws_discover", "description": "Search Moodle Web Service functions this token can call, with description and access type.", "inputSchema": { "type": "object", "properties": { "query": { "type": "string" }, "limit": { "type": "integer", "minimum": 1, "maximum": 100 } } }, "annotations": { "readOnlyHint": true } },
            { "name": "ws_schema", "description": "Show details for an available Moodle Web Service function by exact name.", "inputSchema": { "type": "object", "properties": { "name": { "type": "string" } }, "required": ["name"] }, "annotations": { "readOnlyHint": true } },
            { "name": "ws_call", "description": "Call an available Moodle Web Service function by exact name. Find it with ws_discover first. Writes require enabling Moodle writing in the app.", "inputSchema": crate::catalog::ws_call_schema(), "annotations": { "readOnlyHint": false } }
        ]);
        tools.extend(moodle.as_array().cloned().unwrap_or_default());
    }
    if settings.omnivox().is_some() {
        tools.extend(crate::omnivox::tools());
    }
    if settings.brightspace().is_some() {
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
        "brightspace_status"
        | "brightspace_my_courses"
        | "brightspace_course_content"
        | "brightspace_grades" => brightspace_tool(settings, name, args),
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
            let client = MoodleClient::new(config);
            let info = client.site_info()?;
            let userid = info
                .get("userid")
                .and_then(Value::as_i64)
                .ok_or("Moodle did not return the token's user ID.")?;
            client.call("core_enrol_get_users_courses", &json!({ "userid": userid }))
        }),
        "moodle_course_content" => moodle_tool(settings, |config| {
            let client = MoodleClient::new(config);
            client.call(
                "core_course_get_contents",
                &json!({ "courseid": require_integer(args, "courseid")? }),
            )
        }),
        "moodle_file" => moodle_tool(settings, |config| {
            let fileurl = args
                .get("fileurl")
                .and_then(Value::as_str)
                .ok_or("Provide fileurl from moodle_course_content.")?;
            MoodleClient::new(config).download_file(fileurl)
        }),
        "moodle_assignments" => moodle_tool(settings, |config| {
            let client = MoodleClient::new(config);
            client.call(
                "mod_assign_get_assignments",
                &json!({ "courseids": args.get("courseids").cloned().unwrap_or(Value::Null) }),
            )
        }),
        "moodle_quizzes" => moodle_tool(settings, |config| {
            let client = MoodleClient::new(config);
            client.call(
                "mod_quiz_get_quizzes_by_courses",
                &json!({ "courseids": args.get("courseids").cloned().unwrap_or(Value::Null) }),
            )
        }),
        "moodle_grades" => moodle_tool(settings, |config| {
            let client = MoodleClient::new(config);
            client.call(
                "gradereport_user_get_grade_items",
                &json!({ "courseid": require_integer(args, "courseid")? }),
            )
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
                "note": "See the Moodle documentation for every parameter; writes require allowing changes in the app.",
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
            let catalog = Catalog::load();
            if catalog.get(name).map(|entry| entry.write).unwrap_or(true) && !config.allow_write {
                return Err("This call can change Moodle. Turn on \"Allow changes\" in the student-mcp window.".into());
            }
            client.call(name, &payload)
        }),
        other => Err(format!("Unknown tool \"{other}\".")),
    };
    match outcome {
        Ok(data) => {
            let content = if name == "moodle_file" {
                crate::moodle::mcp_content_for_file(data)
            } else {
                vec![json!({
                    "type": "text",
                    "text": serde_json::to_string_pretty(&data).unwrap_or_default()
                })]
            };
            json!({ "content": content, "isError": false })
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

fn student_status(settings: &Settings) -> ToolResult {
    let accounts: Vec<Value> = ["moodle", "omnivox", "brightspace"].into_iter().flat_map(|service| {
        settings.account_list(service).into_iter().map(move |(id, url)| {
            let selected = settings.for_account(service, &id);
            json!({"account_id": id, "service": service, "url": url,
                "allowWrite": selected.moodle().is_some_and(|c| service == "moodle" && c.allow_write)})
        })
    }).collect();
    Ok(json!({
        "version": VERSION,
        "services": settings.services_active(),
        "accounts": accounts,
        "moodleAllowWrite": settings.moodle().map(|m| m.allow_write).unwrap_or(false)
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
        "brightspace_grades" => client.grades(require_integer(args, "courseid")?),
        _ => Err("Unknown Brightspace tool.".into()),
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

/// GUI form flow: activate Moodle credentials, persisting only the token.
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
    updated.save().map_err(|error| {
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
    updated.save().map_err(|error| {
        format!("Cannot save the disconnection. The service is still connected. {error}")
    })?;
    *settings = updated;
    Ok(())
}

pub fn set_moodle_write(settings: &mut Settings, allow: bool) -> Result<(), String> {
    let mut updated = settings.clone();
    let moodle = updated
        .moodle
        .as_mut()
        .ok_or("Moodle is not connected anymore.")?;
    moodle.allow_write = Some(allow);
    updated.save().map_err(|error| {
        format!("Cannot save Moodle permissions. The previous permissions are still active. {error}")
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
            Ok("Brightspace connected (read only).".into())
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
    fn ws_call_enforces_read_only_and_preserves_valid_calls() {
        let info = json!({"userid": 1, "functions": [
            {"name": "core_course_get_contents"}, {"name": "core_course_delete_courses"}
        ]});
        let (url, worker) =
            crate::moodle::tests::server(vec![info.clone(), info.clone(), info, json!([])]);
        let settings = Settings {
            moodle: Some(MoodleSettings {
                url: Some(url),
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
        assert_eq!(write["isError"], true);
        assert!(write.to_string().contains("Allow changes"));
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
        let requests = worker.join().unwrap();
        assert_eq!(requests.len(), 4);
        assert!(!requests
            .iter()
            .any(|request| request.contains("wsfunction=core_course_delete_courses")));
        assert!(requests[3].contains("options%5B0%5D%5Bname%5D=excludemodules"));
    }

    #[test]
    fn moodle_activation_permissions_and_disconnect_report_save_failures() {
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
            assert!(set_moodle_write(&mut settings, false).is_err());
            assert_eq!(serde_json::to_value(&settings).unwrap(), original);
            assert!(disable_service(&mut settings, "moodle").is_err());
            assert_eq!(serde_json::to_value(&settings).unwrap(), original);

            std::fs::remove_dir(crate::config::config_path()).unwrap();
            let (url, worker) = crate::moodle::tests::server(vec![json!({"userid": 1})]);
            activate_moodle(&mut settings, &url, "", "", "new-token", false).unwrap();
            worker.join().unwrap();
            assert_eq!(Settings::load().moodle().unwrap().token, "new-token");
            set_moodle_write(&mut settings, true).unwrap();
            assert!(Settings::load().moodle().unwrap().allow_write);
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
            });
            settings.omnivox = (enabled & 2 != 0).then(|| crate::config::OmnivoxSettings {
                url: Some("http://127.0.0.1:3000".into()),
                token: Some("test-token".into()),
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
                1 + if enabled & 1 != 0 { 11 } else { 0 }
                    + if enabled & 2 != 0 { 7 } else { 0 }
                    + if enabled & 4 != 0 { 4 } else { 0 }
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
                        | "moodle_file"
                        | "moodle_grades"
                        | "ws_schema"
                        | "ws_call"
                        | "brightspace_course_content"
                        | "brightspace_grades"
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
        let (url, worker) = crate::moodle::tests::server(vec![json!([])]);
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
        assert_eq!(good["isError"], false);
        let request = worker.join().unwrap().remove(0);
        assert!(request.contains("courseid=15"), "{request}");
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
    fn brightspace_tools_are_read_only_and_require_a_connection() {
        for tool in crate::brightspace::tools() {
            assert_eq!(tool["annotations"]["readOnlyHint"], true);
            let reply = dispatch_tool(
                &Settings::default(),
                tool["name"].as_str().unwrap(),
                &json!({}),
            );
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
