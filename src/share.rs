//! Bounded, expiring capability links to original files and optional plain text.
//!
//! Links are served from memory and, when a private store is available, also
//! saved to disk so restarting the app does not invalidate a link that has not
//! reached its expiry yet.
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const DEFAULT_TTL_SECONDS: u64 = 24 * 60 * 60;
static TTL_SECONDS: AtomicU64 = AtomicU64::new(DEFAULT_TTL_SECONDS);
const MAX_BYTES: usize = 24 * 1024 * 1024;
const MAX_FILES: usize = 128;
static ADDRESS: Mutex<(u16, Option<String>)> = Mutex::new((0, None));
static FILES: Mutex<Vec<Artifact>> = Mutex::new(Vec::new());
/// Private directory holding unexpired links across restarts, when usable.
pub(crate) static STORE: Mutex<Option<PathBuf>> = Mutex::new(None);
#[cfg(test)]
pub(crate) static LINK_LOCK: Mutex<()> = Mutex::new(());

struct Artifact {
    id: String,
    name: String,
    mime: String,
    bytes: Arc<Vec<u8>>,
    text: Arc<Vec<u8>>,
    expires: Instant,
    /// Absolute expiry, persisted so a restart keeps the original deadline.
    expires_at: u64,
}

/// File-link lifetime, configured by `link_ttl_hours` (1–168, default 24).
pub(crate) fn ttl_seconds() -> u64 {
    TTL_SECONDS.load(Ordering::Relaxed)
}

pub(crate) fn set_ttl_seconds(seconds: u64) {
    TTL_SECONDS.store(seconds.clamp(60, 7 * 24 * 60 * 60), Ordering::Relaxed);
}

pub(crate) fn set_port(port: u16) {
    ADDRESS.lock().unwrap().0 = port;
}
pub(crate) fn set_origin(origin: Option<String>) {
    ADDRESS.lock().unwrap().1 = origin;
}
fn base_url() -> Option<String> {
    let address = ADDRESS.lock().ok()?;
    address
        .1
        .clone()
        .or_else(|| (address.0 > 0).then(|| format!("http://127.0.0.1:{}", address.0)))
}
pub(crate) fn enabled() -> bool {
    base_url().is_some()
}

pub(crate) fn publish_bytes(
    filename: &str,
    mime: &str,
    bytes: Arc<Vec<u8>>,
    text: &str,
    _pages: u32,
) -> Option<String> {
    crate::cache::start_expiry_worker();
    let id = {
        let mut files = FILES.lock().ok()?;
        publish_into(&mut files, filename, mime, bytes, text)?
    };
    // Durability must never block or break a live link: an unavailable store
    // keeps the in-memory link working until the app stops.
    if let Some(root) = store_root() {
        let snapshot = FILES.lock().ok().and_then(|files| {
            files.iter().find(|file| file.id == id).map(|file| {
                (
                    file.name.clone(),
                    file.mime.clone(),
                    file.expires_at,
                    Arc::clone(&file.bytes),
                    Arc::clone(&file.text),
                )
            })
        });
        if let Some((name, mime, expires_at, bytes, text)) = snapshot {
            let _ = persist(&root, &id, &name, &mime, expires_at, &bytes, &text);
        }
    }
    Some(id)
}

fn publish_into(
    files: &mut Vec<Artifact>,
    filename: &str,
    mime: &str,
    bytes: Arc<Vec<u8>>,
    text: &str,
) -> Option<String> {
    let size = bytes.len().checked_add(text.len())?;
    if bytes.is_empty() || size > MAX_BYTES {
        return None;
    }
    let id = crate::config::random_hex(16);
    let name = safe_name(filename);
    files.retain(|f| f.expires > Instant::now());
    // Never invalidate a live capability to admit a new file. Callers fall back
    // to inline delivery when the bounded store is full.
    if files.len() >= MAX_FILES
        || files
            .iter()
            .map(|f| f.bytes.len() + f.text.len())
            .sum::<usize>()
            + size
            > MAX_BYTES
    {
        return None;
    }
    let ttl = ttl_seconds();
    files.push(Artifact {
        id: id.clone(),
        name,
        mime: mime.replace(['\r', '\n'], ""),
        bytes,
        text: Arc::new(text.as_bytes().to_vec()),
        expires: Instant::now() + Duration::from_secs(ttl),
        expires_at: unix_now() + ttl,
    });
    Some(id)
}

