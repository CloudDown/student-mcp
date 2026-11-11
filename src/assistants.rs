//! Opt-in local MCP setup for supported assistant applications.
//!
//! Local setup uses stdio only: no school credential or MCP bearer token is
//! written to an assistant's configuration. Existing servers are preserved.

use serde_json::{json, Map, Value};
use std::io::Read;
use std::path::{Path, PathBuf};

mod discovery;
use discovery::{find, Assistant, Computer, Format, ASSISTANTS};

const MAX_CONFIG_BYTES: u64 = 8 * 1024 * 1024;

fn executable() -> Result<PathBuf, String> {
    std::env::current_exe()
        .and_then(std::fs::canonicalize)
        .map_err(|error| error.to_string())
}

fn read_config(path: &Path) -> Result<String, String> {
    let metadata = std::fs::symlink_metadata(path).map_err(|error| error.to_string())?;
    if !metadata.file_type().is_file() || metadata.len() > MAX_CONFIG_BYTES {
        return Err(
            "The assistant configuration is not a regular, bounded file; it was left unchanged."
                .into(),
        );
    }
    let file = std::fs::File::open(path).map_err(|error| error.to_string())?;
    let mut text = String::new();
    file.take(MAX_CONFIG_BYTES + 1)
        .read_to_string(&mut text)
        .map_err(|error| error.to_string())?;
    if text.len() as u64 > MAX_CONFIG_BYTES {
        return Err("The assistant configuration is too large; it was left unchanged.".into());
    }
    Ok(text)
}

fn parse_json(text: &str, format: Format) -> Result<Value, String> {
    let value: Value = if matches!(format, Format::OpenCode | Format::Jsonc) {
        jsonc_parser::parse_to_serde_value(text, &Default::default())
            .map_err(|_| "Invalid JSONC")?
    } else {
        serde_json::from_str(text).map_err(|_| "Invalid JSON")?
    };
    if !value.is_object() {
        return Err(
            "The assistant configuration must contain a JSON object; it was left unchanged.".into(),
        );
    }
    Ok(value)
}

fn configured(path: &Path, executable: &Path, assistant: &Assistant) -> bool {
    let Ok(text) = read_config(path) else {
        return false;
    };
    if assistant.format == Format::Toml {
        let Ok(value) = text.parse::<toml_edit::DocumentMut>() else {
            return false;
        };
        let Some(server) = value.get("mcp_servers").and_then(|v| v.get("student-mcp")) else {
            return false;
        };
        return server.get("command").and_then(toml_edit::Item::as_str) == executable.to_str()
            && server
                .get("args")
                .is_none_or(|args| args.as_array().is_some_and(|v| v.is_empty()))
            && server.get("enabled").and_then(toml_edit::Item::as_bool) != Some(false);
    }
    let Ok(value) = parse_json(&text, assistant.format) else {
        return false;
    };
    let server = if assistant.format == Format::OpenCode {
        &value["mcp"]["servers"]["student-mcp"]
    } else {
        &value[assistant.servers_key()]["student-mcp"]
    };
    let expected = if assistant.format == Format::OpenCode {
        json!({"type":"local","command":[executable]})
    } else {
        json!({"type":"stdio","command":executable,"args":[]})
    };
    let command_matches = server["command"] == expected["command"];
    command_matches
        && server["disabled"] != true
        && server["enabled"] != false
        && (if assistant.format == Format::OpenCode {
            server["type"] == "local"
        } else {
            server.get("type").is_none_or(|kind| kind == "stdio")
                && server
                    .get("args")
                    .is_none_or(|args| args.as_array().is_some_and(Vec::is_empty))
        })
        && !value["disabledServers"]
            .as_array()
            .is_some_and(|items| items.contains(&json!("student-mcp")))
}

fn local_status(computer: &Computer, executable: &Path, assistant: &Assistant) -> Value {
    let path = computer.path(assistant);
    let detected = computer.detected(assistant, &path);
    json!({
        "assistant":assistant.id,
        "name":assistant.name,
        "local":true,
        "detected":detected,
        "configured":configured(&path, executable, assistant),
        "reload":match assistant.id { "opencode" => "automatic", "omp" => "mcp", _ => "restart" },
    })
}

