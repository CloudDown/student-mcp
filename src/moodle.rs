use crate::config::MoodleConfig;
use serde_json::{json, Value};
use std::io::Read;
use std::time::Duration;
use url::Url;

const MOBILE_SERVICE: &str = "moodle_mobile_app";
const TIMEOUT: Duration = Duration::from_secs(30);
const MAX_FILE_BYTES: u64 = 12 * 1024 * 1024;
const MAX_TEXT_CHARS: usize = 80_000;
const MAX_PDF_PAGES: u32 = 16;
const MAX_PAGE_BYTES: usize = 1_500_000;
const SENSITIVE_KEYS: [&str; 6] = [
    "wstoken",
    "token",
    "userprivateaccesskey",
    "privateaccesskey",
    "accesskey",
    "password",
];

fn agent() -> ureq::Agent {
    // A 307/308 would replay the token or password onto the next host.
    // School WAFs (AWS/Cloudflare) challenge ureq's default User-Agent.
    ureq::AgentBuilder::new()
        .timeout(TIMEOUT)
        .redirects(0)
        .user_agent(crate::session::BROWSER_UA)
        .build()
}

fn token_login_agent() -> ureq::Agent {
    // School Moodle sites often 301/302 token.php (http→https, trailing slash).
    // Follow those for the login POST only; Web Service calls stay at redirects(0).
    ureq::AgentBuilder::new()
        .timeout(TIMEOUT)
        .user_agent(crate::session::BROWSER_UA)
        .build()
}

fn download_agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout(TIMEOUT)
        .redirects(0)
        .user_agent(crate::session::BROWSER_UA)
        .build()
}

/// Encode key/value pairs as application/x-www-form-urlencoded (Moodle expects that).
fn urlencoded(pairs: &[(String, String)]) -> String {
    let mut serializer = url::form_urlencoded::Serializer::new(String::new());
    for (key, value) in pairs {
        serializer.append_pair(key, value);
    }
    serializer.finish()
}

fn token_from_json(result: &Value) -> Result<Option<String>, String> {
    if let Some(token) = result
        .get("token")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|token| !token.is_empty())
    {
        return Ok(Some(token.to_string()));
    }
    if result.get("errorcode").and_then(Value::as_str) == Some("invalidlogin") {
        return Err("Moodle refused the login. Check your credentials; with SSO or two-factor authentication, ask your school's Moodle support for a token.".into());
    }
    Ok(None)
}

fn token_from_response(response: ureq::Response) -> Result<Option<String>, String> {
    match response.into_json() {
        Ok(result) => token_from_json(&result),
        // HTML/login pages are not a hard failure: try the GET fallback.
        Err(_) => Ok(None),
    }
}

/// Ask Moodle for a mobile-app token. The password is never stored.
/// POST keeps it out of the URL; some school stacks only mint a token on GET, so that is the fallback.
pub fn request_mobile_token(site: &str, username: &str, password: &str) -> Result<String, String> {
    let base = crate::config::normalize_moodle_url(site)?;
    if username.trim().is_empty() || password.is_empty() {
        return Err("Enter your Moodle username and password.".into());
    }
    let form = urlencoded(&[
        ("username".into(), username.trim().to_string()),
        ("password".into(), password.to_string()),
        ("service".into(), MOBILE_SERVICE.to_string()),
    ]);
    match token_login_agent()
        .post(&format!("{base}/login/token.php"))
        .set("Content-Type", "application/x-www-form-urlencoded")
        .send_string(&form)
    {
        Ok(response) => {
            if let Some(token) = token_from_response(response)? {
                return Ok(token);
            }
        }
        Err(ureq::Error::Status(_, response)) => {
            if let Some(token) = token_from_response(response)? {
                return Ok(token);
            }
        }
        Err(_) => {
            return Err(
                "Cannot reach Moodle. Check the site address and your Internet connection.".into(),
            );
        }
    }
    // Some school stacks only mint a token on GET. Do not follow redirects:
    // the password is in the query string.
    let response = agent()
        .get(&format!("{base}/login/token.php?{form}"))
        .call()
        .map_err(error_for_transport)?;
    if (300..400).contains(&response.status()) {
        return Err(
            "Moodle redirected the login request. Check the site address.".into(),
        );
    }
    token_from_response(response)?.ok_or_else(|| {
        "Moodle did not provide a mobile token. Check mobile service access with your school, or use the existing-token option.".into()
    })
}

/// Blocking Moodle Web Services client (one instance per request is fine).
pub struct MoodleClient {
    base_url: String,
    token: String,
    agent: ureq::Agent,
}

impl MoodleClient {
    pub fn new(config: &MoodleConfig) -> Self {
        MoodleClient {
            base_url: config.base_url.clone(),
            token: config.token.clone(),
            agent: agent(),
        }
    }