/// Filename rendered safe for a URL and a `Content-Disposition` header.
fn safe_name(filename: &str) -> String {
    let name: String = filename
        .chars()
        .take(100)
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_') {
                c
            } else {
                '_'
            }
        })
        .collect();
    if name.is_empty() {
        "file".into()
    } else {
        name
    }
}

#[cfg(test)]
pub(crate) fn publish(
    filename: &str,
    mime: &str,
    bytes: &[u8],
    text: &str,
    pages: u32,
) -> Option<String> {
    publish_bytes(filename, mime, Arc::new(bytes.to_vec()), text, pages)
}

pub(crate) struct ShareLinks {
    pub download: String,
    pub text: Option<String>,
}
pub(crate) fn links_for(id: &str) -> Option<ShareLinks> {
    let base = base_url()?;
    let mut files = FILES.lock().ok()?;
    files.retain(|f| f.expires > Instant::now());
    let file = files.iter().find(|f| f.id == id)?;
    Some(ShareLinks {
        download: format!("{base}/s/{id}/{}", file.name),
        text: (!file.text.is_empty()).then(|| format!("{base}/s/{id}/text")),
    })
}

pub(crate) type Reply = (u16, Vec<(String, String)>, Arc<Vec<u8>>);

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0)
}

fn store_root() -> Option<PathBuf> {
    STORE.lock().ok()?.clone()
}

/// Default store directory next to the configuration file
/// (`~/.student-mcp.links/`). Unit fixtures never touch real link files.
pub(crate) fn default_root() -> Option<PathBuf> {
    #[cfg(test)]
    {
        crate::config::tests::override_config_path().map(|path| path.with_extension("links"))
    }
    #[cfg(not(test))]
    {
        Some(crate::config::config_path().with_extension("links"))
    }
}

/// Open the private store and restore links whose deadline has not passed.
pub(crate) fn init_default() {
    if let Some(root) = default_root() {
        init(root);
    }
}

pub(crate) fn init(root: PathBuf) {
    // An unusable location must not fail the app: links keep working from memory.
    if private_dir(&root).is_none() {
        return;
    }
    if let Ok(mut slot) = STORE.lock() {
        *slot = Some(root.clone());
    }
    let loaded = load_from(&root, unix_now());
    if let Ok(mut files) = FILES.lock() {
        for artifact in loaded {
            if !files.iter().any(|file| file.id == artifact.id) {
                files.push(artifact);
            }
        }
    }
    // Stale entries left by an earlier run are reclaimed even if nothing new is
    // ever published in this session.
    crate::cache::start_expiry_worker();
}

fn meta_path(root: &Path, id: &str) -> PathBuf {
    root.join(format!("{id}.json"))
}
fn data_path(root: &Path, id: &str) -> PathBuf {
    root.join(format!("{id}.data"))
}
fn text_path(root: &Path, id: &str) -> PathBuf {
    root.join(format!("{id}.text"))
}

fn regular(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok_and(|meta| meta.file_type().is_file())
}

fn private_dir(path: &Path) -> Option<()> {
    let mut builder = std::fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    match builder.create(path) {
        Ok(()) => Some(()),
        Err(_) if std::fs::symlink_metadata(path).ok()?.is_dir() => {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700)).ok()?;
            }
            Some(())
        }
        _ => None,
    }
}

/// Write one file atomically: a readable link never contains a partial artifact.
fn write_private(path: &Path, bytes: &[u8]) -> Option<()> {
    let temp = path.with_extension(format!("tmp{}", crate::config::random_hex(6)));
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&temp).ok()?;
    let written = file.write_all(bytes).and_then(|()| file.sync_all());
    drop(file);
    if written.is_err() || std::fs::rename(&temp, path).is_err() {
        let _ = std::fs::remove_file(&temp);
        return None;
    }
    Some(())
}

/// Save one artifact. Metadata is written last so a crash mid-write leaves an
/// orphan payload, never a link that claims bytes it does not have.
fn persist(
    root: &Path,
    id: &str,
    name: &str,
    mime: &str,
    expires_at: u64,
    bytes: &[u8],
    text: &[u8],
) -> Option<()> {
    private_dir(root)?;
    write_private(&data_path(root, id), bytes)?;
    if !text.is_empty() {
        write_private(&text_path(root, id), text)?;
    }
    let meta = serde_json::json!({
        "id": id,
        "name": name,
        "mime": mime,
        "expires": expires_at,
        "text": !text.is_empty(),
    });
    write_private(&meta_path(root, id), meta.to_string().as_bytes())
}

fn forget(root: &Path, id: &str) {
    for path in [
        meta_path(root, id),
        data_path(root, id),
        text_path(root, id),
    ] {
        let _ = std::fs::remove_file(path);
    }
}

