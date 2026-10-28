use crate::config::MoodleConfig;
use crate::pdf::{output as pdf_output, split_pages as split_pdf_pages, Workspace};
use serde_json::{json, Value};
use std::io::Read;
use std::sync::Arc;
use std::time::Duration;
use url::Url;
pub(crate) mod catalog;
mod download;

/// What to read from a Moodle file.
#[derive(Default, Clone)]
pub struct FileQuery {
    pub format: FileFormat,
    /// First page (1-based). Defaults to the page matching `find`, else 1.
    pub page_from: Option<u32>,
    /// Last page (inclusive). Defaults to a window of `MAX_PDF_PAGES`.
    pub page_to: Option<u32>,
    /// Section number or phrase to locate in the PDF text layer.
    pub find: Option<String>,
}

#[derive(Default, Clone, Copy, PartialEq, Eq)]
pub enum FileFormat {
    #[default]
    File,
    Text,
    Images,
}

const MOBILE_SERVICE: &str = "moodle_mobile_app";
const TIMEOUT: Duration = Duration::from_secs(30);
const MAX_FILE_BYTES: u64 = 12 * 1024 * 1024;
const MAX_TEXT_CHARS: usize = 20_000;
/// Pages rendered/extracted per moodle_file call. Long PDFs use page_from/page_to.
const MAX_PDF_PAGES: u32 = 24;
/// If the rest of a PDF is short, include it instead of returning a truncated window.
const PDF_TAIL_EXTEND: u32 = 8;
const PDF_HARD_MAX: u32 = MAX_PDF_PAGES + PDF_TAIL_EXTEND;
const MAX_PAGE_BYTES: usize = 1_500_000;
const MAX_IMAGES_TOTAL_BYTES: usize = 8_000_000;
/// Above this size, a PDF is exposed through links instead of a giant base64 blob.
const MAX_INLINE_BLOB_BYTES: usize = 6 * 1024 * 1024;
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
        return Err("Moodle redirected the login request. Check the site address.".into());
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
    /// For PDFs, `query` can target a page window or search the text layer (`find`).
    pub fn download_file(&self, fileurl: &str, query: &FileQuery) -> Result<Value, String> {
        let url = file_download_url(&self.base_url, &self.token, fileurl)?;
        let file = match download::get(&url) {
            Some(file) => file,
            None => {
                let response = download_agent()
                    .get(&url)
                    .call()
                    .map_err(|error| scrub_secret(error_for_file_transport(error), &self.token))?;
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
                    return Err(
                        "This Moodle file is larger than 12 MB. Open it in Moodle instead.".into(),
                    );
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
                let file = Arc::new(download::File {
                    filename,
                    mime,
                    bytes: Arc::new(bytes),
                });
                download::insert(&url, Arc::clone(&file));
                file
            }
        };
        Self::read_document(
            &file.filename,
            &file.mime,
            Arc::clone(&file.bytes),
            query,
            &self.token,
        )
    }

    /// Shared document delivery; platform-specific clients own authentication and download.
    pub(crate) fn read_document(
        filename: &str,
        mime: &str,
        document: Arc<Vec<u8>>,
        query: &FileQuery,
        secret: &str,
    ) -> Result<Value, String> {
        let bytes = document.as_slice();
        let is_pdf = mime.to_ascii_lowercase().contains("pdf") || bytes.starts_with(b"%PDF");
        if !is_pdf {
            if let Some(document) = crate::document::extract(filename, bytes)? {
                if query.format == FileFormat::Images {
                    return Err("Office image rendering is not supported. Request format=text, or export to PDF for diagrams and page images.".into());
                }
                return office_document_result(
                    filename,
                    mime,
                    bytes.len(),
                    document,
                    query,
                    secret,
                );
            }
            let text = extract_text(mime, bytes).map(|text| scrub_secret(text, secret));
            let mut result = json!({
                "filename": filename,
                "mimeType": mime,
                "size": bytes.len(),
            });
            match text {
                Some(text) if !text.trim().is_empty() => result["text"] = json!(text),
                _ => {
                    result["note"] = json!(
                        "Unsupported binary format. No content was extracted; export to PDF or a supported Office/OpenDocument format."
                    );
                }
            }
            return Ok(result);
        }

        // Send one representation. A PDF-capable LLM does not need the same
        // document duplicated as extracted text, OCR and dozens of JPEGs.
        if query.format == FileFormat::File {
            let mut result =
                json!({ "filename": filename, "mimeType": "application/pdf", "size": bytes.len() });
            if crate::share::enabled() {
                if let Some(id) = crate::share::publish_bytes(
                    filename,
                    "application/pdf",
                    Arc::clone(&document),
                    "",
                    0,
                ) {
                    if let Some(links) = crate::share::links_for(&id) {
                        result["links"] =
                            json!({ "download": links.download, "expiresInHours": 3 });
                    }
                }
            }
            if bytes.len() <= MAX_INLINE_BLOB_BYTES || result.get("links").is_none() {
                use base64::Engine;
                result["file"] = json!({ "filename": filename, "mimeType": "application/pdf",
                    "data": base64::engine::general_purpose::STANDARD.encode(bytes) });
            }
            result["note"] = json!("Original PDF. If your client cannot read the attached resource, open links.download or request format=text for selected pages. For scans or diagrams, request format=images with page_from/page_to. No local OCR is performed.");
            return Ok(result);
        }

        let total_pages = pdf_page_count(bytes).ok_or("Cannot inspect this PDF. Install Poppler, or request format=file to read the original without local processing.")?;
        // Extract the whole document once: the text layer doubles as a page index
        // for `find` and as the complete text exposed by the optional text link.
        let full_text = if query.find.is_some() {
            pdftotext_cli_range(bytes, 1, if total_pages > 0 { total_pages } else { 2000 })
                .ok_or("Cannot search this PDF's text layer. Request format=file instead.")?
        } else {
            String::new()
        };
        let page_texts = split_pdf_pages(&full_text, total_pages);
        let matches = query
            .find
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|needle| find_pdf_matches(&page_texts, needle))
            .unwrap_or_default();
        let matched_page = matches
            .iter()
            .map(|(page, _)| *page)
            .find(|page| *page > 2 && !looks_like_toc(*page, &page_texts))
            .or_else(|| matches.first().map(|(page, _)| *page));
        let start_page = query.page_from.or(matched_page).unwrap_or(1);
        let page_to = if query.format == FileFormat::Images {
            Some(
                query
                    .page_to
                    .unwrap_or(start_page.saturating_add(1))
                    .min(start_page.saturating_add(7)),
            )
        } else {
            query.page_to
        };
        let range = resolve_pdf_range(Some(start_page), page_to, total_pages)?;

        // Extract the window directly: some PDFs have no form-feed markers, and a
        // padded page index must never leak another page's text.
        let text = if query.format == FileFormat::Text {
            pdftotext_cli_range(bytes, range.from, range.to).ok_or(
                "PDF text extraction failed or timed out. Check Poppler, or request format=file.",
            )?
        } else {
            String::new()
        };
        let window_pages = (range.to.saturating_sub(range.from) + 1) as usize;
        let pages = if query.format == FileFormat::Images {
            render_pdf_pages(bytes, range.from, range.to)
        } else {
            Vec::new()
        };
        if query.format == FileFormat::Images && pages.is_empty() {
            return Err("PDF rendering failed, timed out or exceeded the image limit. Check Poppler, or request format=file.".into());
        }
        let truncated = total_pages > 0 && range.to < total_pages;
        let next_page_from = if truncated { Some(range.to + 1) } else { None };
        let (text, text_truncated) = match usable_text(text) {
            Some(text) => {
                let over = text.chars().count() > MAX_TEXT_CHARS;
                let text = if over { truncate_text(&text) } else { text };
                (Some(scrub_secret(text, secret)), over)
            }
            None => (None, false),
        };

        // Capability links (public tunnel or localhost). Publishing is cheap and
        // lets browser AIs open/download the file instead of ignoring a blob.
        let shared = if crate::share::enabled() && query.find.is_some() {
            crate::share::publish_bytes(
                filename,
                "application/pdf",
                Arc::clone(&document),
                &scrub_secret(full_text, secret),
                total_pages,
            )
        } else {
            None
        };
        let links = shared
            .as_deref()
            .and_then(|id| share_links_json(id, range.from, range.to));

        let mut result = json!({
            "filename": filename,
            "mimeType": mime,
            "size": bytes.len(),
            "totalPages": if total_pages > 0 { json!(total_pages) } else { Value::Null },
            "pageFrom": range.from,
            "pageTo": range.to,
            "renderedPages": pages.len(),
            "truncated": truncated,
            "nextPageFrom": next_page_from,
            "textTruncated": text_truncated,
        });
        let outline = pdf_outline(&page_texts);
        if !outline.is_empty() {
            result["outline"] = json!(outline
                .iter()
                .map(|(page, title)| json!({ "page": page, "title": title }))
                .collect::<Vec<_>>());
        }
        if let Some(needle) = query
            .find
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            result["find"] = json!(needle);
            result["matchedPage"] = matched_page.map_or(Value::Null, |page| json!(page));
            if !matches.is_empty() {
                result["matches"] = json!(matches
                    .iter()
                    .map(|(page, snippet)| json!({ "page": page, "snippet": snippet }))
                    .collect::<Vec<_>>());
            }
        }
        match &text {
            Some(text) if !text.trim().is_empty() => result["text"] = json!(text),
            _ if pages.is_empty() => {
                result["note"] = json!(
                    "This PDF has no usable text layer. Request format=images for selected pages, or format=file for the original PDF."
                );
            }
            _ => {}
        }
        if text.is_some() {
            result["textSource"] = json!("pdf-text");
        }
        if let Some(links) = links {
            if let Some(url) = links.get("download").and_then(Value::as_str) {
                result["downloadUrl"] = json!(url);
            }
            result["links"] = links;
        }
        let rendered_pages = pages.len();
        if !pages.is_empty() {
            result["pageCount"] = json!(rendered_pages);
            result["pages"] = json!(pages);
        }
        let mut note =
            pdf_result_note(truncated, range.from, range.to, total_pages, next_page_from);
        if query.format == FileFormat::Text && text.is_none() {
            note.push_str(" No usable text layer: request format=images for selected pages or format=file for the original PDF.");
        }
        if let Some(needle) = query
            .find
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            if let Some(page) = matched_page {
                note = format!("Found \"{needle}\" on page {page}. {note}");
            } else {
                note = format!(
                    "Could not find \"{needle}\" in this PDF's text layer (it may be scanned). {note}"
                );
            }
        }
        if text_truncated {
            note.push_str(
                " Text was capped at 20,000 characters; request a smaller page window for the omitted text.",
            );
        }
        if query.format == FileFormat::Images && rendered_pages < window_pages {
            note.push_str(
                " Some page images are unavailable or too large to inline; request a smaller window or the original file.",
            );
        }
        if result.get("links").is_some() {
            note.push_str(" links.download opens the original PDF.");
        }
        result["note"] = json!(note);
        Ok(result)
    }

    fn userid(&self) -> Result<i64, String> {
        self.site_info()?
            .get("userid")
            .and_then(Value::as_i64)
            .ok_or_else(|| "Moodle did not return the token's user ID.".into())
    }

    /// Enrolled courses as a compact list agents can scan. Hidden/past courses stay out unless requested.
    pub fn my_courses(&self, include_hidden: bool) -> Result<Value, String> {
        let courses = self.call(
            "core_enrol_get_users_courses",
            &json!({ "userid": self.userid()? }),
        )?;
        let now = unix_now();
        let mut items = Vec::new();
        for course in courses.as_array().cloned().unwrap_or_default() {
            let visible = course
                .get("visible")
                .and_then(|value| value.as_bool().or_else(|| value.as_i64().map(|n| n != 0)))
                .unwrap_or(true);
            if !include_hidden && !visible {
                continue;
            }
            let enddate = course.get("enddate").and_then(Value::as_i64).unwrap_or(0);
            let past = enddate > 0 && enddate + 30 * 86_400 < now;
            if !include_hidden && past {
                continue;
            }
            items.push(json!({
                "id": course.get("id"),
                "shortname": course.get("shortname"),
                "fullname": course.get("fullname").or_else(|| course.get("displayname")),
                "visible": visible,
                "startdate": course.get("startdate"),
                "enddate": course.get("enddate"),
                "lastaccess": course.get("lastaccess"),
                "isfavourite": course.get("isfavourite"),
                "progress": course.get("progress"),
            }));
        }
        items.sort_by(|left, right| {
            let left_fav = left
                .get("isfavourite")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let right_fav = right
                .get("isfavourite")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            right_fav
                .cmp(&left_fav)
                .then_with(|| {
                    let left_access = left.get("lastaccess").and_then(Value::as_i64).unwrap_or(0);
                    let right_access = right.get("lastaccess").and_then(Value::as_i64).unwrap_or(0);
                    right_access.cmp(&left_access)
                })
                .then_with(|| {
                    let left_name = left.get("shortname").and_then(Value::as_str).unwrap_or("");
                    let right_name = right.get("shortname").and_then(Value::as_str).unwrap_or("");
                    left_name.cmp(right_name)
                })
        });
        Ok(json!({
            "count": items.len(),
            "includeHidden": include_hidden,
            "courses": items
        }))
    }

    /// Upcoming deadlines from assignments, quizzes and the Moodle calendar/action feed.
    pub fn deadlines(&self, days: i64, courseids: Option<Vec<i64>>) -> Result<Value, String> {
        let days = days.clamp(1, 365);
        let now = unix_now();
        let until = now + days * 86_400;
        let filter = courseids_payload(courseids.as_deref());
        let assignments = self.call("mod_assign_get_assignments", &filter)?;
        let quizzes = self.call("mod_quiz_get_quizzes_by_courses", &filter)?;
        let calendar = self.call("core_calendar_get_calendar_upcoming_view", &json!({}))?;
        let actions = self.call(
            "core_calendar_get_action_events_by_timesort",
            &json!({
                "timesortfrom": now.saturating_sub(7 * 86_400),
                "timesortto": until,
                "limitnum": 50
            }),
        );

        let mut items = Vec::new();
        let mut seen = std::collections::HashSet::new();
        let course_filter =
            courseids.map(|ids| ids.into_iter().collect::<std::collections::HashSet<_>>());

        for course in assignments
            .get("courses")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
        {
            let course_id = course.get("id").and_then(Value::as_i64);
            if let (Some(filter), Some(id)) = (course_filter.as_ref(), course_id) {
                if !filter.contains(&id) {
                    continue;
                }
            }
            let shortname = course
                .get("shortname")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            for assignment in course
                .get("assignments")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default()
            {
                let due = assignment
                    .get("duedate")
                    .and_then(Value::as_i64)
                    .unwrap_or(0);
                if due <= 0 || due < now || due > until {
                    continue;
                }
                let id = assignment.get("id").and_then(Value::as_i64).unwrap_or(0);
                let key = format!("assign:{id}");
                if !seen.insert(key.clone()) {
                    continue;
                }
                items.push(json!({
                    "kind": "assignment",
                    "id": id,
                    "name": assignment.get("name"),
                    "courseid": course_id,
                    "course": shortname,
                    "time": due,
                    "opens": assignment.get("allowsubmissionsfromdate"),
                    "cmid": assignment.get("cmid"),
                }));
            }
        }

        for quiz in quizzes
            .get("quizzes")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
        {
            let course_id = quiz.get("course").and_then(Value::as_i64);
            if let (Some(filter), Some(id)) = (course_filter.as_ref(), course_id) {
                if !filter.contains(&id) {
                    continue;
                }
            }
            let close = quiz.get("timeclose").and_then(Value::as_i64).unwrap_or(0);
            let open = quiz.get("timeopen").and_then(Value::as_i64).unwrap_or(0);
            let time = if close > 0 {
                close
            } else if open >= now && open <= until {
                open
            } else {
                0
            };
            if time <= 0 || time < now || time > until {
                continue;
            }
            let id = quiz.get("id").and_then(Value::as_i64).unwrap_or(0);
            let key = format!("quiz:{id}");
            if !seen.insert(key) {
                continue;
            }
            items.push(json!({
                "kind": "quiz",
                "id": id,
                "name": quiz.get("name"),
                "courseid": course_id,
                "time": time,
                "opens": open,
                "closes": close,
                "timelimit": quiz.get("timelimit"),
                "cmid": quiz.get("coursemodule"),
            }));
        }

        for event in calendar
            .get("events")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .chain(
                actions
                    .ok()
                    .and_then(|value| value.get("events").and_then(Value::as_array).cloned())
                    .unwrap_or_default(),
            )
        {
            let time = event
                .get("timesort")
                .or_else(|| event.get("timestart"))
                .and_then(Value::as_i64)
                .unwrap_or(0);
            if time <= 0 || time < now || time > until {
                continue;
            }
            let course_id = event
                .get("course")
                .and_then(|course| course.get("id"))
                .or_else(|| event.get("courseid"))
                .and_then(Value::as_i64);
            if let (Some(filter), Some(id)) = (course_filter.as_ref(), course_id) {
                if !filter.contains(&id) {
                    continue;
                }
            }
            let event_id = event.get("id").and_then(Value::as_i64).unwrap_or(0);
            let module = event
                .get("modulename")
                .and_then(Value::as_str)
                .unwrap_or("event");
            let event_type = event.get("eventtype").and_then(Value::as_str).unwrap_or("");
            if event_type == "auth_outage" {
                continue;
            }
            let instance = event.get("instance").and_then(Value::as_i64).unwrap_or(0);
            if instance > 0
                && ((module == "assign" && seen.contains(&format!("assign:{instance}")))
                    || (module == "quiz" && seen.contains(&format!("quiz:{instance}"))))
            {
                continue;
            }
            let key = format!("event:{module}:{event_id}:{time}");
            if !seen.insert(key) {
                continue;
            }
            let course_name = event
                .get("course")
                .and_then(|course| {
                    course
                        .get("shortname")
                        .or_else(|| course.get("fullname"))
                        .and_then(Value::as_str)
                })
                .map(str::to_string);
            items.push(json!({
                "kind": event.get("eventtype").cloned().unwrap_or_else(|| json!("event")),
                "id": event_id,
                "name": plain_text(event.get("name").and_then(Value::as_str).unwrap_or("")),
                "courseid": course_id,
                "course": course_name,
                "time": time,
                "module": module,
                "action": event.get("action").and_then(|action| action.get("name")),
                "url": event.get("action").and_then(|action| action.get("url")).or_else(|| event.get("url")),
            }));
        }

        items.sort_by(|left, right| {
            let left_time = left.get("time").and_then(Value::as_i64).unwrap_or(0);
            let right_time = right.get("time").and_then(Value::as_i64).unwrap_or(0);
            left_time.cmp(&right_time)
        });
        Ok(json!({
            "from": now,
            "until": until,
            "days": days,
            "count": items.len(),
            "deadlines": items
        }))
    }

    /// Course announcement forums (type=news) with recent discussion posts.
    pub fn announcements(
        &self,
        courseids: Option<Vec<i64>>,
        limit: usize,
    ) -> Result<Value, String> {
        let limit = limit.clamp(1, 50);
        let ids = match courseids {
            Some(ids) if !ids.is_empty() => ids,
            _ => {
                let courses = self.my_courses(false)?;
                current_or_favourite_course_ids(&courses)
            }
        };
        if ids.is_empty() {
            return Ok(json!({ "count": 0, "announcements": [] }));
        }
        let forums = self.call(
            "mod_forum_get_forums_by_courses",
            &json!({ "courseids": ids }),
        )?;
        let mut items = Vec::new();
        for forum in forums.as_array().cloned().unwrap_or_default() {
            let forum_type = forum.get("type").and_then(Value::as_str).unwrap_or("");
            if forum_type != "news" {
                continue;
            }
            let forum_id = match forum.get("id").and_then(Value::as_i64) {
                Some(id) => id,
                None => continue,
            };
            let course_id = forum.get("course").and_then(Value::as_i64);
            let discussions = self.call(
                "mod_forum_get_forum_discussions",
                &json!({ "forumid": forum_id, "perpage": limit as i64 }),
            )?;
            for discussion in discussions
                .get("discussions")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default()
            {
                let message = plain_text(
                    discussion
                        .get("message")
                        .and_then(Value::as_str)
                        .unwrap_or(""),
                );
                let discussion_id = discussion
                    .get("discussion")
                    .and_then(Value::as_i64)
                    .or_else(|| discussion.get("id").and_then(Value::as_i64));
                items.push(json!({
                    "id": discussion.get("id"),
                    "discussionid": discussion_id,
                    "forumid": forum_id,
                    "forum": forum.get("name"),
                    "courseid": course_id,
                    "name": plain_text(discussion.get("name").and_then(Value::as_str).unwrap_or("")),
                    "author": discussion.get("userfullname"),
                    "time": discussion.get("timemodified").or_else(|| discussion.get("created")),
                    "replies": discussion.get("numreplies"),
                    "message": truncate(&message, 4_000),
                }));
            }
        }
        items.sort_by(|left, right| {
            let left_time = left.get("time").and_then(Value::as_i64).unwrap_or(0);
            let right_time = right.get("time").and_then(Value::as_i64).unwrap_or(0);
            right_time.cmp(&left_time)
        });
        if items.len() > limit {
            items.truncate(limit);
        }
        Ok(json!({
            "count": items.len(),
            "announcements": items
        }))
    }

    /// Search enrolled courses first (so TD5 matches "TD Chapitre 5"), then Moodle's global search.
    pub fn search(&self, query: &str, page: i64, courseid: Option<i64>) -> Result<Value, String> {
        let query = query.trim();
        if query.is_empty() {
            return Err("Provide query.".into());
        }
        let page = page.max(0);
        let courses = self.my_courses(false)?;
        let course_list = courses
            .get("courses")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let targets = resolve_search_courses(&course_list, query, courseid);
        let leftover = leftover_search_tokens(query, &targets);
        let mut results = Vec::new();
        if page == 0 {
            if leftover.is_empty() {
                results.extend(targets.iter().filter_map(course_search_hit));
            } else {
                for course in &targets {
                    let Some(id) = course.get("id").and_then(Value::as_i64) else {
                        continue;
                    };
                    let content = self.course_content(id, None)?;
                    results.extend(search_course_modules(course, &content, query));
                }
                if results.is_empty() {
                    results.extend(targets.iter().filter_map(course_search_hit));
                }
            }
        }
        if results.is_empty() {
            let mut payload = json!({ "query": query, "page": page });
            if !targets.is_empty() {
                payload["filters"] = json!({
                    "courseids": targets
                        .iter()
                        .filter_map(|course| course.get("id").and_then(Value::as_i64))
                        .collect::<Vec<_>>()
                });
            }
            let raw = self.call("core_search_get_results", &payload)?;
            results.extend(map_global_search_results(&raw));
        }
        Ok(json!({
            "query": query,
            "page": page,
            "courseids": targets
                .iter()
                .filter_map(|course| course.get("id").and_then(Value::as_i64))
                .collect::<Vec<_>>(),
            "total": results.len(),
            "results": results
        }))
    }

    /// Compact section tree: names, activities and fileurl values, without Moodle's raw blobs.
    pub fn course_content(&self, courseid: i64, query: Option<&str>) -> Result<Value, String> {
        let raw = self.call("core_course_get_contents", &json!({ "courseid": courseid }))?;
        Ok(compact_course_contents(courseid, &raw, query))
    }

    pub fn assignment_status(&self, assignid: i64) -> Result<Value, String> {
        let raw = self.call(
            "mod_assign_get_submission_status",
            &json!({ "assignid": assignid }),
        )?;
        let last = raw.get("lastattempt").cloned().unwrap_or(Value::Null);
        let submission = last.get("submission").cloned().unwrap_or(Value::Null);
        let feedback = raw.get("feedback").cloned().unwrap_or(Value::Null);
        Ok(json!({
            "assignid": assignid,
            "submissionStatus": submission.get("status"),
            "timeSubmitted": submission.get("timemodified"),
            "gradingStatus": last.get("gradingstatus"),
            "canEdit": last.get("canedit"),
            "canSubmit": last.get("cansubmit"),
            "attemptNumber": submission.get("attemptnumber"),
            "grade": feedback.get("grade").and_then(|grade| grade.get("grade")),
            "gradeDate": feedback.get("gradeddate"),
            "feedbackText": truncate(
                &plain_text(
                    feedback
                        .get("plugins")
                        .and_then(Value::as_array)
                        .and_then(|plugins| {
                            plugins.iter().find_map(|plugin| {
                                plugin
                                    .get("editorfields")
                                    .and_then(Value::as_array)
                                    .and_then(|fields| {
                                        fields.first()
                                            .and_then(|field| field.get("text"))
                                            .and_then(Value::as_str)
                                    })
                            })
                        })
                        .unwrap_or(""),
                ),
                2_000,
            ),
            "files": submission
                .get("plugins")
                .and_then(Value::as_array)
                .map(|plugins| {
                    plugins
                        .iter()
                        .flat_map(|plugin| {
                            plugin
                                .get("fileareas")
                                .and_then(Value::as_array)
                                .into_iter()
                                .flatten()
                                .flat_map(|area| {
                                    area.get("files")
                                        .and_then(Value::as_array)
                                        .cloned()
                                        .unwrap_or_default()
                                })
                                .map(|file| {
                                    json!({
                                        "filename": file.get("filename"),
                                        "filesize": file.get("filesize"),
                                        "fileurl": file.get("fileurl"),
                                        "mimetype": file.get("mimetype"),
                                    })
                                })
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default(),
        }))
    }

    pub fn quiz_status(&self, quizid: i64) -> Result<Value, String> {
        let attempts = self.call(
            "mod_quiz_get_user_attempts",
            &json!({ "quizid": quizid, "status": "all" }),
        )?;
        let best = self.call("mod_quiz_get_user_best_grade", &json!({ "quizid": quizid }))?;
        let access = self.call(
            "mod_quiz_get_quiz_access_information",
            &json!({ "quizid": quizid }),
        );
        let attempt_list = attempts
            .get("attempts")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .map(|attempt| {
                json!({
                    "id": attempt.get("id"),
                    "attempt": attempt.get("attempt"),
                    "state": attempt.get("state"),
                    "timestart": attempt.get("timestart"),
                    "timefinish": attempt.get("timefinish"),
                    "sumgrades": attempt.get("sumgrades"),
                })
            })
            .collect::<Vec<_>>();
        Ok(json!({
            "quizid": quizid,
            "hasGrade": best.get("hasgrade"),
            "bestGrade": best.get("grade"),
            "attempts": attempt_list,
            "access": access.ok().map(|value| json!({
                "canAttempt": value.get("canattempt"),
                "canPreview": value.get("canpreview"),
                "canReviewMyAttempts": value.get("canreviewmyattempts"),
                "preventReasons": value.get("accessrules"),
            })),
        }))
    }

    pub fn grades_for_course(&self, courseid: i64) -> Result<Value, String> {
        // Many schools reject this call without the student's userid.
        let raw = self.call(
            "gradereport_user_get_grade_items",
            &json!({ "courseid": courseid, "userid": self.userid()? }),
        )?;
        let grades = compact_grade_items(&raw);
        Ok(json!({
            "courseid": courseid,
            "count": grades.len(),
            "grades": grades
        }))
    }

    /// Recent assessment grades, not course totals. Never invent a publication date.
    pub fn recent_grades(
        &self,
        courseids: Option<Vec<i64>>,
        limit: usize,
    ) -> Result<Value, String> {
        let courses = self.my_courses(false)?;
        let ids = match courseids {
            Some(ids) => ids,
            None => courses["courses"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|c| c["id"].as_i64())
                .collect(),
        };
        if ids.len() > 30 {
            return Err("Select at most 30 courseids for recent grades.".into());
        }
        let userid = self.userid()?;
        let mut grades = Vec::new();
        let mut errors = Vec::new();
        let mut undated = 0;
        for id in ids {
            let raw = match self.call(
                "gradereport_user_get_grade_items",
                &json!({"courseid": id, "userid": userid}),
            ) {
                Ok(raw) => raw,
                Err(error) => {
                    errors.push(json!({"courseid": id, "error": error}));
                    continue;
                }
            };
            if let Some(warnings) = raw
                .get("warnings")
                .and_then(Value::as_array)
                .filter(|w| !w.is_empty())
            {
                errors.push(json!({"courseid": id, "warnings": warnings}));
            }
            for mut grade in compact_grade_items(&raw) {
                if grade["type"] == "course"
                    || grade["type"] == "category"
                    || grade["raw"].is_null()
                {
                    continue;
                }
                if grade["gradedAt"].as_i64().unwrap_or(0) <= 0 {
                    undated += 1;
                    continue;
                }
                grade["courseid"] = json!(id);
                grade["course"] = courses["courses"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .find(|c| c["id"].as_i64() == Some(id))
                    .map(|c| c["fullname"].clone())
                    .unwrap_or(Value::Null);
                grades.push(grade);
            }
        }
        grades.sort_by_key(|g| std::cmp::Reverse(g["gradedAt"].as_i64().unwrap_or(0)));
        let total = grades.len();
        grades.truncate(limit.clamp(1, 50));
        Ok(
            json!({"count": grades.len(), "total": total, "grades": grades,
            "undatedGradesOmitted": undated, "partial": !errors.is_empty(), "errors": errors,
            "note": "Sorted by Moodle's grading timestamp, not necessarily the date a grade became visible. Undated grades are omitted; use moodle_grades for them."}),
        )
    }

    pub fn course_updates(&self, courseid: i64, since: i64) -> Result<Value, String> {
        if since < 0 || since > unix_now() {
            return Err("since must be a past Unix timestamp in seconds.".into());
        }
        let raw = self.call(
            "core_course_get_updates_since",
            &json!({"courseid": courseid, "since": since}),
        )?;
        let content = self.course_content(courseid, None)?;
        let modules: Vec<&Value> = content["sections"]
            .as_array()
            .into_iter()
            .flatten()
            .flat_map(|s| s["modules"].as_array().into_iter().flatten())
            .collect();
        let updates: Vec<Value> = raw["instances"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|instance| {
                instance["updates"]
                    .as_array()
                    .is_some_and(|u| !u.is_empty())
            })
            .map(|instance| {
                let id = instance["id"].as_i64();
                let module = modules.iter().find(|m| m["id"].as_i64() == id);
                json!({"cmid": id, "name": module.map(|m| &m["name"]),
                "modname": module.map(|m| &m["modname"]), "url": module.map(|m| &m["url"]),
                "updates": instance.get("updates")})
            })
            .collect();
        Ok(
            json!({"courseid": courseid, "since": since, "count": updates.len(), "updates": updates,
            "warnings": raw.get("warnings"),
            "note": "Changes reported by Moodle's activity update API, not a full audit log. Some plugins or deleted activities may not be reported."}),
        )
    }

    pub fn grades_overview(&self) -> Result<Value, String> {
        let userid = self.userid()?;
        let raw = self.call(
            "gradereport_overview_get_course_grades",
            &json!({ "userid": userid }),
        )?;
        let courses = self.my_courses(false)?;
        let names: std::collections::HashMap<i64, String> = courses
            .get("courses")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|course| {
                let id = course.get("id").and_then(Value::as_i64)?;
                let name = course
                    .get("shortname")
                    .or_else(|| course.get("fullname"))
                    .and_then(Value::as_str)?
                    .to_string();
                Some((id, name))
            })
            .collect();
        let grades = raw
            .get("grades")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter_map(|grade| {
                let courseid = grade.get("courseid").and_then(Value::as_i64)?;
                if !names.contains_key(&courseid) {
                    return None;
                }
                let formatted = grade
                    .get("grade")
                    .and_then(Value::as_str)
                    .unwrap_or("-")
                    .trim();
                if formatted.is_empty() || formatted == "-" {
                    return None;
                }
                Some(json!({
                    "courseid": courseid,
                    "course": names.get(&courseid).cloned(),
                    "grade": formatted,
                    "rawgrade": grade.get("rawgrade"),
                }))
            })
            .collect::<Vec<_>>();
        Ok(json!({ "count": grades.len(), "grades": grades }))
    }

    pub fn forums(&self, courseids: Option<Vec<i64>>) -> Result<Value, String> {
        let ids = match courseids {
            Some(ids) if !ids.is_empty() => ids,
            _ => self
                .my_courses(false)?
                .get("courses")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|course| course.get("id").and_then(Value::as_i64))
                .collect(),
        };
        if ids.is_empty() {
            return Ok(json!({ "count": 0, "forums": [] }));
        }
        let forums = self.call(
            "mod_forum_get_forums_by_courses",
            &json!({ "courseids": ids }),
        )?;
        let items = forums
            .as_array()
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .map(|forum| {
                json!({
                    "id": forum.get("id"),
                    "courseid": forum.get("course"),
                    "name": forum.get("name"),
                    "type": forum.get("type"),
                    "intro": truncate(
                        &plain_text(forum.get("intro").and_then(Value::as_str).unwrap_or("")),
                        400,
                    ),
                    "numdiscussions": forum.get("numdiscussions"),
                    "cmid": forum.get("cmid"),
                })
            })
            .collect::<Vec<_>>();
        Ok(json!({ "count": items.len(), "forums": items }))
    }

    pub fn forum_discussions(&self, forumid: i64, limit: usize) -> Result<Value, String> {
        let limit = limit.clamp(1, 50) as i64;
        let raw = self.call(
            "mod_forum_get_forum_discussions",
            &json!({ "forumid": forumid, "perpage": limit }),
        )?;
        let discussions = raw
            .get("discussions")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .map(|discussion| {
                let discussion_id = discussion
                    .get("discussion")
                    .and_then(Value::as_i64)
                    .or_else(|| discussion.get("id").and_then(Value::as_i64));
                json!({
                    "id": discussion.get("id"),
                    "discussionid": discussion_id,
                    "name": plain_text(discussion.get("name").and_then(Value::as_str).unwrap_or("")),
                    "author": discussion.get("userfullname"),
                    "time": discussion.get("timemodified").or_else(|| discussion.get("created")),
                    "replies": discussion.get("numreplies"),
                    "message": truncate(
                        &plain_text(discussion.get("message").and_then(Value::as_str).unwrap_or("")),
                        2_000,
                    ),
                })
            })
            .collect::<Vec<_>>();
        Ok(json!({
            "forumid": forumid,
            "count": discussions.len(),
            "discussions": discussions
        }))
    }

    pub fn forum_posts(&self, discussionid: i64) -> Result<Value, String> {
        let raw = self.call(
            "mod_forum_get_discussion_posts",
            &json!({ "discussionid": discussionid }),
        )?;
        let posts = raw
            .get("posts")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .map(|post| {
                json!({
                    "id": post.get("id"),
                    "parent": post.get("parentid").or_else(|| post.get("parent")),
                    "author": post.get("author")
                        .and_then(|author| author.get("fullname"))
                        .or_else(|| post.get("userfullname")),
                    "time": post.get("timecreated").or_else(|| post.get("timemodified")),
                    "subject": plain_text(post.get("subject").and_then(Value::as_str).unwrap_or("")),
                    "message": truncate(
                        &plain_text(post.get("message").and_then(Value::as_str).unwrap_or("")),
                        6_000,
                    ),
                    "hasAttachments": post
                        .get("hasattachments")
                        .cloned()
                        .or_else(|| {
                            post.get("attachments").map(|value| {
                                json!(value.as_array().is_some_and(|items| !items.is_empty())
                                    || value.as_object().is_some_and(|object| !object.is_empty()))
                            })
                        }),
                })
            })
            .collect::<Vec<_>>();
        Ok(json!({
            "discussionid": discussionid,
            "forumid": raw.get("forumid"),
            "courseid": raw.get("courseid"),
            "count": posts.len(),
            "posts": posts
        }))
    }

    pub fn notifications(&self, limit: usize) -> Result<Value, String> {
        let limit = limit.clamp(1, 50) as i64;
        let userid = self.userid()?;
        let unread = self.call(
            "core_message_get_unread_notification_count",
            &json!({ "useridto": userid }),
        )?;
        let raw = self.call(
            "message_popup_get_popup_notifications",
            &json!({ "useridto": userid, "limit": limit }),
        )?;
        let notifications = raw
            .get("notifications")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .map(|item| {
                json!({
                    "id": item.get("id"),
                    "subject": plain_text(item.get("subject").and_then(Value::as_str).unwrap_or("")),
                    "text": truncate(
                        &plain_text(item.get("text").and_then(Value::as_str).unwrap_or("")),
                        1_500,
                    ),
                    "time": item.get("timecreated"),
                    "read": item.get("read"),
                    "url": item.get("contexturl"),
                })
            })
            .collect::<Vec<_>>();
        Ok(json!({
            "unread": unread,
            "count": notifications.len(),
            "notifications": notifications
        }))
    }

    pub fn messages(&self, limit: usize) -> Result<Value, String> {
        let limit = limit.clamp(1, 50) as i64;
        let userid = self.userid()?;
        let counts = self.call(
            "core_message_get_unread_conversation_counts",
            &json!({ "userid": userid }),
        )?;
        let raw = self.call(
            "core_message_get_conversations",
            &json!({ "userid": userid, "limitfrom": 0, "limitnum": limit }),
        )?;
        let conversations = raw
            .get("conversations")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .map(|conversation| {
                let members = conversation
                    .get("members")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default()
                    .into_iter()
                    .filter_map(|member| {
                        let id = member.get("id").and_then(Value::as_i64)?;
                        if id == userid {
                            return None;
                        }
                        Some(json!({
                            "id": id,
                            "fullname": member.get("fullname"),
                        }))
                    })
                    .collect::<Vec<_>>();
                let last = conversation.get("messages")
                    .and_then(Value::as_array)
                    .and_then(|messages| messages.first())
                    .cloned()
                    .unwrap_or(Value::Null);
                json!({
                    "id": conversation.get("id"),
                    "name": conversation.get("name"),
                    "type": conversation.get("type"),
                    "unread": conversation.get("unreadcount"),
                    "isfavourite": conversation.get("isfavourite"),
                    "members": members,
                    "lastMessage": plain_text(last.get("text").and_then(Value::as_str).unwrap_or("")),
                    "lastTime": last.get("timecreated"),
                })
            })
            .collect::<Vec<_>>();
        Ok(json!({
            "unreadCounts": counts,
            "count": conversations.len(),
            "conversations": conversations
        }))
    }

    pub fn conversation_messages(
        &self,
        conversationid: i64,
        limit: usize,
    ) -> Result<Value, String> {
        let limit = limit.clamp(1, 100) as i64;
        let raw = self.call(
            "core_message_get_conversation_messages",
            &json!({
                "currentuserid": self.userid()?,
                "convid": conversationid,
                "limitfrom": 0,
                "limitnum": limit
            }),
        )?;
        let messages = raw
            .get("messages")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .map(|message| {
                json!({
                    "id": message.get("id"),
                    "useridfrom": message.get("useridfrom"),
                    "text": truncate(
                        &plain_text(message.get("text").and_then(Value::as_str).unwrap_or("")),
                        4_000,
                    ),
                    "time": message.get("timecreated"),
                })
            })
            .collect::<Vec<_>>();
        Ok(json!({
            "conversationid": conversationid,
            "count": messages.len(),
            "messages": messages
        }))
    }

    pub fn completion(&self, courseid: i64) -> Result<Value, String> {
        let userid = self.userid()?;
        let activities = self.call(
            "core_completion_get_activities_completion_status",
            &json!({ "courseid": courseid, "userid": userid }),
        )?;
        let course = self.call(
            "core_completion_get_course_completion_status",
            &json!({ "courseid": courseid, "userid": userid }),
        );
        let statuses = activities
            .get("statuses")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .map(|status| {
                json!({
                    "cmid": status.get("cmid"),
                    "modname": status.get("modname"),
                    "instance": status.get("instance"),
                    "state": status.get("state"),
                    "timecompleted": status.get("timecompleted"),
                    "hascompletion": status.get("hascompletion"),
                    "isoverallcomplete": status.get("isoverallcomplete"),
                })
            })
            .collect::<Vec<_>>();
        Ok(json!({
            "courseid": courseid,
            "activities": statuses,
            "courseCompletion": course.ok(),
        }))
    }

    /// Pages, files, URLs, folders and books listed for a course (without full section tree).
    pub fn resources(&self, courseid: i64) -> Result<Value, String> {
        let ids = json!({ "courseids": [courseid] });
        let pages = self.call("mod_page_get_pages_by_courses", &ids)?;
        let files = self.call("mod_resource_get_resources_by_courses", &ids)?;
        let urls = self.call("mod_url_get_urls_by_courses", &ids)?;
        let folders = self.call("mod_folder_get_folders_by_courses", &ids)?;
        let books = self.call("mod_book_get_books_by_courses", &ids)?;

        let map_named = |raw: &Value, key: &str, kind: &str| {
            raw.get(key)
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .map(|item| {
                    json!({
                        "kind": kind,
                        "id": item.get("id"),
                        "cmid": item.get("coursemodule"),
                        "name": item.get("name"),
                        "intro": truncate(
                            &plain_text(item.get("intro").and_then(Value::as_str).unwrap_or("")),
                            300,
                        ),
                        "content": item.get("content").map(|value| {
                            json!(truncate(&plain_text(value.as_str().unwrap_or("")), 2_000))
                        }),
                        "externalurl": item.get("externalurl"),
                        "contentfiles": item.get("contentfiles"),
                    })
                })
                .collect::<Vec<_>>()
        };

        let mut items = Vec::new();
        items.extend(map_named(&pages, "pages", "page"));
        items.extend(map_named(&files, "resources", "file"));
        items.extend(map_named(&urls, "urls", "url"));
        items.extend(map_named(&folders, "folders", "folder"));
        items.extend(map_named(&books, "books", "book"));
        items.sort_by(|left, right| {
            let left_name = left.get("name").and_then(Value::as_str).unwrap_or("");
            let right_name = right.get("name").and_then(Value::as_str).unwrap_or("");
            left_name.cmp(right_name)
        });
        Ok(json!({
            "courseid": courseid,
            "count": items.len(),
            "resources": items
        }))
    }
}

fn office_document_result(
    filename: &str,
    mime: &str,
    size: usize,
    document: crate::document::Document,
    query: &FileQuery,
    secret: &str,
) -> Result<Value, String> {
    // A large sheet/slide must not bypass the token budget. Blocks are not physical pages.
    let mut blocks = Vec::new();
    for (index, part) in document.parts.iter().enumerate() {
        let clean = scrub_secret(part.clone(), secret);
        let chars: Vec<char> = clean.chars().collect();
        for chunk in chars.chunks(8_000) {
            blocks.push((index + 1, chunk.iter().collect::<String>()));
        }
    }
    let total = blocks.len() as u32;
    let needle = query.find.as_deref().map(str::to_lowercase);
    let matches: Vec<u32> = needle
        .as_ref()
        .map(|needle| {
            blocks
                .iter()
                .enumerate()
                .filter(|(_, (_, text))| text.to_lowercase().contains(needle))
                .map(|(i, _)| i as u32 + 1)
                .collect()
        })
        .unwrap_or_default();
    let from = query
        .page_from
        .or_else(|| matches.first().copied())
        .unwrap_or(1);
    if from == 0 || (total > 0 && from > total) {
        return Err("page_from is outside this document's text blocks.".into());
    }
    let to = query.page_to.unwrap_or(from.saturating_add(1));
    if to < from {
        return Err("page_to must be >= page_from.".into());
    }
    let to = to.min(from.saturating_add(1)).min(total);
    let selected: Vec<Value> = blocks
        .iter()
        .enumerate()
        .skip(from.saturating_sub(1) as usize)
        .take(to.saturating_sub(from).saturating_add(1) as usize)
        .map(|(i, (unit, text))| json!({"block": i + 1, "sourceUnit": unit, "text": text}))
        .collect();
    Ok(json!({
        "filename": filename, "mimeType": mime, "size": size,
        "sourceUnit": document.unit, "totalBlocks": total,
        "pageFrom": from, "pageTo": to,
        "blocks": selected, "matches": matches,
        "truncated": to < total,
        "nextPageFrom": if to < total { Some(to + 1) } else { None },
        "note": format!("{} page_from/page_to select text blocks (8000 characters), not physical pages; sourceUnit identifies the original slide/sheet/document. Follow nextPageFrom for the rest.", document.note)
    }))
}

fn compact_course_contents(courseid: i64, raw: &Value, query: Option<&str>) -> Value {
    let mut sections = Vec::new();
    for section in raw.as_array().cloned().unwrap_or_default() {
        let mut modules = Vec::new();
        for module in section
            .get("modules")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
        {
            let name = plain_text(module.get("name").and_then(Value::as_str).unwrap_or(""));
            let leftover = query.map(|query| leftover_search_tokens(query, &[]));
            let mut files = compact_module_files(&module);
            if let Some(leftover) = leftover.as_deref() {
                files.retain(|file| {
                    haystack_matches(
                        file.get("filename").and_then(Value::as_str).unwrap_or(""),
                        &[],
                        leftover,
                    )
                });
                if files.is_empty() && !haystack_matches(&name, &[], leftover) {
                    continue;
                }
            }
            let mut item = json!({
                "id": module.get("id"),
                "modname": module.get("modname"),
                "name": name,
            });
            if let Some(url) = module.get("url").cloned().filter(|value| !value.is_null()) {
                item["url"] = url;
            }
            if !files.is_empty() {
                item["files"] = Value::Array(files);
            }
            modules.push(item);
        }
        if query.is_some() && modules.is_empty() {
            continue;
        }
        let name = plain_text(section.get("name").and_then(Value::as_str).unwrap_or(""));
        let summary = truncate(
            &plain_text(section.get("summary").and_then(Value::as_str).unwrap_or("")),
            240,
        );
        let mut item = json!({
            "id": section.get("id"),
            "section": section.get("section"),
            "name": name,
            "modules": modules,
        });
        if !summary.is_empty() {
            item["summary"] = json!(summary);
        }
        sections.push(item);
    }
    json!({
        "courseid": courseid,
        "count": sections.len(),
        "sections": sections
    })
}

fn compact_module_files(module: &Value) -> Vec<Value> {
    module
        .get("contents")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|file| {
            let filename = file.get("filename").and_then(Value::as_str).unwrap_or("");
            if filename.is_empty() || filename == "." {
                return None;
            }
            if file.get("type").and_then(Value::as_str) == Some("content") {
                return None;
            }
            let mut item = json!({ "filename": filename });
            if let Some(url) = file
                .get("fileurl")
                .cloned()
                .filter(|value| !value.is_null())
            {
                item["fileurl"] = url;
            }
            if let Some(size) = file
                .get("filesize")
                .cloned()
                .filter(|value| !value.is_null())
            {
                item["filesize"] = size;
            }
            if let Some(mime) = file
                .get("mimetype")
                .cloned()
                .filter(|value| !value.is_null())
            {
                item["mimetype"] = mime;
            }
            Some(item)
        })
        .collect()
}

fn compact_grade_items(raw: &Value) -> Vec<Value> {
    raw.get("usergrades")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .flat_map(|row| {
            row.get("gradeitems")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default()
        })
        .filter_map(|item| {
            if item.get("gradeishidden").and_then(Value::as_bool) == Some(true)
                || item.get("hidden").and_then(Value::as_bool) == Some(true) {
                return None;
            }
            let itemtype = item.get("itemtype").and_then(Value::as_str).unwrap_or("");
            let name = item
                .get("itemname")
                .and_then(Value::as_str)
                .map(str::trim)
                .unwrap_or("");
            let name = if name.is_empty() {
                if itemtype == "course" {
                    "Course total"
                } else {
                    return None;
                }
            } else {
                name
            };
            Some(json!({
                "id": item.get("id"),
                "name": name,
                "module": item.get("itemmodule"),
                "type": itemtype,
                "grade": item.get("gradeformatted"),
                "raw": item.get("graderaw"),
                "max": item.get("grademax"),
                "percentage": item.get("percentageformatted"),
                "gradedAt": item.get("gradedategraded"),
                "feedback": truncate(&plain_text(item.get("feedback").and_then(Value::as_str).unwrap_or("")), 2000),
            }))
        })
        .collect()
}

fn current_or_favourite_course_ids(courses: &Value) -> Vec<i64> {
    let now = unix_now();
    courses
        .get("courses")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|course| {
            let id = course.get("id").and_then(Value::as_i64)?;
            let favourite = course
                .get("isfavourite")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let enddate = course.get("enddate").and_then(Value::as_i64).unwrap_or(0);
            if favourite || enddate > now {
                Some(id)
            } else {
                None
            }
        })
        .collect()
}

