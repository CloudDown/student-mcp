//! Shareable file links for MCP tool results.
//!
//! Browser AIs often ignore a base64 PDF blob returned by a tool. The same file
//! served over the local HTTP endpoint (or its srv.us tunnel) can be opened,
//! downloaded, or read as HTML — the closest thing to a drag-and-drop upload.
//!
//! Links are capability URLs: the 128-bit id is the secret, and artifacts expire
//! after a few hours. No Moodle token is ever part of a link.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

const TTL: Duration = Duration::from_secs(3 * 60 * 60);
const MAX_ARTIFACTS: usize = 16;
const MAX_TOTAL_BYTES: usize = 96 * 1024 * 1024;
const MAX_PAGE_CACHE: usize = 160;
/// Page window used for the pre-built "parts" links.
pub(crate) const PART_PAGES: u32 = 24;

struct Address {
    origin: Option<String>,
    port: Option<u16>,
}

#[derive(Clone)]
struct Artifact {
    id: String,
    filename: String,
    slug: String,
    mime: String,
    bytes: Arc<Vec<u8>>,
    text: Arc<String>,
    pages: u32,
    expires: Instant,
}

static ADDRESS: OnceLock<Mutex<Address>> = OnceLock::new();
static ARTIFACTS: OnceLock<Mutex<Vec<Artifact>>> = OnceLock::new();
static PAGE_CACHE: OnceLock<Mutex<HashMap<(String, u32), Arc<Vec<u8>>>>> = OnceLock::new();

/// Tests that change the process-wide link address must not run concurrently.
#[cfg(test)]
pub(crate) static LINK_LOCK: Mutex<()> = Mutex::new(());

fn address() -> &'static Mutex<Address> {
    ADDRESS.get_or_init(|| {
        Mutex::new(Address {
            origin: None,
            port: None,
        })
    })
}

fn artifacts() -> &'static Mutex<Vec<Artifact>> {
    ARTIFACTS.get_or_init(|| Mutex::new(Vec::new()))
}

fn page_cache() -> &'static Mutex<HashMap<(String, u32), Arc<Vec<u8>>>> {
    PAGE_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Local port of the MCP HTTP endpoint (student-mcp up).
pub(crate) fn set_port(port: u16) {
    if let Ok(mut address) = address().lock() {
        address.port = Some(port);
    }
}

/// Public tunnel origin, if one is ready. Clearing it falls back to localhost.
#[cfg_attr(test, allow(dead_code))]
pub(crate) fn set_origin(origin: Option<String>) {
    if let Ok(mut address) = address().lock() {
        address.origin = origin.filter(|value| !value.trim().is_empty());
    }
}

fn base_url() -> Option<String> {
    let address = address().lock().ok()?;
    if let Some(origin) = &address.origin {
        return Some(origin.trim_end_matches('/').to_string());
    }
    address
        .port
        .filter(|port| *port > 0)
        .map(|port| format!("http://127.0.0.1:{port}"))
}

fn slug(filename: &str) -> String {
    let mut out = String::new();
    let mut separator = false;
    for ch in filename.chars() {
        if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '-' | '_') {
            out.push(ch);
            separator = false;
        } else if !separator {
            out.push('_');
            separator = true;
        }
    }
    let out = out.trim_matches(|ch| matches!(ch, '_' | '.' | '-'));
    let mut out = if out.is_empty() {
        "file".to_string()
    } else {
        out.chars().take(80).collect()
    };
    if out.to_ascii_lowercase().ends_with(".pdf") {
        out.truncate(out.len() - 4);
    }
    out.push_str(".pdf");
    out
}

