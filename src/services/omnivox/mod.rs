//! Read-only Omnivox client.
//! College login talks to the official site and the `/Mobl/` JSON endpoints.
//! A self-hosted [omnivox-mcp](https://github.com/Beat-YT/omnivox-mcp) gateway remains supported.
use crate::config::{is_omnivox_host, OmnivoxConfig};
use crate::session;
use cookie_store::CookieStore;
use serde_json::{json, Value};
use std::time::Duration;

const REQUIRED_TOOL: &str = "get-courses-summary";

// The official Omnivox mobile client identifies itself this way. Web and
// mobile logins produce different sessions; `/Mobl/` only accepts the mobile
// one, so the whole login flow must use this user agent.
const MOBILE_DEVICE: &str = "Android-14_samsung_SM-X700_Build/UP1A.231005.007";
const MOBILE_APP_VERSION: &str = "3.8.9";

fn new_device_id() -> String {
    crate::config::random_hex(20)
}

fn mobile_user_agent(device_id: &str, auth_hash: &str) -> String {
    format!(
        "OVX InfoDevice={MOBILE_DEVICE} AppVer={MOBILE_APP_VERSION} IdAppareil={device_id} Code={auth_hash}"
    )
}

fn omnivox_cookie_domain(url: &str) -> Option<&'static str> {
    let host = url::Url::parse(url).ok()?.host_str()?.to_ascii_lowercase();
    (host == "omnivox.ca" || host.ends_with(".omnivox.ca")).then_some("omnivox.ca")
}

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
        let user_agent = match self.config.device_id.as_deref() {
            Some(device_id) => mobile_user_agent(
                device_id,
                self.config.auth_hash.as_deref().unwrap_or_default(),
            ),
            // Sessions saved before the mobile flow used the browser agent.
            None => session::BROWSER_UA.to_string(),
        };
        request
            .set("Cookie", &self.config.token)
            .set("Accept", "application/json, text/plain")
            .set("Origin", &self.config.base_url)
            .set("Referer", &format!("{}/", self.config.base_url))
            .set("User-Agent", &user_agent)
            .set("X-User-Agent-Ovx", &user_agent)
            .set("X-Requested-With", "XMLHttpRequest")
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
        let response = match self
            .authorize_college(
                self.agent
                    .post(&format!("{}/Mobl/{path}", self.config.base_url)),
            )
            .set("Content-Type", "application/json")
            .send_json(body)
        {
            Ok(response) | Err(ureq::Error::Status(_, response)) => response,
            Err(_) => {
                return Err(
                    "Cannot reach Omnivox. Check your Internet connection and try again.".into(),
                )
            }
        };
        if response.status() != 200 {
            return Err(match response.status() {
                401 | 403 => "Omnivox refused the session. Sign in again in the app.".into(),
                300..=399 => "Omnivox redirected the request. Sign in again in the app.".into(),
                status => format!("Omnivox returned HTTP {status}. Try again."),
            });
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
                .ok_or("Omnivox did not confirm access to Léa. Sign in again in the app.")?;
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
                None => self.mobl(
                    "LeaEtudiant/GetTravauxSommaireModel",
                    json!({ "AnSession": "" }),
                ),
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
                None => self.mobl(
                    "LeaEtudiant/GetNotesSommaireModel",
                    json!({ "AnSession": "" }),
                ),
            };
        }
        match course_id {
            Some(course_id) => self.call("get-course-evals", json!({ "course_id": course_id })),
            None => self.call("get-grades-summary", json!({})),
        }
    }

    pub fn schedule(&self) -> Result<Value, String> {
        if self.is_college() {
            return self.mobl(
                "Horaire/GetHoraireModel",
                json!({ "AnSession": Value::Null }),
            );
        }
        self.call("get-schedule", json!({}))
    }

    pub fn calendar(&self, range: Option<&str>) -> Result<Value, String> {
        if self.is_college() {
            if range.is_some() {
                return Err("Date range filters are only supported by the Omnivox MCP gateway. Omit range to read the college's current calendar page.".into());
            }
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
        return Err(
            "Provide course_id as returned by omnivox_my_courses, e.g. 2434K5EM.1011.".into(),
        );
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
    persist(settings, &base_url, token, None, None)
}

pub fn activate_login(
    settings: &mut crate::config::Settings,
    url: &str,
    username: &str,
    password: &str,
) -> Result<LoginOutcome, String> {
    if username.trim().is_empty() || password.is_empty() {
        return Err("Enter your Omnivox student number and password.".into());
    }
    let base_url = crate::config::normalize_omnivox_url(url)?;
    if !is_omnivox_host(&base_url) {
        return Err("Use your college Omnivox address, e.g. https://cegepxxx.omnivox.ca.".into());
    }
    login_session(settings, &base_url, username.trim(), password)
}

pub enum LoginOutcome {
    Connected(String),
    VerificationRequired(Box<PendingLogin>),
}

/// The pending session never contains the password and stays in memory until
/// verification succeeds or the user cancels.
pub struct PendingLogin {
    session: LoginSession,
    method_id: String,
    pub prompt: String,
    pub can_resend: bool,
    pub expired: bool,
}

impl PendingLogin {
    pub fn verify(
        &mut self,
        settings: &mut crate::config::Settings,
        code: &str,
    ) -> Result<String, String> {
        if self.expired {
            return Err("The Omnivox verification session expired. Sign in again.".into());
        }
        let code = code.trim();
        if code.len() != 6 || !code.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err("Enter the 6-digit security code from Omnivox.".into());
        }
        let page_url = url::Url::parse(&self.session.current)
            .map_err(|_| "Invalid Omnivox verification address.")?;
        let tksm_cookie = self
            .session
            .jar
            .get_request_values(&page_url)
            .find(|(name, _)| name.starts_with("TKSM"))
            .map(|(name, value)| (name.to_string(), decode_cookie(value)));
        let tksm = tksm_cookie
            .as_ref()
            .map(|(_, value)| value.as_str())
            .unwrap_or_default();
        let reply = self.api(
            "/api/mfa/login/validate/code",
            Some(json!({
                "idUserValidationMethod": self.method_id,
                "tksm": tksm,
                "code": code,
                "saveDevice": false
            })),
        )?;
        match reply.get("result").and_then(Value::as_str) {
            Some("Valid") => {
                // The code is consumed. A failure while completing the handoff
                // must restart login, rather than submit that code a second time.
                self.expired = true;
                let target = reply.get("url")
                    .and_then(Value::as_str)
                    .filter(|url| !url.is_empty())
                    .ok_or("Omnivox did not return a login continuation. Sign in again.")?;
                let target = self.session.begin_handoff(target)?;
                let olt = reply.get("olt")
                    .and_then(Value::as_str)
                    .filter(|token| !token.is_empty())
                    .ok_or("Omnivox did not return a login token. Sign in again.")?;
                // The official MFA client writes this cookie in JavaScript
                // before navigating to reply.url (it is not a Set-Cookie).
                // It uses domain=omnivox.ca, which also covers a handoff to
                // another college subdomain.
                let domain = omnivox_cookie_domain(&self.session.current)
                    .map(|domain| format!("; Domain={domain}"))
                    .unwrap_or_default();
                self.session.jar.parse(&format!("olt={}; Path=/{domain}", encode_cookie(olt)), &page_url)
                    .map_err(|_| "Omnivox returned an invalid login cookie. Sign in again.")?;
                if let Some((name, _)) = tksm_cookie {
                    let _ = self.session.jar.parse(&format!("{name}=; Path=/; Max-Age=0"), &page_url);
                }
                // The mobile login handoff puts its persistent token in the
                // fragment, which is client-side only.
                self.session.remember_fragment_token(&target)?;
                let response = self.session.get(&target)?;
                let html = self.session.follow(response, target)?;
                self.session.check_authenticated_page(&html)?;
                // A web session is not accepted by /Mobl/. Complete the mobile
                // initialisation that also yields the AuthHash for the UA.
                self.session.establish_mobile_session()?;
                persist(
                    settings,
                    &self.session.base_url,
                    &self.session.cookie_header()?,
                    Some(&self.session.device_id),
                    Some(&self.session.auth_hash),
                )
            }
            Some("SessionExpired") => {
                self.expired = true;
                Err("The Omnivox verification session expired. Sign in again.".into())
            }
            Some("TooManyAttempts") => {
                self.expired = true;
                Err("Omnivox blocked further code attempts. Wait a few minutes, then sign in again.".into())
            }
            Some(_) => Err("The security code is invalid or has expired. Try the latest code or request a new one.".into()),
            None => Err("Omnivox did not return a verification result. Try again.".into()),
        }
    }

    pub fn resend(&mut self) -> Result<String, String> {
        if self.expired || !self.can_resend {
            return Err("A new code cannot be requested for this verification session.".into());
        }
        let reply = self.api(
            "/api/mfa/login/resend-code",
            Some(json!({ "idUserValidationMethod": self.method_id })),
        )?;
        match reply.get("result").and_then(Value::as_str) {
            Some("SessionExpired") => {
                self.expired = true;
                Err("The Omnivox verification session expired. Sign in again.".into())
            }
            Some("TooManyResend") => Err("Omnivox has reached its code resend limit. Wait a few minutes before requesting another code.".into()),
            Some(_) => Ok("A new security code was sent. Use the most recent code.".into()),
            None => Err("Omnivox did not confirm sending a new code. Try again.".into()),
        }
    }

    fn api(&mut self, path: &str, body: Option<Value>) -> Result<Value, String> {
        match self.session.json(path, body) {
            Ok(value) => Ok(value),
            Err((expired, message)) => {
                self.expired |= expired;
                Err(message)
            }
        }
    }
}

