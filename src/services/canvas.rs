//! Canvas LMS (Instructure) read-only student tools.
//! Sign-in uses the school site in a browser profile. Student access tokens are
//! not used: they expire and are against Instructure's student-token policy.
use crate::config::CanvasConfig;
use crate::session;
use serde_json::{json, Value};
use std::io::Read;
use std::sync::Arc;
use std::time::Duration;

#[path = "canvas/student_tools.rs"]
mod student_tools;
pub(crate) use student_tools::dispatch;

/// Canvas page values can be opaque bookmarks, not just integer offsets.
pub(crate) struct Page {
    page: String,
    per_page: u64,
}

impl Default for Page {
    fn default() -> Self {
        Self {
            page: "1".into(),
            per_page: 50,
        }
    }
}

impl Page {
    pub(crate) fn from_args(args: &Value) -> Result<Self, String> {
        let page = match args.get("page") {
            None => "1".into(),
            Some(Value::String(page)) if valid_page(page) => page.clone(),
            _ => {
                return Err(
                    "Provide page as a non-empty Canvas page string (max 1024 bytes).".into(),
                )
            }
        };
        let per_page = match args.get("per_page") {
            None => 50,
            Some(value) => value
                .as_u64()
                .filter(|n| (1..=100).contains(n))
                .ok_or("Provide integer per_page between 1 and 100.")?,
        };
        Ok(Self { page, per_page })
    }
}

fn valid_page(page: &str) -> bool {
    !page.trim().is_empty() && page.len() <= 1024 && !page.chars().any(char::is_control)
}

pub struct CanvasClient {
    config: CanvasConfig,
    agent: ureq::Agent,
    browser: crate::browser::Connection,
}

impl CanvasClient {
    pub fn new(config: CanvasConfig) -> Self {
        Self {
            browser: crate::browser::Connection::new("canvas", &config.base_url, &config.token),
            agent: ureq::AgentBuilder::new()
                .timeout(Duration::from_secs(30))
                .redirects(0)
                .user_agent(session::BROWSER_UA)
                .build(),
            config,
        }
    }

    fn get(&self, path: &str, query: &[(&str, String)]) -> Result<Value, String> {
        self.get_response(path, query).map(|(value, _)| value)
    }

    fn get_response(
        &self,
        path: &str,
        query: &[(&str, String)],
    ) -> Result<(Value, String), String> {
        if self.browser.enabled() {
            let mut url = url::Url::parse(&format!("{}{path}", self.config.base_url))
                .map_err(|_| "Invalid Canvas URL.")?;
            url.query_pairs_mut()
                .extend_pairs(query.iter().map(|(k, v)| (*k, v)));
            let reply =
                self.browser
                    .request("GET", url.as_str(), None, None, false, 2 * 1024 * 1024)?;
            let link = reply.link.clone();
            return reply
                .json()
                .map(|value| (crate::redact::scrub(value, &self.config.token), link));
        }
        let mut request = self
            .agent
            .get(&format!("{}{path}", self.config.base_url))
            .set("Accept", "application/json")
            .set("Cookie", &self.config.token);
        for (key, value) in query {
            request = request.query(key, value);
        }
        let response = request.call().map_err(|error| match error {
            ureq::Error::Status(401, _) => {
                "Canvas session expired or refused. Sign in again in the app.".into()
            }
            ureq::Error::Status(403, _) => {
                "Canvas denied access. Check course permissions; reconnect if the session expired."
                    .into()
            }
            ureq::Error::Status(404, _) => "Canvas resource not found.".into(),
            ureq::Error::Status(code, _) => format!("Canvas returned HTTP error {code}."),
            _ => "Cannot reach Canvas. Check the site address and your Internet connection.".into(),
        })?;
        if response.status() != 200 {
            return Err("Canvas redirected the request. Sign in again in the app.".into());
        }
        let link = response.all("Link").join(", ");
        let mut bytes = Vec::new();
        response
            .into_reader()
            .take(2 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| "Cannot read Canvas JSON response.".to_string())?;
        if bytes.len() > 2 * 1024 * 1024 {
            return Err("Canvas response exceeded 2 MB.".into());
        }
        let value: Value = serde_json::from_slice(&bytes)
            .map_err(|_| "Canvas did not return JSON. Sign in again in the app.".to_string())?;
        Ok((crate::redact::scrub(value, &self.config.token), link))
    }

