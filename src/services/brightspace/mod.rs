//! Read-only Brightspace Valence API.
//! Session cookies pasted from the browser after SSO, or an OAuth token if the school issued one.
//! https://docs.valence.desire2learn.com/
use crate::config::BrightspaceConfig;
use crate::session;
use serde_json::{json, Value};
use std::io::Read;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const LP: &str = "1.49";
const LE: &str = "1.82";

pub struct BrightspaceClient {
    config: BrightspaceConfig,
    agent: ureq::Agent,
}

impl BrightspaceClient {
    pub fn new(config: BrightspaceConfig) -> Self {
        Self {
            config,
            // Do not forward credentials to login pages or redirected hosts.
            agent: ureq::AgentBuilder::new()
                .timeout(Duration::from_secs(30))
                .redirects(0)
                .user_agent(session::BROWSER_UA)
                .build(),
        }
    }

    fn authorize(&self, request: ureq::Request) -> ureq::Request {
        let request = request.set("Accept", "application/json");
        if session::session_cookie_auth(&self.config.token) {
            let mut request = request.set("Cookie", &self.config.token);
            if let Some(csrf) = session::csrf_from_cookies(&self.config.token) {
                request = request.set("X-Csrf-Token", &csrf);
            }
            request
        } else {
            request.set("Authorization", &format!("Bearer {}", self.config.token))
        }
    }

    fn get(&self, path: &str, query: &[(&str, String)]) -> Result<Value, String> {
        let mut request =
            self.authorize(self.agent.get(&format!("{}{path}", self.config.base_url)));
        for (key, value) in query {
            request = request.query(key, value);
        }
        let cookie_auth = session::session_cookie_auth(&self.config.token);
        let response = request.call().map_err(|error| match error {
            ureq::Error::Status(401, _) if cookie_auth => {
                "Brightspace session expired or refused. Sign in again in the app.".into()
            }
            ureq::Error::Status(401, _) => {
                "Brightspace token expired or refused. Replace it in the app.".into()
            }
            ureq::Error::Status(403, _) => {
                "Brightspace denied access. Check the saved session and your institution's permissions.".into()
            }
            ureq::Error::Status(404, _) => "Brightspace resource not found.".into(),
            ureq::Error::Status(code, _) => format!("Brightspace returned HTTP error {code}."),
            _ => "Cannot reach Brightspace. Check the site address and your Internet connection.".into(),
        })?;
        if response.status() != 200 {
            return Err(
                "Brightspace redirected the request. Use the institution's API site address."
                    .into(),
            );
        }
        let value: Value = response
            .into_json()
            .map_err(|_| "Brightspace did not return JSON. Check the site address.".to_string())?;
        Ok(crate::moodle::scrub(value, &self.config.token))
    }

    pub fn whoami(&self) -> Result<Value, String> {
        let info = self.get(&format!("/d2l/api/lp/{LP}/users/whoami"), &[])?;
        if info.get("Identifier").is_none_or(Value::is_null) {
            return Err("Brightspace did not identify the connected account.".into());
        }
        Ok(info)
    }

    /// Return one page, including PagingInfo so callers can request subsequent pages.
    pub fn courses(&self, bookmark: Option<&str>) -> Result<Value, String> {
        let mut pairs = vec![("orgUnitTypeId", "3".into())];
        if let Some(bookmark) = bookmark {
            pairs.push(("bookmark", bookmark.into()));
        }
        self.get(
            &format!("/d2l/api/lp/{LP}/enrollments/myenrollments/"),
            &pairs,
        )
    }

    pub fn content(&self, courseid: i64) -> Result<Value, String> {
        self.course_get(courseid, "content/toc")
    }

