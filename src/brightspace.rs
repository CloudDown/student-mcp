//! Read-only Brightspace Valence API.
//! School login stores session cookies; an OAuth token still works if the school issued one.
//! https://docs.valence.desire2learn.com/
use crate::config::BrightspaceConfig;
use crate::session;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::time::Duration;

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

    fn get(&self, path: &str, bookmark: Option<&str>) -> Result<Value, String> {
        let mut request =
            self.authorize(self.agent.get(&format!("{}{path}", self.config.base_url)));
        if let Some(bookmark) = bookmark {
            request = request.query("bookmark", bookmark);
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
        let info = self.get("/d2l/api/lp/1.49/users/whoami", None)?;
        if info.get("Identifier").is_none_or(Value::is_null) {
            return Err("Brightspace did not identify the connected account.".into());
        }
        Ok(info)
    }

    /// Return one page, including PagingInfo so callers can request subsequent pages.
    pub fn courses(&self, bookmark: Option<&str>) -> Result<Value, String> {
        self.get(
            "/d2l/api/lp/1.49/enrollments/myenrollments/?orgUnitTypeId=3",
            bookmark,
        )
    }

    pub fn content(&self, courseid: i64) -> Result<Value, String> {
        self.course_get(courseid, "content/toc")
    }

    pub fn grades(&self, courseid: i64) -> Result<Value, String> {
        self.course_get(courseid, "grades/values/myGradeValues/")
    }

    fn course_get(&self, courseid: i64, resource: &str) -> Result<Value, String> {
        if courseid <= 0 {
            return Err("Provide an integer courseid > 0.".into());
        }
        self.get(&format!("/d2l/api/le/1.82/{courseid}/{resource}"), None)
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
            "Enter your Brightspace username and password, or a session cookie / access token in advanced options.".into(),
        );
    }
    if !session::session_cookie_auth(token) && token.chars().any(char::is_whitespace) {
        return Err(
            "Enter an access token without spaces, or session cookies such as d2lSessionVal=…; d2lSecureSessionVal=….".into(),
        );
    }
    persist(settings, url, token)
}

pub fn activate_login(
    settings: &mut crate::config::Settings,
    url: &str,
    username: &str,
    password: &str,
) -> Result<String, String> {
    if username.trim().is_empty() || password.is_empty() {
        return Err("Enter your Brightspace username and password.".into());
    }
    let base_url = crate::config::normalize_brightspace_url(url)?;
    let token = login_session(&base_url, username.trim(), password)?;
    persist(settings, &base_url, &token)
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
    updated.save().map_err(|error| {
        format!("Cannot save Brightspace credentials to the private config file: {error}")
    })?;
    *settings = updated;
    Ok("Brightspace connected (read only).".into())
}

fn login_session(base_url: &str, username: &str, password: &str) -> Result<String, String> {
    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(30))
        .redirects(0)
        .user_agent(session::BROWSER_UA)
        .build();
    let mut jar = BTreeMap::new();
    let mut current = format!("{base_url}/d2l/login");
    let mut html = String::new();
    for _ in 0..8 {
        let request = agent
            .get(&current)
            .set("Accept", "text/html,application/xhtml+xml")
            .set("Cookie", &session::cookie_header(&jar));
        let response = send(request)?;
        session::merge_set_cookie(&mut jar, &response);
        let status = response.status();
        if let Some(location) = response.header("location").map(str::to_string) {
            let next = session::resolve_url(&current, &location)?;
            if session::is_sso_host(&next) {
                return Err(sso_message());
            }
            if !session::same_host(base_url, &next) {
                return Err(sso_message());
            }
            if session::looks_like_mfa(&next, "") {
                return Err(mfa_message());
            }
            current = next;
            if (300..400).contains(&status) {
                continue;
            }
        }
        if !(200..300).contains(&status) {
            return Err("Cannot reach the Brightspace login page. Check the site address.".into());
        }
        html = response
            .into_string()
            .map_err(|_| "Brightspace did not return a login page.".to_string())?;
        break;
    }
    if session::looks_like_mfa(&current, &html) {
        return Err(mfa_message());
    }
    if !session::html_has_password_field(&html) {
        return Err(sso_message());
    }
    let (action, body) = session::fill_login_form(
        &html,
        username,
        password,
        &["userName", "username", "login", "loginId", "d2l_username"],
    )?;
    let post_url = if action.trim().is_empty() {
        format!("{base_url}/d2l/lp/auth/login/Login.d2l")
    } else {
        session::resolve_url(&current, &action)?
    };
    if !session::same_host(base_url, &post_url) {
        return Err(sso_message());
    }
    current = post_url;
    let request = agent
        .post(&current)
        .set("Content-Type", "application/x-www-form-urlencoded")
        .set("Cookie", &session::cookie_header(&jar))
        .set("Origin", base_url)
        .set("Referer", &format!("{base_url}/d2l/login"));
    let response = send_form(request, &body)?;
    session::merge_set_cookie(&mut jar, &response);
    let mut location = response.header("location").map(str::to_string);
    let mut body_html = response.into_string().unwrap_or_default();
    for _ in 0..8 {
        let Some(target) = location.take() else {
            break;
        };
        let next = session::resolve_url(&current, &target)?;
        if session::is_sso_host(&next) || !session::same_host(base_url, &next) {
            return Err(sso_message());
        }
        if session::looks_like_mfa(&next, "") {
            return Err(mfa_message());
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
        return Err(mfa_message());
    }
    if session::html_has_password_field(&body_html) {
        return Err("Brightspace refused the login. Check your credentials; with single sign-on or two-factor authentication, paste session cookies in advanced options.".into());
    }
    let header = session::cookie_header(&jar);
    if session::csrf_from_cookies(&header).is_none() {
        return Err("Brightspace did not return a session cookie. Try advanced options with cookies copied from your browser.".into());
    }
    Ok(header)
}

