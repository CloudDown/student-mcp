//! Release checks and installation, accessible only through the local admin UI.
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    io::Read,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

const REPO: &str = "https://github.com/CloudDown/student-mcp/releases/download";
const MAX_DOWNLOAD: u64 = 256 * 1024 * 1024;

#[derive(Default)]
struct State {
    release: Option<Value>,
    checked: Option<Instant>,
    checking: bool,
    installing: bool,
    ready: bool,
    error: Option<String>,
}

#[derive(Clone, Default)]
pub struct Updater(Arc<Mutex<State>>);

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .user_agent(concat!("student-mcp/", env!("CARGO_PKG_VERSION")))
        .timeout(Duration::from_secs(180))
        .build()
}

fn version(value: &str) -> Option<(u64, u64, u64)> {
    let mut parts = value.strip_prefix('v').unwrap_or(value).split('.');
    let result = (
        parts.next()?.parse().ok()?,
        parts.next()?.parse().ok()?,
        parts.next()?.parse().ok()?,
    );
    parts.next().is_none().then_some(result)
}

fn asset_name() -> Result<String, String> {
    match std::env::consts::OS {
        "windows" => Ok("student-mcp-setup.exe".into()),
        "macos" => Ok("student-mcp.dmg".into()),
        "linux" => match std::env::consts::ARCH {
            "x86_64" => Ok("student-mcp-linux-x64".into()),
            "aarch64" => Ok("student-mcp-linux-arm64".into()),
            _ => Err("No release available for this architecture".into()),
        },
        _ => Err("No release available for this system".into()),
    }
}

impl Updater {
    pub fn check(&self) {
        let mut state = self.0.lock().unwrap();
        if state.checking
            || state.installing
            || state.ready
            || state
                .checked
                .is_some_and(|at| at.elapsed() < Duration::from_secs(3600))
        {
            return;
        }
        state.checking = true;
        drop(state);
        let this = self.clone();
        std::thread::spawn(move || {
            let release = (|| -> Result<Value, String> {
                let response = agent()
                    .get("https://api.github.com/repos/CloudDown/student-mcp/releases/latest")
                    .call()
                    .map_err(|e| e.to_string())?;
                serde_json::from_reader(response.into_reader().take(1024 * 1024))
                    .map_err(|e| e.to_string())
            })();
            let mut state = this.0.lock().unwrap();
            state.checking = false;
            state.checked = Some(Instant::now());
            if let Ok(release) = release {
                state.release = Some(release);
            }
        });
    }

    pub fn status(&self) -> Value {
        let state = self.0.lock().unwrap();
        let latest = state.release.as_ref().and_then(|r| r["tag_name"].as_str());
        // Never downgrade a development build or install a prerelease implicitly.
        let available = latest
            .and_then(version)
            .zip(version(crate::VERSION))
            .is_some_and(|(new, old)| new > old)
            && asset_name().ok().is_some_and(|name| {
                state
                    .release
                    .as_ref()
                    .and_then(|r| r["assets"].as_array())
                    .is_some_and(|assets| assets.iter().any(|a| a["name"] == name))
            });
        json!({"current":crate::VERSION,"latest":latest,"available":available,"installing":state.installing,"ready":state.ready,"error":state.error})
    }

    pub fn start(&self) -> Result<(), String> {
        if self.status()["available"] != true {
            return Err("No update available".into());
        }
        let mut state = self.0.lock().unwrap();
        if state.installing || state.ready {
            return Err("Update already in progress".into());
        }
        let tag = state.release.as_ref().unwrap()["tag_name"]
            .as_str()
            .unwrap()
            .to_owned();
        state.installing = true;
        state.error = None;
        drop(state);
        let this = self.clone();
        std::thread::spawn(move || {
            let result = install(&tag);
            let mut state = this.0.lock().unwrap();
            state.installing = false;
            match result {
                Ok(()) => state.ready = true,
                Err(error) => state.error = Some(error),
            }
        });
        Ok(())
    }
}

fn verify(bytes: &[u8], sums: &str, name: &str) -> Result<(), String> {
    let expected = sums
        .lines()
        .find_map(|line| {
            let mut fields = line.split_whitespace();
            let hash = fields.next()?;
            (fields.next()?.trim_start_matches('*') == name).then_some(hash)
        })
        .ok_or("Release checksum is missing")?;
    if data_encoding::HEXLOWER.encode(&Sha256::digest(bytes)) != expected.to_ascii_lowercase() {
        return Err("Release checksum does not match; installation cancelled".into());
    }
    Ok(())
}