    pub fn download_file(
        &self,
        courseid: i64,
        topicid: i64,
        query: &crate::moodle::FileQuery,
    ) -> Result<Value, String> {
        if courseid <= 0 || topicid <= 0 {
            return Err("Provide integer courseid and topicid > 0.".into());
        }
        // Construct the endpoint ourselves: credentials never follow caller-controlled URLs.
        let url = format!(
            "{}/d2l/api/le/{LE}/{courseid}/content/topics/{topicid}/file",
            self.config.base_url
        );
        let response = self.authorize(self.agent.get(&url))
            .set("Accept", "*/*")
            .call()
            .map_err(|error| match error {
                ureq::Error::Status(401, _) => "Brightspace session or token expired or refused. Reconnect in the app.".to_string(),
                ureq::Error::Status(403, _) => "Brightspace denied access to this document.".into(),
                ureq::Error::Status(404, _) => "Brightspace document not found. Use a file topic Id from brightspace_course_content, not a module or external link.".into(),
                ureq::Error::Status(code, _) => format!("Brightspace file returned HTTP error {code}."),
                _ => "Cannot download this Brightspace document.".into(),
            })?;
        if response.status() != 200 {
            return Err("Brightspace redirected the file request. Open the document in your browser; credentials were not forwarded.".into());
        }
        let mime = response.content_type().to_string();
        let filename = crate::moodle::filename_from(response.header("Content-Disposition"), &url);
        const MAX_BYTES: u64 = 12 * 1024 * 1024;
        let mut bytes = Vec::new();
        response
            .into_reader()
            .take(MAX_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| "Cannot read this Brightspace document.".to_string())?;
        if bytes.len() as u64 > MAX_BYTES {
            return Err(
                "This document is larger than 12 MB. Open it in Brightspace instead.".into(),
            );
        }
        // A successful HTTP response can still be an SSO login page or API error.
        let prefix = String::from_utf8_lossy(&bytes[..bytes.len().min(1024)]).to_ascii_lowercase();
        if mime.contains("html")
            || mime.contains("json")
            || prefix.trim_start().starts_with("<!doctype html")
            || prefix.trim_start().starts_with("<html")
        {
            return Err("Brightspace returned a web page or API response instead of a file. Reconnect or open this topic in your browser.".into());
        }
        let mut result = crate::moodle::MoodleClient::read_document(
            &filename,
            &mime,
            Arc::new(bytes),
            query,
            &self.config.token,
        )?;
        if let Some(note) = result.get("note").and_then(Value::as_str) {
            result["note"] = json!(note.replace("moodle_file", "brightspace_file"));
        }
        Ok(result)
    }

    pub fn grades(&self, courseid: i64) -> Result<Value, String> {
        self.course_get(courseid, "grades/values/myGradeValues/")
    }

    pub fn assignments(&self, courseid: i64) -> Result<Value, String> {
        let folders = self.course_get(courseid, "dropbox/folders/")?;
        Ok(summarize_assignments(courseid, &folders))
    }

    pub fn assignment_status(&self, courseid: i64, folderid: i64) -> Result<Value, String> {
        if courseid <= 0 || folderid <= 0 {
            return Err("Provide integer courseid and folderid > 0.".into());
        }
        let folder = self.course_get(courseid, &format!("dropbox/folders/{folderid}"))?;
        let submissions = self
            .course_get(
                courseid,
                &format!("dropbox/folders/{folderid}/submissions/mysubmissions/"),
            )
            .unwrap_or_else(|_| json!([]));
        Ok(json!({
            "courseid": courseid,
            "folder": compact_assignment(&folder),
            "mysubmissions": submissions,
        }))
    }

    pub fn quizzes(&self, courseid: i64) -> Result<Value, String> {
        let page = self.course_get(courseid, "quizzes/")?;
        Ok(summarize_quizzes(courseid, &page))
    }

    pub fn quiz_status(&self, courseid: i64, quizid: i64) -> Result<Value, String> {
        if courseid <= 0 || quizid <= 0 {
            return Err("Provide integer courseid and quizid > 0.".into());
        }
        let quiz = self.course_get(courseid, &format!("quizzes/{quizid}"))?;
        let mut attempts = json!({"Objects": []});
        if let Ok(user) = self.whoami() {
            if let Some(uid) = user.get("Identifier").and_then(Value::as_str) {
                if let Ok(page) = self.get(
                    &format!("/d2l/api/le/{LE}/{courseid}/quizzes/{quizid}/attempts/"),
                    &[("userId", uid.into())],
                ) {
                    attempts = page;
                }
            }
        }
        Ok(json!({
            "courseid": courseid,
            "quiz": compact_quiz(&quiz),
            "attempts": attempts,
        }))
    }

    pub fn calendar(&self, courseid: Option<i64>, days: i64) -> Result<Value, String> {
        let days = days.clamp(1, 365);
        let (start, end) = window_utc(days);
        let course_ids = match courseid {
            Some(id) if id > 0 => vec![id],
            Some(_) => return Err("Provide an integer courseid > 0.".into()),
            None => self.course_ids()?,
        };
        if course_ids.is_empty() {
            return Ok(json!({"days": days, "events": []}));
        }
        let mut events = Vec::new();
        // Brightspace caps multi-OU queries; batch a few courses per request.
        for chunk in course_ids.chunks(10) {
            let csv = chunk
                .iter()
                .map(|id| id.to_string())
                .collect::<Vec<_>>()
                .join(",");
            let page = self.get(
                &format!("/d2l/api/le/{LE}/calendar/events/myEvents/"),
                &[
                    ("orgUnitIdsCSV", csv),
                    ("startDateTime", start.clone()),
                    ("endDateTime", end.clone()),
                ],
            )?;
            push_list_items(&mut events, &page);
        }
        Ok(json!({
            "days": days,
            "startDateTime": start,
            "endDateTime": end,
            "events": events.into_iter().map(compact_event).collect::<Vec<_>>(),
        }))
    }

