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

    /// Upcoming assignment and quiz due dates, sorted by time. Prefer this for due work.
    pub fn deadlines(&self, days: i64, courseids: Option<Vec<i64>>) -> Result<Value, String> {
        let days = days.clamp(1, 365);
        let now = unix_now();
        let until = now + days * 86_400;
        let mut errors = Vec::new();
        let mut names = std::collections::BTreeMap::new();
        let mut course_ids = Vec::new();
        let mut courses_page = Page::default();
        let mut seen_pages = std::collections::HashSet::new();
        loop {
            if !seen_pages.insert(courses_page.page.clone()) || seen_pages.len() > 20 {
                errors.push(json!({
                    "source": "courses",
                    "error": "Canvas course pagination did not finish. Pass courseids to limit the scan."
                }));
                break;
            }
            match self.courses_page(&courses_page) {
                Ok(page) => {
                    for course in as_list(&page["courses"]) {
                        if let Some(id) = course.get("id").and_then(Value::as_i64) {
                            if courseids.as_ref().is_none_or(|filter| filter.contains(&id)) {
                                names.insert(
                                    id,
                                    course
                                        .get("code")
                                        .or_else(|| course.get("name"))
                                        .cloned()
                                        .unwrap_or(Value::Null),
                                );
                                course_ids.push(id);
                            }
                        }
                    }
                    match page["next_page"].as_str() {
                        Some(next) => courses_page.page = next.into(),
                        None => break,
                    }
                }
                Err(error) => {
                    errors.push(json!({"source": "courses", "error": error}));
                    break;
                }
            }
        }
        if let Some(filter) = courseids.as_ref() {
            for id in filter {
                if !course_ids.contains(id) {
                    course_ids.push(*id);
                }
            }
        }
        if course_ids.len() > 40 {
            errors.push(json!({
                "source": "courses",
                "error": "More than 40 courses. Pass courseids to limit the scan."
            }));
            course_ids.truncate(40);
        }
        let mut items = Vec::new();
        let mut seen = std::collections::HashSet::new();
        let mut failed = 0usize;
        for courseid in &course_ids {
            let course = names.get(courseid).cloned().unwrap_or(Value::Null);
            match self.collect_due_assignments(*courseid, now, until) {
                Ok(due) => {
                    for item in due {
                        let key = format!(
                            "{}:{}",
                            item.get("kind").and_then(Value::as_str).unwrap_or(""),
                            item.get("id").map(|v| v.to_string()).unwrap_or_default()
                        );
                        if seen.insert(key) {
                            let mut item = item;
                            item["courseid"] = json!(courseid);
                            item["course"] = course.clone();
                            items.push(item);
                        }
                    }
                }
                Err(error) => {
                    failed += 1;
                    errors.push(json!({"source": courseid, "error": error}));
                }
            }
        }
        match self.calendar(&Page::default()) {
            Ok(cal) => {
                for event in as_list(&cal["events"]) {
                    let due = unix_from_value(
                        event
                            .pointer("/assignment/due_at")
                            .or_else(|| event.get("start")),
                    );
                    let Some(time) = due else { continue };
                    if time < now || time > until {
                        continue;
                    }
                    let assignment_id = event.pointer("/assignment/id");
                    let key = if let Some(id) = assignment_id {
                        format!("assignment:{id}")
                    } else {
                        format!(
                            "calendar:{}",
                            event.get("id").map(|v| v.to_string()).unwrap_or_default()
                        )
                    };
                    if !seen.insert(key) {
                        continue;
                    }
                    items.push(json!({
                        "kind": if assignment_id.is_some() { "assignment" } else { "calendar" },
                        "id": assignment_id.cloned().or_else(|| event.get("id").cloned()),
                        "name": event.get("title"),
                        "courseid": canvas_context_course(event.get("context_code")),
                        "time": time,
                        "due": event.pointer("/assignment/due_at").or_else(|| event.get("start")),
                    }));
                }
            }
            Err(error) => errors.push(json!({"source": "calendar", "error": error})),
        }
        if items.is_empty() && failed > 0 && failed == course_ids.len() {
            return Err(format!("All deadline sources failed: {}", json!(errors)));
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
            "partial": !errors.is_empty(),
            "errors": errors,
            "coverage": "Active-course assignments and Classic Quizzes with due dates in the window, plus Canvas upcoming-events. New Quizzes appear as assignments. canvas_todo is a dashboard feed, not this list.",
            "count": items.len(),
            "deadlines": items
        }))
    }

    fn collect_due_assignments(
        &self,
        courseid: i64,
        now: i64,
        until: i64,
    ) -> Result<Vec<Value>, String> {
        let mut items = Vec::new();
        let mut page = Page::default();
        let mut seen = std::collections::HashSet::new();
        loop {
            if !seen.insert(page.page.clone()) || seen.len() > 5 {
                break;
            }
            let result = self.assignments(courseid, &page)?;
            for assignment in as_list(&result["assignments"]) {
                let Some(time) = unix_from_value(assignment.get("due")) else {
                    continue;
                };
                if time < now || time > until {
                    continue;
                }
                items.push(json!({
                    "kind": if assignment.get("is_quiz_assignment").and_then(Value::as_bool) == Some(true) {
                        "quiz"
                    } else {
                        "assignment"
                    },
                    "id": assignment.get("id"),
                    "name": assignment.get("name"),
                    "time": time,
                    "due": assignment.get("due"),
                }));
            }
            match result["next_page"].as_str() {
                Some(next) => page.page = next.into(),
                None => break,
            }
        }
        page = Page::default();
        seen.clear();
        loop {
            if !seen.insert(page.page.clone()) || seen.len() > 5 {
                break;
            }
            let result = self.list(
                &format!("/api/v1/courses/{courseid}/quizzes"),
                &[],
                &page,
                "quizzes",
            )?;
            for quiz in as_list(&result["quizzes"]) {
                let Some(time) = unix_from_value(quiz.get("due_at").or_else(|| quiz.get("due")))
                else {
                    continue;
                };
                if time < now || time > until {
                    continue;
                }
                items.push(json!({
                    "kind": "quiz",
                    "id": quiz.get("id"),
                    "name": quiz.get("title").or_else(|| quiz.get("name")),
                    "time": time,
                    "due": quiz.get("due_at").or_else(|| quiz.get("due")),
                }));
            }
            match result["next_page"].as_str() {
                Some(next) => page.page = next.into(),
                None => break,
            }
        }
        Ok(items)
    }
}

fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

fn unix_from_value(value: Option<&Value>) -> Option<i64> {
    let value = value?;
    if let Some(n) = value.as_i64().or_else(|| value.as_u64().map(|n| n as i64)) {
        return Some(if n > 10_000_000_000 { n / 1000 } else { n });
    }
    unix_from_iso(value.as_str()?)
}

fn unix_from_iso(raw: &str) -> Option<i64> {
    let s = raw.trim();
    if s.len() < 10 || s.as_bytes().get(4) != Some(&b'-') || s.as_bytes().get(7) != Some(&b'-') {
        return None;
    }
    let year: i64 = s.get(0..4)?.parse().ok()?;
    let month: i64 = s.get(5..7)?.parse().ok()?;
    let day: i64 = s.get(8..10)?.parse().ok()?;
    let (hour, min, sec, rest) =
        if s.len() >= 19 && matches!(s.as_bytes().get(10), Some(b'T' | b' ')) {
            (
                s.get(11..13)?.parse().ok()?,
                s.get(14..16)?.parse().ok()?,
                s.get(17..19)?.parse().ok()?,
                s.get(19..).unwrap_or(""),
            )
        } else {
            (23, 59, 59, "")
        };
    let days = days_from_civil(year, month, day)?;
    let mut unix = days * 86_400 + hour * 3600 + min * 60 + sec;
    let rest = rest.trim_start_matches(|c: char| c == '.' || c.is_ascii_digit());
    if rest.starts_with('+') || rest.starts_with('-') {
        let sign = if rest.starts_with('+') { 1 } else { -1 };
        let hh: i64 = rest.get(1..3)?.parse().ok()?;
        let mm: i64 = rest.get(4..6).and_then(|v| v.parse().ok()).unwrap_or(0);
        unix -= sign * (hh * 3600 + mm * 60);
    }
    Some(unix)
}

fn days_from_civil(year: i64, month: i64, day: i64) -> Option<i64> {
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let yoe = year.rem_euclid(400) as u64;
    let mp = if month > 2 { month - 3 } else { month + 9 };
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy as u64;
    Some(era * 146_097 + doe as i64 - 719_468)
}

fn canvas_context_course(code: Option<&Value>) -> Value {
    code.and_then(Value::as_str)
        .and_then(|code| code.strip_prefix("course_"))
        .and_then(|id| id.parse::<i64>().ok())
        .map(Value::from)
        .unwrap_or(Value::Null)
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
        json!({"name":"canvas_assignments","description":"Assignments, full instructions, own submission summary and due dates for a Canvas course. Use canvas_assignment for the rubric and canvas_assignment_status for feedback/history. Prefer canvas_deadlines for a cross-course due list.","inputSchema":{"type":"object","properties":{"courseid":course},"required":["courseid"]},"annotations":{"readOnlyHint":true}}),
        json!({"name":"canvas_grades","description":"Visible grades for the connected student in a Canvas course.","inputSchema":{"type":"object","properties":{"courseid":course},"required":["courseid"]},"annotations":{"readOnlyHint":true}}),
        json!({"name":"canvas_files","description":"Files in a Canvas course. Pass file id to canvas_file to read a document or obtain a user-openable PDF link in links.download; do not share internal file URLs.","inputSchema":{"type":"object","properties":{"courseid":course},"required":["courseid"]},"annotations":{"readOnlyHint":true}}),
        json!({"name":"canvas_file","description":"Read a Canvas file using fileid from canvas_files. For a user-openable PDF link, use format=file and share links.download; if no link is returned, say so. PDF: original file by default. If Canvas stores the file off-site, open it on the school site instead.","inputSchema":{"type":"object","properties":{"fileid":{"type":"integer","minimum":1},"format":{"type":"string","enum":["file","text","images"]},"find":{"type":"string"},"page_from":{"type":"integer","minimum":1},"page_to":{"type":"integer","minimum":1}},"required":["fileid"]},"annotations":{"readOnlyHint":true}}),
        json!({"name":"canvas_calendar","description":"Upcoming Canvas calendar events for the student. Prefer canvas_deadlines for due work.","annotations":{"readOnlyHint":true}}),
        json!({"name":"canvas_deadlines","description":"Upcoming due work from assignments, Classic Quizzes and the Canvas upcoming-events feed, sorted by date. Prefer this for due work.","inputSchema":{"type":"object","properties":{"days":{"type":"integer","minimum":1,"maximum":365,"description":"How many days ahead to look (default 60)."},"courseids":{"type":"array","items":{"type":"integer","minimum":1}}}},"annotations":{"readOnlyHint":true}}),
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
            "canvas_deadlines",
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
    fn iso_due_dates_convert_to_unix() {
        assert_eq!(unix_from_iso("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(unix_from_iso("1970-01-01T00:00:00+00:00"), Some(0));
        assert!(unix_from_iso("2026-02-01T12:00:00Z").unwrap() > 1_700_000_000);
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