fn install(tag: &str) -> Result<(), String> {
    if version(tag).is_none() {
        return Err("Invalid release version".into());
    }
    let name = asset_name()?;
    let base = format!("{REPO}/{tag}");
    let agent = agent();
    let mut sums = String::new();
    agent
        .get(&format!("{base}/SHA256SUMS.txt"))
        .call()
        .map_err(|e| e.to_string())?
        .into_reader()
        .take(64 * 1024)
        .read_to_string(&mut sums)
        .map_err(|e| e.to_string())?;
    let mut bytes = Vec::new();
    agent
        .get(&format!("{base}/{name}"))
        .call()
        .map_err(|e| e.to_string())?
        .into_reader()
        .take(MAX_DOWNLOAD + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > MAX_DOWNLOAD {
        return Err("Release download is too large".into());
    }
    verify(&bytes, &sums, &name)?;
    install_bytes(&bytes)
}

#[cfg(target_os = "linux")]
fn install_bytes(bytes: &[u8]) -> Result<(), String> {
    let executable = std::env::current_exe().map_err(|e| e.to_string())?;
    replace_executable(bytes, &executable)
}

#[cfg(target_os = "linux")]
fn replace_executable(bytes: &[u8], executable: &std::path::Path) -> Result<(), String> {
    use std::{fs::OpenOptions, io::Write, os::unix::fs::OpenOptionsExt};
    let staged = executable.with_file_name(format!(
        ".student-mcp-update-{}",
        crate::config::random_hex(8)
    ));
    let result = (|| -> std::io::Result<()> {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o755)
            .open(&staged)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        std::fs::rename(&staged, executable)
    })();
    let _ = std::fs::remove_file(staged);
    result.map_err(|e| format!("Cannot install update: {e}"))
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
fn install_bytes(bytes: &[u8]) -> Result<(), String> {
    use std::io::Write;
    let directory = crate::config::config_path().with_file_name(format!(
        ".student-mcp-update-{}",
        crate::config::random_hex(16)
    ));
    let builder = std::fs::DirBuilder::new();
    #[cfg(unix)]
    let mut builder = builder;
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(&directory).map_err(|e| e.to_string())?;
    let path = directory.join(asset_name()?);
    let mut file = std::fs::File::create(&path).map_err(|e| e.to_string())?;
    file.write_all(bytes).map_err(|e| e.to_string())?;
    file.sync_all().map_err(|e| e.to_string())?;
    drop(file);
    #[cfg(target_os = "windows")]
    std::process::Command::new(path)
        .args(["/SP-", "/NORESTART"])
        .spawn()
        .map_err(|e| e.to_string())?;
    #[cfg(target_os = "macos")]
    {
        let mount = directory.join("volume");
        let status = std::process::Command::new("hdiutil")
            .args(["attach", "-nobrowse", "-readonly", "-mountpoint"])
            .arg(&mount)
            .arg(&path)
            .status()
            .map_err(|e| e.to_string())?;
        if !status.success() {
            return Err("Cannot mount the update installer".into());
        }
        std::process::Command::new("open")
            .arg(mount.join("Install student-mcp.app"))
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
fn install_bytes(_: &[u8]) -> Result<(), String> {
    Err("Unsupported system".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(target_os = "linux")]
    #[test]
    fn replacement_preserves_open_executable_and_leaves_no_staging_file() {
        use std::os::unix::fs::PermissionsExt;
        let directory = std::env::temp_dir().join(format!(
            "student-update-test-{}",
            crate::config::random_hex(8)
        ));
        std::fs::create_dir(&directory).unwrap();
        let path = directory.join("student-mcp");
        std::fs::write(&path, b"old executable").unwrap();
        let mut running = std::fs::File::open(&path).unwrap();
        replace_executable(b"new executable", &path).unwrap();
        let mut previous = String::new();
        running.read_to_string(&mut previous).unwrap();
        assert_eq!(previous, "old executable");
        assert_eq!(std::fs::read(&path).unwrap(), b"new executable");
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o755
        );
        assert_eq!(std::fs::read_dir(&directory).unwrap().count(), 1);
        assert!(replace_executable(b"bad", &directory).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"new executable");
        std::fs::remove_dir_all(directory).unwrap();
    }
    #[test]
    fn release_versions_are_numeric_and_stable() {
        assert!(version("v0.10.0") > version("0.9.9"));
        for invalid in ["v1.2.3-beta", "../1.2.3", "1.2", "1.2.3.4"] {
            assert_eq!(version(invalid), None);
        }
    }
    #[test]
    fn update_checksum_requires_exact_asset_and_content() {
        let sums = format!(
            "{}  student-mcp-linux-x64\n",
            data_encoding::HEXLOWER.encode(&Sha256::digest(b"release"))
        );
        assert!(verify(b"release", &sums, "student-mcp-linux-x64").is_ok());
        assert!(verify(b"modified", &sums, "student-mcp-linux-x64").is_err());
        assert!(verify(b"release", &sums, "student-mcp-linux-arm64").is_err());
    }

    #[test]
    fn update_only_offers_newer_compatible_releases() {
        let updater = Updater::default();
        assert_eq!(updater.status()["available"], false);
        assert!(updater.start().is_err());
        for (tag, assets, expected) in [
            (
                crate::VERSION,
                json!([{"name":asset_name().unwrap()}]),
                false,
            ),
            ("v0.0.1", json!([{"name":asset_name().unwrap()}]), false),
            ("v999.0.0", json!([]), false),
            ("v999.0.0", json!([{"name":asset_name().unwrap()}]), true),
        ] {
            updater.0.lock().unwrap().release = Some(json!({"tag_name":tag,"assets":assets}));
            assert_eq!(updater.status()["available"], expected);
        }
        updater.0.lock().unwrap().installing = true;
        assert!(updater.start().is_err());
    }
}
