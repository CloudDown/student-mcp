//! A private Chrome for Testing fallback, installed only by interactive login.
//! The system/default browser is never replaced. MCP only reads current.json.
use super::{private_dir, save, wait_for_profile};
use serde::Serialize;
use serde_json::Value;
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Component, Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    },
    time::Duration,
};

const MANIFEST: &str = "https://googlechromelabs.github.io/chrome-for-testing/last-known-good-versions-with-downloads.json";
const MAX_DOWNLOAD: u64 = 512 * 1024 * 1024;
const MAX_EXPANDED: u64 = 1536 * 1024 * 1024;

#[derive(Serialize)]
pub(super) struct Progress {
    pub phase: &'static str,
    downloaded: u64,
    total: Option<u64>,
}

impl Default for Progress {
    fn default() -> Self {
        Self {
            phase: "preparing",
            downloaded: 0,
            total: None,
        }
    }
}

fn report(progress: &Mutex<Progress>, phase: &'static str, downloaded: u64, total: Option<u64>) {
    *progress.lock().unwrap() = Progress {
        phase,
        downloaded,
        total,
    };
}

pub(super) fn check_cancelled(cancelled: &AtomicBool) -> Result<(), String> {
    if cancelled.load(Ordering::SeqCst) {
        Err("Browser sign-in cancelled.".into())
    } else {
        Ok(())
    }
}

fn platform(os: &str, arch: &str) -> Result<&'static str, String> {
    match (os, arch) {
        ("linux", "x86_64") => Ok("linux64"),
        ("linux", "aarch64") => Ok("linux-arm64"),
        ("macos", "x86_64") => Ok("mac-x64"),
        ("macos", "aarch64") => Ok("mac-arm64"),
        ("windows", "x86_64" | "aarch64") => Ok("win64"),
        ("windows", "x86") => Ok("win32"),
        _ => Err("Automatic browser installation is unavailable for this system. Install a Chromium-based browser and try again.".into()),
    }
}

fn root() -> PathBuf {
    crate::config::config_path().with_extension("browser-runtime")
}

fn valid_version(version: &str) -> bool {
    let parts: Vec<_> = version.split('.').collect();
    parts.len() == 4
        && parts
            .iter()
            .all(|p| !p.is_empty() && p.len() <= 10 && p.bytes().all(|b| b.is_ascii_digit()))
}

fn binary_path(platform: &str) -> PathBuf {
    let executable = if platform.starts_with("mac-") {
        "Google Chrome for Testing.app/Contents/MacOS/Google Chrome for Testing"
    } else if platform.starts_with("win") {
        "chrome.exe"
    } else {
        "chrome"
    };
    PathBuf::from(format!("chrome-{platform}")).join(executable)
}

fn available_at(root: &Path, platform: &str) -> Option<PathBuf> {
    let version: String =
        serde_json::from_slice(&fs::read(root.join("current.json")).ok()?).ok()?;
    if !valid_version(&version) {
        return None;
    }
    let path = root
        .join(platform)
        .join(version)
        .join(binary_path(platform));
    path.is_file().then_some(path)
}

pub(super) fn available() -> Option<PathBuf> {
    available_at(
        &root(),
        platform(std::env::consts::OS, std::env::consts::ARCH).ok()?,
    )
}

fn release(manifest: &Value, platform: &str) -> Result<(String, String), String> {
    let stable = &manifest["channels"]["Stable"];
    let version = stable["version"]
        .as_str()
        .filter(|v| valid_version(v))
        .ok_or("The browser download catalogue has an invalid version.")?;
    let expected = format!("https://storage.googleapis.com/chrome-for-testing-public/{version}/{platform}/chrome-{platform}.zip");
    let downloads = stable["downloads"]["chrome"]
        .as_array()
        .ok_or("The browser download catalogue is unavailable.")?;
    if !downloads
        .iter()
        .any(|d| d["platform"] == platform && d["url"] == expected)
    {
        return Err("No official browser download is available for this system. Install a Chromium-based browser and try again.".into());
    }
    Ok((version.into(), expected))
}

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .user_agent(crate::session::BROWSER_UA)
        .redirects(0)
        .timeout_connect(Duration::from_secs(10))
        .timeout_read(Duration::from_secs(10))
        .timeout_write(Duration::from_secs(10))
        .timeout(Duration::from_secs(600))
        .build()
}