fn decode_cookie(value: &str) -> String {
    // Cookie decoding is decodeURIComponent, not form decoding: '+' is literal.
    url::form_urlencoded::parse(format!("v={}", value.replace('+', "%2B")).as_bytes())
        .next()
        .map(|(_, value)| value.into_owned())
        .unwrap_or_default()
}

fn encode_cookie(value: &str) -> String {
    url::form_urlencoded::byte_serialize(value.as_bytes())
        .collect::<String>()
        .replace('+', "%20")
}

fn persist(
    settings: &mut crate::config::Settings,
    base_url: &str,
    token: &str,
    device_id: Option<&str>,
    auth_hash: Option<&str>,
) -> Result<String, String> {
    // An empty AuthHash is not a value; keep the key absent instead.
    let auth_hash = auth_hash.filter(|hash| !hash.trim().is_empty());
    let client = OmnivoxClient::new(OmnivoxConfig {
        base_url: base_url.to_string(),
        token: token.into(),
        device_id: device_id.map(str::to_string),
        auth_hash: auth_hash.map(str::to_string),
    });
    client.status()?;
    let mut updated = settings.clone();
    updated.omnivox = Some(crate::config::OmnivoxSettings {
        url: Some(base_url.into()),
        token: Some(token.into()),
        device_id: device_id.map(str::to_string),
        auth_hash: auth_hash.map(str::to_string),
    });
    updated.save_service("omnivox").map_err(|error| {
        format!("Cannot save Omnivox credentials to the private config file: {error}")
    })?;
    *settings = updated;
    Ok(if is_omnivox_host(base_url) {
        "Omnivox connected (read only).".into()
    } else {
        "Omnivox connected (read only, through the MCP gateway).".into()
    })
}

struct LoginSession {
    base_url: String,
    current: String,
    device_id: String,
    auth_hash: String,
    agent: ureq::Agent,
    jar: CookieStore,
}

impl LoginSession {
    /// Only a successful MFA reply may switch the college origin. Password/code
    /// POSTs and subsequent redirects remain restricted to the selected origin.
    fn begin_handoff(&mut self, target: &str) -> Result<String, String> {
        let target = session::resolve_url(&self.current, target)?;
        let base = url::Url::parse(&self.base_url).map_err(|_| "Invalid Omnivox address.")?;
        let next = url::Url::parse(&target).map_err(|_| "Invalid Omnivox redirect.")?;
        let official = |url: &url::Url| {
            url.scheme() == "https"
                && url.port_or_known_default() == Some(443)
                && crate::config::is_omnivox_host(url.as_str())
        };
        if !next.username().is_empty()
            || next.password().is_some()
            || (base.origin() != next.origin() && !(official(&base) && official(&next)))
        {
            return Err("Omnivox redirected the login away from the college site.".into());
        }
        self.base_url = next.origin().ascii_serialization();
        Ok(target)
    }

    fn resolve(&self, target: &str) -> Result<String, String> {
        let next = session::resolve_url(&self.current, target)?;
        let base = url::Url::parse(&self.base_url).map_err(|_| "Invalid Omnivox address.")?;
        let parsed = url::Url::parse(&next).map_err(|_| "Invalid Omnivox redirect.")?;
        if base.origin() != parsed.origin()
            || !parsed.username().is_empty()
            || parsed.password().is_some()
        {
            return Err("Omnivox redirected the login away from the college site.".into());
        }
        Ok(next)
    }