fn read_artifact(root: &Path, id: &str, now: u64) -> Option<Artifact> {
    let meta_path = meta_path(root, id);
    if !regular(&meta_path) {
        return None;
    }
    let meta: serde_json::Value = serde_json::from_slice(&std::fs::read(&meta_path).ok()?).ok()?;
    let expires_at = meta.get("expires")?.as_u64()?;
    if expires_at <= now {
        return None;
    }
    let data = data_path(root, id);
    if !regular(&data) {
        return None;
    }
    let bytes = std::fs::read(&data).ok()?;
    if bytes.is_empty() || bytes.len() > MAX_BYTES {
        return None;
    }
    let text = if meta.get("text").and_then(serde_json::Value::as_bool) == Some(true) {
        let text_path = text_path(root, id);
        if regular(&text_path) {
            std::fs::read(&text_path).unwrap_or_default()
        } else {
            Vec::new()
        }
    } else {
        Vec::new()
    };
    if bytes.len() + text.len() > MAX_BYTES {
        return None;
    }
    Some(Artifact {
        id: id.to_string(),
        name: safe_name(
            meta.get("name")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("file"),
        ),
        mime: meta
            .get("mime")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("application/octet-stream")
            .replace(['\r', '\n'], ""),
        bytes: Arc::new(bytes),
        text: Arc::new(text),
        expires: Instant::now() + Duration::from_secs(expires_at - now),
        expires_at,
    })
}

/// Restore the store without trusting its contents: expired, corrupt, linked or
/// oversized entries are removed instead of served.
fn load_from(root: &Path, now: u64) -> Vec<Artifact> {
    let Ok(entries) = std::fs::read_dir(root) else {
        return Vec::new();
    };
    let mut loaded = Vec::new();
    let mut kept: Vec<String> = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().is_none_or(|extension| extension != "json") {
            continue;
        }
        let Some(id) = path.file_stem().and_then(|stem| stem.to_str()) else {
            continue;
        };
        if id.len() != 32 || !id.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            continue;
        }
        match read_artifact(root, id, now) {
            Some(artifact) => {
                kept.push(id.to_string());
                loaded.push(artifact);
            }
            None => forget(root, id),
        }
    }
    // Payload files without metadata are interrupted writes or expired links.
    if let Ok(entries) = std::fs::read_dir(root) {
        for entry in entries.flatten() {
            let path = entry.path();
            let Some(id) = path.file_stem().and_then(|stem| stem.to_str()) else {
                continue;
            };
            if kept.iter().any(|known| known == id) {
                continue;
            }
            if path
                .extension()
                .is_some_and(|extension| extension == "data" || extension == "text")
            {
                let _ = std::fs::remove_file(&path);
            }
        }
    }
    // The same bounds as publishing, keeping the longest-lived links.
    loaded.sort_by_key(|artifact| std::cmp::Reverse(artifact.expires));
    let mut bounded = Vec::new();
    let mut total = 0;
    for artifact in loaded {
        let size = artifact.bytes.len() + artifact.text.len();
        if bounded.len() >= MAX_FILES || total + size > MAX_BYTES {
            forget(root, &artifact.id);
            continue;
        }
        total += size;
        bounded.push(artifact);
    }
    bounded
}

pub(crate) fn expire(now: Instant) {
    let mut removed = Vec::new();
    if let Ok(mut files) = FILES.lock() {
        for file in files.iter().filter(|file| file.expires <= now) {
            removed.push(file.id.clone());
        }
        files.retain(|file| file.expires > now);
    }
    if let Some(root) = store_root() {
        for id in removed {
            forget(&root, &id);
        }
    }
}