    /// One Web Service call; arrays become courseids[0]= form entries.
    pub fn call(&self, function: &str, args: &Value) -> Result<Value, String> {
        if !valid_function_name(function) {
            return Err("Invalid Moodle function name.".into());
        }
        validate_params(args)?;
        let mut form: Vec<(String, String)> = vec![
            ("wstoken".into(), self.token.clone()),
            ("wsfunction".into(), function.to_string()),
            ("moodlewsrestformat".into(), "json".into()),
        ];
        append_params(args, &mut form, "");
        let response = self
            .agent
            .post(&format!("{}/webservice/rest/server.php", self.base_url))
            .set("Content-Type", "application/x-www-form-urlencoded")
            .send_string(&urlencoded(&form))
            .map_err(error_for_transport)?;
        if let Some(error) = status_error(response.status()) {
            return Err(error);
        }
        let value: Value = response
            .into_json()
            .map_err(|_| "Moodle returned a non-JSON response.".to_string())?;
        let value = scrub(value, &self.token);
        if value.get("exception").is_some() || value.get("errorcode").is_some() {
            let code = value
                .get("errorcode")
                .and_then(Value::as_str)
                .unwrap_or("unknown");
            let message = value
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("Request refused.");
            return Err(format!("Moodle error ({code}): {message}"));
        }
        Ok(value)
    }

    pub fn site_info(&self) -> Result<Value, String> {
        let info = self.call("core_webservice_get_site_info", &json!({}))?;
        if info.get("exception").is_some() || info.get("userid").and_then(Value::as_i64).is_none() {
            return Err("This token cannot access this Moodle site. Check its permissions and the site address.".into());
        }
        Ok(info)
    }

    /// Fetch a course file with the stored WS token. The token never appears in the result.
    pub fn download_file(&self, fileurl: &str) -> Result<Value, String> {
        let url = file_download_url(&self.base_url, &self.token, fileurl)?;
        let response = download_agent().get(&url).call().map_err(|error| {
            scrub_secret(error_for_file_transport(error), &self.token)
        })?;
        // redirects(0) returns the 3xx response instead of an error.
        if (300..400).contains(&response.status()) {
            return Err(
                "Moodle redirected this file (login required). Use a fileurl from moodle_course_content."
                    .into(),
            );
        }
        let mime = response.content_type().to_string();
        let filename = filename_from(response.header("Content-Disposition"), fileurl);
        let mut bytes = Vec::new();
        response
            .into_reader()
            .take(MAX_FILE_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| "Cannot read this Moodle file.".to_string())?;
        if bytes.len() as u64 > MAX_FILE_BYTES {
            return Err("This Moodle file is larger than 12 MB. Open it in Moodle instead.".into());
        }
        if mime.contains("json") {
            if let Ok(value) = serde_json::from_slice::<Value>(&bytes) {
                if value.get("exception").is_some() || value.get("errorcode").is_some() {
                    let code = value
                        .get("errorcode")
                        .and_then(Value::as_str)
                        .unwrap_or("unknown");
                    let message = value
                        .get("message")
                        .and_then(Value::as_str)
                        .unwrap_or("Download refused.");
                    return Err(scrub_secret(
                        format!("Moodle error ({code}): {message}"),
                        &self.token,
                    ));
                }
            }
        }
        if looks_like_html(&mime, &bytes) {
            return Err(
                "Moodle returned a login page instead of the file. Use a fileurl from moodle_course_content."
                    .into(),
            );
        }
        let text = extract_text(&mime, &bytes).map(|text| scrub_secret(text, &self.token));
        let is_pdf = mime.to_ascii_lowercase().contains("pdf") || bytes.starts_with(b"%PDF");
        let text_len = text.as_ref().map(|value| value.chars().count()).unwrap_or(0);
        let (pages, ocr) = if is_pdf {
            render_pdf_pages(&bytes, text_len < 800)
        } else {
            (Vec::new(), None)
        };
        let mut result = json!({
            "filename": filename,
            "mimeType": mime,
            "size": bytes.len(),
        });
        match text {
            Some(text) if !text.trim().is_empty() => {
                result["text"] = json!(text);
            }
            _ if is_pdf && pages.is_empty() => {
                result["note"] = json!(
                    "The PDF downloaded, but no readable text or page images could be produced. Install Poppler (pdftoppm) on this computer."
                );
            }
            _ if !is_pdf => {
                result["note"] = json!(
                    "Binary Moodle file downloaded. Text is inlined only for PDF and text documents."
                );
            }
            _ => {}
        }
        if let Some(ocr) = ocr {
            result["ocrText"] = json!(ocr);
        }
        if is_pdf {
            use base64::Engine;
            result["file"] = json!({
                "filename": filename,
                "mimeType": "application/pdf",
                "data": base64::engine::general_purpose::STANDARD.encode(&bytes),
            });
        }
        if !pages.is_empty() {
            result["pageCount"] = json!(pages.len());
            result["pages"] = json!(pages);
        }
        if is_pdf {
            result["note"] = json!(
                "The original PDF is attached. Read that file for the exercises. Page images are a fallback for scanned textbooks; extracted text may only list titles."
            );
        }
        Ok(result)
    }
}