pub fn status(assistant: &str) -> Result<Value, String> {
    if matches!(assistant, "chatgpt" | "grok") {
        return Ok(
            json!({"assistant":assistant,"local":false,"detected":false,"configured":false}),
        );
    }
    Ok(local_status(
        &Computer::current()?,
        &executable()?,
        find(assistant)?,
    ))
}

pub fn scan() -> Result<Value, String> {
    let computer = Computer::current()?;
    let executable = executable()?;
    Ok(
        json!({"assistants":ASSISTANTS.iter().map(|assistant| local_status(&computer, &executable, assistant)).collect::<Vec<_>>() }),
    )
}

fn backup_path(path: &Path) -> Result<PathBuf, String> {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or("The assistant configuration path is invalid.")?;
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|error| error.to_string())?
        .as_secs();
    Ok(path.with_file_name(format!(
        "{name}.student-mcp-backup-{stamp}-{}",
        crate::config::random_hex(4)
    )))
}

fn merge_json(text: &str, executable: &Path, assistant: &Assistant) -> Result<String, String> {
    let mut root = parse_json(text, assistant.format)
        .map_err(|error| format!("{error}; the configuration was left unchanged."))?;
    let command = executable
        .to_str()
        .ok_or("The student-mcp path is not valid UTF-8.")?;
    if matches!(assistant.format, Format::OpenCode | Format::Jsonc) {
        // Keep comments, trailing commas and unrelated formatting in JSONC.
        use jsonc_parser::cst::{CstInputValue as Input, CstRootNode};
        let tree = CstRootNode::parse(text, &Default::default())
            .map_err(|_| "Invalid JSONC; the configuration was left unchanged.")?;
        let object = tree
            .object_value()
            .ok_or("Invalid JSONC object; the configuration was left unchanged.")?;
        let servers = if assistant.format == Format::OpenCode {
            object
                .object_value_or_create("mcp")
                .and_then(|mcp| mcp.object_value_or_create("servers"))
        } else {
            object.object_value_or_create(assistant.servers_key())
        }
        .ok_or("Invalid MCP settings; the configuration was left unchanged.")?;
        let value = if assistant.format == Format::OpenCode {
            Input::Object(vec![
                ("type".into(), "local".into()),
                ("command".into(), Input::Array(vec![command.into()])),
            ])
        } else {
            Input::Object(vec![
                ("command".into(), command.into()),
                ("args".into(), Input::Array(vec![])),
            ])
        };
        if let Some(property) = servers.get("student-mcp") {
            property.set_value(value);
        } else {
            servers.append("student-mcp", value);
        }
        if assistant.format == Format::OpenCode && object.get("$schema").is_none() {
            object.append("$schema", "https://opencode.ai/config.json".into());
        }
        return Ok(tree.to_string());
    }
    let servers = object_entry(&mut root, assistant.servers_key())?;
    servers["student-mcp"] = json!({"command":command,"args":[]});
    if matches!(assistant.id, "cursor" | "omp" | "claude-code" | "crush") {
        servers["student-mcp"]["type"] = json!("stdio");
    }
    // OMP's explicit denylist wins over the server's enabled field.
    if assistant.id == "omp" {
        if let Some(items) = root
            .get_mut("disabledServers")
            .and_then(Value::as_array_mut)
        {
            items.retain(|item| item != "student-mcp");
        }
    }
    serde_json::to_string_pretty(&root)
        .map(|text| text + "\n")
        .map_err(|error| error.to_string())
}

fn object_entry<'a>(root: &'a mut Value, key: &str) -> Result<&'a mut Value, String> {
    let object = root
        .as_object_mut()
        .ok_or("Invalid MCP settings; the configuration was left unchanged.")?;
    let value = object
        .entry(key)
        .or_insert_with(|| Value::Object(Map::new()));
    if !value.is_object() {
        return Err(format!(
            "Invalid {key} settings; the configuration was left unchanged."
        ));
    }
    Ok(value)
}

