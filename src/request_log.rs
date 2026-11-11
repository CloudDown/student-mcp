//! Privacy-bounded MCP request log. Never records tokens, cookies or bodies.
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

const MAX_BYTES: u64 = 1024 * 1024;

/// Replace capability and connector secrets so a shared log is not a credential.
pub(crate) fn sanitize_path(path: &str) -> String {
    let mut out = path.to_string();
    for prefix in ["/mcp/", "/s/"] {
        if let Some(start) = out.find(prefix) {
            let rest = start + prefix.len();
            if rest < out.len() {
                out.replace_range(rest.., "<redacted>");
            }
        }
    }
    out.chars()
        .map(|ch| if ch.is_control() { '?' } else { ch })
        .take(300)
        .collect()
}

fn clean_field(value: &str) -> String {
    value
        .chars()
        .map(|ch| if ch.is_control() { ' ' } else { ch })
        .take(200)
        .collect::<String>()
        .trim()
        .to_string()
}

pub(crate) fn now_seconds() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

fn log_path() -> Option<PathBuf> {
    // Unit tests must not write next to the real configuration.
    #[cfg(test)]
    {
        std::env::var_os("STUDENT_MCP_TEST_LOG").map(PathBuf::from)
    }
    #[cfg(not(test))]
    {
        Some(crate::config::config_path().with_extension("mcp.log"))
    }
}

/// Best effort: logging must never fail or slow down an MCP request.
pub(crate) fn record(method: &str, path: &str, status: u16, user_agent: Option<&str>) {
    let Some(file) = log_path() else { return };
    let line = format!(
        "{} {} {} {} ua=\"{}\"\n",
        crate::request_log::now_seconds(),
        clean_field(method),
        status,
        sanitize_path(path),
        clean_field(user_agent.unwrap_or("-"))
    );
    append(&file, &line);
}

pub(crate) fn append(path: &Path, line: &str) {
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if fs::metadata(path).is_ok_and(|meta| meta.len() > MAX_BYTES) {
        let _ = fs::rename(path, path.with_extension("log.1"));
    }
    let mut options = OpenOptions::new();
    options.create(true).append(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    if let Ok(mut file) = options.open(path) {
        let _ = file.write_all(line.as_bytes());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connector_tokens_and_controls_never_reach_the_log() {
        assert_eq!(sanitize_path("/mcp/secret-token"), "/mcp/<redacted>");
        assert_eq!(sanitize_path("/mcp/secret-token/"), "/mcp/<redacted>");
        assert_eq!(sanitize_path("/s/abcdef"), "/s/<redacted>");
        assert_eq!(sanitize_path("/mcp"), "/mcp");
        assert_eq!(sanitize_path("/other/token"), "/other/token");
        assert!(!sanitize_path("/mcp/a\nb").contains('\n'));
    }

    #[test]
    fn log_is_private_bounded_and_rotated() {
        let dir = std::env::temp_dir().join(format!(
            "student-log-{}-{}",
            std::process::id(),
            crate::config::random_hex(6)
        ));
        let path = dir.join("mcp.log");
        append(&path, "first\n");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
        fs::write(&path, vec![b'x'; MAX_BYTES as usize + 1]).unwrap();
        append(&path, "second\n");
        assert_eq!(
            fs::read_to_string(path.with_extension("log.1"))
                .unwrap()
                .len(),
            MAX_BYTES as usize + 1
        );
        assert_eq!(fs::read_to_string(&path).unwrap(), "second\n");
        let _ = fs::remove_dir_all(dir);
    }
}