/// MCP tool result: JSON summary, the original file, then rendered PDF pages.
pub(crate) fn mcp_content_for_file(mut data: Value) -> Vec<Value> {
    let file = data.get("file").cloned();
    let pages = data
        .get("pages")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if let Some(object) = data.as_object_mut() {
        object.remove("file");
        object.remove("pages");
    }
    let mut content = vec![json!({
        "type": "text",
        "text": serde_json::to_string_pretty(&data).unwrap_or_default()
    })];
    if let Some(file) = file {
        if let Some(blob) = file.get("data").and_then(Value::as_str) {
            let filename = file
                .get("filename")
                .and_then(Value::as_str)
                .unwrap_or("file.pdf");
            let mime = file
                .get("mimeType")
                .and_then(Value::as_str)
                .unwrap_or("application/pdf");
            let uri_name: String = filename
                .chars()
                .map(|ch| {
                    if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '-' | '_') {
                        ch
                    } else {
                        '_'
                    }
                })
                .collect();
            content.push(json!({
                "type": "resource",
                "resource": {
                    "uri": format!("moodle://file/{uri_name}"),
                    "mimeType": mime,
                    "blob": blob
                }
            }));
        }
    }
    for page in pages {
        let Some(image) = page.get("data").and_then(Value::as_str) else {
            continue;
        };
        let mime = page
            .get("mimeType")
            .and_then(Value::as_str)
            .unwrap_or("image/jpeg");
        content.push(json!({
            "type": "image",
            "data": image,
            "mimeType": mime
        }));
    }
    content
}

/// Turn a Moodle fileurl into /webservice/pluginfile.php?...&token= using our stored token.
pub(crate) fn file_download_url(
    base_url: &str,
    token: &str,
    fileurl: &str,
) -> Result<String, String> {
    let fileurl = fileurl.trim();
    if fileurl.is_empty() {
        return Err("Provide fileurl from moodle_course_content.".into());
    }
    let parsed =
        Url::parse(fileurl).map_err(|_| "The file URL is not valid.".to_string())?;
    let base = Url::parse(base_url).map_err(|_| "The Moodle site address is not valid.".to_string())?;
    if parsed.scheme() != base.scheme()
        || parsed.host_str() != base.host_str()
        || parsed.port_or_known_default() != base.port_or_known_default()
    {
        return Err("That file URL is not on the connected Moodle site.".into());
    }
    let path = parsed.path();
    let rest = if let Some(index) = path.find("/webservice/pluginfile.php") {
        &path[index + "/webservice/pluginfile.php".len()..]
    } else if let Some(index) = path.find("/tokenpluginfile.php") {
        strip_tokenplugin_key(&path[index + "/tokenpluginfile.php".len()..])
    } else if let Some(index) = path.find("/pluginfile.php") {
        &path[index + "/pluginfile.php".len()..]
    } else {
        return Err(
            "Use a Moodle fileurl from moodle_course_content (pluginfile.php).".into(),
        );
    };
    if rest.contains("..") {
        return Err("The file URL is not valid.".into());
    }
    let mut download = Url::parse(&format!(
        "{}/webservice/pluginfile.php{rest}",
        base_url.trim_end_matches('/')
    ))
    .map_err(|_| "The file URL is not valid.".to_string())?;
    for (key, value) in parsed.query_pairs() {
        if key.eq_ignore_ascii_case("token") || key.eq_ignore_ascii_case("wstoken") {
            continue;
        }
        download.query_pairs_mut().append_pair(&key, &value);
    }
    download.query_pairs_mut().append_pair("token", token);
    Ok(download.into())
}

fn strip_tokenplugin_key(path: &str) -> &str {
    let trimmed = path.strip_prefix('/').unwrap_or(path);
    let Some((first, rest)) = trimmed.split_once('/') else {
        return path;
    };
    if first.len() >= 32 && first.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        let slash_before_rest = path.len() - rest.len() - 1;
        return &path[slash_before_rest..];
    }
    path
}

fn looks_like_html(mime: &str, bytes: &[u8]) -> bool {
    if mime.to_ascii_lowercase().contains("html") {
        return true;
    }
    let start = bytes
        .iter()
        .position(|byte| !byte.is_ascii_whitespace())
        .unwrap_or(0);
    let head = String::from_utf8_lossy(&bytes[start..bytes.len().min(start + 32)]).to_ascii_lowercase();
    head.starts_with("<!doctype html") || head.starts_with("<html")
}

fn filename_from(disposition: Option<&str>, fileurl: &str) -> String {
    if let Some(header) = disposition {
        if let Some(value) = header.split("filename=").nth(1) {
            let value = value
                .trim()
                .trim_start_matches('"')
                .trim_end_matches('"')
                .split(';')
                .next()
                .unwrap_or("")
                .trim();
            if !value.is_empty() {
                return value.to_string();
            }
        }
    }
    Url::parse(fileurl)
        .ok()
        .and_then(|parsed| {
            parsed
                .path_segments()
                .and_then(|mut segments| segments.next_back().map(str::to_string))
        })
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "file".into())
}

fn extract_text(mime: &str, bytes: &[u8]) -> Option<String> {
    let mime = mime.to_ascii_lowercase();
    if mime.starts_with("text/") || mime.contains("json") || mime.contains("xml") {
        return usable_text(truncate_text(&String::from_utf8_lossy(bytes)));
    }
    if mime.contains("pdf") || bytes.starts_with(b"%PDF") {
        if let Some(text) = pdftotext_cli(bytes).and_then(usable_text) {
            return Some(truncate_text(&text));
        }
        return extract_pdf_text(bytes).and_then(usable_text);
    }
    None
}

fn temp_job_dir() -> Option<std::path::PathBuf> {
    let mut id = [0u8; 8];
    getrandom::getrandom(&mut id).ok()?;
    let root = std::env::temp_dir().join("student-mcp");
    create_private_dir(&root)?;
    let dir = root.join(format!("{:x}", u64::from_le_bytes(id)));
    create_private_dir(&dir)?;
    Some(dir)
}