    /// One bounded API page. Never infer completion from the number of items:
    /// Canvas can cap per_page or filter a page after pagination.
    fn list(
        &self,
        path: &str,
        query: &[(&str, String)],
        page: &Page,
        key: &str,
    ) -> Result<Value, String> {
        let mut query = query.to_vec();
        query.extend([
            ("page", page.page.clone()),
            ("per_page", page.per_page.to_string()),
        ]);
        let (value, link) = self.get_response(path, &query)?;
        if !value.is_array() {
            return Err("Canvas did not return a list.".into());
        }
        let next_page = next_page(&self.config.base_url, path, &link)?;
        if next_page.as_deref() == Some(&page.page) {
            return Err("Canvas repeated the current page bookmark.".into());
        }
        Ok(
            json!({key: value, "page": page.page, "per_page": page.per_page, "next_page": next_page}),
        )
    }

    pub fn whoami(&self) -> Result<Value, String> {
        let user = self.get("/api/v1/users/self", &[])?;
        if user.get("id").is_none_or(Value::is_null) {
            return Err("Canvas did not identify the connected account.".into());
        }
        Ok(json!({
            "id": user.get("id"),
            "name": user.get("name"),
            "login": user.get("login_id"),
        }))
    }

    pub fn courses(&self) -> Result<Value, String> {
        self.courses_page(&Page::default())
    }

    pub(crate) fn courses_page(&self, page: &Page) -> Result<Value, String> {
        let mut result = self.list(
            "/api/v1/courses",
            &[("enrollment_state", "active".into())],
            page,
            "courses",
        )?;
        result["courses"] = json!(summarize_courses(&result["courses"]));
        Ok(result)
    }

    pub(crate) fn announcements(
        &self,
        courseid: Option<i64>,
        page: &Page,
    ) -> Result<Value, String> {
        let mut query = vec![];
        if let Some(id) = courseid {
            require_id(id)?;
            query.push(("context_codes[]", format!("course_{id}")));
        } else {
            let mut courses_page = Page::default();
            let mut seen = std::collections::HashSet::new();
            loop {
                if !seen.insert(courses_page.page.clone()) || seen.len() > 20 {
                    return Err("Canvas course pagination did not finish. Provide courseid for announcements.".into());
                }
                let courses = self.courses_page(&courses_page)?;
                for course in as_list(&courses["courses"]) {
                    if let Some(id) = course.get("id").and_then(Value::as_i64) {
                        query.push(("context_codes[]", format!("course_{id}")));
                    }
                }
                if query.len() > 100 {
                    return Err(
                        "More than 100 active courses. Provide courseid for announcements.".into(),
                    );
                }
                match courses["next_page"].as_str() {
                    Some(next) => courses_page.page = next.into(),
                    None => break,
                }
            }
            if query.is_empty() {
                return Ok(
                    json!({ "announcements": [], "page": page.page, "per_page": page.per_page, "next_page": null }),
                );
            }
        }
        let mut result = self.list("/api/v1/announcements", &query, page, "announcements")?;
        result["announcements"] = json!(summarize_announcements(&result["announcements"]));
        result["date_scope"] = json!("Canvas default: announcements from the last two weeks.");
        Ok(result)
    }

    pub(crate) fn assignments(&self, courseid: i64, page: &Page) -> Result<Value, String> {
        require_id(courseid)?;
        let mut result = self.list(
            &format!("/api/v1/courses/{courseid}/assignments"),
            &[
                ("order_by", "due_at".into()),
                ("include[]", "submission".into()),
            ],
            page,
            "assignments",
        )?;
        result["assignments"] = json!(summarize_assignments(&result["assignments"]));
        Ok(result)
    }