fn merge_toml(text: &str, executable: &Path) -> Result<String, String> {
    let mut root = text
        .parse::<toml_edit::DocumentMut>()
        .map_err(|_| "Invalid TOML; the configuration was left unchanged.")?;
    if root
        .get("mcp_servers")
        .is_some_and(|item| !item.is_table_like())
    {
        return Err("Invalid mcp_servers settings; the configuration was left unchanged.".into());
    }
    if !root.contains_key("mcp_servers") {
        root["mcp_servers"] = toml_edit::Item::Table(toml_edit::Table::new());
    }
    if let Some(table) = root["mcp_servers"].as_inline_table() {
        let table = table.clone().into_table();
        root["mcp_servers"] = toml_edit::Item::Table(table);
    }
    let mut server = toml_edit::Table::new();
    server["command"] = toml_edit::value(
        executable
            .to_str()
            .ok_or("The student-mcp path is not valid UTF-8.")?,
    );
    server["args"] = toml_edit::value(toml_edit::Array::new());
    root["mcp_servers"]["student-mcp"] = toml_edit::Item::Table(server);
    Ok(root.to_string())
}

fn remove_json(text: &str, assistant: &Assistant) -> Result<Option<String>, String> {
    if matches!(assistant.format, Format::OpenCode | Format::Jsonc) {
        use jsonc_parser::cst::CstRootNode;
        let tree = CstRootNode::parse(text, &Default::default())
            .map_err(|_| "Invalid JSONC; the configuration was left unchanged.")?;
        let object = tree
            .object_value()
            .ok_or("Invalid JSONC object; the configuration was left unchanged.")?;
        let servers = if assistant.format == Format::OpenCode {
            object
                .object_value("mcp")
                .and_then(|mcp| mcp.object_value("servers"))
        } else {
            object.object_value(assistant.servers_key())
        };
        let Some(servers) = servers else {
            return Ok(None);
        };
        let Some(property) = servers.get("student-mcp") else {
            return Ok(None);
        };
        property.remove();
        return Ok(Some(tree.to_string()));
    }
    let mut root = parse_json(text, assistant.format)?;
    let Some(servers) = root
        .get_mut(assistant.servers_key())
        .and_then(Value::as_object_mut)
    else {
        return Ok(None);
    };
    if servers.remove("student-mcp").is_none() {
        return Ok(None);
    }
    if assistant.id == "omp" {
        if let Some(items) = root
            .get_mut("disabledServers")
            .and_then(Value::as_array_mut)
        {
            items.retain(|item| item != "student-mcp");
        }
    }
    serde_json::to_string_pretty(&root)
        .map(|text| Some(text + "\n"))
        .map_err(|error| error.to_string())
}

fn remove_toml(text: &str) -> Result<Option<String>, String> {
    let mut root = text
        .parse::<toml_edit::DocumentMut>()
        .map_err(|_| "Invalid TOML; the configuration was left unchanged.")?;
    let Some(servers) = root
        .get_mut("mcp_servers")
        .and_then(toml_edit::Item::as_table_like_mut)
    else {
        return Ok(None);
    };
    if servers.remove("student-mcp").is_none() {
        return Ok(None);
    }
    Ok(Some(root.to_string()))
}

fn remove_config(path: &Path, assistant: &Assistant) -> Result<(), String> {
    let metadata = match std::fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.to_string()),
    };
    if !metadata.file_type().is_file() {
        return Err(format!(
            "{} is not a regular file; it was left unchanged.",
            path.display()
        ));
    }
    let lock_path = path.with_file_name(".student-mcp-assistant.lock");
    let lock = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(lock_path)
        .map_err(|error| error.to_string())?;
    fs2::FileExt::lock_exclusive(&lock).map_err(|error| error.to_string())?;

    let text = read_config(path)?;
    let next = if assistant.format == Format::Toml {
        remove_toml(&text)?
    } else {
        remove_json(&text, assistant)?
    };
    let Some(text) = next else {
        return Ok(());
    };
    let backup = backup_path(path)?;
    std::fs::copy(path, backup).map_err(|error| error.to_string())?;
    crate::config::write_private_with_permissions(
        path,
        text.as_bytes(),
        Some(metadata.permissions()),
    )
    .map_err(|error| error.to_string())
}

