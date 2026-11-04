//! Bounded, expiring capability links to original files and optional plain text.
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const TTL: Duration = Duration::from_secs(3 * 60 * 60);
const MAX_BYTES: usize = 24 * 1024 * 1024;
const MAX_FILES: usize = 128;
static ADDRESS: Mutex<(u16, Option<String>)> = Mutex::new((0, None));
static FILES: Mutex<Vec<Artifact>> = Mutex::new(Vec::new());
#[cfg(test)]
pub(crate) static LINK_LOCK: Mutex<()> = Mutex::new(());

struct Artifact {
    id: String,
    name: String,
    mime: String,
    bytes: Arc<Vec<u8>>,
    text: Arc<Vec<u8>>,
    expires: Instant,
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
    let mut files = FILES.lock().ok()?;
    publish_into(&mut files, filename, mime, bytes, text)
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
    files.push(Artifact {
        id: id.clone(),
        name: if name.is_empty() { "file".into() } else { name },
        mime: mime.replace(['\r', '\n'], ""),
        bytes,
        text: Arc::new(text.as_bytes().to_vec()),
        expires: Instant::now() + TTL,
    });
    Some(id)
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

pub(crate) fn expire(now: Instant) {
    if let Ok(mut files) = FILES.lock() {
        files.retain(|file| file.expires > now);
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
}