    pub(crate) fn grades(&self, courseid: i64, page: &Page) -> Result<Value, String> {
        require_id(courseid)?;
        let mut result = self.list(
            &format!("/api/v1/courses/{courseid}/enrollments"),
            &[("user_id", "self".into())],
            page,
            "grades",
        )?;
        result["grades"] = json!(summarize_grades(&result["grades"]));
        Ok(result)
    }

    pub(crate) fn files(&self, courseid: i64, page: &Page) -> Result<Value, String> {
        require_id(courseid)?;
        let mut result = self.list(
            &format!("/api/v1/courses/{courseid}/files"),
            &[("sort", "updated_at".into())],
            page,
            "files",
        )?;
        result["files"] = json!(summarize_files(&result["files"]));
        Ok(result)
    }

    pub fn download_file(
        &self,
        fileid: i64,
        query: &crate::delivery::FileQuery,
    ) -> Result<Value, String> {
        require_id(fileid)?;
        let meta = self.get(&format!("/api/v1/files/{fileid}"), &[])?;
        let filename = meta
            .get("filename")
            .and_then(Value::as_str)
            .unwrap_or("file")
            .to_string();
        let path = format!("/files/{fileid}/download");
        let (mime, bytes) = if self.browser.enabled() {
            let reply = self
                .browser
                .request("GET", &path, None, None, true, 12 * 1024 * 1024)?;
            if reply.status != 200 {
                return Err("Canvas redirected this file. Open it on the school site.".into());
            }
            (reply.mime, reply.bytes)
        } else {
            let url = format!("{}{path}", self.config.base_url);
            let response = self
                .agent
                .get(&url)
                .set("Accept", "*/*")
                .set("Cookie", &self.config.token)
                .call()
                .map_err(|_| "Cannot download this Canvas file.".to_string())?;
            if response.status() != 200 {
                return Err("Canvas redirected this file. Open it on the school site.".into());
            }
            let mime = response.content_type().to_string();
            let mut bytes = Vec::new();
            response
                .into_reader()
                .take(12 * 1024 * 1024 + 1)
                .read_to_end(&mut bytes)
                .map_err(|_| "Cannot read this Canvas file.".to_string())?;
            if bytes.len() > 12 * 1024 * 1024 {
                return Err(
                    "This document is larger than 12 MB. Open it in Canvas instead.".into(),
                );
            }
            (mime, bytes)
        };
        let prefix = String::from_utf8_lossy(&bytes[..bytes.len().min(1024)]).to_ascii_lowercase();
        if mime.contains("html")
            || mime.contains("json")
            || prefix.trim_start().starts_with("<!doctype html")
            || prefix.trim_start().starts_with("<html")
        {
            return Err(
                "Canvas returned a web page instead of a file. Sign in again or open it on the school site.".into(),
            );
        }
        let mut result = crate::delivery::read_document(
            &filename,
            &mime,
            Arc::new(bytes),
            query,
            &self.config.token,
        )?;
        if let Some(note) = result.get("note").and_then(Value::as_str) {
            result["note"] = json!(note.replace("moodle_file", "canvas_file"));
        }
        Ok(result)
    }

    pub(crate) fn calendar(&self, page: &Page) -> Result<Value, String> {
        let mut result = self.list("/api/v1/users/self/upcoming_events", &[], page, "events")?;
        result["events"] = json!(summarize_events(&result["events"]));
        result["scope"] = json!("Canvas upcoming-events window, not the full calendar history.");
        Ok(result)
    }
}

