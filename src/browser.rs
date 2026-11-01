//! Account-owned Chromium profiles. Only the local configuration UI can open
//! an interactive login; MCP calls resume saved sessions without showing windows.
use base64::Engine;
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::HashSet,
    fs::{self, File, OpenOptions},
    io,
    net::TcpStream,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc, Mutex,
    },
    time::{Duration, Instant},
};
use tungstenite::{Message, WebSocket};

const PREFIX: &str = "browser-profile:";
const RECONNECT: &str = "Browser session needs attention. Open student-mcp and choose Change login → Connect with browser.";
const ACCOUNT_CHANGED: &str = "The browser is signed into a different account. Add it as a new account, or disconnect this one before reconnecting.";
const MOBILE_LOGIN: &str =
    "/intr/Module/Identification/Login/Mobile/LoginMobile.aspx?typeUtilisateur=Etudiant";

pub fn is_profile(token: &str) -> bool {
    token.starts_with(PREFIX)
}

fn profile_path(token: &str) -> Result<PathBuf, String> {
    let id = token
        .strip_prefix(PREFIX)
        .ok_or("Invalid browser profile.")?;
    if id.len() != 32 || !id.bytes().all(|c| c.is_ascii_hexdigit()) {
        return Err("Invalid browser profile.".into());
    }
    Ok(crate::config::config_path()
        .with_extension("browsers")
        .join(id))
}

