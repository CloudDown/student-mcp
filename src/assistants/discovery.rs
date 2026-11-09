//! Bounded, read-only discovery. Never launch an assistant or crawl the disk.

use std::io::Read;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Format {
    Json,
    Jsonc,
    OpenCode,
    Toml,
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
    pub fn servers_key(&self) -> &'static str {
        match self.id {
            "amp" => "amp.mcpServers",
            "crush" => "mcp",
            _ => "mcpServers",
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
        name: "Cursor",
        format: Format::Json,
        config: ".cursor/mcp.json",
        commands: &["cursor", "cursor-agent"],
        marker: ".cursor",
        app: "Cursor",
        windows_app: "cursor/Cursor.exe",
    },
    Assistant {
        id: "grok-build",
        name: "Grok Build",
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
        name: "Kiro",
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
    pub overrides: Vec<(&'static str, PathBuf)>,
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
        ] {
            if let Some(directory) = absolute_env(variable) {
                overrides.push((id, directory.join(file)));
            }
        }
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
        overrides.push(("omp", omp_agent.join("mcp.json")));
        if let Some(agent_dir) = absolute_env("PI_CODING_AGENT_DIR") {
            overrides.push(("pi", agent_dir.join("mcp.json")));
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
        if let Some((_, path)) = self.overrides.iter().find(|(id, _)| *id == assistant.id) {
            return path.clone();
        }
        match assistant.id {
            "claude" => self.support.join(assistant.config),
            "devin" if self.windows => self.support.join(assistant.config),
            "crush" | "devin" => self.config.join(assistant.config),
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

    pub fn detected(&self, assistant: &Assistant, path: &Path) -> bool {
        path.exists()
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
                        [version.path(), version.path().join("bin")]
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
        ] {
            assert_eq!(
                computer.path(find(id).unwrap()),
                root.join(relative),
                "{id}"
            );
        }
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
        assert_eq!(std::fs::read_to_string(launcher).unwrap(), source);
        assert!(!root.join(".pi").exists() && !root.join("config").exists());
        std::fs::remove_dir_all(root).unwrap();
    }
}
