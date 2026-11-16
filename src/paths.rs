//! Where student-mcp keeps its files.
//!
//! Unix (Linux, macOS) follows the XDG base directory specification:
//!
//! ```text
//! $XDG_CONFIG_HOME/student-mcp/config.json    settings and secrets (0600)
//! $XDG_DATA_HOME/student-mcp/index/           private Moodle index
//! $XDG_DATA_HOME/student-mcp/browsers/        Chromium profiles per account
//! $XDG_DATA_HOME/student-mcp/links/           unexpired /s/ links
//! $XDG_STATE_HOME/student-mcp/activity.log    tool and Moodle activity
//! $XDG_STATE_HOME/student-mcp/mcp.log         inbound MCP requests
//! $XDG_STATE_HOME/student-mcp/srvus-key       tunnel identity (0600)
//! $XDG_STATE_HOME/student-mcp/srvus-host      pinned tunnel host key
//! ```
//!
//! Windows uses `%APPDATA%\student-mcp` for configuration and
//! `%LOCALAPPDATA%\student-mcp` for data and state.
//!
//! `STUDENT_MCP_CONFIG_FILE` (or the older `MOODLE_CONFIG_FILE`) still selects one
//! explicit config file; when set, every side file stays next to that file, so an
//! isolated test or an advanced setup keeps everything in one directory.

use std::path::{Path, PathBuf};

const APP: &str = "student-mcp";

