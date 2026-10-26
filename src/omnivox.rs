//! Read-only Omnivox client.
//! College login talks to the official site and the `/Mobl/` JSON endpoints.
//! A self-hosted [omnivox-mcp](https://github.com/Beat-YT/omnivox-mcp) gateway remains supported.
use crate::config::{is_omnivox_host, OmnivoxConfig};
use crate::session;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::time::Duration;

const REQUIRED_TOOL: &str = "get-courses-summary";

pub struct OmnivoxClient {
    config: OmnivoxConfig,
    agent: ureq::Agent,
}

impl OmnivoxClient {
    pub fn new(config: OmnivoxConfig) -> Self {
        Self {
            config,
            agent: ureq::AgentBuilder::new()
                .timeout(Duration::from_secs(30))
                .redirects(0)
                .build(),
        }
    }

    fn is_college(&self) -> bool {
        is_omnivox_host(&self.config.base_url) || session::session_cookie_auth(&self.config.token)
    }

    fn authorize(&self, request: ureq::Request) -> ureq::Request {
        request
            .set("Authorization", &format!("Bearer {}", self.config.token))
            .set("x-mcp-auth", &self.config.token)
            .set("Accept", "application/json, text/plain")
    }

    fn authorize_college(&self, request: ureq::Request) -> ureq::Request {
        request
            .set("Cookie", &self.config.token)
            .set("Accept", "application/json, text/plain")
            .set("Origin", &self.config.base_url)
            .set("Referer", &format!("{}/", self.config.base_url))
            .set("User-Agent", session::BROWSER_UA)
    }

    fn map_error(error: ureq::Error, token: &str) -> String {
        match error {
            ureq::Error::Status(401 | 403, _) => {
                "Omnivox MCP access key expired or refused. Replace it in the app.".into()
            }
            ureq::Error::Status(404, _) => {
                "This Omnivox MCP server does not expose that tool.".into()
            }
            ureq::Error::Status(409, response) => gateway_message(response, token)
                .unwrap_or_else(|| "Omnivox blocked the request. Complete any required document or survey in the portal, then retry.".into()),
            ureq::Error::Status(code, response) => gateway_message(response, token)
                .unwrap_or_else(|| format!("Omnivox MCP returned HTTP error {code}.")),
            _ => "Cannot reach the Omnivox MCP gateway. Check the address and that the server is running.".into(),
        }
    }

    fn catalog(&self) -> Result<Value, String> {
        let response = self
            .authorize(self.agent.get(&format!("{}/tools", self.config.base_url)))
            .call()
            .map_err(|error| Self::map_error(error, &self.config.token))?;
        if response.status() != 200 {
            return Err("Omnivox MCP redirected the request. Use the gateway root address.".into());
        }
        let value: Value = response.into_json().map_err(|_| {
            "Omnivox MCP did not return a tool catalog. Check the address.".to_string()
        })?;
        Ok(crate::moodle::scrub(value, &self.config.token))
    }

    fn mobl(&self, path: &str, body: Value) -> Result<Value, String> {
        if path
            .chars()
            .any(|ch| !(ch.is_ascii_alphanumeric() || ch == '/' || ch == '_'))
        {
            return Err("Unknown Omnivox path.".into());
        }
        let response = self
            .authorize_college(
                self.agent
                    .post(&format!("{}/Mobl/{path}", self.config.base_url)),
            )
            .set("Content-Type", "application/json")
            .send_json(body)
            .map_err(|error| Self::map_error(error, &self.config.token))?;
        if response.status() != 200 {
            return Err("Omnivox redirected the request. Sign in again in the app.".into());
        }
        let value: Value = response
            .into_json()
            .map_err(|_| "Omnivox did not return JSON. Sign in again in the app.".to_string())?;
        if value
            .get("NaviguerPage")
            .and_then(Value::as_str)
            .is_some_and(|page| page.to_ascii_lowercase().contains("error"))
        {
            return Err(
                "Omnivox blocked the request. Complete any required document or survey in the portal, then retry."
                    .into(),
            );
        }
        Ok(crate::moodle::scrub(value, &self.config.token))
    }