fn next_page(origin: &str, path: &str, link: &str) -> Result<Option<String>, String> {
    for part in link.split(',') {
        let Some((target, parameters)) = part.trim().split_once('>') else {
            continue;
        };
        let is_next = parameters
            .split(';')
            .filter_map(|p| p.trim().split_once('='))
            .any(|(key, value)| {
                key.trim().eq_ignore_ascii_case("rel")
                    && value
                        .trim()
                        .trim_matches('"')
                        .split_whitespace()
                        .any(|rel| rel == "next")
            });
        if !is_next {
            continue;
        }
        let base = url::Url::parse(origin).map_err(|_| "Invalid Canvas URL.")?;
        let url = target
            .strip_prefix('<')
            .and_then(|s| base.join(s).ok())
            .ok_or("Canvas returned an invalid next-page link.")?;
        // Only extract the bookmark. Never fetch a server-supplied URL or copy
        // its credentials, filters, or impersonation parameters into a request.
        if url.origin() != base.origin()
            || url.path().trim_end_matches(".json") != path
            || !url.username().is_empty()
            || url.password().is_some()
        {
            return Err("Canvas returned a next-page link outside this endpoint.".into());
        }
        let page = url
            .query_pairs()
            .find(|(key, _)| key == "page")
            .map(|(_, value)| value.into_owned())
            .filter(|value| valid_page(value))
            .ok_or("Canvas returned an unsupported next-page bookmark.".to_string())?;
        return Ok(Some(page));
    }
    Ok(None)
}

fn require_id(id: i64) -> Result<(), String> {
    if id <= 0 {
        return Err("Provide an integer id > 0.".into());
    }
    Ok(())
}

fn as_list(value: &Value) -> &[Value] {
    value.as_array().map(Vec::as_slice).unwrap_or(&[])
}

fn summarize_courses(value: &Value) -> Vec<Value> {
    as_list(value)
        .iter()
        .map(|course| {
            json!({
                "id": course.get("id"),
                "name": course.get("name"),
                "code": course.get("course_code"),
                "start": course.get("start_at"),
                "end": course.get("end_at"),
            })
        })
        .collect()
}

fn summarize_announcements(value: &Value) -> Vec<Value> {
    as_list(value)
        .iter()
        .map(|post| {
            json!({
                "id": post.get("id"),
                "title": post.get("title"),
                "posted": post.get("posted_at"),
                "course": post.get("context_code"),
                "message": post.get("message"),
                "html_url": post.get("html_url"),
                "attachments": post.get("attachments"),
            })
        })
        .collect()
}

fn summarize_assignments(value: &Value) -> Vec<Value> {
    as_list(value)
        .iter()
        .map(|item| {
            json!({
                "id": item.get("id"),
                "name": item.get("name"),
                "due": item.get("due_at"),
                "points": item.get("points_possible"),
                "description": item.get("description"),
                "submission_types": item.get("submission_types"),
                "submission": item.get("submission"),
                "html_url": item.get("html_url"),
                "unlock_at": item.get("unlock_at"),
                "lock_at": item.get("lock_at"),
                "locked_for_user": item.get("locked_for_user"),
                "lock_explanation": item.get("lock_explanation"),
                "is_quiz_assignment": item.get("is_quiz_assignment"),
            })
        })
        .collect()
}

fn summarize_grades(value: &Value) -> Vec<Value> {
    as_list(value)
        .iter()
        .map(|row| {
            json!({
                "type": row.get("type"),
                "score": row.pointer("/grades/current_score"),
                "grade": row.pointer("/grades/current_grade"),
                "final": row.pointer("/grades/final_score"),
            })
        })
        .collect()
}

fn summarize_files(value: &Value) -> Vec<Value> {
    as_list(value)
        .iter()
        .map(|file| {
            json!({
                "id": file.get("id"),
                "filename": file.get("filename"),
                "size": file.get("size"),
                "updated": file.get("updated_at"),
            })
        })
        .collect()
}

fn summarize_events(value: &Value) -> Vec<Value> {
    as_list(value)
        .iter()
        .map(|event| {
            json!({
                "title": event.get("title"),
                "id": event.get("id"),
                "description": event.get("description"),
                "context_code": event.get("context_code"),
                "assignment": event.get("assignment"),
                "start": event.get("start_at"),
                "end": event.get("end_at"),
                "type": event.get("type"),
            })
        })
        .collect()
}

