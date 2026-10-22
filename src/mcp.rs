use crate::catalog::Catalog;
use crate::config::{Settings, VERSION};
use crate::moodle::{request_mobile_token, MoodleClient};
use crate::proxmox::{locate_resource, ProxmoxClient};
use serde_json::{json, Value};

pub type ToolResult = Result<Value, String>;

/// Tool list for the current settings: always student_status, plus enabled services.
pub fn tool_definitions(settings: &Settings) -> Vec<Value> {
    let mut tools = vec![json!({
        "name": "student_status",
        "description": "Show which student services this connector exposes and the accounts in use.",
        "annotations": { "readOnlyHint": true }
    })];
    if settings.moodle().is_some() {
        let moodle = json!([
            { "name": "moodle_status", "description": "Check the Moodle connection and show which site and account this token belongs to.", "annotations": { "readOnlyHint": true } },
            { "name": "moodle_my_courses", "description": "List the courses in which the connected student is enrolled.", "annotations": { "readOnlyHint": true } },
            { "name": "moodle_course_content", "description": "Get the sections, activities and resources in a course.", "inputSchema": { "type": "object", "properties": { "courseid": { "type": "integer", "minimum": 1 } }, "required": ["courseid"] }, "annotations": { "readOnlyHint": true } },
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
    if settings.proxmox().is_some() {
        let allowed = settings
            .proxmox
            .as_ref()
            .and_then(|p| p.allow_write)
            .unwrap_or(false);
        let write_annotation = if allowed {
            json!({ "readOnlyHint": false, "destructiveHint": true })
        } else {
            json!({ "readOnlyHint": true })
        };
        let proxmox = json!([
            { "name": "proxmox_status", "description": "Show the connected Proxmox server version and the nodes this token can see.", "annotations": { "readOnlyHint": true } },
            { "name": "proxmox_list_vms", "description": "List every VM and LXC container this Proxmox token can see, with node and status.", "annotations": { "readOnlyHint": true } },
            { "name": "proxmox_vm_status", "description": "Show detailed status of one VM or LXC container by its VMID.", "inputSchema": { "type": "object", "properties": { "vmid": { "type": "integer", "minimum": 1 } }, "required": ["vmid"] }, "annotations": { "readOnlyHint": true } },
            { "name": "proxmox_vm_start", "description": "Start a stopped VM or LXC container. Requires Proxmox writing enabled.", "inputSchema": { "type": "object", "properties": { "vmid": { "type": "integer", "minimum": 1 } }, "required": ["vmid"] }, "annotations": { "readOnlyHint": false } },
            { "name": "proxmox_vm_stop", "description": "Stop (graceful shutdown, or hard poweroff with force=true) a running VM or LXC container.", "inputSchema": { "type": "object", "properties": { "vmid": { "type": "integer", "minimum": 1 }, "force": { "type": "boolean" } }, "required": ["vmid"] }, "annotations": write_annotation },
            { "name": "proxmox_vm_reboot", "description": "Reboot a running VM or LXC container. Requires Proxmox writing enabled.", "inputSchema": { "type": "object", "properties": { "vmid": { "type": "integer", "minimum": 1 } }, "required": ["vmid"] }, "annotations": write_annotation }
        ]);
        tools.extend(proxmox.as_array().cloned().unwrap_or_default());
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
    }
    tools
}

/// Dispatch one tool call; always returns a JSON-RPC tool result value.
pub fn dispatch_tool(settings: &Settings, name: &str, args: &Value) -> Value {
    let outcome = match name {
        "student_status" => student_status(settings),
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
            let courseid = args.get("courseid").and_then(Value::as_i64);
            let payload = courseid
                .map(|id| json!({ "courseid": id }))
                .unwrap_or_else(|| json!({}));
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
                .and_then(Value::as_i64)
                .unwrap_or(20)
                .clamp(1, 100) as usize;
            let query = args.get("query").and_then(Value::as_str).unwrap_or("");
            let results: Vec<Value> = catalog.search(query, catalog.len()).into_iter()
                .filter(|entry| available.iter().any(|authorized| authorized == &entry.name))
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
            let authorized: Vec<String> = info
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
            if !authorized.iter().any(|authorized| authorized == name) {
                return Err(format!("Function {name} is not available for this token."));
            }
            let catalog = Catalog::load();
            if catalog.get(name).map(|entry| entry.write).unwrap_or(true) && !config.allow_write {
                return Err("This call can change Moodle. Turn on \"Allow changes\" in the student-mcp window.".into());
            }
            client.call(name, &payload)
        }),
        "proxmox_status" => proxmox_tool(settings, |client| {
            Ok(json!({ "version": client.version()?, "nodes": client.nodes()? }))
        }),
        "proxmox_list_vms" => proxmox_tool(settings, |client| {
            let resources = client.resources()?;
            let machines: Vec<Value> = resources
                .as_array()
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .filter_map(|entry| {
                    let vmid = entry.get("vmid").and_then(Value::as_i64)?;
                    Some(json!({
                        "vmid": vmid,
                        "name": entry.get("name").cloned().unwrap_or(Value::Null),
                        "node": entry.get("node").cloned().unwrap_or(Value::Null),
                        "status": entry.get("status").cloned().unwrap_or(Value::Null),
                        "type": entry.get("type").cloned().unwrap_or(Value::Null)
                    }))
                })
                .collect();
            Ok(json!({ "count": machines.len(), "machines": machines }))
        }),
        "proxmox_vm_status" => proxmox_tool(settings, |client| {
            let vmid = require_integer(args, "vmid")?;
            let found = locate_resource(client, vmid)?;
            Ok(
                json!({ "vmid": vmid, "node": found.node, "type": found.kind, "status": found.status, "detail": client.vm_detail(&found.node, &found.kind, vmid)? }),
            )
        }),
        "proxmox_vm_start" => proxmox_write_tool(settings, |client| {
            let vmid = require_integer(args, "vmid")?;
            let found = locate_resource(client, vmid)?;
            if found.status == "running" {
                return Ok(
                    json!({ "vmid": vmid, "status": "running", "note": "Machine already running." }),
                );
            }
            Ok(
                json!({ "vmid": vmid, "node": found.node, "started": true, "task": client.vm_action(&found.node, &found.kind, vmid, "start")? }),
            )
        }),
        "proxmox_vm_stop" => proxmox_write_tool(settings, |client| {
            let vmid = require_integer(args, "vmid")?;
            let force = args.get("force").and_then(Value::as_bool).unwrap_or(false);
            let found = locate_resource(client, vmid)?;
            let task = client.vm_action(
                &found.node,
                &found.kind,
                vmid,
                if force { "stop" } else { "shutdown" },
            )?;
            Ok(
                json!({ "vmid": vmid, "node": found.node, "task": task, "note": if force { "Poweroff requested (forced shutdown)." } else { "Graceful shutdown requested." } }),
            )
        }),
        "proxmox_vm_reboot" => proxmox_write_tool(settings, |client| {
            let vmid = require_integer(args, "vmid")?;
            let found = locate_resource(client, vmid)?;
            Ok(
                json!({ "vmid": vmid, "node": found.node, "task": client.vm_action(&found.node, &found.kind, vmid, "reboot")? }),
            )
        }),
        other => Err(format!("Unknown tool \"{other}\".")),
    };
    match outcome {
        Ok(data) => {
            json!({ "content": [{ "type": "text", "text": serde_json::to_string_pretty(&data).unwrap_or_default() }], "isError": false })
        }
        Err(message) => {
            json!({ "content": [{ "type": "text", "text": message }], "isError": true })
        }
    }
}

fn require_integer(args: &Value, key: &str) -> Result<i64, String> {
    args.get(key)
        .and_then(Value::as_i64)
        .filter(|value| *value > 0)
        .ok_or(format!("Provide an integer {key} > 0."))
}

fn student_status(settings: &Settings) -> ToolResult {
    Ok(json!({
        "version": VERSION,
        "services": settings.services_active(),
        "moodleAllowWrite": settings.moodle().map(|m| m.allow_write).unwrap_or(false),
        "proxmoxAllowWrite": settings.proxmox().map(|p| p.allow_write).unwrap_or(false)
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

fn proxmox_tool<F>(settings: &Settings, handler: F) -> ToolResult
where
    F: FnOnce(&ProxmoxClient) -> Result<Value, String>,
{
    let config = settings
        .proxmox()
        .ok_or("Proxmox is not configured: connect it in the student-mcp window.")?;
    handler(&ProxmoxClient::new(config))
}

fn proxmox_write_tool<F>(settings: &Settings, handler: F) -> ToolResult
where
    F: FnOnce(&ProxmoxClient) -> Result<Value, String>,
{
    if !settings
        .proxmox
        .as_ref()
        .and_then(|p| p.allow_write)
        .unwrap_or(false)
    {
        return Err(
            "Proxmox changes are disabled. Reconnect Proxmox in the window and allow writing."
                .into(),
        );
    }
    proxmox_tool(settings, handler)
}

/// Handle a JSON-RPC message; None for notifications (no reply expected).
pub fn handle_message(settings: &Settings, message: &Value) -> Option<Value> {
    let id = message.get("id").cloned().unwrap_or(Value::Null);
    if id.is_null() {
        return None; // notification: keep silent per spec
    }
    let method = message
        .get("method")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let params = message.get("params").cloned().unwrap_or_else(|| json!({}));
    let response = match method {
        "initialize" => json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": {
                "protocolVersion": params.get("protocolVersion").cloned().unwrap_or_else(|| json!("2025-03-26")),
                "capabilities": { "tools": {} },
                "serverInfo": { "name": "student-mcp", "version": VERSION },
                "instructions": "Connect your student services in the student-mcp window; the tools follow your credentials."
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
    settings.moodle = Some(crate::config::MoodleSettings {
        url: Some(base_url),
        token: Some(token),
        allow_write: Some(allow_write),
    });
    settings.save().ok();
    Ok(format!(
        "Connected to {} as {}.",
        info.get("sitename")
            .and_then(Value::as_str)
            .unwrap_or("Moodle"),
        info.get("username").and_then(Value::as_str).unwrap_or("?")
    ))
}

pub fn activate_proxmox(
    settings: &mut Settings,
    url: &str,
    token_id: &str,
    secret: &str,
    allow_write: bool,
) -> Result<String, String> {
    if !token_id.contains('!') {
        return Err("The token ID looks like \"student@pam!mcp\".".into());
    }
    let base_url = crate::config::normalize_proxmox_url(url)?;
    let client = ProxmoxClient::new(crate::config::ProxmoxConfig {
        base_url: base_url.clone(),
        token_id: token_id.trim().to_string(),
        secret: secret.trim().to_string(),
        allow_write,
    });
    let version = client.version()?;
    let nodes = client.nodes().unwrap_or_else(|_| json!([]));
    let names: Vec<String> = nodes
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|node| node.get("node").and_then(Value::as_str).map(str::to_string))
        .collect();
    let summary = if names.is_empty() {
        "Token connected, but no node is visible (limited permissions?)".to_string()
    } else {
        format!("Accessible nodes: {}", names.join(", "))
    };
    let release = version.get("release").and_then(Value::as_str).unwrap_or("");
    settings.proxmox = Some(crate::config::ProxmoxSettings {
        url: Some(base_url),
        token_id: Some(token_id.trim().to_string()),
        secret: Some(secret.trim().to_string()),
        allow_write: Some(allow_write),
    });
    settings.save().ok();
    Ok(format!("Proxmox VE {release} connected. {summary}"))
}

pub fn disable_service(settings: &mut Settings, id: &str) {
    settings.disable(id);
    settings.save().ok();
}

/// Live connectivity check for one configured service, for the "manage" screen.
pub fn test_service(settings: &Settings, id: &str) -> Result<String, String> {
    match id {
        "brightspace" => {
            let config = settings
                .brightspace()
                .ok_or("Brightspace is not connected anymore.")?;
            crate::brightspace::BrightspaceClient::new(config).whoami()?;
            Ok("Brightspace connected (read only).".into())
        }
        "proxmox" => {
            let config = settings
                .proxmox()
                .ok_or("Proxmox is not connected anymore.")?;
            let client = ProxmoxClient::new(config);
            let version = client.version()?;
            let release = version
                .get("release")
                .and_then(Value::as_str)
                .unwrap_or("?");
            let names: Vec<String> = client
                .nodes()
                .ok()
                .and_then(|nodes| nodes.as_array().cloned())
                .unwrap_or_default()
                .into_iter()
                .filter_map(|node| node.get("node").and_then(Value::as_str).map(str::to_string))
                .collect();
            Ok(if names.is_empty() {
                format!("Proxmox VE {release} · no node visible with this token")
            } else {
                format!("Proxmox VE {release} · {}", names.join(", "))
            })
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
    use crate::config::{MoodleSettings, ProxmoxSettings};

    #[test]
    fn connectivity_check_rejects_unconfigured_services_without_a_network_call() {
        let settings = Settings::default();
        assert_eq!(
            test_service(&settings, "moodle").unwrap_err(),
            "Moodle is not connected anymore."
        );
        assert_eq!(
            test_service(&settings, "proxmox").unwrap_err(),
            "Proxmox is not connected anymore."
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
            settings.moodle = (enabled & 1 != 0).then(|| MoodleSettings {
                url: Some("https://moodle.example.org".into()),
                token: Some("test-token".into()),
                ..Default::default()
            });
            settings.proxmox = (enabled & 2 != 0).then(|| ProxmoxSettings {
                url: Some("https://pve.example.org:8006".into()),
                token_id: Some("test@pam!mcp".into()),
                secret: Some("test-secret".into()),
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
                1 + if enabled & 1 != 0 { 10 } else { 0 }
                    + if enabled & 2 != 0 { 6 } else { 0 }
                    + if enabled & 4 != 0 { 4 } else { 0 }
            );
            for tool in tools {
                assert_eq!(
                    tool["inputSchema"]["type"], "object",
                    "invalid tool: {}",
                    tool["name"]
                );
                assert!(tool["inputSchema"]["properties"].is_object());
            }
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
}
