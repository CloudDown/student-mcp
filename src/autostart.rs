//! Per-user login startup, separate from the saved school credentials.
use std::path::{Path, PathBuf};

#[cfg(target_os = "linux")]
fn entry_path() -> Result<PathBuf, String> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .filter(|value| Path::new(value).is_absolute())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
        .ok_or("Cannot find your user configuration directory.")?;
    Ok(base.join("autostart/student-mcp.desktop"))
}

#[cfg(target_os = "macos")]
fn entry_path() -> Result<PathBuf, String> {
    let home = std::env::var_os("HOME").ok_or("Cannot find your home directory.")?;
    Ok(PathBuf::from(home).join("Library/LaunchAgents/org.student-mcp.plist"))
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn launcher() -> Result<PathBuf, String> {
    std::env::current_exe().map_err(|error| format!("Cannot locate student-mcp: {error}"))
}

#[cfg(target_os = "linux")]
fn entry(binary: &Path) -> Result<String, String> {
    let path = binary.to_str().ok_or("The app path is not valid UTF-8.")?;
    if path.contains(['\n', '\r']) {
        return Err("The app path contains a line break.".into());
    }
    // The Desktop Entry Exec field quotes and escapes independently of a shell.
    let quoted = path
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('$', "\\$")
        .replace('`', "\\`")
        .replace('%', "%%");
    Ok(format!("[Desktop Entry]\nType=Application\nName=student-mcp\nExec=\"{quoted}\" up --background\nTerminal=false\nX-student-mcp-managed=true\n"))
}

#[cfg(target_os = "macos")]
fn entry(binary: &Path) -> Result<String, String> {
    let path = binary
        .to_str()
        .ok_or("The app path is not valid UTF-8.")?
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;");
    Ok(format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n<plist version=\"1.0\"><dict>\n<key>Label</key><string>org.student-mcp</string>\n<key>ProgramArguments</key><array><string>{path}</string><string>up</string><string>--background</string></array>\n<key>RunAtLoad</key><true/>\n</dict></plist>\n"))
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn managed(contents: &str) -> bool {
    #[cfg(target_os = "linux")]
    {
        contents.lines().any(|line| line == "[Desktop Entry]")
            && contents.lines().any(|line| line == "Name=student-mcp")
            && contents
                .lines()
                .any(|line| line == "X-student-mcp-managed=true")
    }
    #[cfg(target_os = "macos")]
    {
        contents.contains("<key>Label</key><string>org.student-mcp</string>")
    }
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn current(path: &Path) -> Result<Option<String>, String> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if !metadata.file_type().is_file() => Err(format!(
            "{} is not a regular file; it was left unchanged.",
            path.display()
        )),
        Ok(_) => std::fs::read_to_string(path)
            .map(Some)
            .map_err(|e| e.to_string()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.to_string()),
    }
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn set_file(path: &Path, binary: &Path, enabled: bool) -> Result<(), String> {
    let existing = current(path)?;
    if existing.as_deref().is_some_and(|text| !managed(text)) {
        return Err(format!(
            "{} is not managed by student-mcp; it was left unchanged.",
            path.display()
        ));
    }
    if !enabled {
        if existing.is_some() {
            std::fs::remove_file(path).map_err(|e| e.to_string())?;
        }
        return Ok(());
    }
    let contents = entry(binary)?;
    let parent = path.parent().ok_or("Missing startup directory.")?;
    std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let temporary = parent.join(format!(".student-mcp-{}.tmp", crate::config::random_hex(8)));
    let write = (|| {
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|e| e.to_string())?;
        file.write_all(contents.as_bytes())
            .map_err(|e| e.to_string())?;
        file.sync_all().map_err(|e| e.to_string())?;
        std::fs::rename(&temporary, path).map_err(|e| e.to_string())
    })();
    if write.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    write
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
pub fn enabled() -> Result<bool, String> {
    Ok(current(&entry_path()?)?.as_deref().is_some_and(managed))
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
pub fn set(enabled: bool) -> Result<(), String> {
    set_file(&entry_path()?, &launcher()?, enabled)
}

#[cfg(target_os = "windows")]
const RUN_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";

#[cfg(target_os = "windows")]
fn command() -> Result<String, String> {
    let launcher = std::env::current_exe()
        .map_err(|e| e.to_string())?
        .with_file_name("student-mcp-desktop.exe");
    if !launcher.is_file() {
        return Err("The student-mcp desktop launcher is missing.".into());
    }
    Ok(format!("\"{}\" --background", launcher.display()))
}

#[cfg(target_os = "windows")]
fn reg(args: &[&str]) -> Result<std::process::Output, String> {
    use std::os::windows::process::CommandExt;
    std::process::Command::new("reg.exe")
        .args(args)
        .creation_flags(0x0800_0000)
        .output()
        .map_err(|e| format!("Cannot update Windows startup: {e}"))
}

#[cfg(target_os = "windows")]
fn existing() -> Result<Option<String>, String> {
    let output = reg(&["query", RUN_KEY, "/v", "student-mcp"])?;
    if !output.status.success() {
        return Ok(None);
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let value = text
        .split_once("REG_SZ")
        .ok_or("Cannot read Windows startup entry.")?
        .1
        .trim();
    Ok(Some(value.to_owned()))
}

#[cfg(target_os = "windows")]
pub fn enabled() -> Result<bool, String> {
    Ok(existing()?.is_some_and(|value| value == command().unwrap_or_default()))
}

#[cfg(target_os = "windows")]
pub fn set(enabled: bool) -> Result<(), String> {
    let current = existing()?;
    let desired = command()?;
    if current.as_deref().is_some_and(|value| value != desired) {
        return Err(
            "The Windows startup entry is not managed by this installation; it was left unchanged."
                .into(),
        );
    }
    if !enabled && current.is_none() {
        return Ok(());
    }
    let output = if enabled {
        reg(&[
            "add",
            RUN_KEY,
            "/v",
            "student-mcp",
            "/t",
            "REG_SZ",
            "/d",
            &desired,
            "/f",
        ])?
    } else {
        reg(&["delete", RUN_KEY, "/v", "student-mcp", "/f"])?
    };
    if output.status.success() {
        Ok(())
    } else {
        Err("Cannot change Windows startup settings.".into())
    }
}

#[cfg(test)]
#[cfg(target_os = "linux")]
mod tests {
    use super::*;
    #[test]
    fn manages_only_its_own_startup_entry() {
        let root = std::env::temp_dir().join(format!(
            "student-mcp-startup-{}",
            crate::config::random_hex(8)
        ));
        let path = root.join("autostart/student-mcp.desktop");
        let binary = Path::new("/home/student/My App/student-mcp");
        set_file(&path, binary, true).unwrap();
        assert!(current(&path).unwrap().as_deref().is_some_and(managed));
        assert!(std::fs::read_to_string(&path)
            .unwrap()
            .contains("Exec=\"/home/student/My App/student-mcp\" up --background"));
        set_file(&path, binary, true).unwrap();
        set_file(&path, binary, false).unwrap();
        assert!(!path.exists());
        std::fs::write(&path, "[Desktop Entry]\nName=Other\n").unwrap();
        assert!(set_file(&path, binary, true).is_err());
        assert!(set_file(&path, binary, false).is_err());
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "[Desktop Entry]\nName=Other\n"
        );
        std::fs::remove_file(&path).unwrap();
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink("/nonexistent", &path).unwrap();
            assert!(set_file(&path, binary, true).is_err());
            assert!(set_file(&path, binary, false).is_err());
            std::fs::remove_file(&path).unwrap();
        }
        std::fs::remove_dir_all(&root).unwrap();
    }
}
