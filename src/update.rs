//! Release checks and installation, accessible only through the local admin UI.
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    io::{Read, Write},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

const REPO: &str = "https://github.com/CloudDown/student-mcp/releases/download";
const MAX_DOWNLOAD: u64 = 256 * 1024 * 1024;

#[derive(Default)]
struct State {
    release: Option<Value>,
    checked: Option<Instant>,
    failures: u32,
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

/// Short, translatable reason instead of ureq's URL + transport diagnostics.
fn reason(error: &ureq::Error, fallback: &'static str) -> String {
    match error {
        ureq::Error::Transport(transport) => match transport.kind() {
            // A DNS hiccup at startup or a dropped Wi-Fi link; the next check retries.
            ureq::ErrorKind::Dns
            | ureq::ErrorKind::ConnectionFailed
            | ureq::ErrorKind::Io
            | ureq::ErrorKind::ProxyConnect => "No network connection".into(),
            _ => fallback.into(),
        },
        _ => fallback.into(),
    }
}

/// Check hourly, but retry a failed attempt quickly and back off while offline.
fn delay_after(previous_failures: u32) -> Duration {
    match previous_failures {
        0 => Duration::from_secs(3600),
        failures => Duration::from_secs(60 << (failures - 1).min(6)).min(Duration::from_secs(3600)),
    }
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
        if state.checking || state.installing || state.ready {
            return;
        }
        let wait = delay_after(state.failures);
        if state.checked.is_some_and(|at| at.elapsed() < wait) {
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
                    .map_err(|e| reason(&e, "Update check failed"))?;
                serde_json::from_reader(response.into_reader().take(1024 * 1024))
                    .map_err(|_| "Update check failed".to_string())
            })();
            let mut state = this.0.lock().unwrap();
            state.checking = false;
            state.checked = Some(Instant::now());
            match release {
                Ok(release) => {
                    state.release = Some(release);
                    state.failures = 0;
                    state.error = None;
                }
                Err(error) => {
                    state.failures = state.failures.saturating_add(1);
                    state.error = Some(error);
                }
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
        json!({"current":crate::VERSION,"latest":latest,"available":available,"checking":state.checking,"checked":state.checked.is_some(),"installing":state.installing,"ready":state.ready,"error":state.error})
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

fn download_verified(
    source: impl Read,
    mut destination: impl Write,
    sums: &str,
    name: &str,
    limit: u64,
) -> Result<(), String> {
    let expected = sums
        .lines()
        .find_map(|line| {
            let mut fields = line.split_whitespace();
            let hash = fields.next()?;
            (fields.next()?.trim_start_matches('*') == name).then_some(hash)
        })
        .ok_or("Release checksum is missing")?;
    let mut source = source.take(limit + 1);
    let mut hash = Sha256::new();
    let mut total = 0;
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let count = match source.read(&mut buffer) {
            Ok(0) => break,
            Ok(count) => count,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error.to_string()),
        };
        total += count as u64;
        if total > limit {
            return Err("Release download is too large".into());
        }
        destination
            .write_all(&buffer[..count])
            .map_err(|e| e.to_string())?;
        hash.update(&buffer[..count]);
    }
    if data_encoding::HEXLOWER.encode(&hash.finalize()) != expected.to_ascii_lowercase() {
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
        .map_err(|e| reason(&e, "Update failed"))?
        .into_reader()
        .take(64 * 1024)
        .read_to_string(&mut sums)
        .map_err(|e| e.to_string())?;
    let source = agent
        .get(&format!("{base}/{name}"))
        .call()
        .map_err(|e| reason(&e, "Update failed"))?
        .into_reader();
    install_download(source, &sums, &name)
}

/// Remove our staging file on failure; only verified downloads can be installed.
fn stage_download(
    source: impl Read,
    path: &std::path::Path,
    sums: &str,
    name: &str,
) -> Result<(), String> {
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o755);
    }
    let mut file = options.open(path).map_err(|e| e.to_string())?;
    let result = download_verified(source, &mut file, sums, name, MAX_DOWNLOAD)
        .and_then(|()| file.sync_all().map_err(|e| e.to_string()));
    drop(file);
    if result.is_err() {
        let _ = std::fs::remove_file(path);
    }
    result
}

#[cfg(target_os = "linux")]
fn install_download(source: impl Read, sums: &str, name: &str) -> Result<(), String> {
    let executable = std::env::current_exe().map_err(|e| e.to_string())?;
    replace_executable(source, sums, name, &executable)
}

