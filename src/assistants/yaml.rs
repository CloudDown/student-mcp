//! Edit native YAML settings without reserializing unrelated options.

use std::path::Path;
use yaml_edit::{Mapping, Scalar, Sequence, YamlFile};

const SERVERS: &str = "mcp_servers";
const SERVER: &str = "student-mcp";
const INVALID: &str = "Unsupported Hermes YAML configuration; the file was left unchanged.";

fn parse_file(text: &str, invalid: &'static str) -> Result<YamlFile, String> {
    let file = text.parse::<YamlFile>().map_err(|_| invalid)?;
    if file.documents().count() > 1 {
        return Err(invalid.into());
    }
    Ok(file)
}

fn check_mapping(map: &Mapping, key: &str) -> Result<(), String> {
    // YAML merge keys and duplicate entries can make the effective server
    // differ from the literal entry the editor would update.
    if map.entries().any(|entry| entry.key_matches("<<"))
        || map.entries().filter(|entry| entry.key_matches(key)).count() > 1
    {
        return Err(INVALID.into());
    }
    Ok(())
}

fn servers(file: &YamlFile) -> Result<Option<Mapping>, String> {
    let Some(document) = file.document() else {
        return Ok(None);
    };
    let root = document.as_mapping().ok_or(INVALID)?;
    check_mapping(&root, SERVERS)?;
    if let Some(node) = root.get(SERVERS) {
        let map = node.as_mapping().ok_or(INVALID)?;
        check_mapping(map, SERVER)?;
        Ok(Some(map.clone()))
    } else {
        Ok(None)
    }
}

pub(super) fn configured(text: &str, executable: &Path) -> bool {
    let Ok(Some(servers)) = parse_file(text, INVALID).and_then(|file| servers(&file)) else {
        return false;
    };
    let Some(server) = servers.get_mapping(SERVER) else {
        return false;
    };
    matches_command(&server, executable)
        && server
            .get("enabled")
            .and_then(|node| node.as_scalar().and_then(Scalar::as_bool))
            != Some(false)
        && server
            .get("disabled")
            .and_then(|node| node.as_scalar().and_then(Scalar::as_bool))
            != Some(true)
        && server.get("url").is_none()
}

fn string(map: &Mapping, key: &str) -> Option<String> {
    map.get(key)
        .and_then(|node| node.as_scalar().map(Scalar::as_string))
}

fn matches_command(map: &Mapping, executable: &Path) -> bool {
    let command = string(map, "command");
    command.is_some()
        && command.as_deref() == executable.to_str()
        && map
            .get("args")
            .is_none_or(|args| args.as_sequence().is_some_and(Sequence::is_empty))
}

pub(super) fn edit(text: &str, executable: Option<&Path>) -> Result<Option<String>, String> {
    let file = parse_file(text, INVALID)?;
    let existing = servers(&file)?;
    let Some(executable) = executable else {
        let Some(servers) = existing else {
            return Ok(None);
        };
        if servers.len() == 1 && servers.get(SERVER).is_some() {
            // An empty block mapping would instead be read as null.
            file.document()
                .ok_or(INVALID)?
                .set(SERVERS, Mapping::new_flow());
        } else if servers.remove(SERVER).is_none() {
            return Ok(None);
        }
        let rendered = file.to_string();
        if servers_after_edit(&rendered)?.is_some_and(|map| map.get(SERVER).is_some()) {
            return Err(INVALID.into());
        }
        return Ok(Some(rendered));
    };
    let command = executable
        .to_str()
        .ok_or("The student-mcp path is not valid UTF-8.")?;
    let server = Mapping::new_flow();
    server.set("command", command);
    server.set("args", Sequence::new_flow());
    if let Some(servers) = existing {
        servers.set(SERVER, &server);
    } else {
        let map = Mapping::new_flow();
        map.set(SERVER, &server);
        if !file.ensure_document().set(SERVERS, &map) {
            return Err(INVALID.into());
        }
    }
    let rendered = file.to_string();
    if !configured(&rendered, executable) {
        return Err(INVALID.into());
    }
    Ok(Some(rendered))
}

fn servers_after_edit(text: &str) -> Result<Option<Mapping>, String> {
    parse_file(text, INVALID).and_then(|file| servers(&file))
}

const CORDIS_ID: &str = "mcp-student-mcp";
const CORDIS_CLIENT: &str = "@deepseek-ai/dsh-mcp-client";
const INVALID_CORDIS: &str = "Unsupported DeepSeek Harness MCP patch; the file was left unchanged.";

// The home overlay is a sequence of Cordis operations. Own a single insert
// operation, so removing student-mcp never deletes a neighboring plugin.
fn cordis_server(patch: &Sequence) -> Result<Option<(usize, Mapping)>, String> {
    let mut found = None;
    for (index, node) in patch.values().enumerate() {
        let Some(operation) = node.as_mapping() else {
            continue;
        };
        if string(operation, "id").as_deref() == Some(CORDIS_ID) {
            return Err(INVALID_CORDIS.into());
        }
        let Some(insert) = operation.get_sequence("insert") else {
            continue;
        };
        for row in insert.values() {
            let Some(row) = row.as_mapping() else {
                continue;
            };
            if string(row, "id").as_deref() != Some(CORDIS_ID) {
                continue;
            }
            if found.is_some()
                || insert.len() != 1
                || operation.len() != 1
                || string(row, "name").as_deref() != Some(CORDIS_CLIENT)
            {
                return Err(INVALID_CORDIS.into());
            }
            found = Some((index, row.get_mapping("config").ok_or(INVALID_CORDIS)?));
        }
    }
    Ok(found)
}