/// Store a PDF and return its capability id. `text` is the full PDF text.
pub(crate) fn publish(
    filename: &str,
    mime: &str,
    bytes: &[u8],
    text: &str,
    pages: u32,
) -> Option<String> {
    if bytes.is_empty() || bytes.len() > MAX_TOTAL_BYTES {
        return None;
    }
    let id = crate::config::random_hex(16);
    let now = Instant::now();
    let artifact = Artifact {
        id: id.clone(),
        filename: filename.to_string(),
        slug: slug(filename),
        mime: mime.to_string(),
        bytes: Arc::new(bytes.to_vec()),
        text: Arc::new(text.to_string()),
        pages,
        expires: now + TTL,
    };
    if let Ok(mut list) = artifacts().lock() {
        list.retain(|item| item.expires > now);
        list.push(artifact);
        while list.len() > MAX_ARTIFACTS {
            let old = list.remove(0);
            if let Ok(mut cache) = page_cache().lock() {
                cache.retain(|(artifact_id, _), _| artifact_id != &old.id);
            }
        }
        let mut total: usize = list.iter().map(|item| item.bytes.len()).sum();
        while total > MAX_TOTAL_BYTES && list.len() > 1 {
            let old = list.remove(0);
            total = total.saturating_sub(old.bytes.len());
            if let Ok(mut cache) = page_cache().lock() {
                cache.retain(|(artifact_id, _), _| artifact_id != &old.id);
            }
        }
    }
    Some(id)
}

fn get(id: &str) -> Option<Artifact> {
    let now = Instant::now();
    let mut list = artifacts().lock().ok()?;
    list.retain(|item| item.expires > now);
    list.iter().find(|item| item.id == id).cloned()
}

pub(crate) struct ShareLinks {
    pub download: String,
    pub reader: String,
    pub text: String,
    pub pages: u32,
}

pub(crate) fn enabled() -> bool {
    base_url().is_some()
}

pub(crate) fn links_for(id: &str) -> Option<ShareLinks> {
    let artifact = get(id)?;
    let base = base_url()?;
    Some(ShareLinks {
        download: format!("{base}/s/{}/{slug}", artifact.id, slug = artifact.slug),
        reader: format!("{base}/s/{}/read", artifact.id),
        text: format!("{base}/s/{}/text", artifact.id),
        pages: artifact.pages,
    })
}

pub(crate) fn page_window_url(id: &str, from: u32, to: u32) -> Option<String> {
    let artifact = get(id)?;
    let base = base_url()?;
    Some(format!(
        "{base}/s/{}/p/{from}-{to}.pdf",
        artifact.id,
        from = from.max(1),
        to = to.max(1)
    ))
}

/// HTTP route for `/s/...`. Returns `None` when the path is not ours.
pub(crate) fn route(method: &str, path: &str) -> Option<(u16, Vec<(String, String)>, Vec<u8>)> {
    let segments: Vec<&str> = path.trim_start_matches('/').split('/').collect();
    if segments.first() != Some(&"s") {
        return None;
    }
    if method != "GET" {
        return Some(text_response(
            405,
            "Only GET is supported for shared files.".to_string(),
        ));
    }
    let id = segments.get(1).copied().unwrap_or_default();
    if id.is_empty() {
        return Some(text_response(404, "File not found.".into()));
    }
    let Some(artifact) = get(id) else {
        return Some(text_response(
            410,
            "This file link expired. Call moodle_file again to get a fresh link.".into(),
        ));
    };
    let rest: Vec<&str> = segments.iter().skip(2).copied().collect();
    match rest.as_slice() {
        ["read"] => Some((
            200,
            vec![
                ("Content-Type".into(), "text/html; charset=utf-8".into()),
                (
                    "Cache-Control".into(),
                    format!("private, max-age={}", TTL.as_secs()),
                ),
                (
                    "Content-Security-Policy".into(),
                    "default-src 'none'; img-src 'self'; style-src 'unsafe-inline'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'".into(),
                ),
                ("X-Content-Type-Options".into(), "nosniff".into()),
                ("Referrer-Policy".into(), "no-referrer".into()),
            ],
            reader_html(&artifact).into_bytes(),
        )),
        ["text"] => Some((
            200,
            vec![
                ("Content-Type".into(), "text/plain; charset=utf-8".into()),
                (
                    "Cache-Control".into(),
                    format!("private, max-age={}", TTL.as_secs()),
                ),
                ("X-Content-Type-Options".into(), "nosniff".into()),
            ],
            artifact.text.as_bytes().to_vec(),
        )),
        [name] if name.ends_with(".pdf") => Some((
            200,
            file_headers(&artifact, None),
            artifact.bytes.as_ref().clone(),
        )),
        ["p", name] if name.ends_with(".jpg") => {
            let Ok(page) = name.trim_end_matches(".jpg").parse::<u32>() else {
                return Some(text_response(404, "Page not found.".into()));
            };
            if page == 0 || (artifact.pages > 0 && page > artifact.pages) {
                return Some(text_response(404, "Page not found.".into()));
            }
            match render_page(&artifact, page) {
                Some(image) => Some((
                    200,
                    vec![
                        ("Content-Type".into(), "image/jpeg".into()),
                        (
                            "Cache-Control".into(),
                            format!("private, max-age={}", TTL.as_secs()),
                        ),
                        ("X-Content-Type-Options".into(), "nosniff".into()),
                    ],
                    image.as_ref().clone(),
                )),
                None => Some(text_response(404, "Page image unavailable on this computer.".into())),
            }
        }
        ["p", name] if name.ends_with(".pdf") => {
            let stem = name.trim_end_matches(".pdf");
            let Some((from, to)) = stem.split_once('-') else {
                return Some(text_response(404, "File not found.".into()));
            };
            let (Ok(from), Ok(to)) = (from.parse::<u32>(), to.parse::<u32>()) else {
                return Some(text_response(404, "File not found.".into()));
            };
            let from = from.max(1);
            let to = to.max(1);
            if to < from {
                return Some(text_response(400, "Invalid page range.".into()));
            }
            let to = if artifact.pages > 0 {
                to.min(artifact.pages)
            } else {
                to
            };
            if from > to {
                return Some(text_response(404, "Page range outside this PDF.".into()));
            }
            let bytes = if from == 1 && (artifact.pages == 0 || to >= artifact.pages) {
                artifact.bytes.as_ref().clone()
            } else {
                slice_pdf(&artifact.bytes, from, to)
                    .unwrap_or_else(|| artifact.bytes.as_ref().clone())
            };
            Some((200, file_headers(&artifact, Some((from, to))), bytes))
        }
        _ => Some(text_response(404, "File not found.".into())),
    }
}