/// Environment the layout is derived from. Kept as a plain value so tests can
/// exercise Windows, XDG and override rules without mutating the process.
#[derive(Clone, Default, Debug)]
pub struct Env {
    pub home: Option<String>,
    pub xdg_config: Option<String>,
    pub xdg_data: Option<String>,
    pub xdg_state: Option<String>,
    pub appdata: Option<String>,
    pub localappdata: Option<String>,
    pub config_override: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Layout {
    pub config_file: PathBuf,
    pub config_dir: PathBuf,
    pub data_dir: PathBuf,
    pub state_dir: PathBuf,
}

impl Layout {
    pub fn lock(&self) -> PathBuf {
        self.config_dir.join("config.lock")
    }
    pub fn index_dir(&self) -> PathBuf {
        self.data_dir.join("index")
    }
    pub fn browsers_dir(&self) -> PathBuf {
        self.data_dir.join("browsers")
    }
    pub fn links_dir(&self) -> PathBuf {
        self.data_dir.join("links")
    }
    pub fn activity_log(&self) -> PathBuf {
        self.state_dir.join("activity.log")
    }
    pub fn mcp_log(&self) -> PathBuf {
        self.state_dir.join("mcp.log")
    }
    pub fn srvus_key(&self) -> PathBuf {
        self.config_dir.join("srvus-key")
    }
    pub fn srvus_host(&self) -> PathBuf {
        self.config_dir.join("srvus-host")
    }
}

fn non_empty(value: Option<&String>) -> Option<PathBuf> {
    let value = value?;
    let trimmed = value.trim();
    (!trimmed.is_empty()).then(|| PathBuf::from(trimmed))
}

fn absolute(value: Option<&String>) -> Option<PathBuf> {
    let path = non_empty(value)?;
    path.is_absolute().then_some(path)
}

fn home_of(env: &Env) -> Option<PathBuf> {
    absolute(env.home.as_ref())
}

/// Resolve a layout from an environment snapshot.
pub fn layout_from(env: &Env) -> Layout {
    if let Some(file) = absolute(env.config_override.as_ref()) {
        let dir = file
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."));
        return Layout {
            config_file: file,
            config_dir: dir.clone(),
            data_dir: dir.clone(),
            state_dir: dir,
        };
    }
    let windows = env.appdata.is_some() || env.localappdata.is_some();
    if windows {
        let roaming = non_empty(env.appdata.as_ref())
            .or_else(|| home_of(env).map(|home| home.join("AppData/Roaming")))
            .unwrap_or_else(|| PathBuf::from("."));
        let local = non_empty(env.localappdata.as_ref())
            .or_else(|| home_of(env).map(|home| home.join("AppData/Local")))
            .unwrap_or_else(|| roaming.clone());
        let config_dir = roaming.join(APP);
        let state_dir = local.join(APP);
        return Layout {
            config_file: config_dir.join("config.json"),
            config_dir,
            data_dir: state_dir.clone(),
            state_dir,
        };
    }
    let home = home_of(env).unwrap_or_else(|| PathBuf::from("."));
    let config_dir = absolute(env.xdg_config.as_ref())
        .unwrap_or_else(|| home.join(".config"))
        .join(APP);
    let data_dir = absolute(env.xdg_data.as_ref())
        .unwrap_or_else(|| home.join(".local/share"))
        .join(APP);
    let state_dir = absolute(env.xdg_state.as_ref())
        .unwrap_or_else(|| home.join(".local/state"))
        .join(APP);
    Layout {
        config_file: config_dir.join("config.json"),
        config_dir,
        data_dir,
        state_dir,
    }
}

fn env_value(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .filter(|value| !value.trim().is_empty())
}

/// Snapshot of the real environment.
pub fn current_env() -> Env {
    Env {
        home: env_value("HOME").or_else(|| env_value("USERPROFILE")),
        xdg_config: env_value("XDG_CONFIG_HOME"),
        xdg_data: env_value("XDG_DATA_HOME"),
        xdg_state: env_value("XDG_STATE_HOME"),
        appdata: env_value("APPDATA"),
        localappdata: env_value("LOCALAPPDATA"),
        config_override: env_value("STUDENT_MCP_CONFIG_FILE")
            .or_else(|| env_value("MOODLE_CONFIG_FILE")),
    }
}

pub fn layout() -> Layout {
    layout_from(&current_env())
}

/// Files the pre-XDG layout kept next to `~/.student-mcp.json`.
fn legacy_home(env: &Env) -> Option<PathBuf> {
    home_of(env)
}

/// Move the legacy `~/.student-mcp.*` files into the XDG layout.
///
/// Returns one line per moved entry. Never deletes: an existing target wins, so a
/// failed or partial migration can simply be run again.
pub fn migrate_from(env: &Env) -> Vec<String> {
    // An explicit config file means the caller owns the layout (tests, portable
    // setups): never move files out of the real home for those.
    if env.config_override.is_some() {
        return Vec::new();
    }
    let Some(home) = legacy_home(env) else {
        return Vec::new();
    };
    let layout = layout_from(env);
    if layout.config_dir == home {
        return Vec::new();
    }
    let moves: Vec<(PathBuf, PathBuf)> = vec![
        (home.join(".student-mcp.json"), layout.config_file.clone()),
        (home.join(".student-mcp.lock"), layout.lock()),
        (home.join(".student-mcp.index"), layout.index_dir()),
        (home.join(".student-mcp.browsers"), layout.browsers_dir()),
        (home.join(".student-mcp.links"), layout.links_dir()),
        (
            home.join(".student-mcp.activity.log"),
            layout.activity_log(),
        ),
        (home.join(".student-mcp.mcp.log"), layout.mcp_log()),
        (home.join(".student-mcp.srvus-key"), layout.srvus_key()),
        (home.join(".student-mcp.srvus-host"), layout.srvus_host()),
    ];
    let mut notes = Vec::new();
    for (from, to) in moves {
        if !from.exists() || to.exists() {
            continue;
        }
        if let Some(parent) = to.parent() {
            if std::fs::create_dir_all(parent).is_err() {
                continue;
            }
        }
        match std::fs::rename(&from, &to) {
            Ok(()) => notes.push(format!("{} -> {}", from.display(), to.display())),
            Err(_) => {
                // Different filesystems: copy, then drop the legacy entry only if
                // the copy landed. A partial copy is worse than no migration.
                if let Ok(bytes) = std::fs::read(&from) {
                    if std::fs::write(&to, bytes).is_ok() {
                        let _ = std::fs::remove_file(&from);
                        notes.push(format!("{} -> {} (copied)", from.display(), to.display()));
                    }
                }
            }
        }
    }
    notes
}

/// Create the directories the app writes to.
pub fn prepare(layout: &Layout) -> std::io::Result<()> {
    for dir in [&layout.config_dir, &layout.data_dir, &layout.state_dir] {
        std::fs::create_dir_all(dir)?;
    }
    Ok(())
}

/// Startup hook: create directories, then adopt the pre-XDG `~/.student-mcp.*`
/// files once. Prints nothing unless something moved, and never fails startup.
pub fn init() {
    #[cfg(not(test))]
    {
        let env = current_env();
        let layout = layout_from(&env);
        let _ = prepare(&layout);
        for note in migrate_from(&env) {
            eprintln!("student-mcp: moved legacy file {note}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unix_env() -> Env {
        Env {
            home: Some("/home/lea".into()),
            ..Default::default()
        }
    }

    #[test]
    fn unix_defaults_follow_xdg() {
        let layout = layout_from(&unix_env());
        assert_eq!(
            layout.config_file,
            PathBuf::from("/home/lea/.config/student-mcp/config.json")
        );
        assert_eq!(
            layout.index_dir(),
            PathBuf::from("/home/lea/.local/share/student-mcp/index")
        );
        assert_eq!(
            layout.activity_log(),
            PathBuf::from("/home/lea/.local/state/student-mcp/activity.log")
        );
    }

    #[test]
    fn xdg_variables_win_when_absolute() {
        let env = Env {
            xdg_config: Some("/xdg/conf".into()),
            xdg_data: Some("/xdg/data".into()),
            xdg_state: Some("/xdg/state".into()),
            ..unix_env()
        };
        let layout = layout_from(&env);
        assert_eq!(
            layout.config_file,
            PathBuf::from("/xdg/conf/student-mcp/config.json")
        );
        assert_eq!(
            layout.browsers_dir(),
            PathBuf::from("/xdg/data/student-mcp/browsers")
        );
        assert_eq!(
            layout.mcp_log(),
            PathBuf::from("/xdg/state/student-mcp/mcp.log")
        );
    }

    #[test]
    fn relative_xdg_values_are_ignored() {
        let env = Env {
            xdg_config: Some("relative/path".into()),
            ..unix_env()
        };
        assert_eq!(
            layout_from(&env).config_dir,
            PathBuf::from("/home/lea/.config/student-mcp")
        );
    }

    #[test]
    fn windows_uses_roaming_config_and_local_data() {
        let roaming = PathBuf::from(r"C:\Users\Lea\AppData\Roaming");
        let local = PathBuf::from(r"C:\Users\Lea\AppData\Local");
        let env = Env {
            home: Some(r"C:\Users\Lea".into()),
            appdata: Some(roaming.display().to_string()),
            localappdata: Some(local.display().to_string()),
            ..Default::default()
        };
        let layout = layout_from(&env);
        assert_eq!(
            layout.config_file,
            roaming.join("student-mcp").join("config.json")
        );
        assert_eq!(layout.data_dir, layout.state_dir);
        assert_eq!(layout.index_dir(), local.join("student-mcp").join("index"));
    }

    #[test]
    fn explicit_config_file_keeps_side_files_beside_it() {
        let env = Env {
            config_override: Some("/tmp/student-mcp-test/settings.json".into()),
            ..unix_env()
        };
        let layout = layout_from(&env);
        assert_eq!(
            layout.config_file,
            PathBuf::from("/tmp/student-mcp-test/settings.json")
        );
        assert_eq!(
            layout.index_dir(),
            PathBuf::from("/tmp/student-mcp-test/index")
        );
        assert_eq!(layout.state_dir, PathBuf::from("/tmp/student-mcp-test"));
    }

    #[test]
    fn migration_moves_legacy_files_once_and_never_overwrites() {
        let root = std::env::temp_dir().join(format!("student-mcp-migrate-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let home = root.join("home");
        std::fs::create_dir_all(home.join(".student-mcp.index")).unwrap();
        std::fs::write(home.join(".student-mcp.json"), "{\"legacy\":true}").unwrap();
        std::fs::write(home.join(".student-mcp.index/a.json"), "{}").unwrap();
        let env = Env {
            home: Some(home.display().to_string()),
            ..Default::default()
        };

        let notes = migrate_from(&env);
        assert_eq!(notes.len(), 2, "{notes:?}");
        assert!(!home.join(".student-mcp.json").exists());
        assert!(home.join(".config/student-mcp/config.json").is_file());
        assert!(home.join(".local/share/student-mcp/index/a.json").is_file());

        assert!(migrate_from(&env).is_empty(), "second run must be a no-op");

        // A target that already exists always wins: migration never merges.
        std::fs::write(home.join(".student-mcp.json"), "legacy").unwrap();
        assert!(migrate_from(&env).is_empty());
        assert_eq!(
            std::fs::read_to_string(home.join(".config/student-mcp/config.json")).unwrap(),
            "{\"legacy\":true}"
        );
        let _ = std::fs::remove_dir_all(&root);
    }
}