fn cordis_matches(server: &Mapping, executable: &Path) -> bool {
    string(server, "transport").as_deref() == Some("stdio")
        && string(server, "serverName").as_deref() == Some(SERVER)
        && matches_command(server, executable)
}

pub(super) fn cordis_configured(text: &str, executable: &Path) -> bool {
    let Ok(file) = parse_file(text, INVALID_CORDIS) else {
        return false;
    };
    file.document()
        .and_then(|doc| doc.as_sequence())
        .and_then(|patch| cordis_server(&patch).ok().flatten())
        .is_some_and(|(_, server)| cordis_matches(&server, executable))
}

pub(super) fn edit_cordis(text: &str, executable: Option<&Path>) -> Result<Option<String>, String> {
    let mut file = parse_file(text, INVALID_CORDIS)?;
    if file.document().is_none() {
        if executable.is_none() {
            return Ok(None);
        }
        file = parse_file(&format!("{text}\n[]\n"), INVALID_CORDIS)?;
    }
    let patch = file
        .document()
        .and_then(|doc| doc.as_sequence())
        .ok_or(INVALID_CORDIS)?;
    let existing = cordis_server(&patch)?;
    if let Some(executable) = executable {
        let command = executable
            .to_str()
            .ok_or("The student-mcp path is not valid UTF-8.")?;
        if let Some((_, server)) = existing {
            server.set("serverName", SERVER);
            server.set("transport", "stdio");
            server.set("command", command);
            server.set("args", Sequence::new_flow());
        } else {
            let server = Mapping::new_flow();
            server.set("serverName", SERVER);
            server.set("transport", "stdio");
            server.set("command", command);
            server.set("args", Sequence::new_flow());
            let row = Mapping::new_flow();
            row.set("id", CORDIS_ID);
            row.set("name", CORDIS_CLIENT);
            row.set("config", &server);
            let insert = Sequence::new_flow();
            insert.push(&row);
            let operation = Mapping::new_flow();
            operation.set("insert", &insert);
            patch.push(&operation);
        }
        let rendered = file.to_string();
        if !cordis_configured(&rendered, executable) {
            return Err(INVALID_CORDIS.into());
        }
        Ok(Some(rendered))
    } else if let Some((index, _)) = existing {
        patch.remove(index);
        let rendered = file.to_string();
        let check = parse_file(&rendered, INVALID_CORDIS)?;
        if let Some(doc) = check.document() {
            if doc
                .as_sequence()
                .is_none_or(|patch| !matches!(cordis_server(&patch), Ok(None)))
            {
                return Err(INVALID_CORDIS.into());
            }
        }
        Ok(Some(rendered))
    } else {
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hermes_preserves_yaml_comments_and_other_servers() {
        let executable = Path::new("/opt/Student MCP/bin/student-mcp");
        for original in [
            "# Keep this comment\nmodel: custom\n",
            "# Keep this comment\nmodel: custom\nmcp_servers:\n  # Other server\n  other:\n    command: other\n    args: ['-a']\n",
            "# Keep this comment\nmodel: custom\nmcp_servers: {other: {command: other}}\n",
            "# Keep this comment\nmodel: custom\nmcp_servers: {}\n",
        ] {
            let updated = edit(original, Some(executable)).unwrap().unwrap();
            assert!(configured(&updated, executable), "{updated}");
            for line in original.lines().filter(|line| !line.contains("mcp_servers")) {
                assert!(updated.contains(line), "{updated}");
            }
            let removed = edit(&updated, None).unwrap().unwrap();
            assert!(!configured(&removed, executable), "{removed}");
            assert!(edit(&removed, None).unwrap().is_none(), "{removed}");
            for line in original.lines().filter(|line| !line.contains("mcp_servers")) {
                assert!(removed.contains(line), "{removed}");
            }
        }
    }

    #[test]
    fn hermes_refuses_ambiguous_or_invalid_mcp_settings() {
        for original in [
            "mcp_servers: [other]\n",
            "mcp_servers: [broken\n",
            "mcp_servers: {}\nmcp_servers: {}\n",
            "mcp_servers: {student-mcp: {}, student-mcp: {}}\n",
            "defaults: &defaults {mcp_servers: {}}\n<<: *defaults\n",
            "first: value\n---\nsecond: value\n",
        ] {
            assert!(
                edit(original, Some(Path::new("/bin/student-mcp"))).is_err(),
                "{original}"
            );
            assert!(edit(original, None).is_err(), "{original}");
        }
    }

    #[test]
    fn cordis_adds_and_removes_one_operation_without_touching_the_others() {
        let executable = Path::new("/opt/Student MCP/bin/student-mcp");
        for original in [
            "[]",
            "# Keep home overlay comment\n",
            "# Keep home overlay comment\n- insert:\n    - id: other\n      name: other-plugin\n      config: {value: !!js 'process.env.OTHER'}\n",
        ] {
            let updated = edit_cordis(original, Some(executable)).unwrap().unwrap();
            assert!(cordis_configured(&updated, executable), "{updated}");
            assert_eq!(
                edit_cordis(&updated, Some(executable)).unwrap().unwrap(),
                updated,
            );
            let removed = edit_cordis(&updated, None).unwrap().unwrap();
            assert!(!cordis_configured(&removed, executable), "{removed}");
            assert!(edit_cordis(&removed, None).unwrap().is_none(), "{removed}");
            for line in original.lines().filter(|line| *line != "[]") {
                assert!(updated.contains(line) && removed.contains(line), "{line}\n{updated}\n{removed}");
            }
        }
        let foreign = "- insert: [{id: mcp-student-mcp, name: 'other-plugin', config: {}}]\n";
        assert!(edit_cordis(foreign, Some(executable)).is_err());
        assert!(edit_cordis(foreign, None).is_err());
    }
}