    fn user_agent(&self) -> String {
        mobile_user_agent(&self.device_id, &self.auth_hash)
    }

    fn get(&self, target: &str) -> Result<ureq::Response, String> {
        let target = self.resolve(target)?;
        let user_agent = self.user_agent();
        send(
            self.agent
                .get(&target)
                .set("Accept", "text/html,application/xhtml+xml")
                .set("User-Agent", &user_agent)
                .set("X-User-Agent-Ovx", &user_agent)
                .set("Cookie", &self.cookies_for(&target)?),
        )
    }

    fn merge_cookies(&mut self, response: &ureq::Response, target: &str) -> Result<(), String> {
        let url = url::Url::parse(target).map_err(|_| "Invalid Omnivox cookie address.")?;
        for header in response.all("set-cookie") {
            // In particular, the MFA page expires `omt` with a nonempty value.
            // Replaying that deletion as a live cookie makes the MFA API return 401.
            let _ = self.jar.parse(header, &url);
        }
        Ok(())
    }

    fn cookies_for(&self, target: &str) -> Result<String, String> {
        let url = url::Url::parse(target).map_err(|_| "Invalid Omnivox cookie address.")?;
        Ok(self
            .jar
            .get_request_values(&url)
            .map(|(name, value)| format!("{name}={value}"))
            .collect::<Vec<_>>()
            .join("; "))
    }

    fn remember_login_token(&mut self, target: &str, token: &str) -> Result<(), String> {
        if token.is_empty() {
            return Err("Omnivox returned an invalid login token. Sign in again.".into());
        }
        let url = url::Url::parse(target).map_err(|_| "Invalid Omnivox login address.")?;
        self.jar
            .parse(
                &format!(
                    "LoginTokenAuthentification={}; Path=/",
                    encode_cookie(token)
                ),
                &url,
            )
            .map_err(|_| "Omnivox returned an invalid login cookie. Sign in again.".to_string())?;
        Ok(())
    }

    /// The mobile app receives its persistent login token in the URL fragment
    /// (`#...Token=...`) and writes it as a cookie in JavaScript. Fragments are
    /// never sent to the server, so the HTTP client must do that write itself.
    fn remember_fragment_token(&mut self, target: &str) -> Result<(), String> {
        let Some(fragment) = url::Url::parse(target)
            .ok()
            .and_then(|url| url.fragment().map(str::to_string))
        else {
            return Ok(());
        };
        let token = url::form_urlencoded::parse(fragment.as_bytes())
            .find(|(name, value)| name.eq_ignore_ascii_case("token") && !value.is_empty())
            .map(|(_, value)| value.into_owned());
        match token {
            Some(token) => self.remember_login_token(target, &token),
            None => Ok(()),
        }
    }

    /// The mobile app calls /Mobl/App/Initialisation right after login. It
    /// returns the AuthHash that must replace `Code=` in the OVX user agent,
    /// and proves that the session is valid for the mobile API.
    fn establish_mobile_session(&mut self) -> Result<(), String> {
        let model = self
            .json("/Mobl/App/Initialisation", Some(json!({})))
            .map_err(|(_, message)| message)?;
        if model
            .get("NaviguerPage")
            .and_then(Value::as_str)
            .is_some_and(|page| page.to_ascii_lowercase().contains("error"))
        {
            return Err(
                "Omnivox blocked the request. Complete any required document or survey in the portal, then retry."
                    .into(),
            );
        }
        // A logged-out call still returns 200 with every field null, so the
        // body must prove the session: a user, an offer, or an AuthHash.
        let hash = model
            .get("AuthHash")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|hash| !hash.is_empty() && hash.len() <= 256)
            .filter(|hash| hash.bytes().all(|byte| byte.is_ascii_graphic()));
        let connected = hash.is_some()
            || model.get("Utilisateur").is_some_and(|user| !user.is_null())
            || model
                .get("OffreService")
                .is_some_and(|offer| !offer.is_null())
            || model
                .get("Username")
                .and_then(Value::as_str)
                .is_some_and(|name| !name.trim().is_empty());
        if !connected {
            return Err(
                "Omnivox did not confirm the mobile session. Sign in again in the app.".into(),
            );
        }
        if let Some(hash) = hash {
            self.auth_hash = hash.to_string();
        }
        Ok(())
    }

    fn follow(
        &mut self,
        mut response: ureq::Response,
        mut current: String,
    ) -> Result<String, String> {
        for _ in 0..=8 {
            self.remember_fragment_token(&current)?;
            self.merge_cookies(&response, &current)?;
            self.current = current;
            if (300..400).contains(&response.status()) {
                let location = response
                    .header("location")
                    .ok_or("Omnivox returned an incomplete redirect.")?;
                current = self.resolve(location)?;
                // Never replay a password or a code, including on 307/308.
                response = self.get(&current)?;
            } else if response.status() == 200 {
                return response
                    .into_string()
                    .map_err(|_| "Omnivox returned an unreadable login page.".into());
            } else {
                return Err(format!(
                    "Omnivox returned HTTP {} during login. Try again.",
                    response.status()
                ));
            }
        }
        Err("Omnivox returned too many login redirects. Try again.".into())
    }

    fn json(&mut self, path: &str, body: Option<Value>) -> Result<Value, (bool, String)> {
        let target = self.resolve(path).map_err(|message| (false, message))?;
        let user_agent = self.user_agent();
        let mut request = if body.is_some() {
            self.agent.post(&target)
        } else {
            self.agent.get(&target)
        }
        .set("Accept", "application/json")
        .set("Origin", &self.base_url)
        .set("Referer", &self.current)
        .set("User-Agent", &user_agent)
        .set("X-User-Agent-Ovx", &user_agent)
        .set("X-Requested-With", "XMLHttpRequest")
        .set(
            "Cookie",
            &self
                .cookies_for(&target)
                .map_err(|message| (false, message))?,
        );
        let page_url = url::Url::parse(&self.current)
            .map_err(|_| (false, "Invalid Omnivox verification address.".into()))?;
        if let Some((_, token)) = self
            .jar
            .get_request_values(&page_url)
            .find(|(name, _)| *name == "XSRF-REQUEST-TOKEN")
        {
            request = request.set("X-XSRF-TOKEN", &decode_cookie(token));
        }
        let response = match body {
            Some(body) => request.send_json(body),
            None => request.call(),
        };
        let response = match response {
            Ok(response) | Err(ureq::Error::Status(_, response)) => response,
            Err(_) => {
                return Err((
                    false,
                    "Cannot reach Omnivox. Check your Internet connection and try again.".into(),
                ))
            }
        };
        self.merge_cookies(&response, &target)
            .map_err(|message| (false, message))?;
        if matches!(response.status(), 401 | 403) {
            return Err((
                true,
                format!(
                    "Omnivox refused the sign-in request (HTTP {}). Sign in again.",
                    response.status()
                ),
            ));
        }
        if (300..400).contains(&response.status()) {
            return Err((
                true,
                "Omnivox redirected the sign-in request. Sign in again.".into(),
            ));
        }
        if response.status() != 200 {
            return Err((
                false,
                format!(
                    "Omnivox returned HTTP {} during verification. Try again.",
                    response.status()
                ),
            ));
        }
        response.into_json().map_err(|_| {
            (
                false,
                "Omnivox did not return a verification response. Try again.".into(),
            )
        })
    }

    fn check_authenticated_page(&self, html: &str) -> Result<(), String> {
        if omnivox_mfa_page(&self.current, html) {
            return Err(
                "Omnivox still requires identity validation. Sign in again to request a new code."
                    .into(),
            );
        }
        // Only a login page means the credentials were refused. Portal pages
        // can embed harmless password inputs (account settings, hidden modals).
        if omnivox_login_page(&self.current) && session::html_has_password_field(html) {
            return Err(
                "Omnivox refused the login. Check your student number and password.".into(),
            );
        }
        Ok(())
    }

    fn cookie_header(&self) -> Result<String, String> {
        let header = self.cookies_for(&format!("{}/Mobl/", self.base_url))?;
        if header.is_empty() {
            return Err("Omnivox did not return a session cookie.".into());
        }
        Ok(header)
    }
}