fn file_headers(artifact: &Artifact, range: Option<(u32, u32)>) -> Vec<(String, String)> {
    let filename = match range {
        Some((from, to)) => {
            let stem = if artifact.filename.to_ascii_lowercase().ends_with(".pdf") {
                &artifact.filename[..artifact.filename.len() - 4]
            } else {
                &artifact.filename
            };
            format!("{stem}-p{from}-{to}.pdf")
        }
        None => artifact.filename.clone(),
    };
    vec![
        ("Content-Type".into(), artifact.mime.clone()),
        (
            "Content-Disposition".into(),
            format!(
                "inline; filename=\"{}\"",
                filename.replace('"', "").replace(['\r', '\n'], " ")
            ),
        ),
        (
            "Cache-Control".into(),
            format!("private, max-age={}", TTL.as_secs()),
        ),
        ("X-Content-Type-Options".into(), "nosniff".into()),
        ("Referrer-Policy".into(), "no-referrer".into()),
    ]
}

fn text_response(status: u16, message: String) -> (u16, Vec<(String, String)>, Vec<u8>) {
    (
        status,
        vec![("Content-Type".into(), "text/plain; charset=utf-8".into())],
        message.into_bytes(),
    )
}

fn reader_html(artifact: &Artifact) -> String {
    let pages = split_pages(&artifact.text, artifact.pages);
    let mut nav = String::new();
    for (index, text) in pages.iter().enumerate() {
        let number = index as u32 + 1;
        let first = text
            .lines()
            .map(str::trim)
            .find(|line| !line.is_empty())
            .unwrap_or("(page sans texte)");
        let first = truncate_chars(first, 90);
        nav.push_str(&format!(
            "<li><a href=\"#p{number}\">Page {number}</a> — {}</li>\n",
            escape(&first)
        ));
    }
    let mut sections = String::new();
    for (index, text) in pages.iter().enumerate() {
        let number = index as u32 + 1;
        let body = if text.trim().is_empty() {
            "<p class=\"empty\">Cette page n'a pas de couche texte. L'image ci-dessous reste lisible.</p>"
                .to_string()
        } else {
            format!("<pre>{}</pre>", escape(text))
        };
        sections.push_str(&format!(
            "<section id=\"p{number}\"><h2>Page {number}</h2>{body}<figure><img loading=\"lazy\" src=\"p/{number}.jpg\" alt=\"Page {number}\"></figure></section>\n"
        ));
    }
    format!(
        r#"<!doctype html>
<html lang="fr">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<meta name="robots" content="noindex">
<title>{title}</title>
<style>
:root {{ color-scheme: light dark; --ink:#1c1b19; --muted:#6b6862; --paper:#fff; --line:#e6e2da; --accent:#b4532a; }}
@media (prefers-color-scheme: dark) {{ :root {{ --ink:#ece8e1; --muted:#a39e95; --paper:#171614; --line:#33302b; --accent:#e08a5f; }} }}
* {{ box-sizing:border-box; }}
body {{ margin:0; background:var(--paper); color:var(--ink); font:16px/1.55 ui-sans-serif,system-ui,-apple-system,"Segoe UI",sans-serif; }}
header {{ max-width:860px; margin:0 auto; padding:32px 20px 8px; }}
h1 {{ font-size:1.35rem; line-height:1.25; margin:0 0 8px; overflow-wrap:anywhere; }}
.meta {{ color:var(--muted); font-size:.9rem; }}
.meta a {{ color:var(--accent); }}
nav {{ max-width:860px; margin:16px auto; padding:0 20px; }}
nav details {{ border:1px solid var(--line); border-radius:10px; padding:10px 14px; }}
nav summary {{ cursor:pointer; color:var(--muted); }}
nav ol {{ margin:12px 0 0; padding-left:1.4em; font-size:.9rem; }}
nav li {{ margin:4px 0; }}
main {{ max-width:860px; margin:0 auto; padding:0 20px 80px; }}
section {{ border-top:1px solid var(--line); padding:26px 0; scroll-margin-top:12px; }}
h2 {{ font-size:.8rem; letter-spacing:.12em; text-transform:uppercase; color:var(--muted); margin:0 0 14px; }}
pre {{ margin:0; white-space:pre-wrap; overflow-wrap:anywhere; font:13.5px/1.5 ui-monospace,SFMono-Regular,Menlo,monospace; }}
.empty {{ color:var(--muted); font-style:italic; }}
figure {{ margin:18px 0 0; }}
img {{ width:100%; height:auto; border:1px solid var(--line); border-radius:8px; background:#fff; }}
@media print {{ nav {{ display:none; }} section {{ break-inside:avoid; }} img {{ max-height:95vh; object-fit:contain; }} }}
</style>
</head>
<body>
<header>
<h1>{title}</h1>
<p class="meta">{pages} page(s) · <a href="{slug}">Télécharger le PDF</a> · <a href="text">Texte brut</a> · lien privé, expire dans 3 h</p>
</header>
<nav><details><summary>Sommaire</summary><ol>{nav}</ol></details></nav>
<main>{sections}</main>
</body>
</html>
"#,
        title = escape(&artifact.filename),
        pages = artifact.pages.max(pages.len() as u32),
        slug = artifact.slug,
        nav = nav,
        sections = sections,
    )
}

fn split_pages(text: &str, total: u32) -> Vec<String> {
    let mut pages: Vec<String> = text
        .split('\u{0C}')
        .map(|page| page.trim_matches('\n').to_string())
        .collect();
    if total > 0 {
        pages.truncate(total as usize);
        while pages.len() < total as usize {
            pages.push(String::new());
        }
    } else if pages.last().is_some_and(|page| page.trim().is_empty()) {
        pages.pop();
    }
    if pages.is_empty() {
        pages.push(String::new());
    }
    pages
}

fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(ch),
        }
    }
    out
}

fn truncate_chars(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        text.to_string()
    } else {
        let mut out: String = text.chars().take(max.saturating_sub(1)).collect();
        out.push('…');
        out
    }
}

fn slice_pdf(bytes: &[u8], from: u32, to: u32) -> Option<Vec<u8>> {
    let dir = temp_dir()?;
    let input = dir.join("in.pdf");
    let output = dir.join("out.pdf");
    let result = std::fs::write(&input, bytes).is_ok()
        && std::process::Command::new("pdftocairo")
            .args(["-pdf", "-f", &from.to_string(), "-l", &to.to_string()])
            .arg(&input)
            .arg(&output)
            .status()
            .map(|status| status.success())
            .unwrap_or(false);
    let bytes = if result {
        std::fs::read(&output).ok().filter(|data| !data.is_empty())
    } else {
        None
    };
    let _ = std::fs::remove_dir_all(&dir);
    bytes
}

fn render_page(artifact: &Artifact, page: u32) -> Option<Arc<Vec<u8>>> {
    if let Ok(cache) = page_cache().lock() {
        if let Some(image) = cache.get(&(artifact.id.clone(), page)) {
            return Some(Arc::clone(image));
        }
    }
    let dir = temp_dir()?;
    let input = dir.join("in.pdf");
    let prefix = dir.join("page");
    let ok = std::fs::write(&input, artifact.bytes.as_ref()).is_ok()
        && std::process::Command::new("pdftoppm")
            .args([
                "-jpeg",
                "-jpegopt",
                "quality=78",
                "-r",
                "120",
                "-scale-to",
                "1600",
                "-f",
                &page.to_string(),
                "-l",
                &page.to_string(),
            ])
            .arg(&input)
            .arg(&prefix)
            .status()
            .map(|status| status.success())
            .unwrap_or(false);
    let image = if ok {
        std::fs::read_dir(&dir)
            .ok()
            .into_iter()
            .flatten()
            .flatten()
            .map(|entry| entry.path())
            .find(|path| {
                path.extension()
                    .and_then(|ext| ext.to_str())
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("jpg"))
            })
            .and_then(|path| std::fs::read(path).ok())
            .filter(|data| !data.is_empty())
            .map(Arc::new)
    } else {
        None
    };
    let _ = std::fs::remove_dir_all(&dir);
    if let Some(image) = &image {
        if let Ok(mut cache) = page_cache().lock() {
            if cache.len() >= MAX_PAGE_CACHE {
                let key = cache.keys().next().cloned();
                if let Some(key) = key {
                    cache.remove(&key);
                }
            }
            cache.insert((artifact.id.clone(), page), Arc::clone(image));
        }
    }
    image
}

fn temp_dir() -> Option<std::path::PathBuf> {
    let mut id = [0u8; 8];
    getrandom::getrandom(&mut id).ok()?;
    let root = std::env::temp_dir().join("student-mcp");
    std::fs::create_dir_all(&root).ok()?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700)).ok()?;
    }
    let dir = root.join(format!("share-{:x}", u64::from_le_bytes(id)));
    std::fs::create_dir_all(&dir).ok()?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).ok()?;
    }
    Some(dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugs_are_safe_and_end_in_pdf() {
        assert_eq!(slug("TD 5 - Dérivées.pdf"), "TD_5_-_D_riv_es.pdf");
        assert_eq!(slug("../../etc/passwd"), "etc_passwd.pdf");
        assert!(slug("").ends_with(".pdf"));
    }

    #[test]
    fn html_escape_covers_the_basics() {
        assert_eq!(
            escape("<script>\"x\" & 'y'</script>"),
            "&lt;script&gt;&quot;x&quot; &amp; &#39;y&#39;&lt;/script&gt;"
        );
    }

    #[test]
    fn published_files_are_served_as_pdf_text_and_html() {
        let id = publish(
            "notes.pdf",
            "application/pdf",
            b"%PDF-1.4 fake",
            "Page one\u{0C}Page two",
            2,
        )
        .unwrap();
        let (status, headers, body) = route("GET", &format!("/s/{id}/notes.pdf")).unwrap();
        assert_eq!(status, 200);
        assert_eq!(body, b"%PDF-1.4 fake");
        assert!(headers
            .iter()
            .any(|(key, value)| key == "Content-Type" && value == "application/pdf"));
        let (status, _, body) = route("GET", &format!("/s/{id}/text")).unwrap();
        assert_eq!(status, 200);
        assert!(String::from_utf8_lossy(&body).contains("Page one"));
        let (status, _, body) = route("GET", &format!("/s/{id}/read")).unwrap();
        assert_eq!(status, 200);
        let html = String::from_utf8_lossy(&body);
        assert!(html.contains("Page two"));
        assert!(html.contains(&format!("src=\"p/2.jpg\"")));
    }

    #[test]
    fn unknown_paths_are_not_claimed() {
        assert!(route("GET", "/mcp/token").is_none());
        assert!(route("GET", "/s/nope/read").is_some());
    }

    fn fixture_pdf() -> Option<Vec<u8>> {
        for tool in ["gs", "pdftocairo", "pdftoppm"] {
            if std::process::Command::new(tool)
                .arg("--version")
                .output()
                .is_err()
            {
                return None;
            }
        }
        let dir = temp_dir()?;
        let postscript = dir.join("fixture.ps");
        let pdf = dir.join("fixture.pdf");
        let source = r#"%!PS
/Helvetica findfont 14 scalefont setfont
1 1 30 {
  /p exch def
  72 740 moveto (MTH0101 - Calcul differentiel - page ) show p 3 string cvs show
  72 710 moveto (Exercices et exemples du chapitre 3 : limites et derivation.) show
  72 680 moveto (On calcule le taux de variation et la pente de la tangente.) show
  p 26 eq { 72 650 moveto (3.3 Taux de variation instantane) show } if
  showpage
} for
"#;
        let built = std::fs::write(&postscript, source).is_ok()
            && std::process::Command::new("gs")
                .args(["-q", "-dNOPAUSE", "-dBATCH", "-sDEVICE=pdfwrite"])
                .arg(format!("-sOutputFile={}", pdf.display()))
                .arg(&postscript)
                .status()
                .map(|status| status.success())
                .unwrap_or(false);
        let bytes = built.then(|| std::fs::read(&pdf).ok()).flatten();
        let _ = std::fs::remove_dir_all(&dir);
        bytes
    }

    #[test]
    fn shared_pdf_serves_sliced_windows_and_page_images_over_http() {
        let Some(pdf) = fixture_pdf() else {
            return; // Poppler/Ghostscript not installed on this machine.
        };
        let _guard = LINK_LOCK.lock().unwrap();
        let listener = crate::http::bind(0).unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            let _ = crate::http::serve_controlled(
                listener,
                Arc::new(std::sync::atomic::AtomicBool::new(true)),
            );
        });
        set_port(port);
        let id = publish("chapter 3.pdf", "application/pdf", &pdf, "text", 30).unwrap();

        let mut window = Vec::new();
        ureq::get(&format!("http://127.0.0.1:{port}/s/{id}/p/26-30.pdf"))
            .call()
            .unwrap()
            .into_reader()
            .read_to_end(&mut window)
            .unwrap();
        assert!(window.starts_with(b"%PDF"), "not a PDF");
        assert_eq!(crate::moodle::pdf_page_count(&window), Some(5));

        let mut image = Vec::new();
        ureq::get(&format!("http://127.0.0.1:{port}/s/{id}/p/26.jpg"))
            .call()
            .unwrap()
            .into_reader()
            .read_to_end(&mut image)
            .unwrap();
        assert!(image.starts_with(&[0xFF, 0xD8]), "not a JPEG");

        let html = ureq::get(&format!("http://127.0.0.1:{port}/s/{id}/read"))
            .call()
            .unwrap()
            .into_string()
            .unwrap();
        assert!(html.contains("Page 30"), "{html}");
        assert!(html.contains("src=\"p/30.jpg\""), "{html}");
    }

    #[test]
    fn published_files_get_browser_links() {
        let _guard = LINK_LOCK.lock().unwrap();
        set_port(65534); // only used to format links; nothing listens there
        let id = publish(
            "chapter 3.pdf",
            "application/pdf",
            b"%PDF-1.4 fake",
            "page one",
            12,
        )
        .unwrap();
        let links = links_for(&id).expect("links");
        assert_eq!(links.reader, format!("http://127.0.0.1:65534/s/{id}/read"));
        assert!(
            links.download.ends_with("/chapter_3.pdf"),
            "{}",
            links.download
        );
        assert_eq!(
            page_window_url(&id, 25, 48).unwrap(),
            format!("http://127.0.0.1:65534/s/{id}/p/25-48.pdf")
        );
    }
}