pub fn tools() -> Vec<Value> {
    let course = json!({"type":"integer","minimum":1});
    let mut tools = vec![
        json!({"name":"canvas_status","description":"Check the connected Canvas account.","annotations":{"readOnlyHint":true}}),
        json!({"name":"canvas_my_courses","description":"List the student's active Canvas courses.","annotations":{"readOnlyHint":true}}),
        json!({"name":"canvas_announcements","description":"Recent course announcements with full message HTML (Canvas defaults to the last two weeks). Pass courseid to limit to one course. Older announcements can be read by topicid with canvas_discussion.","inputSchema":{"type":"object","properties":{"courseid":course}},"annotations":{"readOnlyHint":true}}),
        json!({"name":"canvas_assignments","description":"Assignments, full instructions, own submission summary and due dates for a Canvas course. Use canvas_assignment for the rubric and canvas_assignment_status for feedback/history.","inputSchema":{"type":"object","properties":{"courseid":course},"required":["courseid"]},"annotations":{"readOnlyHint":true}}),
        json!({"name":"canvas_grades","description":"Visible grades for the connected student in a Canvas course.","inputSchema":{"type":"object","properties":{"courseid":course},"required":["courseid"]},"annotations":{"readOnlyHint":true}}),
        json!({"name":"canvas_files","description":"Files in a Canvas course. Pass file id to canvas_file to read a document or obtain a user-openable PDF link in links.download; do not share internal file URLs.","inputSchema":{"type":"object","properties":{"courseid":course},"required":["courseid"]},"annotations":{"readOnlyHint":true}}),
        json!({"name":"canvas_file","description":"Read a Canvas file using fileid from canvas_files. For a user-openable PDF link, use format=file and share links.download; if no link is returned, say so. PDF: original file by default. If Canvas stores the file off-site, open it on the school site instead.","inputSchema":{"type":"object","properties":{"fileid":{"type":"integer","minimum":1},"format":{"type":"string","enum":["file","text","images"]},"find":{"type":"string"},"page_from":{"type":"integer","minimum":1},"page_to":{"type":"integer","minimum":1}},"required":["fileid"]},"annotations":{"readOnlyHint":true}}),
        json!({"name":"canvas_calendar","description":"Upcoming Canvas calendar events for the student.","annotations":{"readOnlyHint":true}}),
    ];
    for tool in &mut tools {
        if matches!(
            tool["name"].as_str(),
            Some(
                "canvas_my_courses"
                    | "canvas_announcements"
                    | "canvas_assignments"
                    | "canvas_grades"
                    | "canvas_files"
                    | "canvas_calendar"
            )
        ) {
            if tool.get("inputSchema").is_none() {
                tool["inputSchema"] = json!({"type":"object","properties":{}});
            }
            student_tools::add_paging(tool);
        }
    }
    tools.extend(student_tools::tools());
    tools
}

pub fn activate(
    settings: &mut crate::config::Settings,
    url: &str,
    token: &str,
) -> Result<String, String> {
    let base_url = crate::config::normalize_canvas_url(url)?;
    let client = CanvasClient::new(CanvasConfig {
        base_url: base_url.clone(),
        token: token.into(),
    });
    client.whoami()?;
    let courses = client.courses()?;
    if courses.get("courses").and_then(Value::as_array).is_none() {
        return Err("Canvas did not return course enrollments.".into());
    }
    let mut updated = settings.clone();
    updated.canvas = Some(crate::config::CanvasSettings {
        url: Some(base_url),
        token: Some(token.into()),
    });
    updated.save_service("canvas").map_err(|error| {
        format!("Cannot save Canvas credentials to the private config file: {error}")
    })?;
    *settings = updated;
    Ok("Canvas connected.".into())
}

