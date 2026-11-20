//! Bounded, read-only discovery. Never launch an assistant or crawl the disk.

use std::io::Read;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Format {
    Json,
    Jsonc,
    OpenCode,
    Toml,
    Yaml,
    Cordis,
}

pub(super) struct Assistant {
    pub id: &'static str,
    pub name: &'static str,
    pub format: Format,
    config: &'static str,
    commands: &'static [&'static str],
    marker: &'static str,
    app: &'static str,
    windows_app: &'static str,
}

impl Assistant {
    pub fn servers_path(&self) -> &'static [&'static str] {
        match self.id {
            "amp" => &["amp.mcpServers"],
            "crush" | "kilo" => &["mcp"],
            "opencode" | "openclaw" => &["mcp", "servers"],
            "hermes" => &["mcp_servers"],
            _ => &["mcpServers"],
        }
    }
}

pub(super) const ASSISTANTS: &[Assistant] = &[
    Assistant {
        id: "claude",
        name: "Claude Desktop",
        format: Format::Json,
        config: "Claude/claude_desktop_config.json",
        commands: &["claude-desktop"],
        marker: "",
        app: "Claude",
        windows_app: "AnthropicClaude/claude.exe",
    },
    Assistant {
        id: "claude-code",
        name: "Claude Code",
        format: Format::Json,
        config: ".claude.json",
        commands: &["claude"],
        marker: ".claude",
        app: "",
        windows_app: "",
    },
    Assistant {
        id: "codex",
        name: "Codex / ChatGPT Desktop",
        format: Format::Toml,
        config: ".codex/config.toml",
        commands: &["codex"],
        marker: ".codex",
        app: "ChatGPT",
        windows_app: "",
    },
    Assistant {
        id: "gemini",
        name: "Gemini CLI",
        format: Format::Json,
        config: ".gemini/settings.json",
        commands: &["gemini"],
        marker: ".gemini",
        app: "",
        windows_app: "",
    },
    Assistant {
        id: "opencode",
        name: "OpenCode",
        format: Format::OpenCode,
        config: "opencode/opencode.json",
        commands: &["opencode", "opencode-desktop"],
        marker: ".opencode",
        app: "OpenCode",
        windows_app: "OpenCode/OpenCode.exe",
    },
    Assistant {
        id: "cursor",
        name: "Cursor / Cursor CLI",
        format: Format::Json,
        config: ".cursor/mcp.json",
        commands: &["cursor", "cursor-agent"],
        marker: ".cursor",
        app: "Cursor",
        windows_app: "cursor/Cursor.exe",
    },
    Assistant {
        id: "grok-build",
        name: "Grok Build (CLI)",
        format: Format::Toml,
        config: ".grok/config.toml",
        commands: &["grok"],
        marker: ".grok",
        app: "",
        windows_app: "",
    },
    Assistant {
        id: "omp",
        name: "Oh My Pi",
        format: Format::Json,
        config: ".omp/agent/mcp.json",
        commands: &["omp"],
        marker: ".omp",
        app: "",
        windows_app: "",
    },
    Assistant {
        id: "vscode",
        name: "GitHub Copilot / VS Code",
        format: Format::Json,
        config: ".copilot/mcp-config.json",
        commands: &["copilot", "code", "code-insiders"],
        marker: ".vscode",
        app: "Visual Studio Code",
        windows_app: "Microsoft VS Code/Code.exe",
    },
    Assistant {
        id: "windsurf",
        name: "Windsurf",
        format: Format::Json,
        config: ".codeium/windsurf/mcp_config.json",
        commands: &["windsurf"],
        marker: ".codeium/windsurf",
        app: "Windsurf",
        windows_app: "Windsurf/Windsurf.exe",
    },
    // Omarchy's current default-agent catalogue is a discovery reference, not
    // a reason to copy its launch flags or disable tool approval policies.
    Assistant {
        id: "pi",
        name: "Pi",
        format: Format::Jsonc,
        config: ".pi/agent/mcp.json",
        commands: &["pi"],
        marker: ".pi/agent",
        app: "",
        windows_app: "",
    },
    Assistant {
        id: "qwen",
        name: "Qwen Code",
        format: Format::Json,
        config: ".qwen/settings.json",
        commands: &["qwen"],
        marker: ".qwen",
        app: "",
        windows_app: "",
    },
    Assistant {
        id: "kiro",
        name: "Kiro / Kiro CLI",
        format: Format::Json,
        config: ".kiro/settings/mcp.json",
        commands: &["kiro-cli", "kiro"],
        marker: ".kiro",
        app: "Kiro",
        windows_app: "Kiro/Kiro.exe",
    },
    Assistant {
        id: "amp",
        name: "Amp",
        format: Format::Jsonc,
        config: "amp/settings.json",
        commands: &["amp"],
        marker: "",
        app: "",
        windows_app: "",
    },
    Assistant {
        id: "crush",
        name: "Crush",
        format: Format::Json,
        config: "crush/crush.json",
        commands: &["crush"],
        marker: "",
        app: "",
        windows_app: "",
    },
    Assistant {
        id: "devin",
        name: "Devin CLI",
        format: Format::Json,
        config: "devin/mcp_config.json",
        commands: &["devin"],
        marker: ".devin",
        app: "Devin",
        windows_app: "Devin/Devin.exe",
    },
    Assistant {
        id: "hermes",
        name: "Hermes Agent / Desktop",
        format: Format::Yaml,
        config: ".hermes/config.yaml",
        commands: &["hermes", "hermes-desktop"],
        marker: "",
        app: "Hermes",
        windows_app: "",
    },
    Assistant {
        id: "antigravity",
        name: "Google Antigravity",
        format: Format::Json,
        config: ".gemini/config/mcp_config.json",
        commands: &["agy", "antigravity"],
        marker: ".gemini/antigravity-cli",
        app: "Antigravity",
        windows_app: "Google/Antigravity/Antigravity.exe",
    },
    Assistant {
        id: "command-code",
        name: "Command Code",
        format: Format::Json,
        config: ".commandcode/mcp.json",
        commands: &["commandcode", "command-code", "cmdc", "cmd"],
        marker: ".commandcode",
        app: "",
        windows_app: "",
    },
    Assistant {
        id: "kilo",
        name: "Kilo Code (CLI)",
        format: Format::Jsonc,
        config: "kilo/kilo.json",
        commands: &["kilo"],
        marker: "",
        app: "",
        windows_app: "",
    },
    Assistant {
        id: "kimi",
        name: "Kimi Code (CLI)",
        format: Format::Json,
        config: ".kimi-code/mcp.json",
        commands: &["kimi"],
        marker: ".kimi-code",
        app: "",
        windows_app: "",
    },
    Assistant {
        id: "openclaw",
        name: "OpenClaw",
        format: Format::Jsonc,
        config: ".openclaw/openclaw.json",
        commands: &["openclaw"],
        marker: "",
        app: "",
        windows_app: "",
    },
    Assistant {
        id: "ori",
        name: "Ori (OpenRouter)",
        format: Format::Json,
        config: ".ori/mcp.json",
        commands: &["ori"],
        marker: ".ori",
        app: "Ori",
        windows_app: "",
    },
    Assistant {
        id: "minimax-code",
        name: "MiniMax Code",
        format: Format::Json,
        config: ".minimax/mcp.json",
        commands: &["mcode"],
        marker: "",
        app: "MiniMax Code",
        windows_app: "",
    },
    Assistant {
        id: "deepseek-harness",
        name: "DeepSeek Harness",
        format: Format::Cordis,
        config: ".dsh/cordis.patch.yml",
        commands: &["dsh"],
        marker: "",
        app: "",
        windows_app: "",
    },
    Assistant {
        id: "muse",
        name: "Muse Code (Meta)",
        format: Format::Json,
        config: "muse/settings.json",
        commands: &["muse"],
        marker: "",
        app: "",
        windows_app: "",
    },
    Assistant {
        id: "freebuff",
        name: "Freebuff",
        format: Format::Json,
        config: ".agents/mcp.json",
        commands: &["freebuff", "freebuff-desktop"],
        marker: "",
        app: "Freebuff",
        windows_app: "",
    },
];