pub fn deconfigure(assistant: &str) -> Result<Value, String> {
    let assistant = find(assistant)?;
    let computer = Computer::current()?;
    let path = computer.path(assistant);
    remove_config(&path, assistant)?;
    let executable = executable()?;
    Ok(local_status(&computer, &executable, assistant))
}

fn merge_config(path: &Path, executable: &Path, assistant: &Assistant) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or("The assistant configuration path is invalid.")?;
    std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    let lock_path = path.with_file_name(".student-mcp-assistant.lock");
    let lock = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(lock_path)
        .map_err(|error| error.to_string())?;
    fs2::FileExt::lock_exclusive(&lock).map_err(|error| error.to_string())?;

    let (text, permissions, existed) = match std::fs::symlink_metadata(path) {
        Ok(metadata) => {
            if !metadata.file_type().is_file() {
                return Err(format!(
                    "{} is not a regular file; it was left unchanged.",
                    path.display()
                ));
            }
            (read_config(path)?, Some(metadata.permissions()), true)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => (
            (if assistant.format == Format::Toml {
                ""
            } else {
                "{}"
            })
            .into(),
            None,
            false,
        ),
        Err(error) => return Err(error.to_string()),
    };
    let text = if assistant.format == Format::Toml {
        merge_toml(&text, executable)?
    } else {
        merge_json(&text, executable, assistant)?
    };

    if existed {
        let backup = backup_path(path)?;
        std::fs::copy(path, backup).map_err(|error| error.to_string())?;
    }
    crate::config::write_private_with_permissions(path, text.as_bytes(), permissions)
        .map_err(|error| error.to_string())
}