fn private_dir(path: &Path) -> io::Result<()> {
    fs::create_dir_all(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

fn save(path: &Path, value: &impl Serialize) -> Result<(), String> {
    // These files are inside an owner-only directory and protected by the profile lock.
    let tmp = path.with_extension("tmp");
    let bytes = serde_json::to_vec(value).map_err(|_| "Cannot encode browser state.")?;
    fs::write(&tmp, bytes).map_err(|_| "Cannot save browser state.")?;
    #[cfg(windows)]
    if path.exists() {
        fs::remove_file(path).map_err(|_| "Cannot replace browser state.")?;
    }
    fs::rename(tmp, path).map_err(|_| "Cannot replace browser state.".into())
}

#[derive(Serialize, Deserialize)]
struct Profile {
    service: String,
    origin: String,
    home: String,
    mobile: Value,
    #[serde(default)]
    identity: Option<String>,
}

fn executable() -> Result<PathBuf, String> {
    if let Some(path) = std::env::var_os("STUDENT_MCP_BROWSER") {
        let path = PathBuf::from(path);
        return if path.is_absolute() && path.is_file() {
            Ok(path)
        } else {
            Err("STUDENT_MCP_BROWSER must be an absolute path to Chrome, Chromium or Edge.".into())
        };
    }
    let mut candidates = vec![
        PathBuf::from("/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"),
        PathBuf::from("/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge"),
        PathBuf::from("/Applications/Chromium.app/Contents/MacOS/Chromium"),
    ];
    for root in ["PROGRAMFILES", "PROGRAMFILES(X86)", "LOCALAPPDATA"] {
        if let Some(root) = std::env::var_os(root) {
            for name in [
                "Google/Chrome/Application/chrome.exe",
                "Microsoft/Edge/Application/msedge.exe",
            ] {
                candidates.push(PathBuf::from(&root).join(name));
            }
        }
    }
    if let Some(paths) = std::env::var_os("PATH") {
        for path in std::env::split_paths(&paths) {
            for name in [
                "chromium",
                "chromium-browser",
                "google-chrome",
                "google-chrome-stable",
                "microsoft-edge",
                "chrome.exe",
                "msedge.exe",
            ] {
                candidates.push(path.join(name));
            }
        }
    }
    candidates.into_iter().find(|p| p.is_file()).ok_or_else(|| {
        "Install Chrome, Chromium or Edge to connect with the browser, then try again.".into()
    })
}

pub struct Reply {
    pub status: u16,
    pub mime: String,
    pub disposition: String,
    pub bytes: Vec<u8>,
}

impl Reply {
    pub fn json(self) -> Result<Value, String> {
        if !(200..300).contains(&self.status) {
            return Err(match self.status {
                401 => "School refused the session (HTTP 401). Reconnect with the browser if needed."
                    .into(),
                403 => {
                    "School denied access (HTTP 403). Check course permissions; reconnect if the session expired."
                        .into()
                }
                status => format!(
                    "School returned HTTP {status}. Check the session and course permissions."
                ),
            });
        }
        serde_json::from_slice(&self.bytes).map_err(|_| {
            "School did not return JSON. Reconnect with the browser if sign-in is required.".into()
        })
    }
}

/// Lazy, per-tool session: one browser covers all calls made by a connector.
/// The OS profile lock also serializes independent stdio and HTTP processes.
pub struct Connection {
    service: &'static str,
    origin: String,
    token: String,
    browser: Mutex<Option<Browser>>,
}

impl Connection {
    pub fn new(service: &'static str, origin: &str, token: &str) -> Self {
        Self {
            service,
            origin: origin.into(),
            token: token.into(),
            browser: Mutex::new(None),
        }
    }

    pub fn enabled(&self) -> bool {
        is_profile(&self.token)
    }

    pub fn request(
        &self,
        method: &str,
        path: &str,
        body: Option<&[u8]>,
        content_type: Option<&str>,
        binary: bool,
        limit: usize,
    ) -> Result<Reply, String> {
        let mut guard = self
            .browser
            .lock()
            .map_err(|_| "Browser session unavailable.")?;
        if guard.is_none() {
            let mut browser = Browser::launch(&self.token, self.service, &self.origin, false)?;
            browser.wait_ready(&AtomicBool::new(false), Duration::from_secs(25))?;
            *guard = Some(browser);
        }
        guard
            .as_mut()
            .unwrap()
            .request(method, path, body, content_type, binary, limit)
    }
}

struct Browser {
    socket: Option<WebSocket<TcpStream>>,
    child: Child,
    _lock: File,
    dir: PathBuf,
    profile: Profile,
    sequence: u64,
    mobile_contexts: HashSet<u64>,
}

impl Browser {
    fn launch(token: &str, service: &str, origin: &str, visible: bool) -> Result<Self, String> {
        let exe = executable()?;
        let dir = profile_path(token)?;
        private_dir(dir.parent().unwrap()).map_err(|_| "Cannot create browser storage.")?;
        private_dir(&dir).map_err(|_| "Cannot create browser profile.")?;
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(dir.join("profile.lock"))
            .map_err(|_| "Cannot lock browser profile.")?;
        let until = Instant::now() + Duration::from_secs(30);
        while lock.try_lock_exclusive().is_err() {
            if Instant::now() >= until {
                return Err(
                    "This account's browser is busy. Finish its sign-in and try again.".into(),
                );
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        let metadata = dir.join("session.json");
        let profile = if metadata.exists() {
            let profile: Profile = serde_json::from_slice(
                &fs::read(&metadata).map_err(|_| "Cannot read browser profile.")?,
            )
            .map_err(|_| "Browser profile is invalid; reconnect using a new account.")?;
            if profile.service != service || profile.origin != origin {
                return Err("Browser profile belongs to another site or service.".into());
            }
            profile
        } else if visible {
            let profile = Profile {
                service: service.into(),
                origin: origin.into(),
                home: format!(
                    "{origin}{}",
                    if service == "omnivox" {
                        MOBILE_LOGIN
                    } else {
                        "/d2l/home"
                    }
                ),
                mobile: json!({"device":crate::config::random_hex(20),"code":"","info":{}}),
                identity: None,
            };
            save(&metadata, &profile)?;
            profile
        } else {
            return Err(RECONNECT.into());
        };
        // A new process must never attach to a stale debug endpoint.
        let active_port = dir.join("DevToolsActivePort");
        let _ = fs::remove_file(&active_port);
        let mut command = Command::new(exe);
        command
            .arg(format!("--user-data-dir={}", dir.display()))
            .args([
                "--remote-debugging-port=0",
                "--remote-debugging-address=127.0.0.1",
                "--no-first-run",
                "--no-default-browser-check",
                "--disable-sync",
                "--disable-background-mode",
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        if !visible {
            command.arg("--headless=new");
        }
        command.arg("about:blank");
        let child = command
            .spawn()
            .map_err(|_| "Cannot launch Chrome/Chromium/Edge.")?;
        let mut browser = Self {
            socket: None,
            child,
            _lock: lock,
            dir,
            profile,
            sequence: 0,
            mobile_contexts: HashSet::new(),
        };
        let until = Instant::now() + Duration::from_secs(15);
        let agent = ureq::AgentBuilder::new()
            .redirects(0)
            .timeout(Duration::from_secs(2))
            .build();
        loop {
            if browser.child.try_wait().ok().flatten().is_some() {
                return Err(
                    "Browser exited. Check that a desktop display is available for sign-in.".into(),
                );
            }
            if let Ok(text) = fs::read_to_string(&active_port) {
                if let Some(port) = text.lines().next().and_then(|s| s.parse::<u16>().ok()) {
                    if let Ok(response) = agent
                        .get(&format!("http://127.0.0.1:{port}/json/list"))
                        .call()
                    {
                        if let Ok(targets) = response.into_json::<Vec<Value>>() {
                            if let Some(ws) = targets
                                .iter()
                                .find(|t| t["type"] == "page")
                                .and_then(|t| t["webSocketDebuggerUrl"].as_str())
                            {
                                let url =
                                    url::Url::parse(ws).map_err(|_| "Invalid browser endpoint.")?;
                                if url.scheme() != "ws"
                                    || url.host_str() != Some("127.0.0.1")
                                    || url.port() != Some(port)
                                {
                                    return Err("Invalid browser endpoint.".into());
                                }
                                let stream = TcpStream::connect(("127.0.0.1", port))
                                    .map_err(|_| "Cannot reach browser.")?;
                                stream.set_read_timeout(Some(Duration::from_secs(35))).ok();
                                stream.set_write_timeout(Some(Duration::from_secs(35))).ok();
                                // A 12 MB file becomes ~16 MB once base64-encoded,
                                // above tungstenite's default 16 MiB frame limit.
                                let config = tungstenite::protocol::WebSocketConfig::default()
                                    .max_message_size(Some(32 << 20))
                                    .max_frame_size(Some(24 << 20));
                                let (socket, _) = tungstenite::client::client_with_config(
                                    ws,
                                    stream,
                                    Some(config),
                                )
                                .map_err(|_| "Cannot connect to browser.")?;
                                browser.socket = Some(socket);
                                break;
                            }
                        }
                    }
                }
            }
            if Instant::now() >= until {
                return Err("Browser did not start in time.".into());
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        browser.call("Page.enable", json!({}))?;
        browser.call("Runtime.enable", json!({}))?;
        browser.call("Network.enable", json!({}))?;
        // Chrome discards session-only cookies on a clean exit. Restore those
        // explicitly; persistent cookies remain owned by the Chromium profile.
        if let Ok(bytes) = fs::read(browser.dir.join("cookies.json")) {
            let cookies: Vec<Value> =
                serde_json::from_slice(&bytes).map_err(|_| "Invalid saved browser cookies.")?;
            if !cookies.is_empty() {
                browser.call("Network.setCookies", json!({"cookies":cookies}))?;
            }
        }
        if service == "omnivox" {
            browser.call("Runtime.addBinding", json!({"name":"studentMobileState"}))?;
            let script = include_str!("resources/browser/omnivox.js").replace(
                "__STUDENT_MOBILE_STATE__",
                &browser.profile.mobile.to_string(),
            );
            browser.call(
                "Page.addScriptToEvaluateOnNewDocument",
                json!({"source":script}),
            )?;
            let ua = browser.mobile_ua();
            browser.call(
                "Network.setUserAgentOverride",
                json!({"userAgent":ua,"platform":"Linux armv8l"}),
            )?;
            browser.call(
                "Network.setExtraHTTPHeaders",
                json!({"headers":{"X-User-Agent-Ovx":ua}}),
            )?;
            // Login-page cookies only; a resumed mobile session must not be
            // forced back through the "new login" path.
            if visible {
                browser.call(
                    "Network.setCookies",
                    json!({"cookies":[
                        {"name":"ForceWeb","value":"false","url":origin},
                        {"name":"IndicateurNouveauLogin","value":"true","url":origin}
                    ]}),
                )?;
            }
        }
        let home = browser.profile.home.clone();
        browser.call("Page.navigate", json!({"url":home}))?;
        Ok(browser)
    }

    fn mobile_ua(&self) -> String {
        format!("OVX InfoDevice=Android-14_samsung_SM-X700_Build/UP1A.231005.007 AppVer=3.8.9 IdAppareil={} Code={}",
            self.profile.mobile["device"].as_str().unwrap_or(""), self.profile.mobile["code"].as_str().unwrap_or(""))
    }

    fn send(&mut self, method: &str, params: Value) -> Result<u64, String> {
        self.sequence += 1;
        self.socket
            .as_mut()
            .ok_or("Browser closed.")?
            .send(Message::Text(
                json!({"id":self.sequence,"method":method,"params":params})
                    .to_string()
                    .into(),
            ))
            .map_err(|_| "Browser connection lost. Verify action state before retrying.")?;
        Ok(self.sequence)
    }

    fn call(&mut self, method: &str, params: Value) -> Result<Value, String> {
        let id = self.send(method, params)?;
        let until = Instant::now() + Duration::from_secs(35);
        loop {
            let remaining = until.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(
                    "Browser request timed out. Verify action state before retrying.".into(),
                );
            }
            let socket = self.socket.as_mut().ok_or("Browser closed.")?;
            socket.get_mut().set_read_timeout(Some(remaining)).ok();
            let message = socket.read().map_err(|_| {
                "Browser connection lost or timed out. Verify action state before retrying."
            })?;
            let Message::Text(text) = message else {
                continue;
            };
            let value: Value = serde_json::from_str(&text).map_err(|_| "Invalid browser reply.")?;
            if value["id"].as_u64() == Some(id) {
                if value.get("error").is_some() {
                    return Err(
                        "Browser operation failed. The page may have navigated or closed.".into(),
                    );
                }
                return Ok(value["result"].clone());
            }
            if value["method"] == "Runtime.executionContextsCleared" {
                self.mobile_contexts.clear();
            }
            if value["method"] == "Runtime.executionContextDestroyed" {
                if let Some(id) = value["params"]["executionContextId"].as_u64() {
                    self.mobile_contexts.remove(&id);
                }
            }
            if value["method"] == "Runtime.executionContextCreated" {
                let context = &value["params"]["context"];
                if context["auxData"]["isDefault"] == true
                    && context["origin"].as_str().is_some_and(|s| {
                        s.starts_with("https://") && crate::config::is_omnivox_host(s)
                    })
                {
                    if let Some(id) = context["id"].as_u64() {
                        self.mobile_contexts.insert(id);
                    }
                }
            }
            if self.profile.service == "omnivox"
                && value["method"] == "Runtime.bindingCalled"
                && value["params"]["name"] == "studentMobileState"
                && value["params"]["executionContextId"]
                    .as_u64()
                    .is_some_and(|id| self.mobile_contexts.contains(&id))
            {
                if let Some(raw) = value["params"]["payload"]
                    .as_str()
                    .filter(|s| s.len() <= 32768)
                {
                    if let Ok(mobile) = serde_json::from_str::<Value>(raw) {
                        if mobile["device"] == self.profile.mobile["device"]
                            && mobile["code"].as_str().is_some_and(|s| {
                                s.len() <= 256 && s.bytes().all(|b| b.is_ascii_graphic())
                            })
                        {
                            if let Some(home) = mobile["home"].as_str() {
                                if let Ok(mut url) = url::Url::parse(home) {
                                    if url.scheme() == "https"
                                        && crate::config::is_omnivox_host(home)
                                        && url.path().starts_with("/Mobl/")
                                        && url.username().is_empty()
                                        && url.password().is_none()
                                    {
                                        url.set_fragment(None);
                                        self.profile.home = url.to_string();
                                    }
                                }
                            }
                            self.profile.mobile = mobile;
                            let ua = self.mobile_ua();
                            self.send(
                                "Network.setUserAgentOverride",
                                json!({"userAgent":ua,"platform":"Linux armv8l"}),
                            )?;
                            self.send(
                                "Network.setExtraHTTPHeaders",
                                json!({"headers":{"X-User-Agent-Ovx":ua}}),
                            )?;
                        }
                    }
                }
            }
        }
    }

    fn evaluate(&mut self, expression: String) -> Result<Value, String> {
        let result = self.call("Runtime.evaluate", json!({"expression":expression,"awaitPromise":true,"returnByValue":true,"timeout":30000}))?;
        if result.get("exceptionDetails").is_some() {
            return Err(
                "Browser page could not complete the request. Verify action state before retrying."
                    .into(),
            );
        }
        Ok(result["result"]["value"].clone())
    }

    fn wait_ready(&mut self, cancelled: &AtomicBool, duration: Duration) -> Result<(), String> {
        let until = Instant::now() + duration;
        while Instant::now() < until {
            if cancelled.load(Ordering::SeqCst) {
                return Err("Cancelled".into());
            }
            if self.child.try_wait().ok().flatten().is_some() {
                return Err("Sign-in browser was closed.".into());
            }
            match self.probe() {
                Ok(()) => {
                    self.checkpoint()?;
                    return Ok(());
                }
                Err(error) if error == ACCOUNT_CHANGED => return Err(error),
                Err(_) => {}
            }
            std::thread::sleep(Duration::from_millis(750));
        }
        Err(RECONNECT.into())
    }

    fn probe(&mut self) -> Result<(), String> {
        let location = self.evaluate("location.href".into())?;
        let url = url::Url::parse(location.as_str().unwrap_or("")).map_err(|_| RECONNECT)?;
        let origin = url.origin().ascii_serialization();
        if self.profile.service == "omnivox" {
            if !crate::config::is_omnivox_host(&origin)
                || url.scheme() != "https"
                || !url.path().starts_with("/Mobl/")
            {
                return Err(RECONNECT.into());
            }
            let ready = self.evaluate(
                "Boolean(window.Skytech?.Commun?.Utils?.HttpRequestWorker?.PostJSON)".into(),
            )?;
            if ready != true {
                return Err(RECONNECT.into());
            }
            // Official MFA may hand the login to another Omnivox subdomain.
            let previous = self.profile.origin.clone();
            self.profile.origin = origin;
            let result = self
                .request(
                    "POST",
                    "/Mobl/LeaCommun/GetDefaultModel",
                    Some(br#"{"AnSession":""}"#),
                    Some("application/json"),
                    false,
                    2 * 1024 * 1024,
                )
                .and_then(Reply::json);
            if !result.as_ref().is_ok_and(|v| v["ListeCours"].is_array()) {
                self.profile.origin = previous;
                return Err(RECONNECT.into());
            }
            let mut home = url;
            home.set_fragment(None);
            self.profile.home = home.to_string();
        } else {
            if origin != self.profile.origin
                || !url.path().starts_with("/d2l/")
                || url.path().contains("login")
            {
                return Err(RECONNECT.into());
            }
            let user = self
                .request(
                    "GET",
                    "/d2l/api/lp/1.49/users/whoami",
                    None,
                    None,
                    false,
                    2 * 1024 * 1024,
                )?
                .json()?;
            let identity = user
                .get("Identifier")
                .filter(|v| !v.is_null())
                .ok_or(RECONNECT)?
                .to_string();
            if self
                .profile
                .identity
                .as_ref()
                .is_some_and(|known| known != &identity)
            {
                return Err(ACCOUNT_CHANGED.into());
            }
            let courses = self
                .request(
                    "GET",
                    "/d2l/api/lp/1.49/enrollments/myenrollments/?orgUnitTypeId=3",
                    None,
                    None,
                    false,
                    2 * 1024 * 1024,
                )?
                .json()?;
            if !courses["Items"].is_array() {
                return Err(RECONNECT.into());
            }
            self.profile.identity = Some(identity);
        }
        Ok(())
    }

    fn request(
        &mut self,
        method: &str,
        path: &str,
        body: Option<&[u8]>,
        content_type: Option<&str>,
        binary: bool,
        limit: usize,
    ) -> Result<Reply, String> {
        let url = url::Url::parse(&self.profile.origin)
            .map_err(|_| "Invalid account origin.")?
            .join(path)
            .map_err(|_| "Invalid request path.")?;
        let prefix = if self.profile.service == "omnivox" {
            "/Mobl/"
        } else {
            "/d2l/api/"
        };
        if url.origin().ascii_serialization() != self.profile.origin
            || !url.path().starts_with(prefix)
        {
            return Err("Browser requests are restricted to the connected school API.".into());
        }
        let mobile = self.profile.service == "omnivox";
        // Omnivox's official worker takes a site-relative path, as the mobile
        // app does; Brightspace's fetch accepts the absolute same-origin URL.
        let target = if mobile {
            match url.query() {
                Some(query) => format!("{}?{query}", url.path()),
                None => url.path().to_string(),
            }
        } else {
            url.as_str().to_string()
        };
        let mut input = json!({"origin":self.profile.origin,"method":method,"path":target,"binary":binary,"limit":limit,"mobile":mobile,"contentType":content_type});
        if mobile {
            if method != "POST" {
                return Err("Unsupported mobile request.".into());
            }
            input["body"] = serde_json::from_slice(body.unwrap_or(b"{}"))
                .map_err(|_| "Invalid mobile request body.")?;
        } else if let Some(body) = body {
            input["bodyBase64"] = json!(base64::engine::general_purpose::STANDARD.encode(body));
        }
        let value = self.evaluate(format!(
            "{}\nbrowserRequest({input})",
            include_str!("resources/browser/request.js")
        ))?;
        if let Some(error) = value["error"].as_str() {
            return Err(match error {
                "reconnect" | "redirect" => RECONNECT.into(),
                "limit" => "School response exceeded the size limit.".into(),
                _ => "School request failed or timed out. Verify action state before retrying."
                    .into(),
            });
        }
        let bytes = if binary {
            base64::engine::general_purpose::STANDARD
                .decode(value["base64"].as_str().ok_or("Missing browser file.")?)
                .map_err(|_| "Invalid browser file.")?
        } else {
            value["text"]
                .as_str()
                .ok_or("Missing browser response.")?
                .as_bytes()
                .to_vec()
        };
        if bytes.len() > limit {
            return Err("School response exceeded the size limit.".into());
        }
        Ok(Reply {
            status: value["status"].as_u64().ok_or("Missing HTTP status.")? as u16,
            mime: value["mime"].as_str().unwrap_or("").into(),
            disposition: value["disposition"].as_str().unwrap_or("").into(),
            bytes,
        })
    }

    fn checkpoint(&mut self) -> Result<(), String> {
        let result = self.call("Network.getAllCookies", json!({}))?;
        let cookies: Vec<Value> = result["cookies"]
            .as_array()
            .ok_or("Cannot save browser cookies.")?
            .iter()
            .filter(|c| c["session"] == true && c.get("partitionKey").is_none())
            .map(|c| {
                let mut out = serde_json::Map::new();
                for key in [
                    "name", "value", "domain", "path", "secure", "httpOnly", "sameSite",
                ] {
                    if let Some(value) = c.get(key) {
                        out.insert(key.into(), value.clone());
                    }
                }
                Value::Object(out)
            })
            .collect();
        save(&self.dir.join("cookies.json"), &cookies)?;
        save(&self.dir.join("session.json"), &self.profile)
    }
}

impl Drop for Browser {
    fn drop(&mut self) {
        if self.socket.is_some() && self.child.try_wait().ok().flatten().is_none() {
            let _ = self.checkpoint();
            let _ = self.send("Browser.close", json!({}));
            let until = Instant::now() + Duration::from_secs(3);
            while self.child.try_wait().ok().flatten().is_none() && Instant::now() < until {
                std::thread::sleep(Duration::from_millis(50));
            }
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

pub struct Login {
    pub service: String,
    pub id: String,
    pub state: String,
    pub error: Option<String>,
    cancelled: Arc<AtomicBool>,
    result: mpsc::Receiver<Result<(String, String), String>>,
    worker: Option<std::thread::JoinHandle<()>>,
    token: String,
    created: bool,
    origin: String,
}

impl Login {
    pub fn start(
        service: &str,
        id: &str,
        origin: &str,
        previous: Option<&str>,
    ) -> Result<Self, String> {
        let origin = match service {
            "brightspace" => crate::config::normalize_brightspace_url(origin)?,
            "omnivox" => {
                let origin = crate::config::normalize_omnivox_url(origin)?;
                if !crate::config::is_omnivox_host(&origin) {
                    return Err("Use your college's https://…omnivox.ca address.".into());
                }
                origin
            }
            _ => return Err("Browser login is available for Brightspace and Omnivox.".into()),
        };
        if !origin.starts_with("https://") {
            return Err("Browser login requires an HTTPS school address.".into());
        }
        executable()?;
        let created = previous.filter(|s| is_profile(s)).is_none();
        let token = previous
            .filter(|s| is_profile(s))
            .map(str::to_owned)
            .unwrap_or_else(|| format!("{PREFIX}{}", crate::config::random_hex(16)));
        let worker_token = token.clone();
        let worker_origin = origin.clone();
        let (tx, result) = mpsc::channel();
        let cancelled = Arc::new(AtomicBool::new(false));
        let stop = cancelled.clone();
        let service_owned = service.to_string();
        let worker = std::thread::spawn(move || {
            let outcome = (|| {
                let mut browser =
                    Browser::launch(&worker_token, &service_owned, &worker_origin, true)?;
                browser.wait_ready(&stop, Duration::from_secs(300))?;
                Ok((worker_token, browser.profile.origin.clone()))
            })();
            let _ = tx.send(outcome);
        });
        Ok(Self {
            service: service.into(),
            id: id.into(),
            state: "waiting".into(),
            error: None,
            cancelled,
            result,
            worker: Some(worker),
            token,
            created,
            origin,
        })
    }

    pub fn poll(&mut self) {
        if self.state != "waiting" {
            return;
        }
        let result = match self.result.try_recv() {
            Ok(result) => result,
            Err(mpsc::TryRecvError::Empty) => return,
            Err(_) => Err("Sign-in worker stopped.".into()),
        };
        let result = result.and_then(|(token, url)| {
            let mut settings = crate::config::Settings::load().for_account(&self.service, &self.id);
            match self.service.as_str() {
                "brightspace" => {
                    settings.brightspace = Some(crate::config::BrightspaceSettings {
                        url: Some(url),
                        token: Some(token),
                        allow_write: Some(true),
                    })
                }
                "omnivox" => {
                    settings.omnivox = Some(crate::config::OmnivoxSettings {
                        url: Some(url),
                        token: Some(token),
                        ..Default::default()
                    })
                }
                _ => return Err("Unknown service.".into()),
            }
            settings
                .save_service(&self.service)
                .map_err(|_| "Cannot save connected account.".into())
        });
        match result {
            Ok(()) => self.state = "connected".into(),
            Err(error) => {
                self.state = "failed".into();
                self.error = Some(error);
            }
        }
    }

    pub fn status(&self) -> Value {
        json!({"service":self.service,"id":self.id,"state":self.state,"error":self.error,"origin":self.origin})
    }

    pub fn cancel(&mut self) {
        self.cancelled.store(true, Ordering::SeqCst);
        self.state = "cancelled".into();
    }

    pub fn running(&self) -> bool {
        self.worker
            .as_ref()
            .is_some_and(|worker| !worker.is_finished())
    }
}

impl Drop for Login {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::SeqCst);
        // On app shutdown, let the worker checkpoint/close Chrome before the
        // process exits. Cancel itself is asynchronous and keeps this handle.
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
        // A cancelled/failed first sign-in leaves an unused profile; do not
        // retain its secrets. A re-login must never delete the saved profile.
        if self.created && self.state != "connected" {
            let _ = remove(&self.token);
        }
    }
}

pub fn remove(token: &str) -> Result<(), String> {
    if !is_profile(token) {
        return Ok(());
    }
    let dir = profile_path(token)?;
    if !dir.exists() {
        return Ok(());
    }
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .open(dir.join("profile.lock"))
        .map_err(|_| "Cannot open browser profile lock.")?;
    lock.try_lock_exclusive().map_err(|_| {
        "Browser is busy. Close sign-in or wait for the current tool, then disconnect again."
    })?;
    // Windows cannot remove an open lock file. Keep that tiny file and directory.
    for entry in fs::read_dir(&dir).map_err(|_| "Cannot clear browser profile.")? {
        let entry = entry.map_err(|_| "Cannot clear browser profile.")?;
        if entry.file_name() == "profile.lock" {
            continue;
        }
        let path = entry.path();
        if path.is_dir() {
            fs::remove_dir_all(path)
        } else {
            fs::remove_file(path)
        }
        .map_err(|_| "Cannot clear browser profile.")?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn browser_login_rejects_incompatible_services_and_origins_before_launch() {
        for (service, origin) in [
            ("moodle", "https://school.example"),
            ("omnivox", "https://other.example"),
            ("brightspace", "http://school.example"),
            ("brightspace", "https://user:password@school.example"),
        ] {
            assert!(Login::start(service, "test", origin, None).is_err());
        }
    }

    #[test]
    fn profile_ids_cannot_escape_account_storage() {
        for token in [
            "browser-profile:../secret",
            "browser-profile:/etc/passwd",
            "browser-profile:",
            "cookie",
        ] {
            assert!(profile_path(token).is_err());
        }
        crate::config::tests::with_temp_config(|| {
            let a = profile_path(&format!("{PREFIX}{}", "a".repeat(32))).unwrap();
            let b = profile_path(&format!("{PREFIX}{}", "b".repeat(32))).unwrap();
            assert_ne!(a, b);
            assert_eq!(a.parent(), b.parent());
        });
    }

    #[test]
    fn cancelled_login_cannot_save_account() {
        crate::config::tests::with_temp_config(|| {
            let (tx, result) = mpsc::channel();
            let cancelled = Arc::new(AtomicBool::new(false));
            let login = Login {
                worker: None,
                service: "brightspace".into(),
                id: "new".into(),
                state: "waiting".into(),
                error: None,
                cancelled: cancelled.clone(),
                result,
                token: format!("{PREFIX}{}", "a".repeat(32)),
                created: true,
                origin: "https://school.example".into(),
            };
            drop(login);
            assert!(cancelled.load(Ordering::SeqCst));
            assert!(tx
                .send(Ok((
                    format!("{PREFIX}{}", "a".repeat(32)),
                    "https://school.example".into()
                )))
                .is_err());
            assert!(crate::config::Settings::load()
                .account_list("brightspace")
                .is_empty());
        });
    }

    #[test]
    fn login_completion_preserves_other_accounts_and_reports_save_errors() {
        crate::config::tests::with_temp_config(|| {
            let settings = crate::config::Settings {
                moodle: Some(crate::config::MoodleSettings {
                    url: Some("https://moodle.example".into()),
                    token: Some("existing".into()),
                    ..Default::default()
                }),
                ..Default::default()
            };
            settings.save().unwrap();
            let (tx, result) = mpsc::channel();
            let mut login = Login {
                worker: None,
                service: "brightspace".into(),
                id: "new".into(),
                state: "waiting".into(),
                error: None,
                cancelled: Arc::new(AtomicBool::new(false)),
                result,
                token: format!("{PREFIX}{}", "b".repeat(32)),
                created: true,
                origin: "https://school.example".into(),
            };
            let token = format!("{PREFIX}{}", "a".repeat(32));
            tx.send(Ok((token.clone(), "https://school.example".into())))
                .unwrap();
            login.poll();
            assert_eq!(login.state, "connected");
            assert!(!login.status().to_string().contains(&token));
            let saved = crate::config::Settings::load();
            assert_eq!(saved.moodle().unwrap().token, "existing");
            assert_eq!(
                saved
                    .for_account("brightspace", "new")
                    .brightspace()
                    .unwrap()
                    .token,
                token
            );
            fs::write(crate::config::config_path(), "broken config").unwrap();
            let (tx, result) = mpsc::channel();
            login.result = result;
            login.state = "waiting".into();
            tx.send(Ok((token, "https://school.example".into())))
                .unwrap();
            login.poll();
            assert_eq!(login.state, "failed");
            assert_eq!(
                fs::read_to_string(crate::config::config_path()).unwrap(),
                "broken config"
            );
        });
    }

    /// Uses a real local Chromium against a local school fixture. No school
    /// credentials, internet login, MFA or live mutations are involved.
    #[test]
    #[ignore = "requires Chrome/Chromium (STUDENT_MCP_BROWSER); local fixture only"]
    fn chromium_session_survives_restart_rotation_and_deletion() {
        use std::sync::atomic::AtomicUsize;
        use tiny_http::{Header, Response, Server};
        crate::config::tests::with_temp_config(|| {
            let server = Server::http("127.0.0.1:0").unwrap();
            let origin = format!("http://{}", server.server_addr());
            let stop = Arc::new(AtomicBool::new(false));
            let writes = Arc::new(AtomicUsize::new(0));
            let finished = stop.clone();
            let counted = writes.clone();
            let worker = std::thread::spawn(move || {
                while !finished.load(Ordering::SeqCst) {
                    let Some(request) = server.recv_timeout(Duration::from_millis(100)).unwrap()
                    else {
                        continue;
                    };
                    let cookie = request
                        .headers()
                        .iter()
                        .find(|h| h.field.equiv("Cookie"))
                        .map(|h| h.value.as_str())
                        .unwrap_or("");
                    let mut status = 200;
                    let mut headers = vec![];
                    let body = match request.url() {
                        "/signin" => {
                            headers.push(("Set-Cookie", "session=first; Path=/; HttpOnly"));
                            headers.push(("Content-Type", "text/html"));
                            "<script>localStorage.setItem('XSRF.Token','csrf-fixture');location.href='/d2l/home'</script>".into()
                        }
                        "/d2l/home" => {
                            headers.push(("Content-Type", "text/html"));
                            "<html>School fixture</html>".into()
                        }
                        "/d2l/api/lp/1.49/users/whoami" => {
                            if cookie.contains("session=") {
                                r#"{"Identifier":"42"}"#.into()
                            } else {
                                status = 401;
                                "{}".into()
                            }
                        }
                        "/d2l/api/lp/1.49/enrollments/myenrollments/?orgUnitTypeId=3" => {
                            r#"{"Items":[],"PagingInfo":{"HasMoreItems":false}}"#.into()
                        }
                        "/d2l/api/rotate" => {
                            headers.push(("Set-Cookie", "session=rotated; Path=/; HttpOnly"));
                            "{}".into()
                        }
                        "/d2l/api/delete" => {
                            headers.push(("Set-Cookie", "session=gone; Path=/; Max-Age=0"));
                            "{}".into()
                        }
                        "/d2l/api/cookie" => json!({"cookie":cookie}).to_string(),
                        "/d2l/api/file" => {
                            headers.push(("Content-Type", "application/pdf"));
                            "%PDF-fixture".into()
                        }
                        "/d2l/api/redirect" => {
                            counted.fetch_add(1, Ordering::SeqCst);
                            status = 302;
                            headers.push(("Location", "/d2l/api/should-not-replay"));
                            String::new()
                        }
                        "/d2l/api/should-not-replay" => {
                            counted.fetch_add(100, Ordering::SeqCst);
                            "{}".into()
                        }
                        "/d2l/api/forbidden" => {
                            counted.fetch_add(1, Ordering::SeqCst);
                            status = 403;
                            "{}".into()
                        }
                        "/d2l/api/csrf" => {
                            assert!(request
                                .headers()
                                .iter()
                                .any(|h| h.field.equiv("X-Csrf-Token")
                                    && h.value.as_str() == "csrf-fixture"));
                            "{}".into()
                        }
                        _ => "{}".into(),
                    };
                    let mut response = Response::from_string(body).with_status_code(status);
                    for (key, value) in headers {
                        response.add_header(Header::from_bytes(key, value).unwrap());
                    }
                    let _ = request.respond(response);
                }
            });
            let token = format!("{PREFIX}{}", crate::config::random_hex(16));
            let dir = profile_path(&token).unwrap();
            private_dir(&dir).unwrap();
            save(
                &dir.join("session.json"),
                &Profile {
                    service: "brightspace".into(),
                    origin: origin.clone(),
                    home: format!("{origin}/signin"),
                    mobile: json!({}),
                    identity: None,
                },
            )
            .unwrap();
            let result = std::panic::catch_unwind(|| {
                let mut browser = Browser::launch(&token, "brightspace", &origin, false).unwrap();
                browser
                    .wait_ready(&AtomicBool::new(false), Duration::from_secs(10))
                    .unwrap();
                browser.profile.home = format!("{origin}/d2l/home");
                assert!(remove(&token).is_err(), "must not delete a running profile");
                browser
                    .request("GET", "/d2l/api/rotate", None, None, false, 1024)
                    .unwrap()
                    .json()
                    .unwrap();
                browser
                    .request(
                        "POST",
                        "/d2l/api/csrf",
                        Some(b"{}"),
                        Some("application/json"),
                        false,
                        1024,
                    )
                    .unwrap()
                    .json()
                    .unwrap();
                let file = browser
                    .request("GET", "/d2l/api/file", None, None, true, 1024)
                    .unwrap();
                assert_eq!(file.bytes, b"%PDF-fixture");
                assert!(browser
                    .request("GET", "/d2l/api/file", None, None, true, 2)
                    .is_err());
                assert!(browser
                    .request(
                        "GET",
                        "https://other.example/d2l/api/test",
                        None,
                        None,
                        false,
                        1024
                    )
                    .is_err());
                assert!(browser
                    .request("POST", "/d2l/api/redirect", Some(b"{}"), None, false, 1024)
                    .is_err());
                assert_eq!(
                    browser
                        .request("POST", "/d2l/api/forbidden", Some(b"{}"), None, false, 1024)
                        .unwrap()
                        .status,
                    403
                );
                assert_eq!(
                    writes.load(Ordering::SeqCst),
                    2,
                    "writes must never be replayed"
                );
                drop(browser);
                // End-to-end: a profile token saved in settings reaches the
                // existing connector and returns parsed course data.
                let settings = crate::config::Settings {
                    brightspace: Some(crate::config::BrightspaceSettings {
                        url: Some(origin.clone()),
                        token: Some(token.clone()),
                        allow_write: Some(true),
                    }),
                    ..Default::default()
                };
                let reply =
                    crate::mcp::dispatch_tool(&settings, "brightspace_my_courses", &json!({}));
                assert_eq!(reply["isError"], false, "{reply}");
                assert!(!reply.to_string().contains(&token));
                assert!(
                    Browser::launch(&token, "brightspace", "https://other.example", false).is_err()
                );
                let mut browser = Browser::launch(&token, "brightspace", &origin, false).unwrap();
                browser
                    .wait_ready(&AtomicBool::new(false), Duration::from_secs(10))
                    .unwrap();
                let value = browser
                    .request("GET", "/d2l/api/cookie", None, None, false, 1024)
                    .unwrap()
                    .json()
                    .unwrap();
                assert!(value["cookie"]
                    .as_str()
                    .unwrap()
                    .contains("session=rotated"));
                browser.profile.identity = Some("\"different-user\"".into());
                assert_eq!(browser.probe().unwrap_err(), ACCOUNT_CHANGED);
                browser
                    .request("GET", "/d2l/api/delete", None, None, false, 1024)
                    .unwrap();
                drop(browser);
                let cookies: Value =
                    serde_json::from_slice(&fs::read(dir.join("cookies.json")).unwrap()).unwrap();
                assert!(cookies
                    .as_array()
                    .unwrap()
                    .iter()
                    .all(|c| c["name"] != "session"));
                let mut browser = Browser::launch(&token, "brightspace", &origin, false).unwrap();
                // Wait only for document load, not authentication (now deliberately gone).
                for _ in 0..20 {
                    if browser
                        .evaluate("location.pathname".into())
                        .unwrap_or(Value::Null)
                        == "/d2l/home"
                    {
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(100));
                }
                assert_eq!(
                    browser
                        .request(
                            "GET",
                            "/d2l/api/lp/1.49/users/whoami",
                            None,
                            None,
                            false,
                            1024
                        )
                        .unwrap()
                        .status,
                    401
                );
                drop(browser);
                remove(&token).unwrap();
                assert!(!dir.join("session.json").exists());
            });
            stop.store(true, Ordering::SeqCst);
            worker.join().unwrap();
            if let Err(error) = result {
                std::panic::resume_unwind(error)
            }
        });
    }
}