/// Course files are written here for Poppler. The directory must not be world-readable.
fn create_private_dir(path: &std::path::Path) -> Option<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
        if !path.is_dir() {
            let mut builder = std::fs::DirBuilder::new();
            builder.mode(0o700);
            if builder.create(path).is_err() && !path.is_dir() {
                return None;
            }
        }
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700)).ok()?;
        return Some(());
    }
    #[cfg(not(unix))]
    {
        std::fs::create_dir_all(path).ok()?;
        Some(())
    }
}

/// Poppler's pdftotext understands CID fonts / ToUnicode; the naive fallback does not.
fn pdftotext_cli(bytes: &[u8]) -> Option<String> {
    let dir = temp_job_dir()?;
    let path = dir.join("file.pdf");
    let written = std::fs::write(&path, bytes).is_ok();
    let output = if written {
        std::process::Command::new("pdftotext")
            .args(["-layout", "-enc", "UTF-8"])
            .arg(&path)
            .arg("-")
            .output()
            .ok()
    } else {
        None
    };
    let _ = std::fs::remove_dir_all(&dir);
    let output = output?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout).into_owned();
    if text.trim().is_empty() {
        None
    } else {
        Some(text)
    }
}

fn render_pdf_pages(bytes: &[u8], ocr: bool) -> (Vec<Value>, Option<String>) {
    let Some(dir) = temp_job_dir() else {
        return (Vec::new(), None);
    };
    let pdf_path = dir.join("file.pdf");
    let prefix = dir.join("page");
    if std::fs::write(&pdf_path, bytes).is_err()
        || !std::process::Command::new("pdftoppm")
            .args([
                "-jpeg",
                "-jpegopt",
                "quality=80",
                "-r",
                "144",
                "-scale-to",
                "1800",
                "-f",
                "1",
                "-l",
            ])
            .arg(MAX_PDF_PAGES.to_string())
            .arg(&pdf_path)
            .arg(&prefix)
            .status()
            .map(|status| status.success())
            .unwrap_or(false)
    {
        let _ = std::fs::remove_dir_all(&dir);
        return (Vec::new(), None);
    }
    let mut files: Vec<_> = std::fs::read_dir(&dir)
        .ok()
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .and_then(|ext| ext.to_str())
                .is_some_and(|ext| ext.eq_ignore_ascii_case("jpg") || ext.eq_ignore_ascii_case("jpeg"))
        })
        .collect();
    files.sort();
    use base64::Engine;
    let engine = base64::engine::general_purpose::STANDARD;
    let mut pages = Vec::new();
    let mut ocr_parts = Vec::new();
    for path in files {
        let Ok(image) = std::fs::read(&path) else {
            continue;
        };
        if image.is_empty() || image.len() > MAX_PAGE_BYTES {
            continue;
        }
        if ocr && ocr_parts.len() < 8 {
            if let Some(text) = ocr_image(&path) {
                ocr_parts.push(text);
            }
        }
        pages.push(json!({
            "mimeType": "image/jpeg",
            "data": engine.encode(image),
        }));
    }
    let _ = std::fs::remove_dir_all(&dir);
    let ocr_text = if ocr_parts.is_empty() {
        None
    } else {
        usable_text(truncate_text(&ocr_parts.join("\n\n")))
    };
    (pages, ocr_text)
}

fn ocr_image(path: &std::path::Path) -> Option<String> {
    let output = std::process::Command::new("tesseract")
        .arg(path)
        .arg("stdout")
        .args(["--psm", "6"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout).into_owned();
    if text.trim().is_empty() {
        None
    } else {
        Some(text)
    }
}

fn usable_text(text: String) -> Option<String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }
    let total = trimmed.chars().count().max(1);
    let junk = trimmed
        .chars()
        .filter(|ch| matches!(ch, '\0'..='\u{08}' | '\u{0B}' | '\u{0C}' | '\u{0E}'..='\u{1F}' | '\u{7F}'))
        .count();
    let letters = trimmed.chars().filter(|ch| ch.is_alphabetic()).count();
    if junk * 10 > total {
        return None;
    }
    if total > 80 && letters * 20 < total {
        return None;
    }
    Some(text)
}

fn extract_pdf_text(bytes: &[u8]) -> Option<String> {
    if !bytes.starts_with(b"%PDF") {
        return None;
    }
    let mut texts = Vec::new();
    let mut cursor = 0;
    while let Some(offset) = find_bytes(&bytes[cursor..], b"stream") {
        let keyword = cursor + offset;
        let dict_start = bytes[..keyword]
            .iter()
            .rposition(|&byte| byte == b'<')
            .unwrap_or(keyword);
        let dict = &bytes[dict_start..keyword];
        if contains_bytes(dict, b"/FontFile")
            || contains_bytes(dict, b"/Type1C")
            || contains_bytes(dict, b"/CIDFontType0C")
            || contains_bytes(dict, b"/ToUnicode")
        {
            cursor = keyword + b"stream".len();
            continue;
        }
        let compressed = contains_bytes(dict, b"/FlateDecode");
        let mut data_start = keyword + b"stream".len();
        if bytes.get(data_start) == Some(&b'\r') {
            data_start += 1;
        }
        if bytes.get(data_start) == Some(&b'\n') {
            data_start += 1;
        }
        let Some(end) = find_bytes(&bytes[data_start..], b"endstream") else {
            break;
        };
        let mut data = &bytes[data_start..data_start + end];
        if let Some(stripped) = data.strip_suffix(b"\r\n") {
            data = stripped;
        } else if let Some(stripped) = data.strip_suffix(b"\n").or_else(|| data.strip_suffix(b"\r"))
        {
            data = stripped;
        }
        let decoded = if compressed {
            inflate(data)
        } else {
            data.to_vec()
        };
        if contains_bytes(&decoded, b"BT") || contains_bytes(&decoded, b"Tj") {
            collect_pdf_strings(&decoded, &mut texts);
        }
        cursor = data_start + end + b"endstream".len();
    }
    let text = texts.join("\n");
    if text.trim().is_empty() {
        None
    } else {
        Some(truncate_text(&text))
    }
}