pub fn configure(assistant: &str) -> Result<Value, String> {
    let assistant = find(assistant)?;
    let computer = Computer::current()?;
    let path = computer.path(assistant);
    if !computer.detected(assistant, &path) {
        return Err("Open the local assistant once before setting it up.".into());
    }
    let executable = executable()?;
    merge_config(&path, &executable, assistant)?;
    Ok(local_status(&computer, &executable, assistant))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn workspace(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "student-mcp-assistant-{name}-{}",
            crate::config::random_hex(8)
        ));
        std::fs::create_dir(&path).unwrap();
        path
    }

    #[test]
    fn chatgpt_has_no_local_setup() {
        let status = status("chatgpt").unwrap();
        assert_eq!(status["local"], false);
        assert_eq!(status["configured"], false);
        assert!(configure("chatgpt").is_err());
    }

    #[test]
    fn merge_preserves_other_settings_and_is_idempotent() {
        let root = workspace("merge");
        let path = root.join("settings.json");
        std::fs::write(
            &path,
            r#"{"theme":"dark","mcpServers":{"other":{"command":"other-server"}}}"#,
        )
        .unwrap();
        let executable = Path::new("/opt/student-mcp/student-mcp");
        merge_config(&path, executable, find("claude").unwrap()).unwrap();
        merge_config(&path, executable, find("claude").unwrap()).unwrap();
        let value: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(value["theme"], "dark");
        assert_eq!(value["mcpServers"]["other"]["command"], "other-server");
        assert_eq!(
            value["mcpServers"]["student-mcp"]["command"],
            executable.to_str().unwrap()
        );
        assert_eq!(value["mcpServers"]["student-mcp"]["args"], json!([]));
        assert!(configured(&path, executable, find("claude").unwrap()));
        assert!(std::fs::read_dir(&root).unwrap().count() >= 3);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn remove_preserves_other_settings_and_is_idempotent() {
        let root = workspace("remove");
        let path = root.join("settings.json");
        std::fs::write(
            &path,
            r#"{"theme":"dark","mcpServers":{"other":{"command":"other-server"},"student-mcp":{"command":"old","args":[]}}}"#,
        )
        .unwrap();
        let assistant = find("claude").unwrap();
        remove_config(&path, assistant).unwrap();
        let value: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(value["theme"], "dark");
        assert_eq!(value["mcpServers"]["other"]["command"], "other-server");
        assert!(value["mcpServers"]["student-mcp"].is_null());
        assert!(!configured(
            &path,
            Path::new("/opt/student-mcp/student-mcp"),
            assistant
        ));
        let count = std::fs::read_dir(&root).unwrap().count();
        remove_config(&path, assistant).unwrap();
        assert_eq!(std::fs::read_dir(&root).unwrap().count(), count);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn invalid_json_is_left_untouched() {
        let root = workspace("invalid");
        let path = root.join("settings.json");
        std::fs::write(&path, "not json").unwrap();
        let error = merge_config(
            &path,
            Path::new("/bin/student-mcp"),
            find("claude").unwrap(),
        )
        .unwrap_err();
        assert!(error.contains("left unchanged"));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "not json");
        assert_eq!(std::fs::read_dir(&root).unwrap().count(), 2);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn all_direct_assistants_save_native_stdio_settings_without_tokens() {
        let root = workspace("formats");
        let executable = Path::new("/Applications/Student MCP/student-mcp");
        for assistant in ASSISTANTS {
            let path = root.join(format!("{}.config", assistant.id));
            merge_config(&path, executable, assistant).unwrap();
            assert!(configured(&path, executable, assistant), "{}", assistant.id);
            let text = std::fs::read_to_string(&path).unwrap();
            assert!(!text.contains("http") || assistant.id == "opencode");
            assert!(!text.contains("/mcp/"));
            merge_config(&path, executable, assistant).unwrap();
            assert_eq!(
                std::fs::read_to_string(&path).unwrap(),
                text,
                "{}",
                assistant.id
            );
            remove_config(&path, assistant).unwrap();
            assert!(
                !configured(&path, executable, assistant),
                "{} remove",
                assistant.id
            );
            remove_config(&path, assistant).unwrap();
        }
        assert!(configure("t3code").is_err());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn opencode_v2_keeps_comments_and_unrelated_servers() {
        let root = workspace("jsonc");
        let path = root.join("opencode.jsonc");
        let original = "{\n  // Keep this comment\n  \"model\": \"example/model\",\n  \"mcp\": {\"timeout\": {\"startup\": 40000}, \"servers\": {\"other\": {\"type\": \"local\", \"command\": [\"other\"]},},},\n}\n";
        std::fs::write(&path, original).unwrap();
        let assistant = find("opencode").unwrap();
        merge_config(&path, Path::new("/bin/student-mcp"), assistant).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("// Keep this comment"));
        let value = parse_json(&text, Format::OpenCode).unwrap();
        assert_eq!(value["model"], "example/model");
        assert_eq!(value["mcp"]["timeout"]["startup"], 40000);
        assert_eq!(
            value["mcp"]["servers"]["other"]["command"],
            json!(["other"])
        );
        assert_eq!(
            value["mcp"]["servers"]["student-mcp"]["command"],
            json!(["/bin/student-mcp"])
        );
        assert!(
            value["mcp"]["student-mcp"].is_null(),
            "Never write the deprecated V1 map"
        );
        let backup = std::fs::read_dir(&root)
            .unwrap()
            .flatten()
            .find(|entry| entry.file_name().to_string_lossy().contains("backup"))
            .unwrap();
        assert_eq!(std::fs::read_to_string(backup.path()).unwrap(), original);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn native_toml_preserves_comments_and_supports_inline_tables() {
        for original in ["# My settings\nmodel = 'example'\n[mcp_servers.other]\ncommand = 'other'\n", "# Inline settings\nmcp_servers = { other = { command = 'other' } }\nmodel = 'example'\n"] {
            let updated = merge_toml(original, Path::new("C:\\Program Files\\Student MCP\\student-mcp.exe")).unwrap();
            assert!(updated.contains(original.lines().next().unwrap()));
            let value = updated.parse::<toml_edit::DocumentMut>().unwrap();
            assert_eq!(value["model"].as_str(), Some("example"));
            assert_eq!(value["mcp_servers"]["other"]["command"].as_str(), Some("other"));
            assert_eq!(value["mcp_servers"]["student-mcp"]["command"].as_str(), Some("C:\\Program Files\\Student MCP\\student-mcp.exe"));
        }
    }

    #[test]
    fn malformed_native_settings_are_never_replaced() {
        let root = workspace("malformed");
        for (id, original) in [
            ("claude", "[]"),
            ("gemini", r#"{"mcpServers":null}"#),
            ("opencode", r#"{"mcp":false}"#),
            ("opencode", r#"{"mcp":{"servers":[]}}"#),
            ("codex", "mcp_servers = false"),
            ("grok-build", "invalid = ["),
        ] {
            let path = root.join(id);
            std::fs::write(&path, original).unwrap();
            assert!(merge_config(&path, Path::new("/bin/student-mcp"), find(id).unwrap()).is_err());
            assert_eq!(std::fs::read_to_string(&path).unwrap(), original);
        }
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn omp_connection_removes_only_its_own_disable_override() {
        let original = r#"{"disabledServers":["other","student-mcp"],"mcpServers":{"other":{"command":"other"}}}"#;
        let updated = merge_json(
            original,
            Path::new("/bin/student-mcp"),
            find("omp").unwrap(),
        )
        .unwrap();
        let value: Value = serde_json::from_str(&updated).unwrap();
        assert_eq!(value["disabledServers"], json!(["other"]));
        assert_eq!(value["mcpServers"]["other"]["command"], "other");
    }

    #[test]
    fn omarchy_agents_keep_their_native_keys_and_approval_policies() {
        let executable = Path::new("/opt/student-mcp/student-mcp");
        for id in ["pi", "qwen", "kiro", "amp", "crush", "devin"] {
            let assistant = find(id).unwrap();
            let mut original = json!({"permissions":{"ask":true},"theme":"custom"});
            original[assistant.servers_key()] =
                json!({"other":{"command":"other","env":{"KEY":"unchanged"}}});
            let original = serde_json::to_string(&original).unwrap();
            let text = if assistant.format == Format::Jsonc {
                format!("// Keep this comment\n{original}")
            } else {
                original
            };
            let updated = merge_json(&text, executable, assistant).unwrap();
            let value = parse_json(&updated, assistant.format).unwrap();
            assert_eq!(value["theme"], "custom", "{id}");
            assert_eq!(value["permissions"]["ask"], true, "{id}");
            assert_eq!(
                value[assistant.servers_key()]["other"]["env"]["KEY"],
                "unchanged",
                "{id}"
            );
            let server = &value[assistant.servers_key()]["student-mcp"];
            assert_eq!(server["command"], executable.to_str().unwrap(), "{id}");
            assert_eq!(server["args"], json!([]), "{id}");
            assert!(
                server.get("autoApprove").is_none() && server.get("trust").is_none(),
                "{id}"
            );
            if assistant.format == Format::Jsonc {
                assert!(updated.contains("// Keep this comment"));
            }
            if matches!(id, "amp" | "crush") {
                assert!(
                    value.get("mcpServers").is_none(),
                    "Never write another client's wrapper key"
                );
            }
        }
    }

    #[cfg(unix)]
    #[test]
    fn configuration_and_backup_keep_private_permissions() {
        use std::os::unix::fs::PermissionsExt;
        let root = workspace("permissions");
        let path = root.join("settings.json");
        for _ in 0..2 {
            merge_config(
                &path,
                Path::new("/bin/student-mcp"),
                find("cursor").unwrap(),
            )
            .unwrap();
        }
        for entry in std::fs::read_dir(&root)
            .unwrap()
            .flatten()
            .filter(|entry| entry.file_name() != ".student-mcp-assistant.lock")
        {
            assert_eq!(
                entry.metadata().unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
        std::fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_configuration_is_rejected() {
        use std::os::unix::fs::symlink;
        let root = workspace("symlink");
        let target = root.join("target.json");
        let path = root.join("settings.json");
        std::fs::write(&target, "{}").unwrap();
        symlink(&target, &path).unwrap();
        assert!(merge_config(
            &path,
            Path::new("/bin/student-mcp"),
            find("claude").unwrap()
        )
        .is_err());
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "{}");
        let _ = std::fs::remove_dir_all(root);
    }
}