pub(crate) fn route(method: &str, path: &str) -> Option<Reply> {
    let path = path.strip_prefix("/s/")?;
    let mut headers = vec![
        ("Cache-Control".into(), "no-store".into()),
        ("X-Content-Type-Options".into(), "nosniff".into()),
        ("Referrer-Policy".into(), "no-referrer".into()),
    ];
    if method != "GET" {
        return Some((405, headers, Arc::default()));
    }
    let mut files = FILES.lock().ok()?;
    files.retain(|f| f.expires > Instant::now());
    let Some((id, name)) = path.split_once('/') else {
        return Some((404, headers, Arc::default()));
    };
    let Some(file) = files.iter().find(|f| f.id == id) else {
        return Some((
            410,
            headers,
            Arc::new(b"File expired. Request a fresh link.".to_vec()),
        ));
    };
    let body = if name == "text" && !file.text.is_empty() {
        headers.push(("Content-Type".into(), "text/plain; charset=utf-8".into()));
        Arc::clone(&file.text)
    } else if name == file.name {
        headers.push(("Content-Type".into(), file.mime.clone()));
        headers.push((
            "Content-Disposition".into(),
            format!("attachment; filename=\"{}\"", file.name),
        ));
        Arc::clone(&file.bytes)
    } else {
        return Some((404, headers, Arc::default()));
    };
    Some((200, headers, body))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expiry_releases_storage_without_copying_or_interrupting_downloads() {
        let _guard = LINK_LOCK.lock().unwrap();
        let bytes = Arc::new(vec![7; 1024 * 1024]);
        let weak = Arc::downgrade(&bytes);
        let id = publish_bytes(
            "large.pdf",
            "application/pdf",
            Arc::clone(&bytes),
            "text",
            1,
        )
        .unwrap();
        let live = publish("live.pdf", "application/pdf", b"live", "", 1).unwrap();
        let body = route("GET", &format!("/s/{id}/large.pdf")).unwrap().2;
        assert!(Arc::ptr_eq(&bytes, &body));
        let text = route("GET", &format!("/s/{id}/text")).unwrap().2;
        assert!(Arc::ptr_eq(
            &text,
            &route("GET", &format!("/s/{id}/text")).unwrap().2
        ));
        drop(bytes);
        FILES
            .lock()
            .unwrap()
            .iter_mut()
            .find(|file| file.id == id)
            .unwrap()
            .expires = Instant::now();
        expire(Instant::now());
        assert!(!FILES.lock().unwrap().iter().any(|file| file.id == id));
        assert_eq!(route("GET", &format!("/s/{id}/large.pdf")).unwrap().0, 410);
        assert_eq!(route("GET", &format!("/s/{live}/live.pdf")).unwrap().0, 200);
        assert_eq!(body.len(), 1024 * 1024);
        assert!(body.iter().all(|byte| *byte == 7));
        drop(body);
        assert!(weak.upgrade().is_none());
    }

    #[test]
    fn capacity_never_evicts_a_live_link_and_expiry_reclaims_space() {
        let mut files = Vec::new();
        let bytes = Arc::new(vec![1]);
        let mut ids = Vec::new();
        for _ in 0..MAX_FILES {
            ids.push(
                publish_into(
                    &mut files,
                    "notes.pdf",
                    "application/pdf",
                    bytes.clone(),
                    "",
                )
                .unwrap(),
            );
        }
        assert!(
            publish_into(&mut files, "new.pdf", "application/pdf", bytes.clone(), "").is_none()
        );
        assert_eq!(
            files.iter().map(|f| &f.id).collect::<Vec<_>>(),
            ids.iter().collect::<Vec<_>>()
        );
        files[0].expires = Instant::now();
        assert!(publish_into(&mut files, "new.pdf", "application/pdf", bytes, "").is_some());
        assert!(!files.iter().any(|f| f.id == ids[0]));
        assert!(files.iter().any(|f| f.id == ids[1]));

        let mut files = Vec::new();
        let id = publish_into(
            &mut files,
            "large.pdf",
            "application/pdf",
            Arc::new(vec![0; MAX_BYTES]),
            "",
        )
        .unwrap();
        assert!(publish_into(
            &mut files,
            "extra.pdf",
            "application/pdf",
            Arc::new(vec![1]),
            ""
        )
        .is_none());
        assert_eq!(files[0].id, id);
    }
    #[test]
    fn original_and_optional_text_only() {
        let _guard = LINK_LOCK.lock().unwrap();
        let id = publish("notes.pdf", "application/pdf", b"original", "hello", 1).unwrap();
        assert_eq!(
            route("GET", &format!("/s/{id}/notes.pdf"))
                .unwrap()
                .2
                .as_slice(),
            b"original"
        );
        assert_eq!(
            route("GET", &format!("/s/{id}/text")).unwrap().2.as_slice(),
            b"hello"
        );
        assert_eq!(route("GET", &format!("/s/{id}/read")).unwrap().0, 404);
        assert_eq!(route("POST", &format!("/s/{id}/notes.pdf")).unwrap().0, 405);
        assert_eq!(route("GET", "/s/unknown/file").unwrap().0, 410);
        FILES
            .lock()
            .unwrap()
            .iter_mut()
            .find(|f| f.id == id)
            .unwrap()
            .expires = Instant::now();
        assert_eq!(route("GET", &format!("/s/{id}/notes.pdf")).unwrap().0, 410);
    }

    #[test]
    fn link_lifetime_is_configurable_and_bounded() {
        let _guard = LINK_LOCK.lock().unwrap();
        let original = ttl_seconds();
        set_ttl_seconds(2 * 60 * 60);
        assert_eq!(ttl_seconds(), 7200);
        set_ttl_seconds(0);
        assert_eq!(ttl_seconds(), 60);
        set_ttl_seconds(u64::MAX);
        assert_eq!(ttl_seconds(), 7 * 24 * 60 * 60);
        set_ttl_seconds(original);
    }

    /// Restore the process-wide store pointer even if an assertion fails.
    struct StoreReset;
    impl Drop for StoreReset {
        fn drop(&mut self) {
            if let Ok(mut slot) = STORE.lock() {
                *slot = None;
            }
        }
    }

    #[test]
    fn restart_keeps_unexpired_links_and_expiry_removes_their_files() {
        crate::config::tests::with_temp_config(|| {
            let _guard = LINK_LOCK.lock().unwrap();
            let _reset = StoreReset;
            let root = default_root().unwrap();
            init(root.clone());
            let id = publish(
                "td7.pdf",
                "application/pdf",
                b"%PDF-1.4 original",
                "page one",
                1,
            )
            .unwrap();
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                assert_eq!(
                    std::fs::metadata(&root).unwrap().permissions().mode() & 0o777,
                    0o700
                );
                assert_eq!(
                    std::fs::metadata(data_path(&root, &id))
                        .unwrap()
                        .permissions()
                        .mode()
                        & 0o777,
                    0o600
                );
            }
            // Simulate a restart: nothing kept in memory, everything from disk.
            FILES.lock().unwrap().clear();
            init(root.clone());
            assert_eq!(
                route("GET", &format!("/s/{id}/td7.pdf"))
                    .unwrap()
                    .2
                    .as_slice(),
                b"%PDF-1.4 original"
            );
            assert_eq!(
                route("GET", &format!("/s/{id}/text")).unwrap().2.as_slice(),
                b"page one"
            );
            // The restored deadline is the original one, not a fresh full TTL.
            let remaining = FILES
                .lock()
                .unwrap()
                .iter()
                .find(|file| file.id == id)
                .unwrap()
                .expires
                .saturating_duration_since(Instant::now());
            assert!(remaining <= Duration::from_secs(ttl_seconds()));
            FILES
                .lock()
                .unwrap()
                .iter_mut()
                .find(|file| file.id == id)
                .unwrap()
                .expires = Instant::now();
            expire(Instant::now());
            assert_eq!(route("GET", &format!("/s/{id}/td7.pdf")).unwrap().0, 410);
            for path in [
                data_path(&root, &id),
                text_path(&root, &id),
                meta_path(&root, &id),
            ] {
                assert!(!path.exists(), "{path:?}");
            }
        });
    }

    #[test]
    fn restore_drops_expired_orphan_and_symlinked_entries() {
        let root = std::env::temp_dir().join(format!(
            "student-mcp-links-{}-{}",
            std::process::id(),
            crate::config::random_hex(6)
        ));
        private_dir(&root).unwrap();
        let expired = "e".repeat(32);
        std::fs::write(
            meta_path(&root, &expired),
            serde_json::json!({"id": expired, "name": "old.pdf", "mime": "application/pdf",
                "expires": unix_now() - 1, "text": false})
            .to_string(),
        )
        .unwrap();
        std::fs::write(data_path(&root, &expired), b"old").unwrap();
        let orphan = "0".repeat(32);
        std::fs::write(data_path(&root, &orphan), b"orphan").unwrap();
        let live = "a".repeat(32);
        std::fs::write(
            meta_path(&root, &live),
            serde_json::json!({"id": live, "name": "kept.pdf", "mime": "application/pdf",
                "expires": unix_now() + 60, "text": false})
            .to_string(),
        )
        .unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;
            symlink("/etc/hostname", data_path(&root, &live)).unwrap();
        }
        #[cfg(not(unix))]
        std::fs::write(data_path(&root, &live), b"kept").unwrap();
        let loaded = load_from(&root, unix_now());
        assert!(
            loaded.is_empty(),
            "expired, linked and incomplete entries are dropped"
        );
        assert!(!meta_path(&root, &expired).exists());
        assert!(!data_path(&root, &expired).exists());
        assert!(!data_path(&root, &orphan).exists());
        assert!(!meta_path(&root, &live).exists());
        assert!(!data_path(&root, &live).exists());
        let _ = std::fs::remove_dir_all(&root);
    }
}