fn resolve_search_courses(courses: &[Value], query: &str, courseid: Option<i64>) -> Vec<Value> {
    if let Some(id) = courseid {
        let found = courses
            .iter()
            .find(|course| course.get("id").and_then(Value::as_i64) == Some(id))
            .cloned()
            .unwrap_or_else(|| json!({ "id": id }));
        return vec![found];
    }
    let tokens = search_tokens(query);
    if tokens.is_empty() {
        return Vec::new();
    }
    let mut scored = courses
        .iter()
        .filter_map(|course| {
            let short = course
                .get("shortname")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_ascii_lowercase();
            let full = course
                .get("fullname")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_ascii_lowercase();
            let short_tokens = normalize_tokens(&short);
            let mut score = 0;
            for token in &tokens {
                if short_tokens.iter().any(|part| part == token) || short == *token {
                    score += 50;
                } else if token.len() >= 3 && short.contains(token) {
                    score += 20;
                } else if token.len() >= 4 && full.contains(token) {
                    score += 8;
                }
            }
            (score > 0).then_some((score, course))
        })
        .collect::<Vec<_>>();
    scored.sort_by_key(|right| std::cmp::Reverse(right.0));
    let best = scored.first().map(|(score, _)| *score).unwrap_or(0);
    scored
        .into_iter()
        .filter(|(score, _)| *score + 10 >= best)
        .take(3)
        .map(|(_, course)| course.clone())
        .collect()
}