pub(super) fn find(id: &str) -> Result<&'static Assistant, String> {
    ASSISTANTS
        .iter()
        .find(|item| item.id == id)
        .ok_or_else(|| "This assistant does not support automatic local setup.".into())
}

pub(super) struct Computer {
    pub home: PathBuf,
    pub config: PathBuf,
    pub support: PathBuf,
    pub bins: Vec<PathBuf>,
    pub apps: Vec<PathBuf>,
    pub programs: Vec<PathBuf>,
    pub windows: bool,
    pub overrides: Vec<(&'static str, Result<PathBuf, String>)>,
    pub mise_installs: PathBuf,
}

impl Computer {
    pub fn current() -> Result<Self, String> {
        let home = std::env::var_os("HOME")
            .or_else(|| std::env::var_os("USERPROFILE"))
            .map(PathBuf::from)
            .ok_or("Cannot find your home directory.")?;
        let config = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .filter(|path| path.is_absolute())
            .unwrap_or_else(|| home.join(".config"));
        let support = if cfg!(target_os = "macos") {
            home.join("Library/Application Support")
        } else if cfg!(windows) {
            std::env::var_os("APPDATA")
                .map(PathBuf::from)
                .ok_or("Cannot find your application configuration directory.")?
        } else {
            config.clone()
        };
        let mut bins: Vec<PathBuf> = std::env::var_os("PATH")
            .map(|paths| {
                std::env::split_paths(&paths)
                    .filter(|p| p.is_absolute())
                    .collect()
            })
            .unwrap_or_default();
        for relative in [
            ".local/bin",
            ".bun/bin",
            ".opencode/bin",
            ".cargo/bin",
            ".npm-global/bin",
        ] {
            bins.push(home.join(relative));
        }
        if let Some(appdata) = std::env::var_os("APPDATA") {
            bins.push(PathBuf::from(appdata).join("npm"));
        }
        let mut programs = Vec::new();
        if let Some(local) = std::env::var_os("LOCALAPPDATA") {
            programs.push(PathBuf::from(&local).join("Programs"));
            programs.push(PathBuf::from(local));
        }
        for variable in ["ProgramFiles", "ProgramFiles(x86)"] {
            if let Some(path) = std::env::var_os(variable) {
                programs.push(PathBuf::from(path));
            }
        }
        let mut overrides = Vec::new();
        for (id, variable, file) in [
            ("codex", "CODEX_HOME", "config.toml"),
            ("vscode", "COPILOT_HOME", "mcp-config.json"),
            ("kimi", "KIMI_CODE_HOME", "mcp.json"),
        ] {
            if let Some(directory) = absolute_env(variable) {
                overrides.push((id, Ok(directory.join(file))));
            }
        }
        if let Some(path) = absolute_env("ORI_MCP_CONFIG") {
            overrides.push(("ori", Ok(path)));
        }
        overrides.push(("minimax-code", minimax_config(&home)));
        overrides.push((
            "deepseek-harness",
            user_path_env(&home, "DSH_HOME").map(|path| {
                path.unwrap_or_else(|| home.join(".dsh"))
                    .join("cordis.patch.yml")
            }),
        ));
        overrides.push(("openclaw", openclaw_config(&home)));
        let hermes_base = if cfg!(windows) {
            absolute_env("LOCALAPPDATA").unwrap_or_else(|| home.join("AppData/Local"))
        } else {
            home.clone()
        };
        let suffix = std::env::var("HERMES_DATA_DIR_SUFFIX").unwrap_or_default();
        let hermes_dir = format!(
            "{}{suffix}",
            if cfg!(windows) { "hermes" } else { ".hermes" }
        );
        let hermes_path = if hermes_dir.contains(['/', '\\']) {
            Err("The Hermes data directory suffix is invalid.".into())
        } else {
            let native = hermes_base.join(hermes_dir);
            let explicit = std::env::var_os("HERMES_HOME")
                .filter(|path| !path.is_empty())
                .map(PathBuf::from)
                .map(|path| match path.strip_prefix("~") {
                    Ok(relative) => home.join(relative),
                    Err(_) => path,
                });
            hermes_config(&native, explicit.as_deref())
        };
        overrides.push(("hermes", hermes_path));
        let omp_root = absolute_env("PI_CONFIG_DIR").unwrap_or_else(|| home.join(".omp"));
        let profile = std::env::var("OMP_PROFILE")
            .or_else(|_| std::env::var("PI_PROFILE"))
            .ok()
            .filter(|p| !p.is_empty() && p != "default");
        let omp_agent = if let Some(profile) = profile {
            if Path::new(&profile).components().count() != 1
                || !matches!(
                    Path::new(&profile).components().next(),
                    Some(std::path::Component::Normal(_))
                )
            {
                return Err("The Oh My Pi profile name is invalid.".into());
            }
            omp_root.join("profiles").join(profile).join("agent")
        } else {
            absolute_env("PI_CODING_AGENT_DIR").unwrap_or_else(|| omp_root.join("agent"))
        };
        overrides.push(("omp", Ok(omp_agent.join("mcp.json"))));
        if let Some(agent_dir) = absolute_env("PI_CODING_AGENT_DIR") {
            overrides.push(("pi", Ok(agent_dir.join("mcp.json"))));
        }
        let data = absolute_env("XDG_DATA_HOME").unwrap_or_else(|| home.join(".local/share"));
        let mise_installs = absolute_env("MISE_DATA_DIR")
            .unwrap_or_else(|| data.join("mise"))
            .join("installs");
        Ok(Self {
            apps: vec![PathBuf::from("/Applications"), home.join("Applications")],
            home,
            config,
            support,
            bins,
            programs,
            windows: cfg!(windows),
            overrides,
            mise_installs,
        })
    }