    fn call(&self, tool: &str, body: Value) -> Result<Value, String> {
        if !tool
            .chars()
            .all(|ch| ch.is_ascii_lowercase() || ch == '-' || ch == '_')
        {
            return Err("Unknown Omnivox tool.".into());
        }
        let response = self
            .authorize(
                self.agent
                    .post(&format!("{}/tools/{tool}", self.config.base_url)),
            )
            .set("Content-Type", "application/json")
            .send_json(body)
            .map_err(|error| Self::map_error(error, &self.config.token))?;
        if response.status() != 200 {
            return Err("Omnivox MCP redirected the request. Use the gateway root address.".into());
        }
        let text = response
            .into_string()
            .map_err(|_| "Omnivox MCP returned an unreadable response.".to_string())?;
        let value =
            serde_json::from_str::<Value>(&text).unwrap_or_else(|_| json!({ "text": text }));
        Ok(crate::moodle::scrub(value, &self.config.token))
    }

    pub fn status(&self) -> Result<Value, String> {
        if self.is_college() {
            let courses = self.mobl("LeaCommun/GetDefaultModel", json!({ "AnSession": "" }))?;
            let count = courses
                .get("ListeCours")
                .and_then(Value::as_array)
                .map(Vec::len)
                .unwrap_or(0);
            return Ok(json!({ "connected": true, "college": true, "courses": count }));
        }
        let names = tool_names(&self.catalog()?);
        if !names.iter().any(|name| name == REQUIRED_TOOL) {
            return Err(
                "This server is not an Omnivox MCP gateway (missing get-courses-summary).".into(),
            );
        }
        match self.call("get-college-list", json!({})) {
            Ok(college) => Ok(json!({ "college": college, "tools": names.len() })),
            Err(_) => Ok(json!({ "connected": true, "tools": names.len() })),
        }
    }

    pub fn courses(&self) -> Result<Value, String> {
        if self.is_college() {
            return self.mobl("LeaCommun/GetDefaultModel", json!({ "AnSession": "" }));
        }
        self.call("get-courses-summary", json!({}))
    }

    pub fn documents(&self, course_id: &str) -> Result<Value, String> {
        if self.is_college() {
            return self.mobl(
                "LeaEtudiant/GetDocumentsListeModel",
                json!({ "AnSession": "", "idClasse": course_id }),
            );
        }
        self.call("get-course-documents", json!({ "course_id": course_id }))
    }

    pub fn assignments(&self, course_id: Option<&str>) -> Result<Value, String> {
        if self.is_college() {
            return match course_id {
                Some(course_id) => self.mobl(
                    "LeaEtudiant/GetTravauxListeModel",
                    json!({ "AnSession": "", "idClasse": course_id }),
                ),
                None => self.mobl("LeaEtudiant/GetTravauxSommaireModel", json!({ "AnSession": "" })),
            };
        }
        match course_id {
            Some(course_id) => {
                self.call("get-course-assignments", json!({ "course_id": course_id }))
            }
            None => self.call("get-assignments-summary", json!({})),
        }
    }

    pub fn grades(&self, course_id: Option<&str>) -> Result<Value, String> {
        if self.is_college() {
            return match course_id {
                Some(course_id) => {
                    let (no_cours, no_groupe) = split_class_id(course_id)?;
                    self.mobl(
                        "LeaEtudiant/GetNotesDetailModel",
                        json!({
                            "AnSession": "",
                            "noCours": no_cours,
                            "noGroupe": no_groupe
                        }),
                    )
                }
                None => self.mobl("LeaEtudiant/GetNotesSommaireModel", json!({ "AnSession": "" })),
            };
        }
        match course_id {
            Some(course_id) => self.call("get-course-evals", json!({ "course_id": course_id })),
            None => self.call("get-grades-summary", json!({})),
        }
    }

    pub fn schedule(&self) -> Result<Value, String> {
        if self.is_college() {
            return self.mobl("Horaire/GetHoraireModel", json!({ "AnSession": Value::Null }));
        }
        self.call("get-schedule", json!({}))
    }

    pub fn calendar(&self, range: Option<&str>) -> Result<Value, String> {
        if self.is_college() {
            let _ = range;
            return self.mobl(
                "Calendrier/GetCalendrierModel",
                json!({
                    "decalagePagination": 0,
                    "typeCalendrier": "scolaire",
                    "anSession": "",
                    "coursGroupe": "",
                    "filtre": "choix_tous"
                }),
            );
        }
        let mut body = json!({});
        if let Some(range) = range {
            body["range"] = json!(range);
        }
        self.call("get-calendar", body)
    }
}