fn leftover_search_tokens(query: &str, courses: &[Value]) -> Vec<String> {
    search_tokens(query)
        .into_iter()
        .filter(|token| {
            if is_filler_token(token) {
                return false;
            }
            !courses.iter().any(|course| {
                let short = course
                    .get("shortname")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_ascii_lowercase();
                short.contains(token) || normalize_tokens(&short).iter().any(|part| part == token)
            })
        })
        .collect()
}

fn course_search_hit(course: &Value) -> Option<Value> {
    let id = course.get("id").and_then(Value::as_i64)?;
    let title = course
        .get("fullname")
        .or_else(|| course.get("shortname"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    Some(json!({
        "title": title,
        "course": title,
        "courseid": id,
        "area": "course",
        "component": "core_course",
    }))
}

fn search_course_modules(course: &Value, content: &Value, query: &str) -> Vec<Value> {
    let course_name = course
        .get("fullname")
        .or_else(|| course.get("shortname"))
        .and_then(Value::as_str)
        .unwrap_or("");
    let courseid = course.get("id").and_then(Value::as_i64);
    let leftover = leftover_search_tokens(query, std::slice::from_ref(course));
    let mut hits = Vec::new();
    for section in content
        .get("sections")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
    {
        let section_name = section.get("name").and_then(Value::as_str).unwrap_or("");
        for module in section
            .get("modules")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
        {
            let name = module.get("name").and_then(Value::as_str).unwrap_or("");
            let files = module
                .get("files")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            if leftover.is_empty() {
                continue;
            }
            let name_hit = haystack_matches(name, &[], &leftover);
            let mut file_hits = 0;
            for file in &files {
                let filename = file.get("filename").and_then(Value::as_str).unwrap_or("");
                if !haystack_matches(filename, &[], &leftover) {
                    continue;
                }
                file_hits += 1;
                hits.push(json!({
                    "title": name,
                    "course": course_name,
                    "courseid": courseid,
                    "section": section_name,
                    "area": "file",
                    "component": module.get("modname"),
                    "url": module.get("url"),
                    "filename": file.get("filename"),
                    "fileurl": file.get("fileurl"),
                    "content": file.get("filename"),
                }));
            }
            if file_hits == 0 && name_hit {
                hits.push(json!({
                    "title": name,
                    "course": course_name,
                    "courseid": courseid,
                    "section": section_name,
                    "area": "activity",
                    "component": module.get("modname"),
                    "url": module.get("url"),
                }));
            }
        }
    }
    hits
}

fn map_global_search_results(raw: &Value) -> Vec<Value> {
    raw.get("results")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .map(|item| {
            json!({
                "title": plain_text(item.get("title").and_then(Value::as_str).unwrap_or("")),
                "course": item.get("coursefullname"),
                "component": item.get("componentname"),
                "area": item.get("areaname"),
                "url": item.get("docurl").or_else(|| item.get("courseurl")),
                "timemodified": item.get("timemodified"),
                "content": truncate(
                    &plain_text(item.get("content").and_then(Value::as_str).unwrap_or("")),
                    500,
                ),
            })
        })
        .collect()
}

fn haystack_matches(name: &str, files: &[Value], leftover: &[String]) -> bool {
    if leftover.is_empty() {
        return false;
    }
    let mut haystack = name.to_string();
    for file in files {
        if let Some(filename) = file.get("filename").and_then(Value::as_str) {
            haystack.push(' ');
            haystack.push_str(filename);
        }
    }
    let hay = normalize_tokens(&haystack);
    if hay.is_empty() {
        return false;
    }
    let digits = leftover
        .iter()
        .filter(|token| token.chars().all(|ch| ch.is_ascii_digit()))
        .cloned()
        .collect::<Vec<_>>();
    let words = leftover
        .iter()
        .filter(|token| !token.chars().all(|ch| ch.is_ascii_digit()))
        .cloned()
        .collect::<Vec<_>>();
    if words.iter().any(|word| word == "td") && !digits.is_empty() {
        return digits.iter().all(|digit| td_number_in(&hay, digit));
    }
    leftover
        .iter()
        .all(|token| hay.iter().any(|part| part == token))
}

fn td_number_in(hay: &[String], number: &str) -> bool {
    let mut index = 0;
    while index < hay.len() {
        if hay[index] == "td" {
            let mut look = index + 1;
            while look < hay.len() && hay[look] != "td" {
                if hay[look].chars().all(|ch| ch.is_ascii_digit()) {
                    if hay[look] == number {
                        return true;
                    }
                    break;
                }
                look += 1;
            }
        }
        index += 1;
    }
    false
}

fn search_tokens(text: &str) -> Vec<String> {
    normalize_tokens(text)
        .into_iter()
        .filter(|token| !is_stopword(token))
        .collect()
}

fn normalize_tokens(text: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let flush = |current: &mut String, tokens: &mut Vec<String>| {
        if current.is_empty() {
            return;
        }
        let chunk = std::mem::take(current);
        let mut piece = String::new();
        let mut digit = None;
        for ch in chunk.chars() {
            let is_digit = ch.is_ascii_digit();
            if let Some(was_digit) = digit {
                if was_digit != is_digit && !piece.is_empty() {
                    tokens.push(std::mem::take(&mut piece));
                }
            }
            digit = Some(is_digit);
            piece.push(ch);
        }
        if !piece.is_empty() {
            tokens.push(piece);
        }
    };
    for ch in text.chars() {
        if ch.is_ascii_alphanumeric() || ch == 'é' || ch == 'è' || ch == 'ê' || ch == 'à' {
            current.extend(ch.to_lowercase());
        } else {
            flush(&mut current, &mut tokens);
        }
    }
    flush(&mut current, &mut tokens);
    tokens
}

fn is_stopword(token: &str) -> bool {
    matches!(
        token,
        "le" | "la"
            | "les"
            | "du"
            | "de"
            | "des"
            | "un"
            | "une"
            | "et"
            | "ou"
            | "a"
            | "au"
            | "aux"
            | "en"
            | "d"
            | "l"
            | "the"
            | "of"
            | "for"
            | "to"
            | "my"
            | "mes"
            | "mon"
            | "ma"
            | "nos"
            | "fait"
            | "faire"
            | "moi"
            | "me"
            | "donne"
            | "trouve"
    )
}

fn is_filler_token(token: &str) -> bool {
    matches!(
        token,
        "exo"
            | "exos"
            | "exercice"
            | "exercices"
            | "devoir"
            | "devoirs"
            | "fichier"
            | "file"
            | "cours"
            | "course"
            | "moodle"
    )
}

fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0)
}

