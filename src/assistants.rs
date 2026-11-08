//! Opt-in local MCP setup for supported assistant applications.
//!
//! Local setup uses stdio only: no school credential or MCP bearer token is
//! written to an assistant's configuration. Existing servers are preserved.

use serde_json::{json, Map, Value};
use std::path::{Path, PathBuf};

fn home() -> Result<PathBuf, String> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .ok_or("Cannot find your home directory.".into())
}

fn config_path(assistant: &str) -> Result<PathBuf, String> {
    match assistant {
        "claude" => {
            #[cfg(target_os = "macos")]
            let base = home()?.join("Library/Application Support");
            #[cfg(target_os = "windows")]
            let base = std::env::var_os("APPDATA")
                .map(PathBuf::from)
                .ok_or("Cannot find the Claude configuration directory.")?;
            #[cfg(not(any(target_os = "macos", target_os = "windows")))]
            let base = std::env::var_os("XDG_CONFIG_HOME")
                .filter(|value| Path::new(value).is_absolute())
                .map(PathBuf::from)
                .unwrap_or(home()?.join(".config"));
            Ok(base.join("Claude/claude_desktop_config.json"))
        }
        "gemini" => Ok(home()?.join(".gemini/settings.json")),
        _ => Err("This assistant does not support automatic local setup.".into()),
    }
}

fn executable_on_path(name: &str) -> bool {
    let Some(paths) = std::env::var_os("PATH") else {
        return false;
    };
    std::env::split_paths(&paths).any(|directory| {
        let candidate = directory.join(name);
        candidate.is_file()
            || (cfg!(windows)
                && ["exe", "cmd", "bat"]
                    .iter()
                    .any(|extension| candidate.with_extension(extension).is_file()))
    })
}

fn detected(assistant: &str, path: &Path) -> bool {
    match assistant {
        "claude" => {
            if path.parent().is_some_and(Path::is_dir) {
                return true;
            }
            #[cfg(target_os = "macos")]
            return Path::new("/Applications/Claude.app").exists()
                || home().is_ok_and(|home| home.join("Applications/Claude.app").exists());
            #[cfg(not(target_os = "macos"))]
            false
        }
        "gemini" => path.exists() || executable_on_path("gemini"),
        _ => false,
    }
}

fn executable() -> Result<PathBuf, String> {
    std::env::current_exe()
        .and_then(std::fs::canonicalize)
        .map_err(|error| error.to_string())
}

fn configured(path: &Path, executable: &Path) -> bool {
    let Ok(text) = std::fs::read_to_string(path) else {
        return false;
    };
    let Ok(value) = serde_json::from_str::<Value>(&text) else {
        return false;
    };
    value["mcpServers"]["student-mcp"]["command"].as_str() == executable.to_str()
}

pub fn status(assistant: &str) -> Result<Value, String> {
    if assistant == "chatgpt" {
        return Ok(
            json!({"assistant":assistant,"local":false,"detected":false,"configured":false}),
        );
    }
    let path = config_path(assistant)?;
    let executable = executable()?;
    Ok(json!({
        "assistant":assistant,
        "local":true,
        "detected":detected(assistant, &path),
        "configured":configured(&path, &executable),
    }))
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

fn merge_config(path: &Path, executable: &Path) -> Result<(), String> {
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

    let (mut root, permissions, existed) = match std::fs::symlink_metadata(path) {
        Ok(metadata) => {
            if !metadata.file_type().is_file() {
                return Err(format!(
                    "{} is not a regular file; it was left unchanged.",
                    path.display()
                ));
            }
            let text = std::fs::read_to_string(path).map_err(|error| error.to_string())?;
            let value = serde_json::from_str::<Value>(&text).map_err(|_| {
                format!(
                    "{} is not valid JSON; it was left unchanged.",
                    path.display()
                )
            })?;
            if !value.is_object() {
                return Err(format!(
                    "{} does not contain a JSON object; it was left unchanged.",
                    path.display()
                ));
            }
            (value, Some(metadata.permissions()), true)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            (Value::Object(Map::new()), None, false)
        }
        Err(error) => return Err(error.to_string()),
    };
    let root_object = root.as_object_mut().ok_or(
        "The assistant configuration does not contain a JSON object; it was left unchanged.",
    )?;
    if root_object
        .get("mcpServers")
        .is_some_and(|value| !value.is_object())
    {
        return Err(format!(
            "{} has an invalid mcpServers value; it was left unchanged.",
            path.display()
        ));
    }
    root_object
        .entry("mcpServers")
        .or_insert_with(|| Value::Object(Map::new()));
    let command = executable
        .to_str()
        .ok_or("The student-mcp path is not valid UTF-8.")?;
    root_object["mcpServers"]["student-mcp"] = json!({"command":command,"args":[]});
    let text = serde_json::to_string_pretty(&root).map_err(|error| error.to_string())? + "\n";

    if existed {
        let backup = backup_path(path)?;
        std::fs::copy(path, backup).map_err(|error| error.to_string())?;
    }
    crate::config::write_private_with_permissions(path, text.as_bytes(), permissions)
        .map_err(|error| error.to_string())
}

pub fn configure(assistant: &str) -> Result<Value, String> {
    if !matches!(assistant, "claude" | "gemini") {
        return Err("This assistant does not support automatic local setup.".into());
    }
    let path = config_path(assistant)?;
    if !detected(assistant, &path) {
        return Err("Open the local assistant once before setting it up.".into());
    }
    let executable = executable()?;
    merge_config(&path, &executable)?;
    status(assistant)
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
        merge_config(&path, executable).unwrap();
        merge_config(&path, executable).unwrap();
        let value: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(value["theme"], "dark");
        assert_eq!(value["mcpServers"]["other"]["command"], "other-server");
        assert_eq!(
            value["mcpServers"]["student-mcp"]["command"],
            executable.to_str().unwrap()
        );
        assert_eq!(value["mcpServers"]["student-mcp"]["args"], json!([]));
        assert!(configured(&path, executable));
        assert!(std::fs::read_dir(&root).unwrap().count() >= 3);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn invalid_json_is_left_untouched() {
        let root = workspace("invalid");
        let path = root.join("settings.json");
        std::fs::write(&path, "not json").unwrap();
        let error = merge_config(&path, Path::new("/bin/student-mcp")).unwrap_err();
        assert!(error.contains("left unchanged"));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "not json");
        assert_eq!(std::fs::read_dir(&root).unwrap().count(), 2);
        let _ = std::fs::remove_dir_all(root);
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
        assert!(merge_config(&path, Path::new("/bin/student-mcp")).is_err());
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "{}");
        let _ = std::fs::remove_dir_all(root);
    }
}