fn inflate(data: &[u8]) -> Vec<u8> {
    use flate2::read::ZlibDecoder;
    let decoder = ZlibDecoder::new(data);
    let mut out = Vec::new();
    if decoder.take(MAX_FILE_BYTES).read_to_end(&mut out).is_err() {
        out.clear();
    }
    out
}

fn collect_pdf_strings(data: &[u8], texts: &mut Vec<String>) {
    let mut i = 0;
    while i < data.len() {
        if data[i] == b'(' {
            let (string, next) = read_pdf_literal(data, i + 1);
            if string.chars().any(|ch| ch.is_alphanumeric()) {
                texts.push(string);
            }
            i = next;
            continue;
        }
        i += 1;
    }
}

fn read_pdf_literal(data: &[u8], mut i: usize) -> (String, usize) {
    let mut out = Vec::new();
    let mut depth = 1;
    while i < data.len() {
        match data[i] {
            b'\\' if i + 1 < data.len() => {
                i += 1;
                out.push(match data[i] {
                    b'n' => b'\n',
                    b'r' => b'\r',
                    b't' => b'\t',
                    other => other,
                });
            }
            b'(' => {
                depth += 1;
                out.push(b'(');
            }
            b')' => {
                depth -= 1;
                if depth == 0 {
                    i += 1;
                    break;
                }
                out.push(b')');
            }
            byte => out.push(byte),
        }
        i += 1;
    }
    (String::from_utf8_lossy(&out).into_owned(), i)
}

fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn contains_bytes(haystack: &[u8], needle: &[u8]) -> bool {
    find_bytes(haystack, needle).is_some()
}

fn truncate_text(text: &str) -> String {
    let mut text = text.replace('\0', "");
    if text.chars().count() > MAX_TEXT_CHARS {
        text = text.chars().take(MAX_TEXT_CHARS).collect();
        text.push_str("\n…");
    }
    text
}

fn scrub_secret(text: String, token: &str) -> String {
    if token.is_empty() {
        text
    } else {
        text.replace(token, "[redacted]")
    }
}

fn error_for_file_transport(error: ureq::Error) -> String {
    match error {
        ureq::Error::Status(401 | 403, _) => {
            "This Moodle token cannot download that file. Check the course access.".into()
        }
        ureq::Error::Status(301 | 302 | 303 | 307 | 308, _) => {
            "Moodle redirected this file (login required). Use a fileurl from moodle_course_content."
                .into()
        }
        other => error_for_transport(other),
    }
}

/// Keep caller arguments separate from the authenticated REST envelope. PHP
/// normalizes form keys, so reject bracket/space/dot aliases as well as duplicates;
/// callers must express arrays and objects as structured JSON instead.
fn validate_params(args: &Value) -> Result<(), String> {
    let object = args
        .as_object()
        .ok_or("Moodle arguments must be an object.")?;
    for key in object.keys() {
        if matches!(
            key.as_str(),
            "wstoken" | "wsfunction" | "moodlewsrestformat" | "wsusername" | "wspassword"
        ) {
            return Err(format!("Reserved Moodle parameter: {key}."));
        }
    }
    fn validate_keys(value: &Value) -> Result<(), String> {
        match value {
            Value::Object(map) => {
                for (key, child) in map {
                    if key.is_empty()
                        || !key.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_')
                    {
                        return Err("Invalid Moodle parameter name; use JSON objects and arrays for nested parameters.".into());
                    }
                    validate_keys(child)?;
                }
            }
            Value::Array(items) => {
                for child in items {
                    validate_keys(child)?;
                }
            }
            _ => {}
        }
        Ok(())
    }
    validate_keys(args)
}

fn valid_function_name(name: &str) -> bool {
    let mut bytes = name.bytes();
    match bytes.next() {
        Some(byte) if byte.is_ascii_alphabetic() => {}
        _ => return false,
    }
    name.len() <= 200 && bytes.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
}

fn status_error(status: u16) -> Option<String> {
    if (300..400).contains(&status) {
        Some(
            "Moodle redirected the request. Use the site address that serves the Web Service directly."
                .into(),
        )
    } else if !(200..300).contains(&status) {
        Some(format!("Moodle returned HTTP error {status}."))
    } else {
        None
    }
}

fn error_for_transport(error: ureq::Error) -> String {
    match error {
        ureq::Error::Status(code, _) if (300..400).contains(&code) => {
            "Moodle redirected the request. Use the site address that serves the Web Service directly.".into()
        }
        ureq::Error::Status(code, _) => format!("Moodle returned HTTP error {code}."),
        _ => "Cannot reach Moodle. Check the site address and your Internet connection.".into(),
    }
}