#[cfg(target_os = "linux")]
fn replace_executable(
    source: impl Read,
    sums: &str,
    name: &str,
    executable: &std::path::Path,
) -> Result<(), String> {
    let staged = executable.with_file_name(format!(
        ".student-mcp-update-{}",
        crate::config::random_hex(8)
    ));
    stage_download(source, &staged, sums, name)?;
    let result = std::fs::rename(&staged, executable);
    let _ = std::fs::remove_file(staged);
    result.map_err(|e| format!("Cannot install update: {e}"))
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
fn install_download(source: impl Read, sums: &str, name: &str) -> Result<(), String> {
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
    let path = directory.join(name);
    if let Err(error) = stage_download(source, &path, sums, name) {
        let _ = std::fs::remove_dir(&directory);
        return Err(error);
    }
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
fn install_download(_: impl Read, _: &str, _: &str) -> Result<(), String> {
    Err("Unsupported system".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failed_staging_removes_partial_downloads_and_preserves_existing_files() {
        struct Disconnected;
        impl Read for Disconnected {
            fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
                Err(std::io::ErrorKind::ConnectionReset.into())
            }
        }
        let directory = crate::pdf::Workspace::new().unwrap();
        let path = directory.join("installer");
        let sums = format!(
            "{}  app",
            data_encoding::HEXLOWER.encode(&Sha256::digest(b"release"))
        );
        assert!(
            stage_download((&b"partial"[..]).chain(Disconnected), &path, &sums, "app").is_err()
        );
        assert!(!path.exists());
        assert!(stage_download(&b"modified"[..], &path, &sums, "app").is_err());
        assert!(!path.exists());
        stage_download(&b"release"[..], &path, &sums, "app").unwrap();
        assert!(stage_download(&b"modified"[..], &path, &sums, "app").is_err());
        assert_eq!(std::fs::read(path).unwrap(), b"release");
    }

    #[test]
    fn streaming_download_is_bounded_and_propagates_io_failures() {
        struct Chunked {
            remaining: usize,
            interrupt: bool,
            fail: bool,
        }
        impl Read for Chunked {
            fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
                assert!(buffer.len() <= 64 * 1024);
                if std::mem::take(&mut self.interrupt) {
                    return Err(std::io::ErrorKind::Interrupted.into());
                }
                if self.remaining == 0 && self.fail {
                    return Err(std::io::ErrorKind::ConnectionReset.into());
                }
                let count = self.remaining.min(buffer.len()).min(8191);
                buffer[..count].fill(42);
                self.remaining -= count;
                Ok(count)
            }
        }
        let size = 2 * 1024 * 1024 + 3;
        let sums = format!(
            "{}  app",
            data_encoding::HEXLOWER.encode(&Sha256::digest(vec![42; size]))
        );
        let source = |fail| Chunked {
            remaining: size,
            interrupt: true,
            fail,
        };
        let mut output = Vec::new();
        download_verified(source(false), &mut output, &sums, "app", size as u64).unwrap();
        assert_eq!(output.len(), size);
        assert!(output.iter().all(|byte| *byte == 42));
        assert!(
            download_verified(source(true), std::io::sink(), &sums, "app", size as u64).is_err()
        );
        let mut full_disk = &mut [0u8; 1][..];
        assert!(
            download_verified(source(false), &mut full_disk, &sums, "app", size as u64).is_err()
        );
    }

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
        let sums = format!(
            "{}  app",
            data_encoding::HEXLOWER.encode(&Sha256::digest(b"new executable"))
        );
        replace_executable(&b"new executable"[..], &sums, "app", &path).unwrap();
        let mut previous = String::new();
        running.read_to_string(&mut previous).unwrap();
        assert_eq!(previous, "old executable");
        assert_eq!(std::fs::read(&path).unwrap(), b"new executable");
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o755
        );
        assert_eq!(std::fs::read_dir(&directory).unwrap().count(), 1);
        assert!(replace_executable(&b"bad"[..], &sums, "app", &path).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"new executable");
        assert_eq!(std::fs::read_dir(&directory).unwrap().count(), 1);
        assert!(replace_executable(&b"new executable"[..], &sums, "app", &directory).is_err());
        assert_eq!(std::fs::read_dir(&directory).unwrap().count(), 1);
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
    fn failed_checks_retry_quickly_then_back_off() {
        assert_eq!(delay_after(0), Duration::from_secs(3600));
        assert_eq!(delay_after(1), Duration::from_secs(60));
        assert_eq!(delay_after(2), Duration::from_secs(120));
        assert_eq!(delay_after(6), Duration::from_secs(1920));
        assert_eq!(delay_after(7), Duration::from_secs(3600));
        assert_eq!(delay_after(u32::MAX), Duration::from_secs(3600));
    }
    #[test]
    fn update_checksum_requires_exact_asset_and_content() {
        let sums = format!(
            "{}  student-mcp-linux-x64\n",
            data_encoding::HEXLOWER.encode(&Sha256::digest(b"release"))
        );
        let mut output = Vec::new();
        assert!(download_verified(
            &b"release"[..],
            &mut output,
            &sums,
            "student-mcp-linux-x64",
            7
        )
        .is_ok());
        assert_eq!(output, b"release");
        assert!(download_verified(
            &b"modified"[..],
            std::io::sink(),
            &sums,
            "student-mcp-linux-x64",
            8
        )
        .is_err());
        assert!(download_verified(
            &b"release"[..],
            std::io::sink(),
            &sums,
            "student-mcp-linux-arm64",
            7
        )
        .is_err());
        assert!(download_verified(
            &b"release"[..],
            std::io::sink(),
            &sums,
            "student-mcp-linux-x64",
            6
        )
        .unwrap_err()
        .contains("too large"));
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