fn gateway_message(response: ureq::Response, token: &str) -> Option<String> {
    let text = response.into_string().ok()?;
    let value: Value = serde_json::from_str(&text).ok()?;
    value
        .get("error")
        .and_then(Value::as_str)
        .filter(|message| !message.is_empty())
        .map(|message| message.replace(token, "[redacted]"))
}

fn tool_names(catalog: &Value) -> Vec<String> {
    catalog
        .get("tools")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|tool| tool.get("name").and_then(Value::as_str).map(str::to_string))
        .collect()
}

fn split_class_id(course_id: &str) -> Result<(String, String), String> {
    let (course, group) = course_id.split_once('.').ok_or_else(|| {
        "Provide course_id as returned by omnivox_my_courses, e.g. 2434K5EM.1011.".to_string()
    })?;
    if course.is_empty() || group.is_empty() {
        return Err("Provide course_id as returned by omnivox_my_courses, e.g. 2434K5EM.1011.".into());
    }
    Ok((course.to_string(), group.to_string()))
}

fn require_course_id(args: &Value) -> Result<String, String> {
    optional_course_id(args)?.ok_or_else(|| {
        "Provide course_id as returned by omnivox_my_courses, e.g. 2434K5EM.1011.".into()
    })
}

fn optional_course_id(args: &Value) -> Result<Option<String>, String> {
    let Some(value) = args.get("course_id") else {
        return Ok(None);
    };
    let Some(id) = value.as_str().map(str::trim).filter(|id| !id.is_empty()) else {
        return Err("Provide course_id as a string, e.g. 2434K5EM.1011.".into());
    };
    if id.len() > 64 || id.contains(['/', '\\', ' ', '?', '#', '&', ':']) || id.contains("..") {
        return Err("Provide a valid Omnivox course_id.".into());
    }
    Ok(Some(id.to_string()))
}

fn optional_range(args: &Value) -> Result<Option<String>, String> {
    let Some(value) = args.get("range") else {
        return Ok(None);
    };
    let Some(range) = value.as_str() else {
        return Err("Provide range as today, week, month or all.".into());
    };
    if !matches!(range, "today" | "week" | "month" | "all") {
        return Err("Provide range as today, week, month or all.".into());
    }
    Ok(Some(range.to_string()))
}

pub fn activate(
    settings: &mut crate::config::Settings,
    url: &str,
    token: &str,
) -> Result<String, String> {
    let token = token.trim();
    if token.is_empty() {
        return Err("Enter your Omnivox student number and password, or an MCP access key in advanced options.".into());
    }
    let base_url = crate::config::normalize_omnivox_url(url)?;
    if !is_omnivox_host(&base_url) && token.chars().any(char::is_whitespace) {
        return Err("Enter the Omnivox MCP access key, without the Bearer prefix.".into());
    }
    persist(settings, &base_url, token)
}

pub fn activate_login(
    settings: &mut crate::config::Settings,
    url: &str,
    username: &str,
    password: &str,
) -> Result<String, String> {
    if username.trim().is_empty() || password.is_empty() {
        return Err("Enter your Omnivox student number and password.".into());
    }
    let base_url = crate::config::normalize_omnivox_url(url)?;
    if !is_omnivox_host(&base_url) {
        return Err("Use your college Omnivox address, e.g. https://cegepxxx.omnivox.ca.".into());
    }
    let token = login_session(&base_url, username.trim(), password)?;
    persist(settings, &base_url, &token)
}

fn persist(
    settings: &mut crate::config::Settings,
    base_url: &str,
    token: &str,
) -> Result<String, String> {
    let client = OmnivoxClient::new(OmnivoxConfig {
        base_url: base_url.to_string(),
        token: token.into(),
    });
    client.status()?;
    let mut updated = settings.clone();
    updated.omnivox = Some(crate::config::OmnivoxSettings {
        url: Some(base_url.into()),
        token: Some(token.into()),
    });
    updated.save().map_err(|error| {
        format!("Cannot save Omnivox credentials to the private config file: {error}")
    })?;
    *settings = updated;
    Ok(if is_omnivox_host(base_url) {
        "Omnivox connected (read only).".into()
    } else {
        "Omnivox connected (read only, through the MCP gateway).".into()
    })
}