    pub fn deadlines(&self, days: i64, courseids: Option<Vec<i64>>) -> Result<Value, String> {
        let days = days.clamp(1, 365);
        let names = self.course_labels()?;
        let course_ids = match courseids {
            Some(ids) if !ids.is_empty() => ids,
            _ => names.keys().copied().collect(),
        };
        let now = utc_now();
        let end = utc_plus_days(days);
        let mut items = Vec::new();
        for courseid in course_ids {
            let course_name = names
                .get(&courseid)
                .cloned()
                .unwrap_or(Value::Null);
            if let Ok(folders) = self.course_get(courseid, "dropbox/folders/") {
                for folder in as_array(&folders) {
                    if folder.get("IsHidden").and_then(Value::as_bool) == Some(true) {
                        continue;
                    }
                    if let Some(due) = folder.get("DueDate").and_then(Value::as_str) {
                        if due >= now.as_str() && due <= end.as_str() {
                            items.push(json!({
                                "type": "assignment",
                                "courseid": courseid,
                                "course": course_name,
                                "id": folder.get("Id"),
                                "name": folder.get("Name"),
                                "due": due,
                            }));
                        }
                    }
                }
            }
            if let Ok(page) = self.course_get(courseid, "quizzes/") {
                for quiz in list_objects(&page) {
                    if quiz.get("IsActive").and_then(Value::as_bool) == Some(false) {
                        continue;
                    }
                    if let Some(due) = quiz.get("DueDate").and_then(Value::as_str) {
                        if due >= now.as_str() && due <= end.as_str() {
                            items.push(json!({
                                "type": "quiz",
                                "courseid": courseid,
                                "course": course_name,
                                "id": quiz.get("QuizId").or_else(|| quiz.get("Id")),
                                "name": quiz.get("Name"),
                                "due": due,
                            }));
                        }
                    }
                }
            }
        }
        if let Ok(cal) = self.calendar(None, days) {
            if let Some(events) = cal.get("events").and_then(Value::as_array) {
                for event in events {
                    let event_type = event.get("EventType").and_then(Value::as_i64).unwrap_or(0);
                    // 6 = DueDate in Valence EVENTTYPE_T
                    if event_type != 0 && event_type != 6 {
                        continue;
                    }
                    let due = event
                        .get("StartDateTime")
                        .and_then(Value::as_str)
                        .or_else(|| event.get("StartDay").and_then(Value::as_str));
                    let Some(due) = due else { continue };
                    if due < now.as_str() || due > end.as_str() {
                        continue;
                    }
                    items.push(json!({
                        "type": "calendar",
                        "courseid": event.get("OrgUnitId"),
                        "course": event.get("OrgUnitName").or_else(|| event.get("OrgUnitCode")),
                        "id": event.get("CalendarEventId"),
                        "name": event.get("Title"),
                        "due": due,
                        "eventType": event.get("EventType"),
                    }));
                }
            }
        }
        items.sort_by(|a, b| {
            let left = a.get("due").and_then(Value::as_str).unwrap_or("");
            let right = b.get("due").and_then(Value::as_str).unwrap_or("");
            left.cmp(right)
        });
        // Drop obvious calendar duplicates of assignment/quiz dues already listed.
        let mut seen = std::collections::HashSet::new();
        items.retain(|item| {
            let key = format!(
                "{}|{}|{}",
                item.get("type").and_then(Value::as_str).unwrap_or(""),
                item.get("courseid").map(|v| v.to_string()).unwrap_or_default(),
                item.get("name").and_then(Value::as_str).unwrap_or("")
            );
            seen.insert(key)
        });
        Ok(json!({
            "days": days,
            "from": now,
            "until": end,
            "items": items,
        }))
    }

    pub fn news(&self, courseid: i64) -> Result<Value, String> {
        let items = self.course_get(courseid, "news/")?;
        Ok(json!({
            "courseid": courseid,
            "items": as_array(&items).into_iter().map(compact_news).collect::<Vec<_>>(),
        }))
    }

    fn course_get(&self, courseid: i64, resource: &str) -> Result<Value, String> {
        if courseid <= 0 {
            return Err("Provide an integer courseid > 0.".into());
        }
        self.get(
            &format!("/d2l/api/le/{LE}/{courseid}/{resource}"),
            &[],
        )
    }

    fn course_ids(&self) -> Result<Vec<i64>, String> {
        Ok(self.course_labels()?.keys().copied().collect())
    }

    fn course_labels(&self) -> Result<std::collections::BTreeMap<i64, Value>, String> {
        let mut labels = std::collections::BTreeMap::new();
        let mut bookmark: Option<String> = None;
        for _ in 0..20 {
            let page = self.courses(bookmark.as_deref())?;
            for item in page
                .get("Items")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default()
            {
                if let Some(id) = item
                    .pointer("/OrgUnit/Id")
                    .and_then(Value::as_i64)
                    .filter(|id| *id > 0)
                {
                    let name = item
                        .pointer("/OrgUnit/Name")
                        .cloned()
                        .or_else(|| item.pointer("/OrgUnit/Code").cloned())
                        .unwrap_or(Value::Null);
                    labels.insert(id, name);
                }
            }
            let more = page
                .pointer("/PagingInfo/HasMoreItems")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            if !more {
                break;
            }
            bookmark = page
                .pointer("/PagingInfo/Bookmark")
                .and_then(Value::as_str)
                .map(str::to_string);
            if bookmark.as_ref().is_none_or(String::is_empty) {
                break;
            }
        }
        Ok(labels)
    }
}