fn courseids_payload(courseids: Option<&[i64]>) -> Value {
    match courseids {
        Some(ids) if !ids.is_empty() => json!({ "courseids": ids }),
        _ => json!({}),
    }
}

fn plain_text(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut in_tag = false;
    let mut chars = input.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '<' => in_tag = true,
            '>' if in_tag => in_tag = false,
            _ if in_tag => {}
            '&' => {
                let mut entity = String::new();
                while let Some(&next) = chars.peek() {
                    if next == ';' {
                        chars.next();
                        break;
                    }
                    if entity.len() >= 10 || (!next.is_ascii_alphanumeric() && next != '#') {
                        break;
                    }
                    entity.push(next);
                    chars.next();
                }
                match entity.as_str() {
                    "nbsp" => out.push(' '),
                    "amp" => out.push('&'),
                    "lt" => out.push('<'),
                    "gt" => out.push('>'),
                    "quot" => out.push('"'),
                    "apos" | "#39" => out.push('\''),
                    other if other.starts_with('#') => {
                        if let Some(decoded) = other
                            .trim_start_matches('#')
                            .parse::<u32>()
                            .ok()
                            .and_then(char::from_u32)
                        {
                            out.push(decoded);
                        }
                    }
                    _ => {
                        out.push('&');
                        out.push_str(&entity);
                    }
                }
            }
            _ => out.push(ch),
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn truncate(text: &str, max_chars: usize) -> String {
    let count = text.chars().count();
    if count <= max_chars {
        return text.to_string();
    }
    let mut clipped: String = text.chars().take(max_chars.saturating_sub(1)).collect();
    clipped.push('…');
    clipped
}

/// MCP tool result: JSON summary, the original file, then rendered PDF pages.
pub(crate) fn mcp_content_for_file(mut data: Value) -> Vec<Value> {
    let file = data
        .as_object_mut()
        .and_then(|object| object.remove("file"));
    let links = data.get("links").cloned();
    let pages = match data
        .as_object_mut()
        .and_then(|object| object.remove("pages"))
    {
        Some(Value::Array(pages)) => pages,
        _ => Vec::new(),
    };
    let mut content = vec![json!({
        "type": "text",
        "text": serde_json::to_string_pretty(&data).unwrap_or_default()
    })];
    if let Some(mut file) = file {
        let blob = file.get_mut("data").map(Value::take);
        if let Some(blob @ Value::String(_)) = blob {
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
    for mut page in pages {
        let Some(image @ Value::String(_)) = page.get_mut("data").map(Value::take) else {
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
    if let Some(links) = links {
        for (key, name, description) in [
            (
                "download",
                "PDF file",
                "Direct link to the original PDF. Open or download it, then attach it to the chat if the client supports uploads.",
            ),
            (
                "text",
                "PDF text",
                "Extracted text, when requested.",
            ),
        ] {
            let Some(uri) = links.get(key).and_then(Value::as_str) else {
                continue;
            };
            content.push(json!({
                "type": "resource_link",
                "uri": uri,
                "name": name,
                "description": description,
                "mimeType": if key == "download" { "application/pdf" } else { "text/plain" }
            }));
        }
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
    let parsed = Url::parse(fileurl).map_err(|_| "The file URL is not valid.".to_string())?;
    let base =
        Url::parse(base_url).map_err(|_| "The Moodle site address is not valid.".to_string())?;
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
        return Err("Use a Moodle fileurl from moodle_course_content (pluginfile.php).".into());
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
    let head =
        String::from_utf8_lossy(&bytes[start..bytes.len().min(start + 32)]).to_ascii_lowercase();
    head.starts_with("<!doctype html") || head.starts_with("<html")
}

pub(crate) fn filename_from(disposition: Option<&str>, fileurl: &str) -> String {
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
    None
}

fn temp_job_dir() -> Option<Workspace> {
    Workspace::new()
}

#[derive(Clone, Copy)]
struct PdfRange {
    from: u32,
    to: u32,
}

fn resolve_pdf_range(
    page_from: Option<u32>,
    page_to: Option<u32>,
    total_pages: u32,
) -> Result<PdfRange, String> {
    let from = page_from.unwrap_or(1).max(1);
    if let Some(to) = page_to {
        if to < from {
            return Err("page_to must be >= page_from.".into());
        }
    }
    let hard_cap = from.saturating_add(PDF_HARD_MAX - 1);
    let requested_to = page_to.unwrap_or(from.saturating_add(MAX_PDF_PAGES - 1));
    let mut to = requested_to.min(hard_cap);
    if total_pages > 0 {
        if from > total_pages {
            return Err(format!(
                "page_from ({from}) is past the end of this PDF ({total_pages} pages)."
            ));
        }
        to = to.min(total_pages);
        // Never hide a short tail: a 30-page chapter must come back in one call.
        if page_to.is_none()
            && to < total_pages
            && total_pages - to <= PDF_TAIL_EXTEND
            && total_pages <= hard_cap
        {
            to = total_pages;
        }
    }
    Ok(PdfRange { from, to })
}

fn fold_char(ch: char) -> char {
    match ch {
        'à' | 'â' | 'ä' | 'á' | 'ã' | 'å' => 'a',
        'ç' => 'c',
        'é' | 'è' | 'ê' | 'ë' => 'e',
        'í' | 'ì' | 'î' | 'ï' => 'i',
        'ñ' => 'n',
        'ó' | 'ò' | 'ô' | 'ö' | 'õ' => 'o',
        'ú' | 'ù' | 'û' | 'ü' => 'u',
        'ý' | 'ÿ' => 'y',
        _ => ch,
    }
}

/// Lowercase, accent-folded, whitespace-collapsed text for honest section search.
fn normalize_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut space = false;
    for ch in text.chars().flat_map(char::to_lowercase) {
        let ch = fold_char(ch);
        if ch.is_whitespace() {
            space = true;
            continue;
        }
        if space && !out.is_empty() {
            out.push(' ');
        }
        space = false;
        out.push(ch);
    }
    out
}

fn find_pdf_matches(pages: &[String], needle: &str) -> Vec<(u32, String)> {
    let needle = normalize_text(needle);
    if needle.is_empty() {
        return Vec::new();
    }
    let mut matches = Vec::new();
    for (index, page) in pages.iter().enumerate() {
        if page.trim().is_empty() {
            continue;
        }
        if !contains_section(&normalize_text(page), &needle) {
            continue;
        }
        let snippet = page
            .lines()
            .map(str::trim)
            .find(|line| !line.is_empty() && contains_section(&normalize_text(line), &needle))
            .or_else(|| page.lines().map(str::trim).find(|line| !line.is_empty()))
            .unwrap_or_default();
        matches.push((index as u32 + 1, truncate_chars(snippet, 160)));
        if matches.len() >= 10 {
            break;
        }
    }
    matches
}

/// Cheap table of contents from numbered headings; lets an agent jump straight
/// to "3.3" even when it does not pass `find`.
fn pdf_outline(pages: &[String]) -> Vec<(u32, String)> {
    let mut outline = Vec::new();
    for (index, page) in pages.iter().enumerate() {
        let number = index as u32 + 1;
        if looks_like_toc(number, pages) {
            continue;
        }
        for line in page
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .take(6)
        {
            if line.chars().count() > 140 {
                continue;
            }
            let head: String = line.chars().take(8).collect();
            let numbered =
                head.chars().next().is_some_and(|ch| ch.is_ascii_digit()) && head.contains('.');
            let named = {
                let lower = normalize_text(line);
                lower.starts_with("chapitre ")
                    || lower.starts_with("section ")
                    || lower.starts_with("annexe ")
            };
            if numbered || named {
                outline.push((number, truncate_chars(line, 140)));
            }
            if outline.len() >= 40 {
                return outline;
            }
        }
    }
    outline
}

/// `3.3` must not match `13.3` or `3.30`.
fn contains_section(haystack: &str, needle: &str) -> bool {
    let mut start = 0;
    while let Some(offset) = haystack[start..].find(needle) {
        let at = start + offset;
        let before = haystack[..at].chars().next_back();
        let after = haystack[at + needle.len()..].chars().next();
        let digit_before = before.is_some_and(|ch| ch.is_ascii_digit());
        let digit_after = after.is_some_and(|ch| ch.is_ascii_digit());
        if !digit_before && !digit_after {
            return true;
        }
        start = at + haystack[at..].chars().next().map_or(1, char::len_utf8);
    }
    false
}

fn looks_like_toc(page: u32, pages: &[String]) -> bool {
    let text = pages
        .get(page.saturating_sub(1) as usize)
        .map(String::as_str)
        .unwrap_or_default();
    let head: String = text.chars().take(600).collect();
    let head = normalize_text(&head);
    head.contains("table des matieres")
        || head.contains("table of contents")
        || head.contains("sommaire")
        || head.contains("contenu du cours")
}

pub(crate) fn truncate_chars(text: &str, max: usize) -> String {
    truncate(text, max)
}

/// Links served by the local MCP endpoint / its tunnel. They expose the file,
/// the original document and optional plain text.
fn share_links_json(id: &str, _from: u32, _to: u32) -> Option<Value> {
    let links = crate::share::links_for(id)?;
    let local_only = links.download.contains("://127.0.0.1");
    Some(json!({
        "download": links.download,
        "text": links.text,
        "expiresInHours": 3,
        "localOnly": local_only,
    }))
}

fn pdf_result_note(
    truncated: bool,
    from: u32,
    to: u32,
    total_pages: u32,
    next_page_from: Option<u32>,
) -> String {
    if truncated {
        let next = next_page_from.unwrap_or(to + 1);
        let total = if total_pages > 0 {
            total_pages.to_string()
        } else {
            "unknown".into()
        };
        format!(
            "Rendered PDF pages {from}-{to} of {total}. This window is incomplete. Call moodle_file again with the same fileurl and page_from={next} (and optional page_to) to continue. Prefer the extracted text and page images over guessing missing pages."
        )
    } else if total_pages > 0 {
        format!(
            "Rendered PDF pages {from}-{to} of {total_pages}. Read the extracted text and page images; do not invent content for pages outside this range."
        )
    } else {
        format!(
            "Rendered PDF pages {from}-{to}. Read the extracted text and page images; if a section is missing, call moodle_file again with a higher page_from."
        )
    }
}

fn pdftotext_cli_range(bytes: &[u8], from: u32, to: u32) -> Option<String> {
    let dir = temp_job_dir()?;
    let path = dir.join("file.pdf");
    let written = std::fs::write(&path, bytes).is_ok();
    let output = if written {
        pdf_output(
            std::process::Command::new("pdftotext")
                .args([
                    "-layout",
                    "-enc",
                    "UTF-8",
                    "-f",
                    &from.to_string(),
                    "-l",
                    &to.to_string(),
                ])
                .arg(&path)
                .arg("-"),
        )
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
    Some(text)
}

pub(crate) fn pdf_page_count(bytes: &[u8]) -> Option<u32> {
    let dir = temp_job_dir()?;
    let path = dir.join("file.pdf");
    if std::fs::write(&path, bytes).is_err() {
        let _ = std::fs::remove_dir_all(&dir);
        return None;
    }
    let output = pdf_output(std::process::Command::new("pdfinfo").arg(&path)).ok();
    let _ = std::fs::remove_dir_all(&dir);
    let output = output?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    for line in text.lines() {
        let line = line.trim();
        if let Some(rest) = line
            .strip_prefix("Pages:")
            .or_else(|| line.strip_prefix("pages:"))
        {
            if let Ok(count) = rest.trim().parse::<u32>() {
                return (count <= 10_000).then_some(count);
            }
        }
    }
    None
}

fn render_pdf_pages(bytes: &[u8], from: u32, to: u32) -> Vec<Value> {
    let Some(dir) = temp_job_dir() else {
        return Vec::new();
    };
    let pdf_path = dir.join("file.pdf");
    let prefix = dir.join("page");
    if std::fs::write(&pdf_path, bytes).is_err()
        || !pdf_output(
            std::process::Command::new("pdftoppm")
                .args([
                    "-jpeg",
                    "-jpegopt",
                    "quality=78",
                    "-r",
                    "128",
                    "-scale-to",
                    "1600",
                    "-f",
                    &from.to_string(),
                    "-l",
                    &to.to_string(),
                ])
                .arg(&pdf_path)
                .arg(&prefix),
        )
        .map(|output| output.status.success())
        .unwrap_or(false)
    {
        let _ = std::fs::remove_dir_all(&dir);
        return Vec::new();
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
                .is_some_and(|ext| {
                    ext.eq_ignore_ascii_case("jpg") || ext.eq_ignore_ascii_case("jpeg")
                })
        })
        .collect();
    files.sort();
    use base64::Engine;
    let engine = base64::engine::general_purpose::STANDARD;
    let mut pages = Vec::new();
    let mut total_image_bytes = 0usize;
    for (index, path) in files.into_iter().enumerate() {
        let Ok(image) = std::fs::read(&path) else {
            continue;
        };
        if image.is_empty() || image.len() > MAX_PAGE_BYTES {
            continue;
        }
        let page_number = from.saturating_add(index as u32);
        if total_image_bytes.saturating_add(image.len()) > MAX_IMAGES_TOTAL_BYTES {
            continue;
        }
        total_image_bytes += image.len();
        pages.push(json!({
            "page": page_number,
            "mimeType": "image/jpeg",
            "data": engine.encode(image),
        }));
    }
    let _ = std::fs::remove_dir_all(&dir);
    pages
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

    type RawResponse = (u16, String, Vec<(String, String)>, Vec<u8>);
    pub(crate) fn raw_server(
        responses: Vec<RawResponse>,
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
            headers.to_ascii_lowercase().contains("user-agent:") && headers.contains("Mozilla/5.0"),
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
        assert!(
            requests[1].starts_with("GET /moodle/login/token.php?"),
            "{requests:?}"
        );
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
        assert!(
            requests[0].contains("wstoken=PRIVATE-TOKEN"),
            "{requests:?}"
        );
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
            .call(
                "core_course_get_contents&wsfunction=core_user_create_users",
                &json!({}),
            )
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
        let parent = std::fs::metadata(dir.parent().unwrap())
            .unwrap()
            .permissions()
            .mode();
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
    fn plain_text_strips_html_and_entities() {
        assert_eq!(
            plain_text("<p>Bonjour&nbsp;<b>Max</b> &amp; cie&#39;s</p>"),
            "Bonjour Max & cie's"
        );
    }

    #[test]
    fn pdf_range_caps_per_call_and_reports_continuation() {
        let first = resolve_pdf_range(None, None, 100).unwrap();
        assert_eq!((first.from, first.to), (1, MAX_PDF_PAGES));
        let mid = resolve_pdf_range(Some(90), None, 100).unwrap();
        assert_eq!((mid.from, mid.to), (90, 100));
        let note = pdf_result_note(true, 1, MAX_PDF_PAGES, 100, Some(MAX_PDF_PAGES + 1));
        assert!(
            note.contains("nextPageFrom")
                || note.contains(&format!("page_from={}", MAX_PDF_PAGES + 1)),
            "{note}"
        );
        assert!(
            note.contains("incomplete") || note.contains("continue"),
            "{note}"
        );
        assert!(resolve_pdf_range(Some(20), Some(10), 50).is_err());
        assert!(resolve_pdf_range(Some(200), None, 50).is_err());
        // A short tail must never be silently dropped (the "last two pages" bug).
        let short = resolve_pdf_range(None, None, 30).unwrap();
        assert_eq!((short.from, short.to), (1, 30));
        assert!(!pdf_result_note(false, 1, 30, 30, None).contains("incomplete"));
        // Huge documents still window, and explicit page_to is honored.
        let huge = resolve_pdf_range(Some(25), Some(30), 400).unwrap();
        assert_eq!((huge.from, huge.to), (25, 30));
    }

    #[test]
    fn pdf_search_skips_the_toc_and_finds_the_real_section() {
        assert!(!contains_section("1œuvre", "œuvre"));
        assert!(contains_section("1œuvre puis œuvre", "œuvre"));
        let mut pages: Vec<String> = (1..=30)
            .map(|page| format!("Page {page}\nExercices du chapitre 3"))
            .collect();
        pages[1] = "Sommaire\n3.3 Taux de variation instantane .... 26".into();
        pages[25] = "3.3 Taux de variation instantane\nLe mouvement rectiligne".into();
        let matches = find_pdf_matches(&pages, "3.3");
        assert_eq!(matches[0].0, 2, "{matches:?}");
        assert!(looks_like_toc(2, &pages));
        let best = matches
            .iter()
            .map(|(page, _)| *page)
            .find(|page| *page > 2 && !looks_like_toc(*page, &pages));
        assert_eq!(best, Some(26));
        assert!(contains_section("chapitre 3.3 ici", "3.3"));
        assert!(!contains_section("chapitre 13.3 ici", "3.3"));
        assert!(!contains_section("exercice 3.30 ici", "3.3"));
        assert!(contains_section(
            &normalize_text("La dérivée et le taux de variation"),
            &normalize_text("derivee")
        ));
        let outline = pdf_outline(&pages);
        assert!(
            outline
                .iter()
                .any(|(page, title)| *page == 26 && title.contains("3.3")),
            "{outline:?}"
        );
    }

    #[test]
    #[ignore = "requires Ghostscript and Poppler; run explicitly in PDF integration CI"]
    fn pdf_section_search_targets_the_matching_page() {
        crate::config::tests::with_temp_config(|| {
            if std::process::Command::new("gs")
                .arg("--version")
                .output()
                .is_err()
            {
                panic!("Ghostscript is required for this integration test");
            }
            let Some(dir) = temp_job_dir() else {
                panic!("Cannot create PDF workspace");
            };
            let postscript = dir.join("fixture.ps");
            let pdf_path = dir.join("fixture.pdf");
            let source = r#"%!PS
/Helvetica findfont 14 scalefont setfont
1 1 30 {
  /p exch def
  72 740 moveto (MTH0101 - Calcul differentiel - page ) show p 3 string cvs show
  72 710 moveto (Exercices et exemples du chapitre 3 : limites, continuite, derivation.) show
  72 680 moveto (On calcule le taux de variation et la pente de la tangente.) show
  72 650 moveto (La notation de Leibniz resume la derivee comme un quotient differentiel.) show
  p 26 eq {
    72 620 moveto (3.3 Taux de variation instantane) show
    72 590 moveto (Le mouvement rectiligne et la notation de Leibniz.) show
  } if
  showpage
} for
"#;
            let built = std::fs::write(&postscript, source).is_ok()
                && std::process::Command::new("gs")
                    .args(["-q", "-dNOPAUSE", "-dBATCH", "-sDEVICE=pdfwrite"])
                    .arg(format!("-sOutputFile={}", pdf_path.display()))
                    .arg(&postscript)
                    .status()
                    .map(|status| status.success())
                    .unwrap_or(false);
            if !built {
                let _ = std::fs::remove_dir_all(&dir);
                panic!("Cannot build PDF fixture");
            }
            let pdf = std::fs::read(&pdf_path).unwrap();
            let (url, worker) = raw_server(vec![(200, "application/pdf".into(), Vec::new(), pdf)]);
            let client = MoodleClient::new(&MoodleConfig {
                base_url: url.clone(),
                token: "private-token".into(),
                allow_write: false,
            });
            let fileurl = format!("{url}/pluginfile.php/1/mod_resource/content/0/ch3.pdf");
            let listener = crate::http::bind(0).unwrap();
            let share_port = listener.local_addr().unwrap().port();
            std::thread::spawn(move || {
                let _ = crate::http::serve_controlled(
                    listener,
                    std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true)),
                );
            });
            let link_guard = crate::share::LINK_LOCK.lock().unwrap();
            crate::share::set_port(share_port);
            let data = client
                .download_file(
                    &fileurl,
                    &FileQuery {
                        format: FileFormat::Text,
                        find: Some("3.3".into()),
                        ..Default::default()
                    },
                )
                .unwrap();
            let links = &data["links"];
            assert_eq!(links["localOnly"], true, "{data}");
            let reader_url = links["text"].as_str().unwrap();
            let reader = ureq::get(reader_url).call().unwrap().into_string().unwrap();
            assert!(reader.contains("Taux de variation"), "{reader}");
            let mut part = Vec::new();
            ureq::get(links["download"].as_str().unwrap())
                .call()
                .unwrap()
                .into_reader()
                .read_to_end(&mut part)
                .unwrap();
            assert_eq!(pdf_page_count(&part), Some(30));
            drop(link_guard);
            let _ = worker.join();
            let _ = std::fs::remove_dir_all(&dir);
            assert_eq!(data["totalPages"], 30);
            assert_eq!(data["matchedPage"], 26, "{data}");
            assert_eq!(data["pageFrom"], 26);
            assert_eq!(data["pageTo"], 30);
            assert_eq!(data["truncated"], false);
            assert!(
                data["text"]
                    .as_str()
                    .unwrap_or_default()
                    .contains("Taux de variation"),
                "{data}"
            );
            assert!(data.get("pages").is_none());
            assert!(data.get("file").is_none());
            let images = client
                .download_file(
                    &fileurl,
                    &FileQuery {
                        format: FileFormat::Images,
                        page_from: Some(26),
                        page_to: Some(27),
                        ..Default::default()
                    },
                )
                .unwrap();
            assert_eq!(images["pages"].as_array().unwrap().len(), 2);
            assert!(images.get("file").is_none());
            assert!(images.get("text").is_none());
            assert!(
                data["outline"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|entry| entry["page"] == 26
                        && entry["title"].as_str().unwrap_or_default().contains("3.3")),
                "{data}"
            );
            assert!(
                data["note"]
                    .as_str()
                    .unwrap_or_default()
                    .contains("page 26"),
                "{data}"
            );
        });
    }

    #[test]
    fn my_courses_hides_ended_and_hidden_by_default() {
        let now = unix_now();
        let (url, worker) = server(vec![
            json!({"userid": 7, "functions": []}),
            json!([
                {
                    "id": 1,
                    "shortname": "LIVE",
                    "fullname": "Live",
                    "visible": 1,
                    "startdate": now - 10,
                    "enddate": now + 86_400,
                    "lastaccess": now,
                    "isfavourite": true,
                    "progress": null
                },
                {
                    "id": 2,
                    "shortname": "OLD",
                    "fullname": "Old",
                    "visible": 1,
                    "startdate": now - 200 * 86_400,
                    "enddate": now - 40 * 86_400,
                    "lastaccess": now - 50 * 86_400,
                    "isfavourite": false,
                    "progress": null
                },
                {
                    "id": 3,
                    "shortname": "HIDE",
                    "fullname": "Hidden",
                    "visible": 0,
                    "startdate": now - 10,
                    "enddate": now + 86_400,
                    "lastaccess": now,
                    "isfavourite": false,
                    "progress": null
                }
            ]),
        ]);
        let client = MoodleClient::new(&crate::config::MoodleConfig {
            base_url: url,
            token: "token".into(),
            allow_write: false,
        });
        let compact = client.my_courses(false).unwrap();
        let ids: Vec<i64> = compact["courses"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|course| course["id"].as_i64())
            .collect();
        assert_eq!(ids, vec![1]);
        worker.join().unwrap();
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
            vec![(
                "Content-Disposition".into(),
                r#"attachment; filename="TD5.txt""#.into(),
            )],
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
        let reply =
            crate::mcp::dispatch_tool(&settings, "moodle_file", &json!({ "fileurl": fileurl }));
        let request = worker.join().unwrap().remove(0);
        assert!(
            request.starts_with(
                "GET /moodle/webservice/pluginfile.php/9/mod_resource/content/0/TD5.txt?"
            ),
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
    fn usable_text_rejects_embedded_font_noise() {
        let noise = "001\u{0004}\u{0004}\\u0008\\LZdnx\u{0001}\u{0004}".repeat(20);
        assert!(usable_text(noise).is_none());
        assert!(usable_text("TD 5 — Calcul différentiel\nExercice 1".into()).is_some());
    }

    #[test]
    fn default_pdf_delivery_needs_no_pdf_tools_or_duplicate_representations() {
        let pdf = b"%PDF-1.4 original-file";
        let (url, worker) = raw_server(vec![(200, "application/pdf".into(), vec![], pdf.to_vec())]);
        let client = MoodleClient::new(&MoodleConfig {
            base_url: url.clone(),
            token: "private-token".into(),
            allow_write: false,
        });
        let fileurl = format!("{url}/pluginfile.php/1/mod_resource/content/0/notes.pdf");
        let data = client
            .download_file(&fileurl, &FileQuery::default())
            .unwrap();
        assert!(data.get("file").is_some());
        for key in ["text", "ocrText", "pages", "outline", "matches"] {
            assert!(data.get(key).is_none(), "unexpected {key}");
        }
        assert_eq!(worker.join().unwrap().len(), 1);
        let again = client
            .download_file(&fileurl, &FileQuery::default())
            .unwrap();
        assert_eq!(again["file"], data["file"]);
    }

    #[test]
    fn resources_do_not_report_an_expired_token_as_an_empty_course() {
        let (url, worker) = server(vec![
            json!({"exception":"moodle_exception","errorcode":"invalidtoken","message":"Expired"}),
        ]);
        let client = MoodleClient::new(&MoodleConfig {
            base_url: url,
            token: "private-token".into(),
            allow_write: false,
        });
        assert!(client.resources(1).unwrap_err().contains("invalidtoken"));
        assert_eq!(worker.join().unwrap().len(), 1);
    }

    #[test]
    fn mcp_content_attaches_pdf_pages_as_images_without_inlining_them_in_text() {
        let data = json!({
            "filename": "TD5.pdf",
            "size": 10,
            "text": "Exercice 1 page 452",
            "file": { "filename": "TD5.pdf", "mimeType": "application/pdf", "data": "JVBERg==" },
            "pages": [{ "mimeType": "image/jpeg", "data": "QUJDRA==" }],
            "links": {
                "download": "https://example.test/s/1/TD5.pdf",
                "text": "https://example.test/s/1/text"
            }
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
        assert_eq!(content[3]["type"], "resource_link");
        assert_eq!(content[3]["uri"], "https://example.test/s/1/TD5.pdf");
        assert_eq!(content[3]["mimeType"], "application/pdf");
        assert_eq!(content[4]["type"], "resource_link");
        assert_eq!(content[4]["uri"], "https://example.test/s/1/text");
    }

    fn td5_contents() -> Value {
        json!([{
            "id": 80,
            "section": 5,
            "name": "Exercices TD",
            "summary": "<p>Liste des&nbsp;exercices</p>",
            "hidden": "noise",
            "modules": [
                {
                    "id": 10,
                    "modname": "resource",
                    "name": "TD Chapitre 5",
                    "url": "https://moodle.example/mod/resource/view.php?id=10",
                    "contents": [{
                        "type": "file",
                        "filename": "TD_Chapitre_5.doc",
                        "fileurl": "https://moodle.example/pluginfile.php/1/mod_resource/content/0/TD_Chapitre_5.doc",
                        "filesize": 281600,
                        "mimetype": "application/msword"
                    }],
                    "description": "x".repeat(4000)
                },
                {
                    "id": 11,
                    "modname": "resource",
                    "name": "TD Chapitre 6",
                    "contents": [{
                        "type": "file",
                        "filename": "TD_Chapitre_6.doc",
                        "fileurl": "https://moodle.example/pluginfile.php/1/mod_resource/content/0/TD_Chapitre_6.doc"
                    }]
                }
            ]
        }])
    }

    #[test]
    fn office_delivery_paginates_searches_and_scrubs_secrets() {
        let bytes = crate::document::tests::archive(&[(
            "word/document.xml",
            &format!(
                "<document><p>{}</p><p>PRIVATE-TOKEN Needle</p></document>",
                "é".repeat(17_000)
            ),
        )]);
        let read = |query: FileQuery| {
            MoodleClient::read_document(
                "notes.docx",
                "application/octet-stream",
                Arc::new(bytes.clone()),
                &query,
                "PRIVATE-TOKEN",
            )
            .unwrap()
        };
        let first = read(FileQuery::default());
        assert_eq!(first["totalBlocks"], 3);
        assert_eq!(first["nextPageFrom"], 3);
        assert_eq!(first["blocks"].as_array().unwrap().len(), 2);
        let found = read(FileQuery {
            find: Some("needle".into()),
            ..Default::default()
        });
        assert_eq!(found["pageFrom"], 3);
        assert!(found["blocks"][0]["text"]
            .as_str()
            .unwrap()
            .contains("[redacted] Needle"));
        assert!(!found.to_string().contains("PRIVATE-TOKEN"));
        assert_eq!(found["truncated"], false);
        assert!(MoodleClient::read_document(
            "notes.docx",
            "application/octet-stream",
            Arc::new(bytes),
            &FileQuery {
                format: FileFormat::Images,
                ..Default::default()
            },
            "secret"
        )
        .is_err());
    }

    #[test]
    fn recent_grades_sort_dated_assessments_and_report_partial_failures() {
        let (url, worker) = server(vec![
            json!({"userid": 42}),
            json!([]),
            json!({"userid": 42}),
            json!({"usergrades": [{"gradeitems": [
                {"itemname": "Older", "itemtype": "mod", "graderaw": 10, "gradedategraded": 100},
                {"itemname": "Latest", "itemtype": "mod", "graderaw": 0, "gradedategraded": 200},
                {"itemname": "Undated", "itemtype": "mod", "graderaw": 12},
                {"itemname": "Ungraded", "itemtype": "mod", "graderaw": null},
                {"itemname": "Hidden", "itemtype": "mod", "gradeishidden": true, "graderaw": 15, "gradedategraded": 300},
                {"itemname": "Total", "itemtype": "course", "graderaw": 12, "gradedategraded": 500}
            ]}]}),
            json!({"exception": "moodle_exception", "errorcode": "accessdenied", "message": "Denied"}),
        ]);
        let client = MoodleClient::new(&MoodleConfig {
            base_url: url,
            token: "private-token".into(),
            allow_write: false,
        });
        let result = client.recent_grades(Some(vec![1, 2]), 1).unwrap();
        assert_eq!(result["count"], 1);
        assert_eq!(result["total"], 2);
        assert_eq!(result["grades"][0]["name"], "Latest");
        assert_eq!(result["grades"][0]["raw"], 0);
        assert_eq!(result["undatedGradesOmitted"], 1);
        assert_eq!(result["partial"], true);
        assert_eq!(result["errors"][0]["courseid"], 2);
        worker.join().unwrap();
    }

    #[test]
    fn course_updates_use_standard_moodle_api_and_resolve_names() {
        let (url, worker) = server(vec![
            json!({"instances": [
                {"id": 10, "contextlevel": "module", "updates": [{"name": "files", "timeupdated": 200}]},
                {"id": 11, "contextlevel": "module", "updates": []}
            ], "warnings": []}),
            td5_contents(),
        ]);
        let client = MoodleClient::new(&MoodleConfig {
            base_url: url,
            token: "private-token".into(),
            allow_write: false,
        });
        let result = client.course_updates(5476, 100).unwrap();
        assert_eq!(result["count"], 1);
        assert_eq!(result["updates"][0]["name"], "TD Chapitre 5");
        assert_eq!(result["updates"][0]["updates"][0]["timeupdated"], 200);
        let requests = worker.join().unwrap();
        assert!(requests[0].contains("wsfunction=core_course_get_updates_since"));
        assert!(requests[0].contains("since=100"));
        assert!(client.course_updates(1, -1).is_err());
    }

    #[test]
    fn course_content_is_compact_and_query_keeps_td5() {
        let payload = td5_contents();
        let (url, worker) = server(vec![payload.clone(), payload]);
        let client = MoodleClient::new(&MoodleConfig {
            base_url: url,
            token: "token".into(),
            allow_write: false,
        });
        let all = client.course_content(5476, None).unwrap();
        assert_eq!(all["courseid"], 5476);
        assert_eq!(all["count"], 1);
        assert_eq!(all["sections"][0]["name"], "Exercices TD");
        assert_eq!(all["sections"][0]["summary"], "Liste des exercices");
        assert_eq!(all["sections"][0]["modules"].as_array().unwrap().len(), 2);
        assert!(all["sections"][0]["modules"][0]["files"][0]["fileurl"]
            .as_str()
            .unwrap()
            .contains("TD_Chapitre_5.doc"));
        let dumped = all.to_string();
        assert!(!dumped.contains("description"));
        assert!(!dumped.contains("xxxx"));
        let filtered = client.course_content(5476, Some("TD5")).unwrap();
        assert_eq!(
            filtered["sections"][0]["modules"].as_array().unwrap().len(),
            1
        );
        assert_eq!(
            filtered["sections"][0]["modules"][0]["name"],
            "TD Chapitre 5"
        );
        worker.join().unwrap();
    }

    #[test]
    fn search_resolves_phs0102_td5_to_the_chapter_file() {
        let now = unix_now();
        let (url, worker) = server(vec![
            json!({"userid": 7, "functions": []}),
            json!([{
                "id": 5476,
                "shortname": "PHS0102 (20263)",
                "fullname": "PHS0102 (A2026) - Électricité et magnétisme",
                "visible": 1,
                "startdate": now - 10,
                "enddate": now + 86_400,
                "lastaccess": now,
                "isfavourite": true
            }]),
            td5_contents(),
        ]);
        let client = MoodleClient::new(&MoodleConfig {
            base_url: url,
            token: "token".into(),
            allow_write: false,
        });
        let found = client.search("PHS0102 TD5", 0, None).unwrap();
        assert_eq!(found["total"], 1);
        assert_eq!(found["results"][0]["title"], "TD Chapitre 5");
        assert_eq!(found["results"][0]["courseid"], 5476);
        assert!(found["results"][0]["fileurl"]
            .as_str()
            .unwrap()
            .contains("TD_Chapitre_5.doc"));
        worker.join().unwrap();
    }

    #[test]
    fn grades_for_course_flatten_moodle_usergrades() {
        let (url, worker) = server(vec![
            json!({"userid": 42, "functions": []}),
            json!({
                "usergrades": [{
                    "courseid": 15,
                    "gradeitems": [
                        {
                            "itemname": "Quiz 1",
                            "itemtype": "mod",
                            "itemmodule": "quiz",
                            "gradeformatted": "12,00",
                            "graderaw": 12.0,
                            "grademax": 20.0,
                            "percentageformatted": "60 %",
                            "gradedategraded": 1_700_000_000
                        },
                        {
                            "itemname": null,
                            "itemtype": "category",
                            "gradeformatted": "-"
                        },
                        {
                            "itemname": "",
                            "itemtype": "course",
                            "gradeformatted": "12,00",
                            "graderaw": 12.0,
                            "grademax": 20.0
                        }
                    ]
                }]
            }),
        ]);
        let client = MoodleClient::new(&MoodleConfig {
            base_url: url,
            token: "token".into(),
            allow_write: false,
        });
        let grades = client.grades_for_course(15).unwrap();
        assert_eq!(grades["courseid"], 15);
        assert_eq!(grades["count"], 2);
        assert_eq!(grades["grades"][0]["name"], "Quiz 1");
        assert_eq!(grades["grades"][1]["name"], "Course total");
        assert!(grades.get("usergrades").is_none());
        worker.join().unwrap();
    }

    #[test]
    fn grades_overview_keeps_current_courses_with_a_grade() {
        let now = unix_now();
        let (url, worker) = server(vec![
            json!({"userid": 42, "functions": []}),
            json!({"grades": [
                {"courseid": 1, "grade": "A", "rawgrade": "90"},
                {"courseid": 2, "grade": "-", "rawgrade": null},
                {"courseid": 9, "grade": "F", "rawgrade": "0"}
            ]}),
            json!({"userid": 42, "functions": []}),
            json!([{
                "id": 1,
                "shortname": "PHS0102",
                "fullname": "Physique",
                "visible": 1,
                "startdate": now - 10,
                "enddate": now + 86_400,
                "lastaccess": now,
                "isfavourite": true
            }]),
        ]);
        let client = MoodleClient::new(&MoodleConfig {
            base_url: url,
            token: "token".into(),
            allow_write: false,
        });
        let grades = client.grades_overview().unwrap();
        assert_eq!(grades["count"], 1);
        assert_eq!(grades["grades"][0]["courseid"], 1);
        assert_eq!(grades["grades"][0]["grade"], "A");
        worker.join().unwrap();
    }

    #[test]
    fn deadlines_skip_calendar_duplicates_and_outages() {
        let now = unix_now();
        let due = now + 86_400;
        let (url, worker) = server(vec![
            json!({"courses": [{
                "id": 1,
                "shortname": "PHS",
                "assignments": [{
                    "id": 99,
                    "name": "Devoir",
                    "duedate": due,
                    "cmid": 50
                }]
            }]}),
            json!({"quizzes": []}),
            json!({"events": [
                {
                    "id": 1,
                    "name": "Devoir doit être rendu",
                    "modulename": "assign",
                    "instance": 99,
                    "eventtype": "due",
                    "timesort": due
                },
                {
                    "id": 2,
                    "name": "Maintenance",
                    "modulename": "",
                    "eventtype": "auth_outage",
                    "timesort": due
                }
            ]}),
            json!({"events": []}),
        ]);
        let client = MoodleClient::new(&MoodleConfig {
            base_url: url,
            token: "token".into(),
            allow_write: false,
        });
        let deadlines = client.deadlines(7, None).unwrap();
        assert_eq!(deadlines["count"], 1);
        assert_eq!(deadlines["deadlines"][0]["kind"], "assignment");
        assert_eq!(deadlines["deadlines"][0]["id"], 99);
        worker.join().unwrap();
    }

    #[test]
    fn td5_token_split_matches_chapitre_five() {
        assert_eq!(normalize_tokens("TD5"), vec!["td", "5"]);
        assert_eq!(
            normalize_tokens("TD Chapitre 5"),
            vec!["td", "chapitre", "5"]
        );
        assert!(haystack_matches(
            "TD Chapitre 5",
            &[json!({"filename": "TD_Chapitre_5.doc"})],
            &["td".into(), "5".into()]
        ));
        assert!(!haystack_matches(
            "TD Chapitre 6",
            &[],
            &["td".into(), "5".into()]
        ));
        assert!(!haystack_matches(
            "TD Chapitre 6 (Le chap. 6 est enseignée avant le chap. 5)",
            &[],
            &["td".into(), "5".into()]
        ));
    }
}