fn login_session(base_url: &str, username: &str, password: &str) -> Result<String, String> {
    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(30))
        .redirects(0)
        .user_agent(session::BROWSER_UA)
        .build();
    let mut jar = BTreeMap::new();
    let mut current = format!("{base_url}/Login/Account/Login");
    let response = send(
        agent
            .get(&current)
            .set("Accept", "text/html,application/xhtml+xml")
            .set("Cookie", &session::cookie_header(&jar)),
    )?;
    session::merge_set_cookie(&mut jar, &response);
    if let Some(location) = response.header("location").map(str::to_string) {
        let next = session::resolve_url(&current, &location)?;
        if !session::same_host(base_url, &next) {
            return Err("Omnivox redirected the login page away from the college site.".into());
        }
        current = next;
    }
    let html = if (300..400).contains(&response.status()) {
        let follow = send(
            agent
                .get(&current)
                .set("Accept", "text/html,application/xhtml+xml")
                .set("Cookie", &session::cookie_header(&jar)),
        )?;
        session::merge_set_cookie(&mut jar, &follow);
        follow
            .into_string()
            .map_err(|_| "Omnivox did not return a login page.".to_string())?
    } else if (200..300).contains(&response.status()) {
        response
            .into_string()
            .map_err(|_| "Omnivox did not return a login page.".to_string())?
    } else {
        return Err("Cannot reach the Omnivox login page. Check the college address.".into());
    };
    if session::looks_like_mfa(&current, &html) {
        return Err(omnivox_mfa_message());
    }
    if !session::html_has_password_field(&html) {
        return Err("This Omnivox site did not show a student password form. Check the college address.".into());
    }
    let (action, body) = omnivox_login_form(&html, username, password)?;
    let post_url = session::resolve_url(&current, &action)?;
    if !session::same_host(base_url, &post_url) {
        return Err("Omnivox redirected the login form away from the college site.".into());
    }
    let posted = send_form(
        agent
            .post(&post_url)
            .set("Content-Type", "application/x-www-form-urlencoded")
            .set("Cookie", &session::cookie_header(&jar))
            .set("Origin", base_url)
            .set("Referer", &current),
        &body,
    )?;
    session::merge_set_cookie(&mut jar, &posted);
    let mut location = posted.header("location").map(str::to_string);
    let mut body_html = posted.into_string().unwrap_or_default();
    current = post_url;
    for _ in 0..8 {
        let Some(target) = location.take() else {
            break;
        };
        let next = session::resolve_url(&current, &target)?;
        if !session::same_host(base_url, &next) {
            return Err("Omnivox redirected the login away from the college site.".into());
        }
        if session::looks_like_mfa(&next, "") {
            return Err(omnivox_mfa_message());
        }
        current = next;
        let follow = send(
            agent
                .get(&current)
                .set("Accept", "text/html,application/xhtml+xml")
                .set("Cookie", &session::cookie_header(&jar)),
        )?;
        session::merge_set_cookie(&mut jar, &follow);
        location = follow.header("location").map(str::to_string);
        body_html = follow.into_string().unwrap_or_default();
    }
    if session::looks_like_mfa(&current, &body_html) {
        return Err(omnivox_mfa_message());
    }
    if session::html_has_password_field(&body_html) {
        return Err("Omnivox refused the login. Check your student number and password.".into());
    }
    let header = session::cookie_header(&jar);
    if header.is_empty() {
        return Err("Omnivox did not return a session cookie.".into());
    }
    Ok(header)
}