pub fn activate(
    settings: &mut crate::config::Settings,
    url: &str,
    token: &str,
) -> Result<String, String> {
    let token = token.trim();
    if token.is_empty() {
        return Err(
            "Paste d2lSessionVal and d2lSecureSessionVal, or an OAuth access token.".into(),
        );
    }
    if !session::session_cookie_auth(token) && token.chars().any(char::is_whitespace) {
        return Err(
            "Enter an access token without spaces, or both d2lSessionVal and d2lSecureSessionVal cookie values.".into(),
        );
    }
    persist(settings, url, token)
}

fn persist(
    settings: &mut crate::config::Settings,
    url: &str,
    token: &str,
) -> Result<String, String> {
    let base_url = crate::config::normalize_brightspace_url(url)?;
    let client = BrightspaceClient::new(BrightspaceConfig {
        base_url: base_url.clone(),
        token: token.into(),
    });
    client.whoami()?;
    // Verify useful student access too, rather than saving a profile-only token.
    let courses = client.courses(None)?;
    if courses.get("Items").and_then(Value::as_array).is_none() {
        return Err("Brightspace did not return course enrollments.".into());
    }
    let mut updated = settings.clone();
    updated.brightspace = Some(crate::config::BrightspaceSettings {
        url: Some(base_url),
        token: Some(token.into()),
    });
    updated.save_service("brightspace").map_err(|error| {
        format!("Cannot save Brightspace credentials to the private config file: {error}")
    })?;
    *settings = updated;
    Ok("Brightspace connected (read only).".into())
}

pub fn tools() -> Vec<Value> {
    let course = json!({"type":"integer","minimum":1});
    let mut tools = vec![
        json!({"name":"brightspace_status","description":"Check the connected Brightspace / myCourses account.","annotations":{"readOnlyHint":true}}),
        json!({"name":"brightspace_my_courses","description":"List one page of the connected student's Brightspace courses. Use PagingInfo.Bookmark when HasMoreItems is true.","inputSchema":{"type":"object","properties":{"bookmark":{"type":"string"}}},"annotations":{"readOnlyHint":true}}),
        json!({"name":"brightspace_course_content","description":"Table of contents and resources for a Brightspace course. Pass a file topic's Id (not a module Id) as topicid to brightspace_file to read its document.","inputSchema":{"type":"object","properties":{"courseid":course},"required":["courseid"]},"annotations":{"readOnlyHint":true}}),
        json!({"name":"brightspace_file","description":"Read a file topic from brightspace_course_content using courseid and topicid (topic Id). Default: original PDF resource/download link. Use format=text for text and page search, or format=images for scans/diagrams (max 8 pages). Page/find arguments default to text mode. Follow nextPageFrom when truncated; never guess missing content. External link topics are not downloaded. Cookies and tokens stay private.","inputSchema":{"type":"object","properties":{"courseid":course,"topicid":{"type":"integer","minimum":1},"format":{"type":"string","enum":["file","text","images"]},"find":{"type":"string"},"page_from":{"type":"integer","minimum":1},"page_to":{"type":"integer","minimum":1}},"required":["courseid","topicid"]},"annotations":{"readOnlyHint":true}}),
        json!({"name":"brightspace_grades","description":"Visible grade values for the connected student in a Brightspace course.","inputSchema":{"type":"object","properties":{"courseid":course},"required":["courseid"]},"annotations":{"readOnlyHint":true}}),
        json!({"name":"brightspace_assignments","description":"Dropbox / assignment folders for a course, including DueDate when set. Prefer brightspace_deadlines for a cross-course due list.","inputSchema":{"type":"object","properties":{"courseid":course},"required":["courseid"]},"annotations":{"readOnlyHint":true}}),
        json!({"name":"brightspace_assignment_status","description":"One dropbox folder plus the connected student's submissions and feedback.","inputSchema":{"type":"object","properties":{"courseid":course,"folderid":{"type":"integer","minimum":1}},"required":["courseid","folderid"]},"annotations":{"readOnlyHint":true}}),
        json!({"name":"brightspace_quizzes","description":"Quizzes in a course, including DueDate / StartDate / EndDate when set.","inputSchema":{"type":"object","properties":{"courseid":course},"required":["courseid"]},"annotations":{"readOnlyHint":true}}),
        json!({"name":"brightspace_quiz_status","description":"One quiz definition plus the connected student's attempts when available.","inputSchema":{"type":"object","properties":{"courseid":course,"quizid":{"type":"integer","minimum":1}},"required":["courseid","quizid"]},"annotations":{"readOnlyHint":true}}),
        json!({"name":"brightspace_calendar","description":"Upcoming calendar events for the student. Optional courseid limits to one course; days defaults to 60.","inputSchema":{"type":"object","properties":{"courseid":course,"days":{"type":"integer","minimum":1,"maximum":365}}},"annotations":{"readOnlyHint":true}}),
        json!({"name":"brightspace_deadlines","description":"Upcoming due work from assignments, quizzes and calendar DueDate events, sorted by date. Prefer this for due work.","inputSchema":{"type":"object","properties":{"days":{"type":"integer","minimum":1,"maximum":365,"description":"How many days ahead to look (default 60)."},"courseids":{"type":"array","items":{"type":"integer"}}}},"annotations":{"readOnlyHint":true}}),
        json!({"name":"brightspace_news","description":"Course news / announcements for one Brightspace course.","inputSchema":{"type":"object","properties":{"courseid":course},"required":["courseid"]},"annotations":{"readOnlyHint":true}}),
    ];
    for tool in &mut tools {
        if tool.get("annotations").is_none() {
            tool["annotations"] = json!({"readOnlyHint": true});
        }
    }
    tools
}