// Immutable version directories allow existing tools to finish on the previous
// browser. Only a fully extracted install becomes current. Older versions are
// intentionally retained: another process may still be running one of them.
pub(super) fn ensure(
    cancelled: &AtomicBool,
    progress: &Mutex<Progress>,
) -> Result<PathBuf, String> {
    check_cancelled(cancelled)?;
    let platform = platform(std::env::consts::OS, std::env::consts::ARCH)?;
    let root = root();
    private_dir(&root).map_err(|_| "Cannot create browser installation storage.")?;
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(root.join("install.lock"))
        .map_err(|_| "Cannot lock browser installation.")?;
    report(progress, "preparing", 0, None);
    wait_for_profile(&lock, cancelled, Duration::from_secs(660))?;
    check_cancelled(cancelled)?;
    let agent = agent();
    let latest = (|| {
        let response = agent.get(MANIFEST).call().map_err(|_| {
            "Cannot check the browser download. Check your internet connection and try again."
        })?;
        let manifest: Value = serde_json::from_reader(response.into_reader().take(1024 * 1024))
            .map_err(|_| "Cannot read the browser download catalogue.")?;
        release(&manifest, platform)
    })();
    check_cancelled(cancelled)?;
    // An offline reconnect can still use the previously installed browser.
    let (version, url) = match latest {
        Ok(release) => release,
        Err(error) => return available_at(&root, platform).ok_or(error),
    };
    let versions = root.join(platform);
    private_dir(&versions).map_err(|_| "Cannot create browser installation storage.")?;
    let destination = versions.join(&version);
    let binary = destination.join(binary_path(platform));
    if binary.is_file() {
        save(&root.join("current.json"), &version)?;
        return Ok(binary);
    }
    let stage = versions.join(format!(".install-{}", crate::config::random_hex(16)));
    private_dir(&stage).map_err(|_| "Cannot create browser download storage.")?;
    let result = (|| {
        let response = agent.get(&url).call().map_err(|_| {
            "Browser download failed. Check your internet connection and try again."
        })?;
        let total = response
            .header("Content-Length")
            .and_then(|s| s.parse::<u64>().ok());
        let archive = stage.join("download.zip");
        download(response.into_reader(), &archive, total, cancelled, progress)?;
        report(progress, "extracting", total.unwrap_or(0), total);
        let unpacked = stage.join("unpacked");
        private_dir(&unpacked).map_err(|_| "Cannot unpack the browser.")?;
        extract(&archive, &unpacked, platform, cancelled)?;
        if !unpacked.join(binary_path(platform)).is_file() {
            return Err("The browser download does not contain its executable.".into());
        }
        check_cancelled(cancelled)?;
        // An incomplete older attempt may exist, but never replace a usable version.
        if destination.exists() {
            fs::remove_dir_all(&destination)
                .map_err(|_| "Cannot clear an incomplete browser installation.")?;
        }
        fs::rename(&unpacked, &destination)
            .map_err(|_| "Cannot install the downloaded browser.")?;
        save(&root.join("current.json"), &version)?;
        Ok(binary)
    })();
    let _ = fs::remove_dir_all(&stage);
    result
}

fn download(
    mut reader: impl Read,
    path: &Path,
    total: Option<u64>,
    cancelled: &AtomicBool,
    progress: &Mutex<Progress>,
) -> Result<(), String> {
    if total.is_some_and(|n| n > MAX_DOWNLOAD) {
        return Err("Browser download is too large.".into());
    }
    let mut file = File::create(path)
        .map_err(|_| "Cannot save the browser download. Check free disk space.")?;
    let mut downloaded = 0;
    let mut buffer = [0u8; 64 * 1024];
    loop {
        check_cancelled(cancelled)?;
        report(progress, "downloading", downloaded, total);
        let n = reader
            .read(&mut buffer)
            .map_err(|_| "Browser download was interrupted. Try connecting again.")?;
        if n == 0 {
            break;
        }
        downloaded += n as u64;
        if downloaded > MAX_DOWNLOAD {
            return Err("Browser download is too large.".into());
        }
        file.write_all(&buffer[..n])
            .map_err(|_| "Cannot save the browser download. Check free disk space.")?;
    }
    if total.is_some_and(|n| n != downloaded) || downloaded == 0 {
        return Err("Browser download is incomplete. Try connecting again.".into());
    }
    Ok(())
}