fn login_session(
    settings: &mut crate::config::Settings,
    base_url: &str,
    username: &str,
    password: &str,
) -> Result<LoginOutcome, String> {
    let device_id = new_device_id();
    let login_agent = mobile_user_agent(&device_id, "");
    let mut session = LoginSession {
        base_url: base_url.into(),
        // The mobile app signs in through this page. The same credentials on
        // /Login/Account/Login create a web session that /Mobl/ rejects.
        current: format!(
            "{base_url}/intr/Module/Identification/Login/Mobile/LoginMobile.aspx?typeUtilisateur=Etudiant"
        ),
        device_id: device_id.clone(),
        auth_hash: String::new(),
        agent: ureq::AgentBuilder::new()
            .timeout(Duration::from_secs(30))
            .redirects(0)
            .user_agent(&login_agent)
            .build(),
        jar: CookieStore::default(),
    };
    let response = session.get(&session.current)?;
    let html = session.follow(response, session.current.clone())?;
    if !session::html_has_password_field(&html) {
        return Err(
            "This Omnivox site did not show a student password form. Check the college address."
                .into(),
        );
    }
    let (action, body) = omnivox_login_form(&html, username, password)?;
    let post_url = session.resolve(&action)?;
    // These two cookies are written by LoginMobile.js before submitting. They
    // tell Omnivox to open a mobile session instead of a web session.
    let page_url = url::Url::parse(&post_url).map_err(|_| "Invalid Omnivox login address.")?;
    let _ = session.jar.parse("ForceWeb=false; Path=/", &page_url);
    let _ = session
        .jar
        .parse("IndicateurNouveauLogin=true; Path=/", &page_url);
    let user_agent = session.user_agent();
    let posted = send_form(
        session
            .agent
            .post(&post_url)
            .set("Content-Type", "application/x-www-form-urlencoded")
            .set("Cookie", &session.cookies_for(&post_url)?)
            .set("Origin", base_url)
            .set("Referer", &session.current)
            .set("User-Agent", &user_agent)
            .set("X-User-Agent-Ovx", &user_agent),
        &body,
    )?;
    let html = session.follow(posted, post_url)?;
    if let Some(method_id) = verification_method_id(&session.current) {
        let model = session
            .json(&format!("/api/mfa/login/validate-method/{method_id}"), None)
            .map_err(|(_, message)| message)?;
        let method_type = model
            .get("methodType")
            .and_then(Value::as_str)
            .unwrap_or("");
        let destination = model.get("data").and_then(Value::as_str).unwrap_or("");
        let prompt = match method_type {
            "E" | "E2" => format!(
                "Enter the 6-digit security code sent by email{}.",
                if destination.is_empty() {
                    String::new()
                } else {
                    format!(" to {destination}")
                }
            ),
            "M" => "Enter the 6-digit security code sent to your Omnivox Mobile app.".into(),
            "T" | "J" => "Enter the 6-digit code from your authenticator or security token.".into(),
            "CT" => "Enter the 6-digit security code provided by your college.".into(),
            _ => {
                return Err(
                    "This Omnivox identity verification method is not supported by the app.".into(),
                )
            }
        };
        return Ok(LoginOutcome::VerificationRequired(Box::new(PendingLogin {
            session,
            method_id,
            prompt,
            can_resend: matches!(method_type, "E" | "E2" | "M"),
            expired: false,
        })));
    }
    session.check_authenticated_page(&html)?;
    session.establish_mobile_session()?;
    persist(
        settings,
        base_url,
        &session.cookie_header()?,
        Some(&session.device_id),
        Some(&session.auth_hash),
    )
    .map(LoginOutcome::Connected)
}