fn sso_message() -> String {
    "This Brightspace site uses single sign-on. Open the site in your browser, sign in, then paste d2lSessionVal and d2lSecureSessionVal in advanced options.".into()
}

fn mfa_message() -> String {
    "Brightspace asked for two-factor authentication. Complete it in the browser, then paste the session cookies in advanced options.".into()
}

fn send(request: ureq::Request) -> Result<ureq::Response, String> {
    match request.call() {
        Ok(response) | Err(ureq::Error::Status(_, response)) => Ok(response),
        Err(_) => Err(
            "Cannot reach Brightspace. Check the site address and your Internet connection.".into(),
        ),
    }
}

fn send_form(request: ureq::Request, body: &str) -> Result<ureq::Response, String> {
    match request.send_string(body) {
        Ok(response) | Err(ureq::Error::Status(_, response)) => Ok(response),
        Err(_) => Err(
            "Cannot reach Brightspace. Check the site address and your Internet connection.".into(),
        ),
    }
}

pub fn tools() -> Vec<Value> {
    let mut tools = vec![
        json!({"name":"brightspace_status", "description":"Check the connected Brightspace / myCourses account."}),
        json!({"name":"brightspace_my_courses", "description":"List one page of the connected student's Brightspace courses. Use PagingInfo.Bookmark when HasMoreItems is true.", "inputSchema":{"type":"object", "properties":{"bookmark":{"type":"string"}}}}),
    ];
    for (name, description) in [
        (
            "brightspace_course_content",
            "Get the table of contents and resources of a Brightspace course.",
        ),
        (
            "brightspace_grades",
            "Get the connected student's visible grades for a Brightspace course.",
        ),
    ] {
        tools.push(json!({"name":name,"description":description,"inputSchema":{"type":"object","properties":{"courseid":{"type":"integer","minimum":1}},"required":["courseid"]}}));
    }
    for tool in &mut tools {
        tool["annotations"] = json!({"readOnlyHint":true});
    }
    tools
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::TcpListener;

    fn server(
        responses: Vec<(u16, String)>,
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
                write!(stream, "HTTP/1.1 {status} Test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
            }
            requests
        });
        (config, worker)
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
        ]);
        let client = BrightspaceClient::new(config);
        assert_eq!(client.whoami().unwrap()["Identifier"], "123");
        assert_eq!(
            client.courses(Some("a&b")).unwrap()["PagingInfo"]["HasMoreItems"],
            true
        );
        client.content(42).unwrap();
        client.grades(42).unwrap();
        let requests = worker.join().unwrap();
        for request in &requests {
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
        assert!(requests[0].starts_with("GET /d2l/api/lp/1.49/users/whoami "));
        assert!(requests[1].contains("orgUnitTypeId=3&bookmark=a%26b"));
        assert!(requests[2].starts_with("GET /d2l/api/le/1.82/42/content/toc "));
        assert!(requests[3].starts_with("GET /d2l/api/le/1.82/42/grades/values/myGradeValues/ "));
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
    fn school_login_stores_session_cookies_not_the_password() {
        crate::config::tests::with_temp_config(|| {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let base = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
            let worker = std::thread::spawn(move || {
                let mut requests = Vec::new();
                for (status, extra, body) in [
                    (
                        200,
                        "",
                        r#"<form action="/d2l/lp/auth/login/Login.d2l" method="post"><input name="userName" /><input name="password" type="password" /></form>"#,
                    ),
                    (
                        302,
                        "Location: /d2l/home\r\nSet-Cookie: d2lSessionVal=sess; Path=/\r\nSet-Cookie: d2lSecureSessionVal=secure; Path=/; HttpOnly\r\n",
                        "",
                    ),
                    (200, "", "<html><body>home</body></html>"),
                    (200, "", r#"{"Identifier":"123"}"#),
                    (
                        200,
                        "",
                        r#"{"Items":[],"PagingInfo":{"HasMoreItems":false}}"#,
                    ),
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
            let mut settings = crate::config::Settings::default();
            activate_login(&mut settings, &base, "student", "secret-pass").unwrap();
            let saved = crate::config::Settings::load().brightspace().unwrap();
            assert!(saved.token.contains("d2lSessionVal=sess"));
            assert!(saved.token.contains("d2lSecureSessionVal=secure"));
            assert!(!saved.token.contains("secret-pass"));
            let requests = worker.join().unwrap();
            assert!(requests[1].contains("userName=student"));
            assert!(requests[1].contains("password=secret-pass"));
            assert!(requests[2].to_lowercase().contains("cookie:"));
        });
    }

    #[test]
    fn sso_login_pages_are_refused_without_posting_the_password() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
        let worker = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            loop {
                let mut line = String::new();
                if reader.read_line(&mut line).unwrap() == 0 || line == "\r\n" {
                    break;
                }
            }
            write!(
                stream,
                "HTTP/1.1 302 Test\r\nLocation: https://login.microsoftonline.com/common\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
            )
            .unwrap();
        });
        let mut settings = crate::config::Settings::default();
        let error = activate_login(&mut settings, &base, "student", "secret-pass").unwrap_err();
        assert!(error.to_lowercase().contains("single sign-on"), "{error}");
        assert!(settings.brightspace().is_none());
        worker.join().unwrap();
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
                .any(|tool| tool["name"] == "brightspace_my_courses"));
            worker.join().unwrap();
        });
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
