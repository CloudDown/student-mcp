//! Bounded, expiring capability links to original files and optional plain text.
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const TTL: Duration = Duration::from_secs(3 * 60 * 60);
const MAX_BYTES: usize = 24 * 1024 * 1024;
static ADDRESS: Mutex<(u16, Option<String>)> = Mutex::new((0, None));
static FILES: Mutex<Vec<Artifact>> = Mutex::new(Vec::new());
#[cfg(test)]
pub(crate) static LINK_LOCK: Mutex<()> = Mutex::new(());

struct Artifact {
    id: String,
    name: String,
    mime: String,
    bytes: Arc<Vec<u8>>,
    text: String,
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
    let mut files = FILES.lock().ok()?;
    files.retain(|f| f.expires > Instant::now());
    while files.len() >= 4
        || files
            .iter()
            .map(|f| f.bytes.len() + f.text.len())
            .sum::<usize>()
            + size
            > MAX_BYTES
    {
        files.remove(0);
    }
    files.push(Artifact {
        id: id.clone(),
        name: if name.is_empty() { "file".into() } else { name },
        mime: mime.replace(['\r', '\n'], ""),
        bytes,
        text: text.into(),
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

type Reply = (u16, Vec<(String, String)>, Vec<u8>);
pub(crate) fn route(method: &str, path: &str) -> Option<Reply> {
    let path = path.strip_prefix("/s/")?;
    let mut headers = vec![
        ("Cache-Control".into(), "no-store".into()),
        ("X-Content-Type-Options".into(), "nosniff".into()),
        ("Referrer-Policy".into(), "no-referrer".into()),
    ];
    if method != "GET" {
        return Some((405, headers, Vec::new()));
    }
    let mut files = FILES.lock().ok()?;
    files.retain(|f| f.expires > Instant::now());
    let Some((id, name)) = path.split_once('/') else {
        return Some((404, headers, Vec::new()));
    };
    let Some(file) = files.iter().find(|f| f.id == id) else {
        return Some((
            410,
            headers,
            b"File expired. Request a fresh link.".to_vec(),
        ));
    };
    let body = if name == "text" && !file.text.is_empty() {
        headers.push(("Content-Type".into(), "text/plain; charset=utf-8".into()));
        file.text.as_bytes().to_vec()
    } else if name == file.name {
        headers.push(("Content-Type".into(), file.mime.clone()));
        headers.push((
            "Content-Disposition".into(),
            format!("attachment; filename=\"{}\"", file.name),
        ));
        file.bytes.as_ref().clone()
    } else {
        return Some((404, headers, Vec::new()));
    };
    Some((200, headers, body))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_and_optional_text_only() {
        let _guard = LINK_LOCK.lock().unwrap();
        let id = publish("notes.pdf", "application/pdf", b"original", "hello", 1).unwrap();
        assert_eq!(
            route("GET", &format!("/s/{id}/notes.pdf")).unwrap().2,
            b"original"
        );
        assert_eq!(route("GET", &format!("/s/{id}/text")).unwrap().2, b"hello");
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