#[cfg(test)]
#[path = "canvas/tests.rs"]
mod student_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use tiny_http::{Header, Response, Server};

    fn server(
        replies: Vec<(u16, &'static str, &'static [u8])>,
    ) -> (CanvasConfig, std::thread::JoinHandle<()>) {
        let http = Server::http("127.0.0.1:0").unwrap();
        let origin = format!("http://{}", http.server_addr());
        let worker = std::thread::spawn(move || {
            for (status, mime, body) in replies {
                let request = http.recv().unwrap();
                let mut response = Response::from_data(body.to_vec()).with_status_code(status);
                response.add_header(Header::from_bytes("Content-Type", mime).unwrap());
                let _ = request.respond(response);
            }
        });
        (
            CanvasConfig {
                base_url: origin,
                token: "canvas_session=fixture".into(),
            },
            worker,
        )
    }

    #[test]
    fn read_tools_are_read_only_and_cover_the_student_surface() {
        let names: Vec<_> = tools()
            .into_iter()
            .map(|tool| tool["name"].as_str().unwrap().to_string())
            .collect();
        for required in [
            "canvas_status",
            "canvas_my_courses",
            "canvas_announcements",
            "canvas_assignments",
            "canvas_grades",
            "canvas_files",
            "canvas_file",
            "canvas_calendar",
            "canvas_course",
            "canvas_modules",
            "canvas_module_items",
            "canvas_pages",
            "canvas_page",
            "canvas_assignment",
            "canvas_assignment_status",
            "canvas_quizzes",
            "canvas_quiz_status",
            "canvas_discussions",
            "canvas_discussion",
            "canvas_discussion_entries",
            "canvas_discussion_replies",
            "canvas_conversations",
            "canvas_conversation",
            "canvas_todo",
        ] {
            assert!(
                names.iter().any(|name| name == required),
                "missing {required}"
            );
        }
        for tool in tools() {
            assert_eq!(tool["annotations"]["readOnlyHint"], true);
            assert_ne!(tool["annotations"]["readOnlyHint"], false);
        }
    }

    #[test]
    fn student_reads_go_through_the_session_and_never_ask_for_a_token() {
        let (config, worker) = server(vec![
            (200, "application/json", br#"{"id":9,"name":"Ada","login_id":"ada"}"#),
            (200, "application/json", br#"[{"id":1,"name":"Algo","course_code":"INF100"}]"#),
            (200, "application/json", br#"[{"id":2,"title":"Hello","posted_at":"2026-01-01","context_code":"course_1"}]"#),
            (200, "application/json", br#"[{"id":3,"name":"TP1","due_at":"2026-02-01","points_possible":10}]"#),
            (200, "application/json", br#"[{"type":"StudentEnrollment","grades":{"current_score":18,"current_grade":"A","final_score":18}}]"#),
            (200, "application/json", br#"[{"id":4,"filename":"notes.pdf","size":12,"updated_at":"2026-01-02"}]"#),
            (200, "application/json", br#"{"id":4,"filename":"notes.pdf"}"#),
            (200, "application/pdf", b"%PDF-1.4 fixture"),
            (200, "application/json", br#"[{"title":"Exam","start_at":"2026-03-01","type":"event"}]"#),
        ]);
        let client = CanvasClient::new(config);
        assert_eq!(client.whoami().unwrap()["id"], 9);
        assert_eq!(client.courses().unwrap()["courses"][0]["code"], "INF100");
        assert_eq!(
            client.announcements(Some(1), &Page::default()).unwrap()["announcements"][0]["title"],
            "Hello"
        );
        assert_eq!(
            client.assignments(1, &Page::default()).unwrap()["assignments"][0]["due"],
            "2026-02-01"
        );
        assert_eq!(
            client.grades(1, &Page::default()).unwrap()["grades"][0]["score"],
            18
        );
        assert_eq!(
            client.files(1, &Page::default()).unwrap()["files"][0]["id"],
            4
        );
        let file = client
            .download_file(4, &crate::delivery::FileQuery::default())
            .unwrap();
        assert!(file["filename"].as_str().unwrap().contains("notes"));
        assert_eq!(
            client.calendar(&Page::default()).unwrap()["events"][0]["title"],
            "Exam"
        );
        worker.join().unwrap();
    }

    #[test]
    fn a_login_page_is_not_a_file() {
        let (config, worker) = server(vec![
            (
                200,
                "application/json",
                br#"{"id":4,"filename":"notes.pdf"}"#,
            ),
            (200, "text/html", b"<html>login</html>"),
        ]);
        let error = CanvasClient::new(config)
            .download_file(4, &crate::delivery::FileQuery::default())
            .unwrap_err();
        assert!(error.contains("web page"), "{error}");
        worker.join().unwrap();
    }
}