fn archive_path(name: &str, platform: &str) -> Result<PathBuf, String> {
    let path = Path::new(name);
    if name.contains(['\\', ':'])
        || !path.components().all(|c| matches!(c, Component::Normal(_)))
        || !path.starts_with(format!("chrome-{platform}"))
    {
        return Err("The browser archive contains an unsafe path.".into());
    }
    Ok(path.to_path_buf())
}

fn extract(
    archive: &Path,
    destination: &Path,
    platform: &str,
    cancelled: &AtomicBool,
) -> Result<(), String> {
    let file = File::open(archive).map_err(|_| "Cannot open the browser download.")?;
    let mut zip = zip::ZipArchive::new(file).map_err(|_| "Invalid browser archive.")?;
    if zip.len() > 25000 {
        return Err("Browser archive has too many files.".into());
    }
    let mut size = 0u64;
    let mut extracted = 0u64;
    let mut links = Vec::new();
    for i in 0..zip.len() {
        check_cancelled(cancelled)?;
        let mut entry = zip
            .by_index(i)
            .map_err(|_| "Invalid browser archive entry.")?;
        size = size
            .checked_add(entry.size())
            .filter(|s| *s <= MAX_EXPANDED)
            .ok_or("Unpacked browser is too large.")?;
        let relative = archive_path(
            &entry
                .name()
                .map_err(|_| "Invalid browser archive filename.")?,
            platform,
        )?;
        let path = destination.join(&relative);
        let mode = entry.unix_mode().unwrap_or(0o644);
        if entry.is_dir() {
            fs::create_dir_all(&path).map_err(|_| "Cannot create a browser directory.")?;
        } else if mode & 0o170000 == 0o120000 {
            let mut target = String::new();
            entry
                .take(4097)
                .read_to_string(&mut target)
                .map_err(|_| "Invalid browser symlink.")?;
            if target.len() > 4096 || target.is_empty() {
                return Err("Invalid browser symlink.".into());
            }
            // Defer links until all regular files have been written. An archive
            // entry must never cause a later write to follow a symlink.
            let mut resolved = relative.parent().unwrap().to_path_buf();
            for component in Path::new(&target).components() {
                match component {
                    Component::Normal(part) => resolved.push(part),
                    Component::CurDir => {}
                    Component::ParentDir if resolved.components().count() > 1 => {
                        resolved.pop();
                    }
                    _ => return Err("The browser archive contains an unsafe symlink.".into()),
                }
            }
            archive_path(
                resolved.to_str().ok_or("Invalid browser symlink.")?,
                platform,
            )?;
            links.push((path, target));
        } else {
            fs::create_dir_all(path.parent().unwrap())
                .map_err(|_| "Cannot create a browser directory.")?;
            let mut out = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
                .map_err(|_| "Cannot unpack a browser file.")?;
            // Read to EOF, including zip's CRC check; bound memory and observe Cancel.
            let mut buffer = [0u8; 64 * 1024];
            loop {
                check_cancelled(cancelled)?;
                let n = entry
                    .read(&mut buffer)
                    .map_err(|_| "The browser archive is corrupt.")?;
                if n == 0 {
                    break;
                }
                extracted += n as u64;
                if extracted > MAX_EXPANDED {
                    return Err("Unpacked browser is too large.".into());
                }
                out.write_all(&buffer[..n])
                    .map_err(|_| "Cannot unpack the browser. Check free disk space.")?;
            }
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(
                    &path,
                    fs::Permissions::from_mode(if mode & 0o111 != 0 { 0o755 } else { 0o644 }),
                )
                .map_err(|_| "Cannot set browser file permissions.")?;
            }
        }
    }
    for (path, target) in &links {
        check_cancelled(cancelled)?;
        fs::create_dir_all(path.parent().unwrap())
            .map_err(|_| "Cannot create a browser directory.")?;
        #[cfg(unix)]
        std::os::unix::fs::symlink(target, path).map_err(|_| "Cannot create a browser symlink.")?;
        #[cfg(not(unix))]
        {
            let _ = (path, target);
            return Err("Unexpected symlink in browser archive.".into());
        }
    }
    let root = destination
        .canonicalize()
        .map_err(|_| "Cannot check the browser installation.")?;
    for (path, _) in links {
        if !path.canonicalize().is_ok_and(|p| p.starts_with(&root)) {
            return Err("The browser archive contains an unsafe symlink.".into());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use zip::{write::SimpleFileOptions, ZipWriter};

    #[test]
    fn only_official_stable_downloads_match_the_platform() {
        for (os, arch, expected) in [
            ("linux", "x86_64", "linux64"),
            ("linux", "aarch64", "linux-arm64"),
            ("macos", "aarch64", "mac-arm64"),
            ("macos", "x86_64", "mac-x64"),
            ("windows", "x86_64", "win64"),
            ("windows", "aarch64", "win64"),
        ] {
            assert_eq!(platform(os, arch).unwrap(), expected);
        }
        assert!(platform("linux", "riscv64").is_err());
        let url = "https://storage.googleapis.com/chrome-for-testing-public/150.0.1.2/linux64/chrome-linux64.zip";
        let manifest = json!({"channels":{"Stable":{"version":"150.0.1.2","downloads":{"chrome":[{"platform":"linux64","url":url}]}}}});
        assert_eq!(
            release(&manifest, "linux64").unwrap(),
            ("150.0.1.2".into(), url.into())
        );
        assert!(release(&manifest, "mac-x64").is_err());
        for malicious in [
            "http://storage.googleapis.com/file.zip",
            "https://other.example/browser.zip",
            "https://storage.googleapis.com.evil.test/file.zip",
        ] {
            let mut changed = manifest.clone();
            changed["channels"]["Stable"]["downloads"]["chrome"][0]["url"] = json!(malicious);
            assert!(release(&changed, "linux64").is_err());
        }
        for bad in ["../150.0.1.2", "150.0.1.2/evil", "150.0.1", "150..1.2"] {
            assert!(!valid_version(bad));
        }
    }

    #[test]
    fn downloads_report_progress_and_reject_truncation_limits_and_cancellation() {
        crate::config::tests::with_temp_config(|| {
            let path = crate::config::config_path().with_extension("zip");
            let progress = Mutex::new(Progress::default());
            let stop = AtomicBool::new(false);
            download(&b"browser"[..], &path, Some(7), &stop, &progress).unwrap();
            assert_eq!(fs::read(&path).unwrap(), b"browser");
            assert_eq!(progress.lock().unwrap().downloaded, 7);
            assert!(download(&b"short"[..], &path, Some(7), &stop, &progress).is_err());
            assert!(download(&b""[..], &path, None, &stop, &progress).is_err());
            assert!(download(&b""[..], &path, Some(MAX_DOWNLOAD + 1), &stop, &progress).is_err());
            stop.store(true, Ordering::SeqCst);
            assert!(download(&b"browser"[..], &path, Some(7), &stop, &progress)
                .unwrap_err()
                .contains("cancelled"));
            assert!(ensure(&stop, &progress).unwrap_err().contains("cancelled"));
            assert!(
                !root().exists(),
                "cancel before creating storage or contacting Google"
            );
        });
    }

    fn fixture(path: &Path, entries: &[(&str, &str)], links: &[(&str, &str)]) {
        let mut zip = ZipWriter::new(File::create(path).unwrap());
        for (name, bytes) in entries {
            zip.start_file(*name, SimpleFileOptions::default().unix_permissions(0o755))
                .unwrap();
            zip.write_all(bytes.as_bytes()).unwrap();
        }
        for (name, target) in links {
            zip.add_symlink(*name, *target, SimpleFileOptions::default())
                .unwrap();
        }
        zip.finish().unwrap();
    }

    #[test]
    fn extraction_and_publication_keep_incomplete_installs_invisible() {
        crate::config::tests::with_temp_config(|| {
            let root = root();
            private_dir(&root).unwrap();
            let archive = root.join("download.zip");
            let destination = root.join("linux64/150.0.1.2");
            private_dir(&destination).unwrap();
            fixture(&archive, &[("chrome-linux64/chrome", "fixture")], &[]);
            extract(&archive, &destination, "linux64", &AtomicBool::new(false)).unwrap();
            assert!(available_at(&root, "linux64").is_none());
            save(&root.join("current.json"), &"150.0.1.2").unwrap();
            assert_eq!(
                available_at(&root, "linux64").unwrap(),
                destination.join("chrome-linux64/chrome")
            );
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                assert_eq!(
                    fs::metadata(destination.join("chrome-linux64/chrome"))
                        .unwrap()
                        .permissions()
                        .mode()
                        & 0o777,
                    0o755
                );
            }
            save(&root.join("current.json"), &"150.0.1.3").unwrap();
            assert!(available_at(&root, "linux64").is_none());
            save(&root.join("current.json"), &"../../elsewhere").unwrap();
            assert!(available_at(&root, "linux64").is_none());
        });
    }

    #[test]
    fn archives_cannot_escape_storage_or_write_through_links() {
        for name in [
            "../escape",
            "/chrome-linux64/file",
            "chrome-linux64/../../escape",
            "chrome-linux64\\escape",
            "chrome-linux64/C:evil",
            "other/file",
        ] {
            assert!(archive_path(name, "linux64").is_err(), "{name}");
        }
        crate::config::tests::with_temp_config(|| {
            let root = root();
            private_dir(&root).unwrap();
            let archive = root.join("download.zip");
            let destination = root.join("out");
            private_dir(&destination).unwrap();
            fixture(&archive, &[("chrome-linux64/../../escape", "bad")], &[]);
            assert!(extract(&archive, &destination, "linux64", &AtomicBool::new(false)).is_err());
            assert!(!root.join("escape").exists());
            fixture(&archive, &[], &[("chrome-linux64/link", "../../escape")]);
            assert!(extract(&archive, &destination, "linux64", &AtomicBool::new(false)).is_err());
            fixture(
                &archive,
                &[("chrome-linux64/link/file", "fixture")],
                &[("chrome-linux64/link", "inside")],
            );
            assert!(extract(&archive, &destination, "linux64", &AtomicBool::new(false)).is_err());
        });
    }

    #[cfg(unix)]
    #[test]
    fn mac_framework_relative_symlinks_survive_extraction() {
        crate::config::tests::with_temp_config(|| {
            let root = root();
            private_dir(&root).unwrap();
            let archive = root.join("download.zip");
            let destination = root.join("out");
            private_dir(&destination).unwrap();
            fixture(
                &archive,
                &[(
                    "chrome-mac-arm64/Framework/Versions/A/Resources/file",
                    "fixture",
                )],
                &[
                    ("chrome-mac-arm64/Framework/Versions/Current", "A"),
                    (
                        "chrome-mac-arm64/Framework/Resources",
                        "Versions/Current/Resources",
                    ),
                ],
            );
            extract(&archive, &destination, "mac-arm64", &AtomicBool::new(false)).unwrap();
            assert_eq!(
                fs::read(destination.join("chrome-mac-arm64/Framework/Resources/file")).unwrap(),
                b"fixture"
            );
        });
    }

    #[test]
    #[ignore = "downloads the official browser (~150-200 MB); no school credentials"]
    fn managed_browser_download_and_launch() {
        crate::config::tests::with_temp_config(|| {
            let path = ensure(&AtomicBool::new(false), &Mutex::new(Progress::default())).unwrap();
            assert_eq!(available(), Some(path.clone()));
            let output = std::process::Command::new(path)
                .arg("--version")
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert!(String::from_utf8_lossy(&output.stdout).contains("Chrome"));
        });
    }
}