    pub fn path(&self, assistant: &Assistant) -> PathBuf {
        match assistant.id {
            "claude" => self.support.join(assistant.config),
            "devin" if self.windows => self.support.join(assistant.config),
            "crush" | "devin" | "muse" => self.config.join(assistant.config),
            "kilo" => {
                let json = self.config.join(assistant.config);
                let folder = json.parent().unwrap();
                // Kilo loads multiple global layers; edit the highest existing
                // one so an older definition cannot shadow this change.
                [
                    "opencode.jsonc",
                    "opencode.json",
                    "kilo.jsonc",
                    "kilo.json",
                    "config.json",
                ]
                .iter()
                .map(|name| folder.join(name))
                .find(|path| path.symlink_metadata().is_ok())
                .unwrap_or(json)
            }
            "amp" | "opencode" => {
                let json = self.config.join(assistant.config);
                let jsonc = json.with_extension("jsonc");
                // OpenCode prioritizes JSONC; Amp uses JSONC as a fallback.
                // Never bypass an existing symlink or an invalid native file.
                if jsonc.symlink_metadata().is_ok()
                    && (assistant.id == "opencode" || json.symlink_metadata().is_err())
                {
                    jsonc
                } else {
                    json
                }
            }
            _ => self.home.join(assistant.config),
        }
    }