/// Moodle REST expects form fields such as courseids[0] and options[name].
pub fn append_params(value: &Value, form: &mut Vec<(String, String)>, key: &str) {
    match value {
        Value::Null => {}
        Value::Array(items) => {
            for (index, item) in items.iter().enumerate() {
                append_params(item, form, &format!("{key}[{index}]"));
            }
        }
        Value::Object(map) => {
            for (child, item) in map {
                let child_key = if key.is_empty() {
                    child.clone()
                } else {
                    format!("{key}[{child}]")
                };
                append_params(item, form, &child_key);
            }
        }
        other => {
            let text = match other {
                Value::Bool(true) => "1".to_string(),
                Value::Bool(false) => "0".to_string(),
                Value::Number(number) => number.to_string(),
                Value::String(text) => text.clone(),
                _ => return,
            };
            let key = if key.is_empty() {
                "unnamed".to_string()
            } else {
                key.to_string()
            };
            form.push((key, text));
        }
    }
}

/// Remove credential-like fields and literal token occurrences from results.
pub fn scrub(value: Value, token: &str) -> Value {
    match value {
        Value::String(text) => Value::String(text.replace(token, "[redacted]")),
        Value::Array(items) => {
            Value::Array(items.into_iter().map(|item| scrub(item, token)).collect())
        }
        Value::Object(map) => Value::Object(
            map.into_iter()
                .map(|(key, item)| {
                    let cleaned = if SENSITIVE_KEYS
                        .iter()
                        .any(|sensitive| key.eq_ignore_ascii_case(sensitive))
                    {
                        Value::String("[redacted]".into())
                    } else {
                        scrub(item, token)
                    };
                    (key, cleaned)
                })
                .collect(),
        ),
        other => other,
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) fn server(responses: Vec<Value>) -> (String, std::thread::JoinHandle<Vec<String>>) {
        raw_server(
            responses
                .into_iter()
                .map(|response| {
                    let body = response.to_string().into_bytes();
                    (200, "application/json".into(), Vec::new(), body)
                })
                .collect(),
        )
    }

    pub(crate) fn raw_server(
        responses: Vec<(u16, String, Vec<(String, String)>, Vec<u8>)>,
    ) -> (String, std::thread::JoinHandle<Vec<String>>) {
        use std::io::{BufRead, BufReader, Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let base_url = format!("http://{}/moodle", listener.local_addr().unwrap());
        let worker = std::thread::spawn(move || {
            let mut requests = Vec::new();
            for (status, content_type, extra_headers, body) in responses {
                let (mut stream, _) = listener.accept().unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut request = String::new();
                let mut length = 0;
                loop {
                    let mut line = String::new();
                    assert_ne!(reader.read_line(&mut line).unwrap(), 0);
                    request.push_str(&line);
                    if line == "\r\n" {
                        break;
                    }
                    if let Some((key, value)) = line.split_once(':') {
                        if key.eq_ignore_ascii_case("Content-Length") {
                            length = value.trim().parse().unwrap();
                        }
                    }
                }
                let mut incoming = vec![0; length];
                reader.read_exact(&mut incoming).unwrap();
                request.push_str(std::str::from_utf8(&incoming).unwrap());
                requests.push(request);
                let mut headers = format!(
                    "HTTP/1.1 {status} OK\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n",
                    body.len()
                );
                for (key, value) in extra_headers {
                    headers.push_str(&format!("{key}: {value}\r\n"));
                }
                headers.push_str("\r\n");
                stream.write_all(headers.as_bytes()).unwrap();
                stream.write_all(&body).unwrap();
            }
            requests
        });
        (base_url, worker)
    }

    #[test]
    fn reserved_parameters_and_php_aliases_are_rejected_before_network_access() {
        let client = MoodleClient::new(&MoodleConfig {
            base_url: "http://127.0.0.1:0".into(),
            token: "private-token".into(),
            allow_write: false,
        });
        for key in [
            "wsfunction",
            "wstoken",
            "moodlewsrestformat",
            "wsusername",
            "wspassword",
            "wsfunction[0]",
            " wsfunction",
            "wsfunction\0",
        ] {
            let args = json!({key: "replacement"});
            let error = client.call("core_course_get_contents", &args).unwrap_err();
            assert!(error.contains("parameter"), "{key}: {error}");
        }
        assert!(client
            .call("core_course_get_contents", &json!([]))
            .unwrap_err()
            .contains("object"));
    }

    #[test]
    fn moodle_api_errors_are_mcp_errors_and_secrets_are_scrubbed() {
        let (url, worker) = server(vec![
            json!({"exception":"moodle_exception","errorcode":"accessdenied","message":"Denied PRIVATE-TOKEN","debuginfo":"PRIVATE-TOKEN"}),
        ]);
        let settings = crate::config::Settings {
            moodle: Some(crate::config::MoodleSettings {
                url: Some(url),
                token: Some("PRIVATE-TOKEN".into()),
                allow_write: Some(false),
            }),
            ..Default::default()
        };
        let reply =
            crate::mcp::dispatch_tool(&settings, "moodle_course_content", &json!({"courseid": 1}));
        assert!(worker.join().unwrap()[0]
            .starts_with("POST /moodle/webservice/rest/server.php HTTP/1.1\r\n"));
        assert_eq!(reply["isError"], true);
        assert!(reply.to_string().contains("accessdenied"));
        assert!(!reply.to_string().contains("PRIVATE-TOKEN"));
    }

    #[test]
    fn mobile_token_prefers_a_post_body() {
        let (url, worker) = server(vec![json!({"token": "MOBILE-TOKEN"})]);
        let token = request_mobile_token(&url, "jane.doe", "p@ss+word").unwrap();
        assert_eq!(token, "MOBILE-TOKEN");
        let request = worker.join().unwrap().remove(0);
        assert!(
            request.starts_with("POST /moodle/login/token.php HTTP/1.1"),
            "{request}"
        );
        let (headers, body) = request.split_once("\r\n\r\n").unwrap();
        assert!(!headers.contains("password="), "{request}");
        assert!(body.contains("username=jane.doe"), "{request}");
        assert!(body.contains("password=p%40ss%2Bword"), "{request}");
        assert!(body.contains("service=moodle_mobile_app"), "{request}");
        assert!(
            headers.to_ascii_lowercase().contains("user-agent:")
                && headers.contains("Mozilla/5.0"),
            "school WAFs reject the default ureq agent: {request}"
        );
    }

    #[test]
    fn mobile_token_falls_back_to_get_when_post_returns_no_token() {
        let (url, worker) = server(vec![
            json!({"error": "use the query string"}),
            json!({"token": "MOBILE-TOKEN"}),
        ]);
        let token = request_mobile_token(&url, "jane.doe", "secret").unwrap();
        assert_eq!(token, "MOBILE-TOKEN");
        let requests = worker.join().unwrap();
        assert!(requests[0].starts_with("POST "), "{requests:?}");
        assert!(requests[1].starts_with("GET /moodle/login/token.php?"), "{requests:?}");
        assert!(requests[1].contains("password=secret"), "{requests:?}");
    }

    #[test]
    fn mobile_token_falls_back_when_post_returns_html() {
        let (url, worker) = raw_server(vec![
            (
                200,
                "text/html".into(),
                Vec::new(),
                b"<html>login</html>".to_vec(),
            ),
            (
                200,
                "application/json".into(),
                Vec::new(),
                br#"{"token":"MOBILE-TOKEN"}"#.to_vec(),
            ),
        ]);
        let token = request_mobile_token(&url, "jane.doe", "secret").unwrap();
        assert_eq!(token, "MOBILE-TOKEN");
        let requests = worker.join().unwrap();
        assert!(requests[0].starts_with("POST "), "{requests:?}");
        assert!(requests[1].starts_with("GET "), "{requests:?}");
    }

    #[test]
    fn mobile_token_html_does_not_use_the_old_toast() {
        let (url, worker) = raw_server(vec![
            (
                200,
                "text/html".into(),
                Vec::new(),
                b"<html>login</html>".to_vec(),
            ),
            (
                200,
                "text/html".into(),
                Vec::new(),
                b"<html>still html</html>".to_vec(),
            ),
        ]);
        let error = request_mobile_token(&url, "jane.doe", "secret").unwrap_err();
        let _ = worker.join();
        assert!(!error.contains("did not return a token"), "{error}");
        assert!(error.contains("did not provide a mobile token"), "{error}");
    }

    #[test]
    fn credentialed_calls_do_not_follow_redirects() {
        let (url, worker) = raw_server(vec![(
            307,
            "application/json".into(),
            vec![("Location".into(), "http://127.0.0.1:9/stolen".into())],
            b"{}".to_vec(),
        )]);
        let error = MoodleClient::new(&MoodleConfig {
            base_url: url,
            token: "PRIVATE-TOKEN".into(),
            allow_write: false,
        })
        .call("core_webservice_get_site_info", &json!({}))
        .unwrap_err();
        let requests = worker.join().unwrap();
        assert_eq!(requests.len(), 1);
        assert!(requests[0].contains("wstoken=PRIVATE-TOKEN"), "{requests:?}");
        assert!(error.contains("redirected"), "{error}");
    }

    #[test]
    fn function_names_are_identifiers() {
        let client = MoodleClient::new(&MoodleConfig {
            base_url: "http://127.0.0.1:0".into(),
            token: "private-token".into(),
            allow_write: false,
        });
        let error = client
            .call("core_course_get_contents&wsfunction=core_user_create_users", &json!({}))
            .unwrap_err();
        assert!(error.contains("function name"), "{error}");
    }

    #[cfg(unix)]
    #[test]
    fn pdf_workspace_is_owner_only() {
        use std::os::unix::fs::PermissionsExt;
        let dir = temp_job_dir().expect("temp dir");
        let mode = std::fs::metadata(&dir).unwrap().permissions().mode();
        assert_eq!(mode & 0o077, 0, "mode {mode:o}");
        let parent = std::fs::metadata(dir.parent().unwrap()).unwrap().permissions().mode();
        assert_eq!(parent & 0o077, 0, "parent mode {parent:o}");
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn mobile_token_maps_invalidlogin() {
        let (url, worker) = server(vec![json!({"errorcode":"invalidlogin","error":"nope"})]);
        let error = request_mobile_token(&url, "jane.doe", "secret").unwrap_err();
        let requests = worker.join().unwrap();
        assert_eq!(requests.len(), 1, "a refused login must not be retried");
        assert!(requests[0].starts_with("POST "), "{requests:?}");
        assert!(error.contains("Moodle refused the login"), "{error}");
    }

    #[test]
    fn params_flatten_like_moodle_expects() {
        let mut form = Vec::new();
        append_params(
            &json!({ "courseid": 42, "courseids": [7, 8] }),
            &mut form,
            "",
        );
        assert!(form.contains(&("courseid".to_string(), "42".to_string())));
        assert!(form.contains(&("courseids[0]".to_string(), "7".to_string())));
        assert!(form.contains(&("courseids[1]".to_string(), "8".to_string())));
    }

    #[test]
    fn scrub_hides_tokens_and_passwords() {
        let value = scrub(
            json!({ "wstoken": "SECRET", "other": "contains SECRET here", "ok": 1 }),
            "SECRET",
        );
        assert_eq!(value["wstoken"], json!("[redacted]"));
        assert_eq!(value["other"], json!("contains [redacted] here"));
        assert_eq!(value["ok"], json!(1));
    }

    #[test]
    fn file_url_rewrites_pluginfile_and_ignores_injected_tokens() {
        let url = file_download_url(
            "https://moodle.polymtl.ca",
            "SECRET",
            "https://moodle.polymtl.ca/pluginfile.php/9/mod_resource/content/0/TD5.pdf?forcedownload=1&token=injected",
        )
        .unwrap();
        assert!(
            url.starts_with("https://moodle.polymtl.ca/webservice/pluginfile.php/9/mod_resource/content/0/TD5.pdf?"),
            "{url}"
        );
        assert!(url.contains("token=SECRET"), "{url}");
        assert!(url.contains("forcedownload=1"), "{url}");
        assert!(!url.contains("injected"), "{url}");
        assert!(file_download_url(
            "https://moodle.polymtl.ca",
            "SECRET",
            "https://evil.example/pluginfile.php/9/mod_resource/content/0/TD5.pdf",
        )
        .is_err());
        assert!(file_download_url(
            "https://moodle.polymtl.ca",
            "SECRET",
            "https://moodle.polymtl.ca/login/index.php",
        )
        .is_err());
    }

    #[test]
    fn moodle_file_downloads_with_stored_token_and_returns_text() {
        let body = b"TD 5: exercise one";
        let (url, worker) = raw_server(vec![(
            200,
            "text/plain".into(),
            vec![("Content-Disposition".into(), r#"attachment; filename="TD5.txt""#.into())],
            body.to_vec(),
        )]);
        let settings = crate::config::Settings {
            moodle: Some(crate::config::MoodleSettings {
                url: Some(url.clone()),
                token: Some("PRIVATE-TOKEN".into()),
                allow_write: Some(false),
            }),
            ..Default::default()
        };
        let fileurl = format!("{url}/pluginfile.php/9/mod_resource/content/0/TD5.txt");
        let reply = crate::mcp::dispatch_tool(&settings, "moodle_file", &json!({ "fileurl": fileurl }));
        let request = worker.join().unwrap().remove(0);
        assert!(
            request.starts_with("GET /moodle/webservice/pluginfile.php/9/mod_resource/content/0/TD5.txt?"),
            "{request}"
        );
        assert!(request.contains("token=PRIVATE-TOKEN"), "{request}");
        assert_eq!(reply["isError"], false);
        let text = reply["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("TD 5: exercise one"), "{text}");
        assert!(text.contains("TD5.txt"), "{text}");
        assert!(!text.contains("PRIVATE-TOKEN"), "{text}");
        assert!(!reply.to_string().contains("PRIVATE-TOKEN"));
    }

    #[test]
    fn pdf_text_extracts_uncompressed_literals() {
        let pdf = b"%PDF-1.1\n1 0 obj<<>>endobj\n2 0 obj<</Length 44>>stream\nBT (Hello TD 5) Tj ET\nendstream\nendobj\n%%EOF\n";
        let text = extract_pdf_text(pdf).unwrap();
        assert!(text.contains("Hello TD 5"), "{text}");
    }

    #[test]
    fn usable_text_rejects_embedded_font_noise() {
        let noise = "001\u{0004}\u{0004}\\u0008\\LZdnx\u{0001}\u{0004}".repeat(20);
        assert!(usable_text(noise).is_none());
        assert!(usable_text("TD 5 — Calcul différentiel\nExercice 1".into()).is_some());
    }

    #[test]
    fn mcp_content_attaches_pdf_pages_as_images_without_inlining_them_in_text() {
        let data = json!({
            "filename": "TD5.pdf",
            "size": 10,
            "text": "Exercice 1 page 452",
            "file": { "filename": "TD5.pdf", "mimeType": "application/pdf", "data": "JVBERg==" },
            "pages": [{ "mimeType": "image/jpeg", "data": "QUJDRA==" }]
        });
        let content = mcp_content_for_file(data);
        assert_eq!(content[0]["type"], "text");
        let text = content[0]["text"].as_str().unwrap();
        assert!(text.contains("Exercice 1 page 452"), "{text}");
        assert!(!text.contains("QUJDRA=="), "{text}");
        assert!(!text.contains("JVBERg=="), "{text}");
        assert!(!text.contains("pages"), "{text}");
        assert_eq!(content[1]["type"], "resource");
        assert_eq!(content[1]["resource"]["mimeType"], "application/pdf");
        assert_eq!(content[1]["resource"]["blob"], "JVBERg==");
        assert_eq!(content[2]["type"], "image");
        assert_eq!(content[2]["data"], "QUJDRA==");
        assert_eq!(content[2]["mimeType"], "image/jpeg");
    }
}