fn summarize_assignments(courseid: i64, folders: &Value) -> Value {
    let items: Vec<Value> = as_array(folders)
        .into_iter()
        .filter(|folder| folder.get("IsHidden").and_then(Value::as_bool) != Some(true))
        .map(|folder| compact_assignment(&folder))
        .collect();
    json!({"courseid": courseid, "assignments": items})
}

fn compact_assignment(folder: &Value) -> Value {
    json!({
        "id": folder.get("Id"),
        "name": folder.get("Name"),
        "due": folder.get("DueDate"),
        "availability": folder.get("Availability"),
        "submissionType": folder.get("SubmissionType"),
        "dropboxType": folder.get("DropboxType"),
        "gradeItemId": folder.get("GradeItemId"),
        "isHidden": folder.get("IsHidden"),
        "totalFiles": folder.get("TotalFiles"),
        "instructions": richtext_plain(folder.get("CustomInstructions")),
    })
}

fn summarize_quizzes(courseid: i64, page: &Value) -> Value {
    let items: Vec<Value> = list_objects(page).into_iter().map(|q| compact_quiz(&q)).collect();
    json!({"courseid": courseid, "quizzes": items, "paging": page.get("Next").or_else(|| page.get("PagingInfo"))})
}

fn compact_quiz(quiz: &Value) -> Value {
    json!({
        "id": quiz.get("QuizId").or_else(|| quiz.get("Id")),
        "name": quiz.get("Name"),
        "isActive": quiz.get("IsActive"),
        "due": quiz.get("DueDate"),
        "start": quiz.get("StartDate"),
        "end": quiz.get("EndDate"),
        "attemptsAllowed": quiz.get("AttemptsAllowed"),
        "timeLimit": quiz.get("SubmissionTimeLimit"),
        "gradeItemId": quiz.get("GradeItemId"),
    })
}

fn compact_event(event: Value) -> Value {
    json!({
        "id": event.get("CalendarEventId"),
        "courseid": event.get("OrgUnitId"),
        "course": event.get("OrgUnitName").or_else(|| event.get("OrgUnitCode")),
        "title": event.get("Title"),
        "description": event.get("Description"),
        "start": event.get("StartDateTime").or_else(|| event.get("StartDay")),
        "end": event.get("EndDateTime").or_else(|| event.get("EndDay")),
        "eventType": event.get("EventType"),
        "associated": event.get("AssociatedEntity"),
        "url": event.get("CalendarEventViewUrl"),
    })
}

fn compact_news(item: Value) -> Value {
    json!({
        "id": item.get("Id"),
        "title": item.get("Title"),
        "body": richtext_plain(item.get("Body")),
        "created": item.get("CreatedDate"),
        "start": item.get("StartDate"),
        "end": item.get("EndDate"),
        "isPinned": item.get("IsPinned"),
        "attachments": item.get("Attachments"),
    })
}

fn richtext_plain(value: Option<&Value>) -> Value {
    let Some(value) = value else {
        return Value::Null;
    };
    if let Some(text) = value.get("Text").and_then(Value::as_str) {
        return json!(text);
    }
    if let Some(html) = value.get("Html").and_then(Value::as_str) {
        return json!(html);
    }
    value.clone()
}

fn as_array(value: &Value) -> Vec<Value> {
    value.as_array().cloned().unwrap_or_default()
}