    pub fn config_path(&self, assistant: &Assistant) -> Result<PathBuf, String> {
        self.overrides
            .iter()
            .find(|(id, _)| *id == assistant.id)
            .map_or_else(|| Ok(self.path(assistant)), |(_, path)| path.clone())
    }

    pub fn detected(&self, assistant: &Assistant, path: &Path) -> bool {
        // ~/.agents/mcp.json is shared, not evidence that Freebuff is installed.
        (assistant.id != "freebuff" && path.exists())
            || match assistant.id {
                "hermes" => path.parent().is_some_and(|home| {
                    let profile_dir = home.parent().filter(|dir| dir.ends_with("profiles"));
                    let root = profile_dir.and_then(Path::parent).unwrap_or(home);
                    root.join("hermes-agent/pyproject.toml").is_file()
                }),
                "openclaw" => {
                    is_executable(&self.home.join(".openclaw/bin/openclaw"), self.windows)
                }
                "antigravity" => self.home.join(".gemini/antigravity").is_dir(),
                "minimax-code" => path
                    .parent()
                    .is_some_and(|directory| directory.join("config.yaml").is_file()),
                _ => false,
            }
            || (matches!(
                assistant.id,
                "claude" | "vscode" | "amp" | "crush" | "devin"
            ) && path.parent().is_some_and(Path::is_dir))
            || (!assistant.marker.is_empty() && self.home.join(assistant.marker).is_dir())
            || (!assistant.app.is_empty()
                && self
                    .apps
                    .iter()
                    .any(|root| root.join(format!("{}.app", assistant.app)).is_dir()))
            || (!assistant.windows_app.is_empty()
                && self
                    .programs
                    .iter()
                    .any(|root| root.join(assistant.windows_app).is_file()))
            || assistant.commands.iter().any(|name| {
                // Windows itself ships cmd.exe; the npm package also provides
                // three unambiguous command names on every platform.
                if self.windows && *name == "cmd" {
                    return false;
                }
                self.bins.iter().any(|directory| {
                    if directory == &self.mise_installs.with_file_name("shims") {
                        return false;
                    }
                    let candidate = directory.join(name);
                    is_executable(&candidate, self.windows)
                        || (self.windows
                            && ["exe", "cmd", "bat"]
                                .iter()
                                .any(|ext| is_executable(&candidate.with_extension(ext), true)))
                })
            })
            || self.mise_detected(assistant)
    }