fn omnivox_login_form(html: &str, username: &str, password: &str) -> Result<(String, String), String> {
    let action = session::html_form_action(html)
        .filter(|action| !action.is_empty())
        .unwrap_or_else(|| "/intr/Module/Identification/Login/Login.aspx".into());
    let mut pairs = session::html_form_inputs(html);
    if pairs.is_empty() {
        return Err("The Omnivox login page did not include a form.".into());
    }
    let mut has_user = false;
    let mut has_pass = false;
    for (name, value) in &mut pairs {
        let lower = name.to_ascii_lowercase();
        if matches!(lower.as_str(), "identifiant" | "noda" | "username") {
            *value = username.to_string();
            has_user = true;
        }
        if lower.contains("password") {
            *value = password.to_string();
            has_pass = true;
        }
        if lower == "typeidentification" && value.trim().is_empty() {
            *value = "Etudiant".into();
        }
        if lower == "typelogin" && value.trim().is_empty() {
            *value = "PostSolutionLogin".into();
        }
    }
    if !has_user {
        pairs.push(("Identifiant".into(), username.into()));
        pairs.push(("NoDA".into(), username.into()));
    }
    if !has_pass {
        pairs.push(("Password".into(), password.into()));
        pairs.push(("PasswordEtu".into(), password.into()));
    }
    let encoded = session::form(
        &pairs
            .iter()
            .map(|(name, value)| (name.as_str(), value.as_str()))
            .collect::<Vec<_>>(),
    );
    Ok((action, encoded))
}

fn omnivox_mfa_message() -> String {
    "Omnivox asked for two-factor authentication. Complete it in the portal on this computer, then try again, or use the Omnivox MCP gateway in advanced options.".into()
}

fn send(request: ureq::Request) -> Result<ureq::Response, String> {
    match request.call() {
        Ok(response) | Err(ureq::Error::Status(_, response)) => Ok(response),
        Err(_) => {
            Err("Cannot reach Omnivox. Check the college address and your Internet connection.".into())
        }
    }
}

fn send_form(request: ureq::Request, body: &str) -> Result<ureq::Response, String> {
    match request.send_string(body) {
        Ok(response) | Err(ureq::Error::Status(_, response)) => Ok(response),
        Err(_) => {
            Err("Cannot reach Omnivox. Check the college address and your Internet connection.".into())
        }
    }
}

pub fn tools() -> Vec<Value> {
    let mut tools = vec![
        json!({"name":"omnivox_status","description":"Check the connected Omnivox / Léa account."}),
        json!({"name":"omnivox_my_courses","description":"List the connected student's Omnivox / Léa courses for the current term."}),
        json!({"name":"omnivox_schedule","description":"Get the connected student's static weekly Omnivox timetable."}),
        json!({"name":"omnivox_course_documents","description":"List Léa documents for one Omnivox course.","inputSchema":{"type":"object","properties":{"course_id":{"type":"string","description":"Course id from omnivox_my_courses, e.g. 2434K5EM.1011."}},"required":["course_id"]}}),
        json!({"name":"omnivox_assignments","description":"List Omnivox assignments. Omit course_id for a summary of every course.","inputSchema":{"type":"object","properties":{"course_id":{"type":"string"}}}}),
        json!({"name":"omnivox_grades","description":"Get Omnivox grades. Omit course_id for a summary of every course.","inputSchema":{"type":"object","properties":{"course_id":{"type":"string"}}}}),
        json!({"name":"omnivox_calendar","description":"Get the Omnivox day-by-day calendar, including holidays and posted deadlines.","inputSchema":{"type":"object","properties":{"range":{"type":"string","enum":["today","week","month","all"]}}}}),
    ];
    for tool in &mut tools {
        tool["annotations"] = json!({"readOnlyHint":true});
    }
    tools
}