fn list_objects(page: &Value) -> Vec<Value> {
    page.get("Objects")
        .or_else(|| page.get("Items"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_else(|| as_array(page))
}

fn push_list_items(out: &mut Vec<Value>, page: &Value) {
    out.extend(list_objects(page));
}

fn utc_now() -> String {
    format_utc(SystemTime::now())
}

fn utc_plus_days(days: i64) -> String {
    let secs = days.saturating_mul(24 * 60 * 60) as u64;
    format_utc(SystemTime::now() + Duration::from_secs(secs))
}

fn window_utc(days: i64) -> (String, String) {
    (utc_now(), utc_plus_days(days))
}

fn format_utc(time: SystemTime) -> String {
    let secs = time
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    // Manual UTC formatting avoids pulling chrono solely for Brightspace windows.
    let days = secs.div_euclid(86_400);
    let tod = secs.rem_euclid(86_400) as u32;
    let (y, m, d) = civil_from_days(days);
    let hh = tod / 3600;
    let mm = (tod % 3600) / 60;
    let ss = tod % 60;
    format!("{y:04}-{m:02}-{d:02}T{hh:02}:{mm:02}:{ss:02}.000Z")
}

/// Howard Hinnant civil_from_days (proleptic Gregorian).
fn civil_from_days(z: i64) -> (i32, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = (yoe as i64 + era * 400) as i32;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m as u32, d as u32)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader, Write};
    use std::net::TcpListener;

    fn server(
        responses: Vec<(u16, String)>,
    ) -> (BrightspaceConfig, std::thread::JoinHandle<Vec<String>>) {
        server_with_mime(responses, "application/json")
    }

    fn server_with_mime(
        responses: Vec<(u16, String)>,
        mime: &'static str,
    ) -> (BrightspaceConfig, std::thread::JoinHandle<Vec<String>>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let config = BrightspaceConfig {
            base_url: format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port()),
            token: "test-private-token".into(),
        };
        let worker = std::thread::spawn(move || {
            let mut requests = Vec::new();
            for (status, body) in responses {
                let (mut stream, _) = listener.accept().unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut request = String::new();
                loop {
                    let mut line = String::new();
                    if reader.read_line(&mut line).unwrap() == 0 || line == "\r\n" {
                        break;
                    }
                    request.push_str(&line);
                }
                requests.push(request);
                write!(stream, "HTTP/1.1 {status} Test\r\nContent-Type: {mime}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
            }
            requests
        });
        (config, worker)
    }

    #[test]
    fn file_tool_delivers_pdf_resource_without_exposing_authentication() {
        let (mut config, worker) = server_with_mime(
            vec![(200, "%PDF-1.4\nfixture\n%%EOF".into())],
            "application/pdf",
        );
        config.token = "d2lSessionVal=private-one; d2lSecureSessionVal=private-two".into();
        let settings = crate::config::Settings {
            brightspace: Some(crate::config::BrightspaceSettings {
                url: Some(config.base_url),
                token: Some(config.token),
            }),
            ..Default::default()
        };
        let reply = crate::mcp::dispatch_tool(
            &settings, "brightspace_file", &json!({"courseid":42,"topicid":17}),
        );
        assert_eq!(reply["isError"], false, "{reply}");
        let content = reply["content"].as_array().unwrap();
        let resource = content.iter().find_map(|item| item.get("resource")).unwrap();
        assert_eq!(resource["mimeType"], "application/pdf");
        assert!(resource["uri"].as_str().unwrap().starts_with("brightspace://file/"));
        assert!(resource["blob"].is_string());
        assert!(!reply.to_string().contains("private-one"));
        assert!(!reply.to_string().contains("private-two"));
        let requests = worker.join().unwrap();
        assert_eq!(requests.len(), 1);
        assert!(requests[0].starts_with("GET /d2l/api/le/1.82/42/content/topics/17/file "));
        assert!(requests[0].contains(session::BROWSER_UA));
        assert!(requests[0].to_lowercase().contains("cookie: d2lsessionval=private-one"));
    }

    #[test]
    fn file_download_supports_text_and_bearer_auth() {
        let (config, worker) = server_with_mime(
            vec![(200, "Read chapter five.".into())], "text/plain",
        );
        let result = BrightspaceClient::new(config).download_file(
            42, 17, &crate::moodle::FileQuery::default(),
        ).unwrap();
        assert_eq!(result["text"], "Read chapter five.");
        let requests = worker.join().unwrap();
        assert!(requests[0].to_lowercase().contains("authorization: bearer test-private-token"));
    }

    #[test]
    fn file_download_rejects_errors_redirects_login_pages_and_oversized_files() {
        for (status, mime, body) in [
            (401, "text/plain", "test-private-token".into()),
            (403, "text/plain", "test-private-token".into()),
            (404, "text/plain", "test-private-token".into()),
            (302, "text/plain", String::new()),
            (200, "text/html", "<html>SSO login</html>".into()),
            (200, "application/octet-stream", "<!doctype html><html>login</html>".into()),
            (200, "application/json", r#"{"error":"test-private-token"}"#.into()),
            (200, "application/pdf", "x".repeat(12 * 1024 * 1024 + 1)),
        ] {
            let (config, worker) = server_with_mime(vec![(status, body)], mime);
            let error = BrightspaceClient::new(config).download_file(
                42, 17, &crate::moodle::FileQuery::default(),
            ).unwrap_err();
            assert!(!error.contains("test-private-token"));
            assert_eq!(worker.join().unwrap().len(), 1);
        }
    }

    #[test]
    fn file_tool_validates_ids_and_format_before_network_access() {
        let settings = crate::config::Settings {
            brightspace: Some(crate::config::BrightspaceSettings {
                url: Some("http://127.0.0.1:1".into()),
                token: Some("secret".into()),
            }),
            ..Default::default()
        };
        for args in [
            json!({"courseid":42}),
            json!({"courseid":42,"topicid":"https://evil.example/file"}),
            json!({"courseid":0,"topicid":17}),
            json!({"courseid":42,"topicid":-1}),
            json!({"courseid":42,"topicid":17,"format":"invalid"}),
            json!({"courseid":42,"topicid":17,"page_from":0}),
        ] {
            let reply = crate::mcp::dispatch_tool(&settings, "brightspace_file", &args);
            assert_eq!(reply["isError"], true);
            assert!(!reply.to_string().contains("Cannot download"), "{reply}");
        }
    }

    #[test]
    fn reads_documented_routes_with_bearer_auth_and_encoded_bookmarks() {
        let (config, worker) = server(vec![
            (200, r#"{"Identifier":"123"}"#.into()),
            (
                200,
                r#"{"Items":[],"PagingInfo":{"HasMoreItems":true,"Bookmark":"next"}}"#.into(),
            ),
            (200, "[]".into()),
            (200, "[]".into()),
            (200, r#"[{"Id":9,"Name":"HW1","DueDate":"2026-11-01T23:59:00.000Z","IsHidden":false}]"#.into()),
            (200, r#"{"Objects":[{"QuizId":3,"Name":"Quiz 1","DueDate":"2026-11-02T12:00:00.000Z","IsActive":true}]}"#.into()),
            (200, r#"[{"Id":1,"Title":"Hello","Body":{"Text":"hi"},"CreatedDate":"2026-10-01T00:00:00.000Z"}]"#.into()),
        ]);
        let client = BrightspaceClient::new(config);
        assert_eq!(client.whoami().unwrap()["Identifier"], "123");
        assert_eq!(
            client.courses(Some("a&b")).unwrap()["PagingInfo"]["HasMoreItems"],
            true
        );
        client.content(42).unwrap();
        client.grades(42).unwrap();
        let assigns = client.assignments(42).unwrap();
        assert_eq!(assigns["assignments"][0]["name"], "HW1");
        assert_eq!(assigns["assignments"][0]["due"], "2026-11-01T23:59:00.000Z");
        let quizzes = client.quizzes(42).unwrap();
        assert_eq!(quizzes["quizzes"][0]["id"], 3);
        client.news(42).unwrap();
        let requests = worker.join().unwrap();
        for request in &requests {
            assert!(request.contains(session::BROWSER_UA));
            assert!(request
                .to_lowercase()
                .contains("authorization: bearer test-private-token"));
            assert!(!request.to_lowercase().contains("cookie:"));
            assert!(!request
                .lines()
                .next()
                .unwrap()
                .contains("test-private-token"));
        }
        assert!(requests[0].starts_with(&format!("GET /d2l/api/lp/{LP}/users/whoami ")));
        assert!(requests[1].contains("orgUnitTypeId=3&bookmark=a%26b"));
        assert!(requests[2].starts_with(&format!("GET /d2l/api/le/{LE}/42/content/toc ")));
        assert!(requests[3]
            .starts_with(&format!("GET /d2l/api/le/{LE}/42/grades/values/myGradeValues/ ")));
        assert!(requests[4].starts_with(&format!("GET /d2l/api/le/{LE}/42/dropbox/folders/ ")));
        assert!(requests[5].starts_with(&format!("GET /d2l/api/le/{LE}/42/quizzes/ ")));
        assert!(requests[6].starts_with(&format!("GET /d2l/api/le/{LE}/42/news/ ")));
    }

    #[test]
    fn session_cookies_are_sent_instead_of_a_bearer_token() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let config = BrightspaceConfig {
            base_url: format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port()),
            token: "d2lSessionVal=sess; d2lSecureSessionVal=secure".into(),
        };
        let worker = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut request = String::new();
            loop {
                let mut line = String::new();
                if reader.read_line(&mut line).unwrap() == 0 || line == "\r\n" {
                    break;
                }
                request.push_str(&line);
            }
            let body = r#"{"Identifier":"123"}"#;
            write!(stream, "HTTP/1.1 200 Test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
            request
        });
        let client = BrightspaceClient::new(config);
        assert_eq!(client.whoami().unwrap()["Identifier"], "123");
        let request = worker.join().unwrap().to_lowercase();
        assert!(request.contains("cookie: d2lsessionval=sess; d2lsecuresessionval=secure"));
        assert!(request.contains("x-csrf-token: sess"));
        assert!(!request.contains("authorization:"));
    }

    #[test]
    fn expired_and_forbidden_tokens_do_not_expose_response_secrets() {
        for code in [401, 403, 429] {
            let (config, worker) = server(vec![(code, "test-private-token".into())]);
            let error = BrightspaceClient::new(config).whoami().unwrap_err();
            assert!(!error.contains("test-private-token"));
            worker.join().unwrap();
        }
    }

    #[test]
    fn rejects_login_pages_redirects_and_unidentified_accounts() {
        for (status, body) in [(200, "<html>login</html>"), (200, "{}"), (302, "")] {
            let (config, worker) = server(vec![(status, body.into())]);
            assert!(BrightspaceClient::new(config).whoami().is_err());
            worker.join().unwrap();
        }
    }

    #[test]
    fn scrubs_credentials_from_api_results() {
        let (config, worker) = server(vec![(
            200,
            r#"{"Identifier":"123","token":"another-secret","message":"test-private-token"}"#
                .into(),
        )]);
        let result = BrightspaceClient::new(config).whoami().unwrap();
        assert_eq!(result["token"], "[redacted]");
        assert_eq!(result["message"], "[redacted]");
        worker.join().unwrap();
    }

    #[test]
    fn activation_failure_preserves_existing_credentials() {
        let (config, worker) = server(vec![(401, "{}".into())]);
        let mut settings = crate::config::Settings {
            brightspace: Some(crate::config::BrightspaceSettings {
                url: Some("https://old.example".into()),
                token: Some("old-token".into()),
            }),
            ..Default::default()
        };
        assert!(activate(&mut settings, &config.base_url, &config.token).is_err());
        assert_eq!(settings.brightspace().unwrap().token, "old-token");
        worker.join().unwrap();
    }

    #[test]
    fn activation_persists_only_after_profile_and_courses_are_verified() {
        crate::config::tests::with_temp_config(|| {
            let (config, worker) = server(vec![
                (200, r#"{"Identifier":"123"}"#.into()),
                (
                    200,
                    r#"{"Items":[],"PagingInfo":{"HasMoreItems":false}}"#.into(),
                ),
            ]);
            let mut settings = crate::config::Settings::default();
            activate(&mut settings, &config.base_url, &config.token).unwrap();
            let saved = crate::config::Settings::load();
            assert_eq!(saved.brightspace().unwrap().token, config.token);
            assert_eq!(saved.services_active(), vec!["brightspace"]);
            assert!(crate::mcp::tool_definitions(&saved)
                .iter()
                .any(|tool| tool["name"] == "brightspace_deadlines"));
            worker.join().unwrap();
        });
    }

    #[test]
    fn deadlines_merge_assignments_and_quizzes() {
        // Stay inside the default 60-day window used by deadlines().
        let future = utc_plus_days(14);
        let (config, worker) = server(vec![
            (
                200,
                r#"{"Items":[{"OrgUnit":{"Id":42,"Name":"CIVE"}}],"PagingInfo":{"HasMoreItems":false}}"#
                    .into(),
            ),
            (
                200,
                format!(
                    r#"[{{"Id":9,"Name":"HW1","DueDate":"{future}","IsHidden":false}}]"#
                ),
            ),
            (
                200,
                format!(
                    r#"{{"Objects":[{{"QuizId":3,"Name":"Quiz 1","DueDate":"{future}","IsActive":true}}]}}"#
                ),
            ),
            // calendar batch (deadlines also loads myEvents)
            (200, r#"{"Objects":[]}"#.into()),
        ]);
        let client = BrightspaceClient::new(config);
        let result = client.deadlines(60, Some(vec![42])).unwrap();
        assert_eq!(result["items"].as_array().unwrap().len(), 2);
        assert_eq!(result["items"][0]["type"], "assignment");
        assert_eq!(result["items"][1]["type"], "quiz");
        worker.join().unwrap();
    }

    #[test]
    fn mcp_dispatch_reads_courses_and_rejects_invalid_course_ids() {
        let (config, worker) = server(vec![(
            200,
            r#"{"Items":[{"OrgUnit":{"Id":42}}],"PagingInfo":{"HasMoreItems":false}}"#.into(),
        )]);
        let settings = crate::config::Settings {
            brightspace: Some(crate::config::BrightspaceSettings {
                url: Some(config.base_url),
                token: Some(config.token),
            }),
            ..Default::default()
        };
        let reply = crate::mcp::dispatch_tool(&settings, "brightspace_my_courses", &json!({}));
        assert_eq!(reply["isError"], false);
        let courses: Value =
            serde_json::from_str(reply["content"][0]["text"].as_str().unwrap()).unwrap();
        assert_eq!(courses["Items"][0]["OrgUnit"]["Id"], 42);
        worker.join().unwrap();
        for args in [
            json!({}),
            json!({"courseid":0}),
            json!({"courseid":"../users"}),
        ] {
            assert_eq!(
                crate::mcp::dispatch_tool(&settings, "brightspace_course_content", &args)
                    ["isError"],
                true
            );
        }
    }
}