    fn mise_detected(&self, assistant: &Assistant) -> bool {
        // Omarchy ships lazy launchers even for tools not installed yet. Check
        // only known mise tool directories, at most 32 versions each; no launch.
        let alias = match assistant.id {
            "omp" => "oh-my-pi",
            "kiro" => "http-kiro-cli",
            "devin" => "http-devin",
            "antigravity" => "antigravity-cli",
            "command-code" => "npm-command-code",
            "kilo" => "npm-kilocode-cli",
            "kimi" => "npm-moonshot-ai-kimi-code",
            "minimax-code" => "npm-minimax-ai-code",
            "deepseek-harness" => "npm-deepseek-ai-dsh",
            "muse" => "http-muse",
            "freebuff" => "npm-freebuff",
            _ => assistant.id,
        };
        assistant
            .commands
            .iter()
            .copied()
            .chain([alias])
            .any(|tool| {
                let Ok(versions) = std::fs::read_dir(self.mise_installs.join(tool)) else {
                    return false;
                };
                versions.take(32).flatten().any(|version| {
                    assistant.commands.iter().any(|command| {
                        [
                            version.path(),
                            version.path().join("bin"),
                            version.path().join("node_modules/.bin"),
                        ]
                        .iter()
                        .any(|dir| {
                            let candidate = dir.join(command);
                            is_executable(&candidate, self.windows)
                                || (self.windows
                                    && ["exe", "cmd", "bat"].iter().any(|ext| {
                                        is_executable(&candidate.with_extension(ext), true)
                                    }))
                        })
                    })
                })
            })
    }
}

fn simple_profile(name: &str) -> bool {
    !name.is_empty()
        && name
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
}

fn minimax_config(home: &Path) -> Result<PathBuf, String> {
    for name in ["MINIMAX_DATA_DIR", "MAVIS_DATA_DIR"] {
        if let Some(value) = std::env::var_os(name).filter(|value| !value.is_empty()) {
            let path = PathBuf::from(value);
            if !path.is_absolute() {
                return Err(format!(
                    "Use an absolute {name} before setting up MiniMax Code."
                ));
            }
            return Ok(path.join("mcp.json"));
        }
    }
    let profile = std::env::var("MAVIS_PROFILE").unwrap_or_default();
    let directory = match profile.trim() {
        "" | "default" => ".minimax".into(),
        name if simple_profile(name) => format!(".minimax-{name}"),
        _ => return Err("The MiniMax Code profile name is invalid.".into()),
    };
    Ok(home.join(directory).join("mcp.json"))
}

fn openclaw_config(home: &Path) -> Result<PathBuf, String> {
    if let Some(path) = user_path_env(home, "OPENCLAW_CONFIG_PATH")? {
        return Ok(path);
    }
    if let Some(state) = user_path_env(home, "OPENCLAW_STATE_DIR")? {
        return Ok(state.join("openclaw.json"));
    }
    let profile = std::env::var("OPENCLAW_PROFILE").unwrap_or_default();
    let folder = match profile.trim() {
        "" | "default" => ".openclaw".into(),
        name if simple_profile(name) => format!(".openclaw-{name}"),
        _ => return Err("The OpenClaw profile name is invalid.".into()),
    };
    let root = user_path_env(home, "OPENCLAW_HOME")?.unwrap_or_else(|| home.to_path_buf());
    Ok(root.join(folder).join("openclaw.json"))
}

fn user_path_env(home: &Path, name: &str) -> Result<Option<PathBuf>, String> {
    let Some(value) = std::env::var_os(name).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    let path = PathBuf::from(value);
    let path = match path.strip_prefix("~") {
        Ok(relative) => home.join(relative),
        Err(_) => path,
    };
    if path.is_absolute()
        && !path
            .components()
            .any(|c| c == std::path::Component::ParentDir)
    {
        Ok(Some(path))
    } else {
        Err(format!(
            "Use an absolute {name} before setting up this assistant."
        ))
    }
}

fn hermes_config(native: &Path, explicit: Option<&Path>) -> Result<PathBuf, String> {
    let home = explicit.unwrap_or(native);
    if !home.is_absolute()
        || home
            .components()
            .any(|c| c == std::path::Component::ParentDir)
    {
        return Err("Use an absolute HERMES_HOME before setting up Hermes.".into());
    }
    if home
        .parent()
        .and_then(Path::file_name)
        .is_some_and(|name| name == "profiles")
    {
        return Ok(home.join("config.yaml"));
    }
    let root = if home.starts_with(native) {
        native
    } else {
        home
    };
    let active = root.join("active_profile");
    let file = match std::fs::symlink_metadata(&active) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(home.join("config.yaml"))
        }
        Ok(meta) if meta.is_file() && meta.len() <= 256 => std::fs::File::open(active).ok(),
        _ => None,
    };
    let mut name = String::new();
    if file
        .and_then(|file| file.take(257).read_to_string(&mut name).ok())
        .is_none()
        || name.len() > 256
    {
        return Err("The active Hermes profile could not be read.".into());
    }
    let name = name
        .trim_start_matches('\u{feff}')
        .trim()
        .to_ascii_lowercase();
    if name.is_empty() || name == "default" {
        return Ok(home.join("config.yaml"));
    }
    let profile = home.join("profiles").join(&name);
    if !simple_profile(&name) || !profile.is_dir() {
        return Err("The active Hermes profile is invalid or missing. Switch profiles in Hermes and scan again.".into());
    }
    Ok(profile.join("config.yaml"))
}