pub fn dispatch(client: &OmnivoxClient, name: &str, args: &Value) -> Result<Value, String> {
    match name {
        "omnivox_status" => client.status(),
        "omnivox_my_courses" => client.courses(),
        "omnivox_course_documents" => client.documents(&require_course_id(args)?),
        "omnivox_assignments" => client.assignments(optional_course_id(args)?.as_deref()),
        "omnivox_grades" => client.grades(optional_course_id(args)?.as_deref()),
        "omnivox_schedule" => client.schedule(),
        "omnivox_calendar" => client.calendar(optional_range(args)?.as_deref()),
        _ => Err("Unknown Omnivox tool.".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::TcpListener;

    fn server(
        responses: Vec<(u16, String)>,
    ) -> (OmnivoxConfig, std::thread::JoinHandle<Vec<String>>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let config = OmnivoxConfig {
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
                let mut content_length = 0usize;
                loop {
                    let mut line = String::new();
                    if reader.read_line(&mut line).unwrap() == 0 || line == "\r\n" {
                        break;
                    }
                    if let Some(value) = line
                        .split_once(':')
                        .filter(|(name, _)| name.eq_ignore_ascii_case("content-length"))
                        .map(|(_, value)| value.trim().parse().unwrap_or(0))
                    {
                        content_length = value;
                    }
                    request.push_str(&line);
                }
                if content_length > 0 {
                    let mut body = vec![0; content_length];
                    reader.read_exact(&mut body).unwrap();
                    request.push_str(&String::from_utf8_lossy(&body));
                }
                requests.push(request);
                write!(
                    stream,
                    "HTTP/1.1 {status} Test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                )
                .unwrap();
            }
            requests
        });
        (config, worker)
    }

    fn catalog_body() -> String {
        r#"{"tools":[{"name":"get-courses-summary"},{"name":"get-college-list"}]}"#.into()
    }

    #[test]
    fn reads_gateway_routes_with_bearer_and_header_auth() {
        let (config, worker) = server(vec![
            (200, catalog_body()),
            (200, r#"{"college":"Demo"}"#.into()),
            (200, r#"{"courses":[]}"#.into()),
            (200, r#"{"documents":[]}"#.into()),
            (200, r#"{"assignments":[]}"#.into()),
            (200, r#"{"grades":[]}"#.into()),
            (200, r#"{"events":[]}"#.into()),
        ]);
        let client = OmnivoxClient::new(config);
        assert_eq!(client.status().unwrap()["college"]["college"], "Demo");
        client.courses().unwrap();
        client.documents("2434K5EM.1011").unwrap();
        client.assignments(Some("2434K5EM.1011")).unwrap();
        client.grades(None).unwrap();
        client.calendar(Some("week")).unwrap();
        let requests = worker.join().unwrap();
        for request in &requests {
            let lower = request.to_lowercase();
            assert!(lower.contains("authorization: bearer test-private-token"));
            assert!(lower.contains("x-mcp-auth: test-private-token"));
            assert!(!request
                .lines()
                .next()
                .unwrap()
                .contains("test-private-token"));
        }
        assert!(requests[0].starts_with("GET /tools "));
        assert!(requests[1].starts_with("POST /tools/get-college-list "));
        assert!(requests[2].starts_with("POST /tools/get-courses-summary "));
        assert!(requests[3].contains(r#""course_id":"2434K5EM.1011""#));
        assert!(requests[5].starts_with("POST /tools/get-grades-summary "));
        assert!(requests[6].contains(r#""range":"week""#));
    }

    #[test]
    fn expired_keys_and_blocks_do_not_expose_secrets() {
        for (code, body) in [
            (401, "test-private-token"),
            (403, "test-private-token"),
            (409, r#"{"error":"complete form test-private-token"}"#),
        ] {
            let (config, worker) = server(vec![(code, body.into())]);
            let error = OmnivoxClient::new(config).status().unwrap_err();
            assert!(!error.contains("test-private-token"));
            worker.join().unwrap();
        }
    }

    #[test]
    fn rejects_html_catalogs_and_unknown_gateways() {
        let (config, worker) = server(vec![(200, "<html>login</html>".into())]);
        assert!(OmnivoxClient::new(config).status().is_err());
        worker.join().unwrap();
        let (config, worker) = server(vec![(200, r#"{"tools":[{"name":"other"}]}"#.into())]);
        assert!(OmnivoxClient::new(config).status().is_err());
        worker.join().unwrap();
    }

    #[test]
    fn activation_failure_preserves_existing_credentials() {
        let (config, worker) = server(vec![(401, "{}".into())]);
        let mut settings = crate::config::Settings {
            omnivox: Some(crate::config::OmnivoxSettings {
                url: Some("http://127.0.0.1:3999".into()),
                token: Some("old-token".into()),
            }),
            ..Default::default()
        };
        assert!(activate(&mut settings, &config.base_url, &config.token).is_err());
        assert_eq!(settings.omnivox().unwrap().token, "old-token");
        worker.join().unwrap();
    }

    #[test]
    fn activation_persists_only_after_the_gateway_catalog_is_verified() {
        crate::config::tests::with_temp_config(|| {
            let (config, worker) = server(vec![
                (200, catalog_body()),
                (200, r#"{"college":"Demo"}"#.into()),
            ]);
            let mut settings = crate::config::Settings::default();
            activate(&mut settings, &config.base_url, &config.token).unwrap();
            let saved = crate::config::Settings::load();
            assert_eq!(saved.omnivox().unwrap().token, config.token);
            assert_eq!(saved.services_active(), vec!["omnivox"]);
            assert!(crate::mcp::tool_definitions(&saved)
                .iter()
                .any(|tool| tool["name"] == "omnivox_my_courses"));
            worker.join().unwrap();
        });
    }

    #[test]
    fn mcp_dispatch_reads_courses_and_rejects_invalid_course_ids() {
        let (config, worker) = server(vec![(
            200,
            r#"{"courses":[{"id":"2434K5EM.1011"}]}"#.into(),
        )]);
        let settings = crate::config::Settings {
            omnivox: Some(crate::config::OmnivoxSettings {
                url: Some(config.base_url),
                token: Some(config.token),
            }),
            ..Default::default()
        };
        let reply = crate::mcp::dispatch_tool(&settings, "omnivox_my_courses", &json!({}));
        assert_eq!(reply["isError"], false);
        let courses: Value =
            serde_json::from_str(reply["content"][0]["text"].as_str().unwrap()).unwrap();
        assert_eq!(courses["courses"][0]["id"], "2434K5EM.1011");
        worker.join().unwrap();
        for args in [
            json!({}),
            json!({"course_id":""}),
            json!({"course_id":"../users"}),
            json!({"course_id":"a/b"}),
        ] {
            assert_eq!(
                crate::mcp::dispatch_tool(&settings, "omnivox_course_documents", &args)["isError"],
                true
            );
        }
    }

    #[test]
    fn college_login_stores_session_cookies_and_calls_mobile_json() {
        crate::config::tests::with_temp_config(|| {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let base = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
            let worker = std::thread::spawn(move || {
                let mut requests = Vec::new();
                for (status, extra, body) in [
                    (
                        200,
                        "",
                        r#"<form action="/intr/Module/Identification/Login/Login.aspx" method="post"><input name="k" value="99" /><input name="Identifiant" /><input name="Password" type="password" /><input name="TypeIdentification" value="" /></form>"#,
                    ),
                    (
                        302,
                        "Location: /intr/\r\nSet-Cookie: SidLea=abc; Path=/\r\nSet-Cookie: Mobile_AuthentificationASPX=tok; Path=/\r\n",
                        "",
                    ),
                    (200, "", "<html><body>portal</body></html>"),
                    (200, "", r#"{"ListeCours":[{"NoCours":"2434K5EM"}]}"#),
                ] {
                    let (mut stream, _) = listener.accept().unwrap();
                    stream
                        .set_read_timeout(Some(Duration::from_secs(5)))
                        .unwrap();
                    let mut reader = BufReader::new(stream.try_clone().unwrap());
                    let mut request = String::new();
                    let mut content_length = 0usize;
                    loop {
                        let mut line = String::new();
                        if reader.read_line(&mut line).unwrap() == 0 || line == "\r\n" {
                            break;
                        }
                        if let Some(value) = line
                            .split_once(':')
                            .filter(|(name, _)| name.eq_ignore_ascii_case("content-length"))
                            .map(|(_, value)| value.trim().parse().unwrap_or(0))
                        {
                            content_length = value;
                        }
                        request.push_str(&line);
                    }
                    if content_length > 0 {
                        let mut payload = vec![0; content_length];
                        reader.read_exact(&mut payload).unwrap();
                        request.push_str(&String::from_utf8_lossy(&payload));
                    }
                    requests.push(request);
                    write!(
                        stream,
                        "HTTP/1.1 {status} Test\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n{extra}\r\n{body}",
                        body.len()
                    )
                    .unwrap();
                }
                requests
            });
            let token = login_session(&base, "1234567", "secret-pass").unwrap();
            assert!(token.contains("SidLea=abc"));
            assert!(!token.contains("secret-pass"));
            let mut settings = crate::config::Settings::default();
            persist(&mut settings, &base, &token).unwrap();
            let requests = worker.join().unwrap();
            assert!(requests[1].contains("Identifiant=1234567"));
            assert!(requests[1].contains("Password=secret-pass"));
            assert!(requests[1].contains("TypeIdentification=Etudiant"));
            assert!(requests[3].starts_with("POST /Mobl/LeaCommun/GetDefaultModel "));
            assert!(requests[3].to_lowercase().contains("cookie:"));
        });
    }
}