fn verification_method_id(current: &str) -> Option<String> {
    let parsed = url::Url::parse(current).ok()?;
    let id = parsed
        .path()
        .strip_prefix("/apps/mfa/login/validate-method/")?;
    (!id.is_empty()
        && id.len() <= 256
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_'))
    .then(|| id.to_string())
}

fn omnivox_login_page(current: &str) -> bool {
    let path = url::Url::parse(current)
        .map(|url| url.path().to_ascii_lowercase())
        .unwrap_or_default();
    path.contains("loginmobile.aspx") || path.contains("login.aspx") || path.contains("/login/")
}

fn omnivox_mfa_page(current: &str, html: &str) -> bool {
    let path = url::Url::parse(current)
        .map(|url| url.path().to_ascii_lowercase())
        .unwrap_or_default();
    if path.starts_with("/apps/mfa") || path.contains("/mfa/") {
        return true;
    }
    // Portal menus can link to MFA settings after a successful login. Only
    // treat the page as a challenge when the challenge copy AND a six-digit
    // input are both present.
    let lower = html.to_lowercase();
    let challenge_copy = lower.contains("validation d'identité")
        || lower.contains("validation d’identité")
        || lower.contains("code de sécurité")
        || lower.contains("code de securite");
    challenge_copy
        && (lower.contains("6 chiffres")
            || lower.contains("6-digit")
            || lower.contains("inputmode=\"numeric\"")
            || lower.contains("maxlength=\"6\""))
}

fn first_form(html: &str) -> &str {
    let lower = html.to_ascii_lowercase();
    let Some(start) = lower.find("<form") else {
        return html;
    };
    let rest = &html[start..];
    let end = rest
        .to_ascii_lowercase()
        .find("</form>")
        .unwrap_or(rest.len());
    &rest[..end]
}

fn omnivox_login_form(
    html: &str,
    username: &str,
    password: &str,
) -> Result<(String, String), String> {
    let mut action = session::html_form_action(html)
        .filter(|action| !action.is_empty())
        .unwrap_or_else(|| "/intr/Module/Identification/Login/Login.aspx".into());
    // The mobile login page contains student and employee forms. Only submit
    // the first one, which is the student form on this page.
    let mut pairs = session::html_form_inputs(first_form(html));
    if pairs.is_empty() {
        return Err("The Omnivox login page did not include a form.".into());
    }
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis().to_string())
        .unwrap_or_default();
    let mut has_user = false;
    let mut has_pass = false;
    // The current portal renames Identifiant/Password in RoleClick JavaScript.
    // Reproduce the student role instead of posting the uninitialized HTML names.
    let student_role = html
        .split('<')
        .filter_map(|part| part.split_once('>').map(|(tag, _)| tag))
        .find(|tag| {
            session::html_attr(tag, "data-type-utilisateur").as_deref() == Some("Etudiant")
        });
    let user_name = student_role.and_then(|tag| session::html_attr(tag, "data-name-identifiant"));
    let password_name = student_role.and_then(|tag| session::html_attr(tag, "data-name-password"));
    if student_role.is_some() {
        action.push_str(if action.contains('?') {
            "&t=Etudiant"
        } else {
            "?t=Etudiant"
        });
    }
    for (name, value) in &mut pairs {
        let lower = name.to_ascii_lowercase();
        if matches!(lower.as_str(), "identifiant" | "noda" | "username") {
            *value = username.to_string();
            if let Some(student_name) = &user_name {
                *name = student_name.clone();
            }
            has_user = true;
        }
        if lower.contains("password") {
            *value = password.to_string();
            if let Some(student_name) = &password_name {
                *name = student_name.clone();
            }
            has_pass = true;
        }
        if lower == "typeidentification" && value.trim().is_empty() {
            *value = "Etudiant".into();
        }
        if lower == "typelogin" && value.trim().is_empty() {
            *value = "PostSolutionLogin".into();
        }
        if lower == "devicetimestamp" && value.trim().is_empty() {
            *value = now_ms.clone();
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

fn send(request: ureq::Request) -> Result<ureq::Response, String> {
    match request.call() {
        Ok(response) | Err(ureq::Error::Status(_, response)) => Ok(response),
        Err(_) => Err(
            "Cannot reach Omnivox. Check the college address and your Internet connection.".into(),
        ),
    }
}

fn send_form(request: ureq::Request, body: &str) -> Result<ureq::Response, String> {
    match request.send_string(body) {
        Ok(response) | Err(ureq::Error::Status(_, response)) => Ok(response),
        Err(_) => Err(
            "Cannot reach Omnivox. Check the college address and your Internet connection.".into(),
        ),
    }
}

pub fn tools() -> Vec<Value> {
    let mut tools = vec![
        json!({"name":"omnivox_status","description":"Check the connected Omnivox / Léa account."}),
        json!({"name":"omnivox_my_courses","description":"List the connected student's Omnivox / Léa courses for the current term. course_id values are strings (e.g. 2434K5EM.1011), not integers."}),
        json!({"name":"omnivox_schedule","description":"Get the connected student's static weekly Omnivox timetable."}),
        json!({"name":"omnivox_course_documents","description":"List Léa documents for one Omnivox course. Returns metadata only; there is no omnivox_file reader yet.","inputSchema":{"type":"object","properties":{"course_id":{"type":"string","description":"String course id from omnivox_my_courses, e.g. 2434K5EM.1011."}},"required":["course_id"]}}),
        json!({"name":"omnivox_assignments","description":"List Omnivox assignments. Omit course_id for a summary of every course.","inputSchema":{"type":"object","properties":{"course_id":{"type":"string","description":"String course id from omnivox_my_courses."}}}}),
        json!({"name":"omnivox_grades","description":"Get Omnivox grades. Omit course_id for a summary of every course.","inputSchema":{"type":"object","properties":{"course_id":{"type":"string","description":"String course id from omnivox_my_courses."}}}}),
        json!({"name":"omnivox_calendar","description":"College calendar page: holidays and posted deadlines. Optional range is supported only via the MCP gateway; omit it for a direct college login. Prefer this for posted dates; there is no omnivox_deadlines aggregator yet.","inputSchema":{"type":"object","properties":{"range":{"type":"string","enum":["today","week","month","all"]}}}}),
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
pub(crate) mod tests {
    use super::*;
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::TcpListener;

    fn server(
        responses: Vec<(u16, String)>,
    ) -> (OmnivoxConfig, std::thread::JoinHandle<Vec<String>>) {
        login_server(
            responses
                .into_iter()
                .map(|(status, body)| (status, String::new(), body))
                .collect(),
        )
    }

    fn login_server(
        responses: Vec<(u16, String, String)>,
    ) -> (OmnivoxConfig, std::thread::JoinHandle<Vec<String>>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let config = OmnivoxConfig {
            base_url: format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port()),
            token: "test-private-token".into(),
            device_id: None,
            auth_hash: None,
        };
        let worker = std::thread::spawn(move || {
            let mut requests = Vec::new();
            for (status, extra, body) in responses {
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
                    "HTTP/1.1 {status} Test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n{extra}\r\n{body}",
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

    // Sanitized shape of the current Maisonneuve login HTML and MFA SPA/API.
    const STUDENT_FORM: &str = r#"
        <a data-type-utilisateur="Etudiant" data-name-identifiant="NoDA" data-name-password="PasswordEtu">Étudiants</a>
        <form action="/intr/Module/Identification/Login/Login.aspx" method="post">
        <input name="k" value="test-form-key"><input name="TypeLogin" value="PostSolutionLogin">
        <input name="TypeIdentification" value=""><input name="Identifiant">
        <input name="Password" type="password"></form>"#;

    fn challenge_responses() -> Vec<(u16, String, String)> {
        vec![
            (200, "Set-Cookie: preauth=pending; Path=/\r\n".into(), STUDENT_FORM.into()),
            (302, "Location: /apps/mfa/login/validate-method/test-method\r\n".into(), String::new()),
            (200, "Set-Cookie: XSRF-REQUEST-TOKEN=csrf%2Bvalue%3D; Path=/\r\nSet-Cookie: TKSMTESTP=device%2Btoken; Path=/\r\n".into(),
                r#"<div id="app"></div><script src="/apps/mfa/login.validate-method.js"></script>"#.into()),
            (200, String::new(), json!({"idUserValidationMethod":"test-method","methodType":"E","data":"s***@example.test"}).to_string()),
        ]
    }

    fn pending_login(settings: &mut crate::config::Settings, base: &str) -> Box<PendingLogin> {
        match login_session(settings, base, "1234567", "secret-pass").unwrap() {
            LoginOutcome::VerificationRequired(pending) => pending,
            LoginOutcome::Connected(_) => panic!("MFA must not activate an account"),
        }
    }

    #[test]
    fn mfa_code_form_drops_deleted_cookies_and_preserves_cookie_paths() {
        let mut responses = challenge_responses();
        responses[0].1.push_str(
            "Set-Cookie: sameName=keep-root; Path=/\r\n\
             Set-Cookie: sameName=login-only; Path=/Login\r\n",
        );
        responses[1].1.push_str(
            "Set-Cookie: omt=temporary-login-token; Path=/\r\n\
             Set-Cookie: removeByAge=temporary; Path=/\r\n",
        );
        // Real Omnivox behavior: a deletion with a NONEMPTY value. Keeping it
        // caused an immediate 401 on GET /api/mfa/login/validate-method/{id}.
        responses[2].1.push_str(
            "Set-Cookie: omt=deleted-marker; Path=/; Expires=Thu, 01-Jan-1970 05:00:00 GMT\r\n\
             Set-Cookie: removeByAge=deleted-marker; Path=/; Max-Age=0; Expires=Thu, 01-Jan-2099 00:00:00 GMT\r\n\
             Set-Cookie: sameName=deleted-marker; Path=/Login; Max-Age=0\r\n\
             Set-Cookie: api_key=api-only; Path=/api\r\n",
        );
        let (config, worker) = login_server(responses);
        let pending = pending_login(&mut crate::config::Settings::default(), &config.base_url);
        assert!(pending.prompt.contains("s***@example.test"));
        let requests = worker.join().unwrap();
        assert!(requests[2].contains("omt=temporary-login-token"));
        let api_request = &requests[3];
        assert!(api_request.starts_with("GET /api/mfa/login/validate-method/test-method "));
        assert!(!api_request.contains("omt="));
        assert!(!api_request.contains("removeByAge="));
        assert!(!api_request.contains("deleted-marker"));
        assert!(!api_request.contains("login-only"));
        assert!(api_request.contains("sameName=keep-root"));
        assert!(api_request.contains("api_key=api-only"));
        let saved_header = pending.session.cookie_header().unwrap();
        assert!(saved_header.contains("sameName=keep-root"));
        assert!(!saved_header.contains("api_key="));
        assert!(!saved_header.contains("omt="));
    }

    #[test]
    fn deleting_host_cookie_preserves_parent_domain_cookie() {
        let url = url::Url::parse("https://college.omnivox.ca/apps/mfa/login").unwrap();
        let mut login = LoginSession {
            base_url: "https://college.omnivox.ca".into(),
            current: url.to_string(),
            device_id: new_device_id(),
            auth_hash: String::new(),
            agent: ureq::AgentBuilder::new()
                .user_agent(session::BROWSER_UA)
                .build(),
            jar: CookieStore::default(),
        };
        login
            .jar
            .parse("k=parent; Domain=omnivox.ca; Path=/", &url)
            .unwrap();
        login.jar.parse("k=host; Path=/", &url).unwrap();
        login
            .jar
            .parse("k=deleted; Path=/; Max-Age=0", &url)
            .unwrap();
        let header = login
            .cookies_for("https://college.omnivox.ca/api/mfa/login")
            .unwrap();
        assert_eq!(header, "k=parent");
        assert!(login.resolve("https://other.omnivox.ca/intr/").is_err());
        for target in [
            "https://omnivox.ca.evil.example/",
            "http://other.omnivox.ca/",
            "https://other.omnivox.ca:8443/",
            "https://user@other.omnivox.ca/",
        ] {
            assert!(login.begin_handoff(target).is_err());
        }
        let next = login
            .begin_handoff("https://other.omnivox.ca/intr/")
            .unwrap();
        assert_eq!(login.base_url, "https://other.omnivox.ca");
        assert_eq!(login.cookies_for(&next).unwrap(), "k=parent");
        assert!(login.resolve(&next).is_ok());
    }

    #[test]
    fn mobile_login_form_uses_only_the_student_form_and_sets_the_device_time() {
        const MOBILE_FORM: &str = r#"
            <form method="post" name="formEtudiant" action="/intr/Module/Identification/Login/Login.aspx?Mobile=True&amp;typeUtilisateur=Etudiant">
              <input type="hidden" name="TypeIdentification" value="Etudiant">
              <input type="hidden" name="LoginMobile" value="1">
              <input type="hidden" name="k" value="mobile-key">
              <input type="hidden" name="DeviceTimestamp" value="">
              <input type="text" name="NoDA">
              <input type="password" name="PasswordEtu">
            </form>
            <form method="post" name="formEmployeNormal" action="/intr/Module/Identification/Login/Login.aspx">
              <input type="hidden" name="TypeIdentification" value="EmployeNormal">
              <input type="text" name="NoEmplEmployeNormal">
              <input type="password" name="PasswordEmplEmployeNormal">
            </form>"#;
        let (action, body) = omnivox_login_form(MOBILE_FORM, "1234567", "secret-pass").unwrap();
        assert_eq!(
            action,
            "/intr/Module/Identification/Login/Login.aspx?Mobile=True&typeUtilisateur=Etudiant"
        );
        assert!(body.contains("NoDA=1234567"));
        assert!(body.contains("PasswordEtu=secret-pass"));
        assert!(body.contains("LoginMobile=1"));
        assert!(body.contains("k=mobile-key"));
        assert!(!body.contains("PasswordEmplEmployeNormal"));
        assert!(!body.contains("NoEmplEmployeNormal"));
        assert!(body.contains("DeviceTimestamp="));
        assert!(!body.ends_with("DeviceTimestamp="));
    }

    #[test]
    fn mobile_user_agent_and_fragment_token_survive_the_handoff() {
        let device_id = "0123456789abcdef0123456789abcdef01234567";
        let user_agent = mobile_user_agent(device_id, "auth-hash");
        assert!(user_agent.starts_with("OVX InfoDevice=Android-14_samsung"));
        assert!(user_agent.contains("AppVer=3.8.9"));
        assert!(user_agent.contains(&format!("IdAppareil={device_id}")));
        assert!(user_agent.ends_with("Code=auth-hash"));

        let url = url::Url::parse("https://college.omnivox.ca/apps/mfa/login").unwrap();
        let mut login = LoginSession {
            base_url: "https://college.omnivox.ca".into(),
            current: url.to_string(),
            device_id: device_id.into(),
            auth_hash: String::new(),
            agent: ureq::AgentBuilder::new().build(),
            jar: CookieStore::default(),
        };
        login
            .remember_fragment_token(
                "https://college.omnivox.ca/Mobl/Home#TempsInstallation=1&Token=abc%2Bdef",
            )
            .unwrap();
        let header = login
            .cookies_for("https://college.omnivox.ca/Mobl/LeaCommun/GetDefaultModel")
            .unwrap();
        assert!(header.contains("LoginTokenAuthentification=abc%2Bdef"));
        assert_eq!(
            omnivox_cookie_domain("https://cmaisonneuve.omnivox.ca/apps/mfa/login"),
            Some("omnivox.ca")
        );
        assert_eq!(omnivox_cookie_domain("http://127.0.0.1:3000"), None);
    }

    #[test]
    fn college_mobl_reports_a_refused_session_without_gateway_wording() {
        let (mut config, worker) = server(vec![(401, String::new())]);
        config.token = "SidLea=abc; Mobile=tok".into();
        config.device_id = Some("0123456789abcdef0123456789abcdef01234567".into());
        config.auth_hash = Some("hash".into());
        let error = OmnivoxClient::new(config).status().unwrap_err();
        assert_eq!(
            error,
            "Omnivox refused the session. Sign in again in the app."
        );
        worker.join().unwrap();
    }

    #[test]
    fn mobile_initialisation_rejects_a_logged_out_body() {
        let (config, worker) = login_server(vec![(
            200,
            String::new(),
            r#"{"Utilisateur":null,"AuthHash":null,"TypeUtilisateur":0,"OID":null}"#.into(),
        )]);
        let mut login = LoginSession {
            base_url: config.base_url.clone(),
            current: format!("{}/Mobl/", config.base_url),
            device_id: "0123456789abcdef0123456789abcdef01234567".into(),
            auth_hash: String::new(),
            agent: ureq::AgentBuilder::new().redirects(0).build(),
            jar: CookieStore::default(),
        };
        let error = login.establish_mobile_session().unwrap_err();
        assert!(
            error.contains("did not confirm the mobile session"),
            "{error}"
        );
        assert!(login.auth_hash.is_empty());
        worker.join().unwrap();
    }

    #[test]
    #[ignore = "Signs in to a real Omnivox account and can send an email security code"]
    fn live_omnivox_email_verification_form() {
        let url = std::env::var("STUDENT_MCP_TEST_OMNIVOX_URL")
            .expect("Set STUDENT_MCP_TEST_OMNIVOX_URL");
        let username = std::env::var("STUDENT_MCP_TEST_OMNIVOX_USERNAME")
            .expect("Set STUDENT_MCP_TEST_OMNIVOX_USERNAME");
        let password = std::env::var("STUDENT_MCP_TEST_OMNIVOX_PASSWORD")
            .expect("Set STUDENT_MCP_TEST_OMNIVOX_PASSWORD");
        crate::config::tests::with_temp_config(|| {
            let mut settings = crate::config::Settings::default();
            let outcome = activate_login(&mut settings, &url, &username, &password).unwrap();
            match outcome {
                LoginOutcome::VerificationRequired(pending) => {
                    assert!(pending.prompt.contains("sent by email"));
                    assert!(pending.can_resend);
                    assert!(!pending.expired);
                    assert!(settings.omnivox.is_none());
                    assert!(crate::config::Settings::load().omnivox.is_none());
                }
                LoginOutcome::Connected(_) => panic!("Expected the email verification step"),
            }
        });
    }

    #[test]
    fn email_code_retries_resend_and_handoff_keep_the_same_session() {
        crate::config::tests::with_temp_config(|| {
            let mut settings = crate::config::Settings {
                omnivox: Some(crate::config::OmnivoxSettings {
                    url: Some("https://old.omnivox.ca".into()),
                    token: Some("old-session".into()),
                    ..Default::default()
                }),
                ..Default::default()
            };
            settings.save().unwrap();
            let before = std::fs::read(crate::config::config_path()).unwrap();
            let mut responses = challenge_responses();
            responses.extend([
                (200, "Set-Cookie: XSRF-REQUEST-TOKEN=rotated%2Bcsrf; Path=/\r\n".into(), r#"{"result":"Invalid"}"#.into()),
                (200, String::new(), r#"{"result":"Sent"}"#.into()),
                (200, String::new(), r#"{"result":"Valid","olt":"one+use/=","url":"/intr/continue"}"#.into()),
                (302, "Location: /intr/\r\nSet-Cookie: SidLea=abc; Path=/\r\nSet-Cookie: Mobile_AuthentificationASPX=tok; Path=/\r\n".into(), String::new()),
                (200, String::new(), r#"<html>portal <a href="/apps/mfa/settings">2FA settings</a></html>"#.into()),
                (200, String::new(), r#"{"Username":"1234567","AuthHash":"mobile-hash"}"#.into()),
                (200, String::new(), r#"{"ListeCours":[]}"#.into()),
            ]);
            let (config, worker) = login_server(responses);
            let mut pending = pending_login(&mut settings, &config.base_url);
            assert!(pending.prompt.contains("s***@example.test"));
            assert!(pending.can_resend);
            assert_eq!(std::fs::read(crate::config::config_path()).unwrap(), before);
            // Malformed input must not consume a server-side attempt.
            assert!(pending
                .verify(&mut settings, "12345")
                .unwrap_err()
                .contains("6-digit"));
            assert!(pending
                .verify(&mut settings, "000000")
                .unwrap_err()
                .contains("invalid"));
            assert!(!pending.expired);
            pending.resend().unwrap();
            assert_eq!(std::fs::read(crate::config::config_path()).unwrap(), before);
            pending.verify(&mut settings, " 123456 ").unwrap();
            let saved = crate::config::Settings::load().omnivox.unwrap();
            let token = saved.token.unwrap();
            assert!(token.contains("SidLea=abc"));
            assert!(!token.contains("TKSMTESTP"));
            let persisted = std::fs::read_to_string(crate::config::config_path()).unwrap();
            for secret in ["secret-pass", "1234567", "123456", "000000"] {
                assert!(!persisted.contains(secret));
            }
            let requests = worker.join().unwrap();
            assert_eq!(requests.len(), 11);
            assert!(requests[0].starts_with(
                "GET /intr/Module/Identification/Login/Mobile/LoginMobile.aspx?typeUtilisateur=Etudiant "
            ));
            assert!(requests[1]
                .starts_with("POST /intr/Module/Identification/Login/Login.aspx?t=Etudiant "));
            let body = requests[1].lines().last().unwrap();
            assert!(body.contains("NoDA=1234567"));
            assert!(body.contains("PasswordEtu=secret-pass"));
            assert!(!body.contains("Identifiant="));
            assert!(!body.contains("Password="));
            assert!(requests[3].starts_with("GET /api/mfa/login/validate-method/test-method "));
            assert!(requests[4].contains("\"tksm\":\"device+token\""));
            assert!(requests[4]
                .to_lowercase()
                .contains("x-xsrf-token: csrf+value="));
            assert!(requests[5].starts_with("POST /api/mfa/login/resend-code "));
            assert!(requests[6]
                .to_lowercase()
                .contains("x-xsrf-token: rotated+csrf"));
            assert!(requests[6].contains("\"code\":\"123456\""));
            assert!(requests[7].starts_with("GET /intr/continue "));
            assert!(requests[7].contains("olt=one%2Buse%2F%3D"));
            assert!(requests[9].starts_with("POST /Mobl/App/Initialisation "));
            assert!(requests[10].starts_with("POST /Mobl/LeaCommun/GetDefaultModel "));
            assert_eq!(pending.session.auth_hash, "mobile-hash");
            let device_id = pending.session.device_id.clone();
            for (index, request) in requests.iter().enumerate() {
                assert!(request.contains("OVX InfoDevice="));
                assert!(request.contains(&format!("IdAppareil={device_id}")));
                assert!(request
                    .to_lowercase()
                    .contains("x-user-agent-ovx: ovx infodevice="));
                if index > 1 {
                    assert!(request.contains("preauth=pending"));
                    assert!(!request.contains("secret-pass"));
                    assert!(!request.lines().next().unwrap().contains("123456"));
                }
            }
        });
    }

    #[test]
    fn email_code_session_expiry_limits_and_network_errors_preserve_saved_login() {
        crate::config::tests::with_temp_config(|| {
            for (status, body, expired, expected) in [
                (200, r#"{"result":"SessionExpired"}"#, true, "expired"),
                (200, r#"{"result":"TooManyAttempts"}"#, true, "blocked"),
                (401, "", true, "refused"),
                (403, "", true, "refused"),
                (302, "", true, "redirected"),
                (503, "", false, "503"),
                (200, "<html>error</html>", false, "response"),
            ] {
                let mut responses = challenge_responses();
                responses.push((status, String::new(), body.into()));
                let (config, worker) = login_server(responses);
                let mut settings = crate::config::Settings::default();
                let mut pending = pending_login(&mut settings, &config.base_url);
                assert!(pending
                    .verify(&mut settings, "123456")
                    .unwrap_err()
                    .contains(expected));
                assert_eq!(pending.expired, expired);
                assert!(settings.omnivox.is_none());
                assert!(crate::config::Settings::load().omnivox.is_none());
                worker.join().unwrap();
            }
        });
    }

    #[test]
    fn email_code_resend_limits_keep_the_challenge_for_the_current_code() {
        let mut responses = challenge_responses();
        responses.extend([
            (200, String::new(), r#"{"result":"TooManyResend"}"#.into()),
            (200, String::new(), r#"{"result":"SessionExpired"}"#.into()),
        ]);
        let (config, worker) = login_server(responses);
        let mut settings = crate::config::Settings::default();
        let mut pending = pending_login(&mut settings, &config.base_url);
        assert!(pending.resend().unwrap_err().contains("limit"));
        assert!(!pending.expired);
        assert!(pending.resend().unwrap_err().contains("expired"));
        assert!(pending.expired);
        worker.join().unwrap();
    }

    #[test]
    fn email_code_handoff_rejects_external_targets_and_unverified_sessions() {
        crate::config::tests::with_temp_config(|| {
            for (reply, continuation, expected) in [
                (
                    r#"{"result":"Valid","olt":"token","url":"https://elsewhere.invalid/"}"#,
                    vec![],
                    "away",
                ),
                (
                    r#"{"result":"Valid","olt":"token","url":"/intr/"}"#,
                    vec![
                        (200, String::new(), "<html>portal</html>".into()),
                        (
                            200,
                            String::new(),
                            r#"{"NaviguerPage":"/apps/mfa/login"}"#.into(),
                        ),
                    ],
                    "confirm the mobile session",
                ),
            ] {
                let mut responses = challenge_responses();
                responses.push((200, String::new(), reply.into()));
                responses.extend(continuation);
                let (config, worker) = login_server(responses);
                let mut settings = crate::config::Settings::default();
                let mut pending = pending_login(&mut settings, &config.base_url);
                assert!(pending
                    .verify(&mut settings, "123456")
                    .unwrap_err()
                    .contains(expected));
                assert!(pending.expired);
                assert!(crate::config::Settings::load().omnivox.is_none());
                worker.join().unwrap();
            }
        });
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
                ..Default::default()
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
                ..Default::default()
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
                    (200, "", r#"{"Username":"1234567","AuthHash":"hash"}"#),
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
            let mut settings = crate::config::Settings::default();
            assert!(matches!(
                login_session(&mut settings, &base, "1234567", "secret-pass").unwrap(),
                LoginOutcome::Connected(_)
            ));
            let saved = settings.omnivox.unwrap();
            assert_eq!(saved.auth_hash.as_deref(), Some("hash"));
            assert!(saved.device_id.as_deref().is_some_and(|id| id.len() == 40));
            let token = saved.token.unwrap();
            assert!(token.contains("SidLea=abc"));
            assert!(!token.contains("secret-pass"));
            let requests = worker.join().unwrap();
            assert!(requests[1].contains("Identifiant=1234567"));
            assert!(requests[1].contains("Password=secret-pass"));
            assert!(requests[1].contains("TypeIdentification=Etudiant"));
            assert!(requests[3].starts_with("POST /Mobl/App/Initialisation "));
            assert!(requests[4].starts_with("POST /Mobl/LeaCommun/GetDefaultModel "));
            assert!(requests[4].to_lowercase().contains("cookie:"));
            assert!(requests[4].contains("X-User-Agent-Ovx: OVX InfoDevice="));
            assert!(requests[4].contains("Code=hash"));
        });
    }
}