fn absolute_env(name: &str) -> Option<PathBuf> {
    std::env::var_os(name)
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
}

fn is_executable(path: &Path, windows: bool) -> bool {
    let Ok(metadata) = path.metadata() else {
        return false;
    };
    if !metadata.is_file() {
        return false;
    }
    if path
        .symlink_metadata()
        .is_ok_and(|metadata| !metadata.file_type().is_symlink())
    {
        // Read only the bounded launcher prefix; never source or execute it.
        if let Ok(file) = std::fs::File::open(path) {
            let mut prefix = Vec::new();
            if file.take(4096).read_to_end(&mut prefix).is_ok() {
                let prefix = String::from_utf8_lossy(&prefix);
                if prefix.starts_with("#!")
                    && (prefix.contains("mise use ")
                        || prefix.contains("mise x ")
                        || prefix.contains("mise exec "))
                {
                    return false;
                }
            }
        }
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        windows || metadata.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        let _ = windows;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discovery_is_bounded_and_does_not_create_files() {
        let root = std::env::temp_dir().join(format!(
            "student-mcp-discovery-{}",
            crate::config::random_hex(8)
        ));
        std::fs::create_dir_all(root.join("bin")).unwrap();
        let mut computer = Computer {
            home: root.clone(),
            config: root.join("config"),
            support: root.join("support"),
            bins: vec![root.join("bin")],
            apps: vec![root.join("apps")],
            programs: vec![root.join("programs")],
            windows: true,
            overrides: vec![],
            mise_installs: root.join("mise/installs"),
        };
        for assistant in ASSISTANTS {
            assert!(!computer.detected(assistant, &computer.path(assistant)));
        }
        std::fs::write(root.join("bin/omp.cmd"), "must never be executed").unwrap();
        std::fs::create_dir_all(root.join("apps/Cursor.app")).unwrap();
        std::fs::create_dir_all(root.join("programs/OpenCode")).unwrap();
        std::fs::write(
            root.join("programs/OpenCode/OpenCode.exe"),
            "not executable",
        )
        .unwrap();
        for id in ["omp", "cursor", "opencode"] {
            let a = find(id).unwrap();
            assert!(computer.detected(a, &computer.path(a)));
        }
        computer.windows = false;
        assert!(!computer.detected(
            find("claude-code").unwrap(),
            &computer.path(find("claude-code").unwrap())
        ));
        assert!(!root.join("config").exists());
        assert!(!root.join(".omp").exists());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn jsonc_has_precedence_and_claude_code_is_not_desktop() {
        let root = std::env::temp_dir().join(format!(
            "student-mcp-paths-{}",
            crate::config::random_hex(8)
        ));
        std::fs::create_dir_all(root.join("opencode")).unwrap();
        let computer = Computer {
            home: root.clone(),
            config: root.clone(),
            support: root.join("support"),
            bins: vec![],
            apps: vec![],
            programs: vec![],
            windows: false,
            overrides: vec![],
            mise_installs: root.join("mise/installs"),
        };
        std::fs::write(root.join(".claude.json"), "{}").unwrap();
        assert!(computer.detected(
            find("claude-code").unwrap(),
            &computer.path(find("claude-code").unwrap())
        ));
        assert!(!computer.detected(
            find("claude").unwrap(),
            &computer.path(find("claude").unwrap())
        ));
        std::fs::write(root.join("opencode/opencode.jsonc"), "{}").unwrap();
        assert_eq!(
            computer.path(find("opencode").unwrap()),
            root.join("opencode/opencode.jsonc")
        );
        std::fs::create_dir_all(root.join("amp")).unwrap();
        std::fs::write(root.join("amp/settings.jsonc"), "{}").unwrap();
        assert_eq!(
            computer.path(find("amp").unwrap()),
            root.join("amp/settings.jsonc")
        );
        std::fs::write(root.join("amp/settings.json"), "{}").unwrap();
        assert_eq!(
            computer.path(find("amp").unwrap()),
            root.join("amp/settings.json")
        );
        for (id, relative) in [
            ("pi", ".pi/agent/mcp.json"),
            ("qwen", ".qwen/settings.json"),
            ("kiro", ".kiro/settings/mcp.json"),
            ("crush", "crush/crush.json"),
            ("devin", "devin/mcp_config.json"),
            ("hermes", ".hermes/config.yaml"),
            ("antigravity", ".gemini/config/mcp_config.json"),
            ("command-code", ".commandcode/mcp.json"),
            ("kilo", "kilo/kilo.json"),
            ("kimi", ".kimi-code/mcp.json"),
            ("openclaw", ".openclaw/openclaw.json"),
            ("ori", ".ori/mcp.json"),
            ("minimax-code", ".minimax/mcp.json"),
            ("deepseek-harness", ".dsh/cordis.patch.yml"),
            ("muse", "muse/settings.json"),
            ("freebuff", ".agents/mcp.json"),
        ] {
            assert_eq!(
                computer.path(find(id).unwrap()),
                root.join(relative),
                "{id}"
            );
        }
        std::fs::create_dir_all(root.join(".agents")).unwrap();
        let freebuff = find("freebuff").unwrap();
        std::fs::write(computer.path(freebuff), "{}").unwrap();
        assert!(!computer.detected(freebuff, &computer.path(freebuff)));
        std::fs::create_dir_all(root.join("kilo")).unwrap();
        for (name, expected) in [
            ("config.json", "config.json"),
            ("kilo.jsonc", "kilo.jsonc"),
            ("kilo.json", "kilo.jsonc"),
            ("opencode.json", "opencode.json"),
        ] {
            std::fs::write(root.join("kilo").join(name), "{}").unwrap();
            assert_eq!(
                computer.path(find("kilo").unwrap()),
                root.join("kilo").join(expected)
            );
        }
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn hermes_targets_the_active_or_explicit_profile_without_a_fallback_write() {
        let root = std::env::temp_dir().join(format!(
            "student-mcp-hermes-profiles-{}",
            crate::config::random_hex(8)
        ));
        let default_home = root.join(".hermes");
        let work = default_home.join("profiles/work");
        let study = default_home.join("profiles/study");
        std::fs::create_dir_all(&work).unwrap();
        std::fs::create_dir_all(&study).unwrap();
        assert_eq!(
            hermes_config(&default_home, None).unwrap(),
            default_home.join("config.yaml")
        );
        std::fs::write(default_home.join("active_profile"), "\u{feff}Study\n").unwrap();
        assert_eq!(
            hermes_config(&default_home, None).unwrap(),
            study.join("config.yaml")
        );
        assert_eq!(
            hermes_config(&default_home, Some(&work)).unwrap(),
            work.join("config.yaml")
        );
        std::fs::write(default_home.join("active_profile"), "../../outside\n").unwrap();
        assert!(hermes_config(&default_home, None).is_err());
        std::fs::write(default_home.join("active_profile"), "missing\n").unwrap();
        assert!(hermes_config(&default_home, None).is_err());
        assert!(!default_home.join("config.yaml").exists());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn omarchy_lazy_launchers_are_not_installations_and_mise_is_read_only() {
        use std::os::unix::fs::PermissionsExt;
        let root =
            std::env::temp_dir().join(format!("student-mcp-mise-{}", crate::config::random_hex(8)));
        std::fs::create_dir_all(root.join("bin")).unwrap();
        let mut computer = Computer {
            home: root.clone(),
            config: root.join("config"),
            support: root.join("support"),
            bins: vec![root.join("bin")],
            apps: vec![],
            programs: vec![],
            windows: false,
            overrides: vec![],
            mise_installs: root.join("mise/installs"),
        };
        let launcher = root.join("bin/pi");
        let source = "#!/bin/sh\nmise use -g pi\nmise x -- pi \"$@\"\n";
        std::fs::write(&launcher, source).unwrap();
        std::fs::set_permissions(&launcher, std::fs::Permissions::from_mode(0o700)).unwrap();
        let pi = find("pi").unwrap();
        let shims = root.join("mise/shims");
        std::fs::create_dir_all(&shims).unwrap();
        std::os::unix::fs::symlink("/bin/sh", shims.join("pi")).unwrap();
        computer.bins.push(shims);
        assert!(
            !computer.detected(pi, &computer.path(pi)),
            "The lazy wrapper must not trigger installation or count as installed"
        );
        let installed = computer.mise_installs.join("pi/1.3.0/bin/pi");
        std::fs::create_dir_all(installed.parent().unwrap()).unwrap();
        std::fs::write(&installed, "#!/bin/sh\nexit 77\n").unwrap();
        std::fs::set_permissions(&installed, std::fs::Permissions::from_mode(0o700)).unwrap();
        assert!(computer.detected(pi, &computer.path(pi)));
        let kilo = find("kilo").unwrap();
        let npm_bin = computer
            .mise_installs
            .join("npm-kilocode-cli/1.0.0/node_modules/.bin/kilo");
        std::fs::create_dir_all(npm_bin.parent().unwrap()).unwrap();
        std::fs::write(&npm_bin, "#!/bin/sh\nexit 77\n").unwrap();
        std::fs::set_permissions(&npm_bin, std::fs::Permissions::from_mode(0o700)).unwrap();
        assert!(computer.detected(kilo, &computer.path(kilo)));
        assert_eq!(std::fs::read_to_string(launcher).unwrap(), source);
        assert!(!root.join(".pi").exists() && !root.join("config").exists());
        std::fs::remove_dir_all(root).unwrap();
    }
}
