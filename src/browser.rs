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

mod install;
mod moodle;

const PREFIX: &str = "browser-profile:";
const RECONNECT: &str = "Browser session needs attention. Open student-mcp and choose Edit sign-in → Connect with browser.";
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

fn is_chromium_browser(path: &Path) -> bool {
    let name = path
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .to_ascii_lowercase();
    let name = name.replace(['-', '_', ' '], "");
    [
        "chrome",
        "chromium",
        "msedge",
        "edgedev",
        "brave",
        "vivaldi",
        "opera",
        "helium",
        "ungoogled",
        "thorium",
        "iridium",
        "cromite",
    ]
    .iter()
    .any(|marker| name.contains(marker))
        && !name.contains("firefox")
        && !name.contains("safari")
}

#[cfg(any(target_os = "linux", test))]
fn parse_desktop_exec(contents: &str) -> Option<String> {
    contents
        .lines()
        .find_map(|line| line.trim().strip_prefix("Exec=").and_then(split_command))
}

#[cfg(any(target_os = "windows", test))]
fn parse_reg_sz(output: &str) -> Option<String> {
    output.lines().find_map(|line| {
        line.split_once("REG_SZ")
            .map(|(_, value)| value.trim().to_string())
            .filter(|value| !value.is_empty())
    })
}

#[cfg(any(target_os = "linux", target_os = "windows", test))]
fn split_command(command: &str) -> Option<String> {
    let command = command.trim();
    if let Some(rest) = command.strip_prefix('"') {
        let end = rest.find('"')?;
        return Some(rest[..end].into());
    }
    command
        .split_whitespace()
        .next()
        .map(str::to_string)
        .filter(|value| !value.is_empty())
}

#[cfg(target_os = "linux")]
fn resolve_command(command: &str) -> Option<PathBuf> {
    let path = PathBuf::from(command);
    if path.is_absolute() {
        return path.is_file().then_some(path);
    }
    std::env::split_paths(&std::env::var_os("PATH")?)
        .map(|dir| dir.join(command))
        .find(|path| path.is_file())
}

fn prefer_real_chromium(path: PathBuf) -> PathBuf {
    let Ok(target) = path.canonicalize() else {
        return path;
    };
    let Some(dir) = target.parent() else {
        return path;
    };
    for name in [
        "helium",
        "chrome",
        "chrome.exe",
        "chromium",
        "msedge",
        "msedge.exe",
    ] {
        let candidate = dir.join(name);
        if candidate.is_file() && candidate != target {
            return candidate;
        }
    }
    path
}

fn command_stdout(bin: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(bin).args(args).output().ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_string())
        .filter(|value| !value.is_empty())
}

#[cfg(target_os = "linux")]
fn linux_desktop_file(id: &str) -> Option<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(home) = std::env::var_os("XDG_DATA_HOME") {
        dirs.push(PathBuf::from(home));
    } else if let Some(home) = std::env::var_os("HOME") {
        dirs.push(PathBuf::from(home).join(".local/share"));
    }
    dirs.push(PathBuf::from("/usr/share/omarchy"));
    if let Some(data) = std::env::var_os("XDG_DATA_DIRS") {
        dirs.extend(std::env::split_paths(&data));
    } else {
        dirs.extend([
            PathBuf::from("/usr/local/share"),
            PathBuf::from("/usr/share"),
        ]);
    }
    dirs.into_iter()
        .map(|dir| dir.join("applications").join(id))
        .find(|path| path.is_file())
}

fn default_https_browser() -> Option<PathBuf> {
    #[cfg(target_os = "linux")]
    {
        let mut ids = Vec::new();
        for (bin, args) in [
            ("omarchy-cmd-default-browser", &[] as &[&str]),
            ("xdg-settings", &["get", "default-web-browser"]),
        ] {
            if let Some(id) = command_stdout(bin, args) {
                if id.ends_with(".desktop") && !ids.contains(&id) {
                    ids.push(id);
                }
            }
        }
        for id in ids {
            let Some(file) = linux_desktop_file(&id) else {
                continue;
            };
            let Ok(text) = fs::read_to_string(file) else {
                continue;
            };
            let Some(command) = parse_desktop_exec(&text) else {
                continue;
            };
            let Some(path) = resolve_command(&command) else {
                continue;
            };
            let name = path.file_name().unwrap_or_default().to_string_lossy();
            if name.contains("omarchy-launch-browser") || name.contains("xdg-open") {
                continue;
            }
            if is_chromium_browser(&path) {
                return Some(prefer_real_chromium(path));
            }
        }
        None
    }
    #[cfg(target_os = "macos")]
    {
        let script = r#"ObjC.import("AppKit"); const app = $.NSWorkspace.sharedWorkspace.URLForApplicationToOpenURL($.NSURL.URLWithString("https:")); app ? app.path.js : """#;
        let path = command_stdout("osascript", &["-l", "JavaScript", "-e", script])?;
        let path = PathBuf::from(path.trim_end_matches('/'));
        let binary = if path.extension().is_some_and(|ext| ext == "app") {
            let macos = path.join("Contents/MacOS");
            [
                "Google Chrome",
                "Chromium",
                "Microsoft Edge",
                "Brave Browser",
                "Helium",
                "Vivaldi",
                "Opera",
            ]
            .into_iter()
            .map(|name| macos.join(name))
            .find(|candidate| candidate.is_file())
            .or_else(|| {
                fs::read_dir(macos)
                    .ok()?
                    .flatten()
                    .map(|e| e.path())
                    .find(|p| p.is_file())
            })?
        } else {
            path
        };
        is_chromium_browser(&binary).then_some(prefer_real_chromium(binary))
    }
    #[cfg(target_os = "windows")]
    {
        let progid = parse_reg_sz(&command_stdout(
            "reg",
            &[
                "query",
                r"HKCU\Software\Microsoft\Windows\Shell\Associations\UrlAssociations\https\UserChoice",
                "/v",
                "ProgId",
            ],
        )?)?;
        let command = parse_reg_sz(&command_stdout(
            "reg",
            &[
                "query",
                &format!(r"HKCR\{progid}\shell\open\command"),
                "/ve",
            ],
        )?)?;
        let path = PathBuf::from(split_command(&command)?);
        path.is_file()
            .then_some(path)
            .filter(|path| is_chromium_browser(path))
            .map(prefer_real_chromium)
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        None
    }
}

// Discovery never downloads anything: MCP requests may only use an installed browser.
fn system_executable() -> Result<Option<PathBuf>, String> {
    if let Some(path) = std::env::var_os("STUDENT_MCP_BROWSER") {
        let path = PathBuf::from(path);
        return if path.is_absolute() && path.is_file() && is_chromium_browser(&path) {
            Ok(Some(prefer_real_chromium(path)))
        } else {
            Err("STUDENT_MCP_BROWSER must be an absolute path to a Chromium-based browser.".into())
        };
    }
    if let Some(path) = default_https_browser() {
        return Ok(Some(path));
    }
    let mut candidates = vec![
        PathBuf::from("/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"),
        PathBuf::from("/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge"),
        PathBuf::from("/Applications/Brave Browser.app/Contents/MacOS/Brave Browser"),
        PathBuf::from("/Applications/Helium.app/Contents/MacOS/Helium"),
        PathBuf::from("/Applications/Chromium.app/Contents/MacOS/Chromium"),
    ];
    if let Some(home) = std::env::var_os("HOME") {
        for name in [
            "Google Chrome",
            "Microsoft Edge",
            "Brave Browser",
            "Helium",
            "Chromium",
        ] {
            candidates.push(
                PathBuf::from(&home).join(format!("Applications/{name}.app/Contents/MacOS/{name}")),
            );
        }
    }
    for root in ["PROGRAMFILES", "PROGRAMFILES(X86)", "LOCALAPPDATA"] {
        if let Some(root) = std::env::var_os(root) {
            for name in [
                "Google/Chrome/Application/chrome.exe",
                "Microsoft/Edge/Application/msedge.exe",
                "BraveSoftware/Brave-Browser/Application/brave.exe",
            ] {
                candidates.push(PathBuf::from(&root).join(name));
            }
        }
    }
    if let Some(paths) = std::env::var_os("PATH") {
        for path in std::env::split_paths(&paths) {
            for name in [
                "helium-browser",
                "helium",
                "chromium",
                "chromium-browser",
                "google-chrome",
                "google-chrome-stable",
                "brave-browser",
                "vivaldi",
                "microsoft-edge",
                "microsoft-edge-stable",
                "chrome.exe",
                "msedge.exe",
                "brave.exe",
            ] {
                candidates.push(path.join(name));
            }
        }
    }
    Ok(candidates
        .into_iter()
        .find(|path| path.is_file() && is_chromium_browser(path))
        .map(prefer_real_chromium))
}

fn executable() -> Result<PathBuf, String> {
    system_executable()?.or_else(install::available).ok_or_else(|| {
        "No compatible browser is installed. Open student-mcp and choose Connect with browser to approve a browser download.".into()
    })
}

pub fn needs_download() -> Result<bool, String> {
    Ok(system_executable()?.or_else(install::available).is_none())
}

fn download_permitted(installed: bool, consent: bool) -> Result<bool, String> {
    if installed {
        return Ok(false);
    }
    if consent {
        return Ok(true);
    }
    Err("No compatible browser found. Approve the Chrome for Testing download in student-mcp, or install Chrome, Edge or Brave and try again.".into())
}

pub struct Reply {
    pub status: u16,
    pub mime: String,
    pub disposition: String,
    pub link: String,
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
        self.with_browser(|browser| {
            browser.request(method, path, body, content_type, binary, limit)
        })
    }

    /// Single-attempt Omnivox writes must not use PostJSON's challenge replay.
    pub fn mobile_write(&self, path: &str, body: &Value) -> Result<Reply, String> {
        if self.service != "omnivox" {
            return Err("Mobile writes are only supported for Omnivox.".into());
        }
        self.with_browser(|browser| browser.mobile_write(path, body))
    }

    fn with_browser<T>(
        &self,
        action: impl FnOnce(&mut Browser) -> Result<T, String>,
    ) -> Result<T, String> {
        let mut guard = self
            .browser
            .lock()
            .map_err(|_| "Browser session unavailable.")?;
        if guard.is_none() {
            let mut browser = Browser::launch(&self.token, self.service, &self.origin, false)?;
            browser.wait_ready(&AtomicBool::new(false), Duration::from_secs(25))?;
            *guard = Some(browser);
        }
        action(guard.as_mut().unwrap())
    }
}

fn wait_for_profile(lock: &File, cancelled: &AtomicBool, timeout: Duration) -> Result<(), String> {
    let deadline = Instant::now() + timeout;
    loop {
        if cancelled.load(Ordering::SeqCst) {
            return Err("Browser sign-in cancelled.".into());
        }
        match lock.try_lock_exclusive() {
            Ok(()) => return Ok(()),
            Err(error) if error.raw_os_error() == fs2::lock_contended_error().raw_os_error() => {}
            Err(_) => {
                return Err("Cannot lock browser profile. Check its storage permissions.".into())
            }
        }
        if Instant::now() >= deadline {
            return Err("This account's browser is still busy after waiting. Finish its sign-in or wait for the current tool, then try again.".into());
        }
        std::thread::sleep(Duration::from_millis(50));
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
    moodle_sso: Option<moodle::Sso>,
}

impl Browser {
    fn launch(token: &str, service: &str, origin: &str, visible: bool) -> Result<Self, String> {
        Self::launch_cancellable(token, service, origin, visible, &AtomicBool::new(false))
    }

    fn launch_cancellable(
        token: &str,
        service: &str,
        origin: &str,
        visible: bool,
        cancelled: &AtomicBool,
    ) -> Result<Self, String> {
        install::check_cancelled(cancelled)?;
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
        // A normal tool can take longer than 30s (startup + school request).
        // Queue behind it without launching another Chrome. Keep a deadline so
        // an abandoned interactive sign-in cannot block all callers forever.
        wait_for_profile(&lock, cancelled, Duration::from_secs(120))?;
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
                    } else if service == "canvas" {
                        "/login/canvas"
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
                "--disable-dev-shm-usage",
                "--disable-gpu",
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        if !visible {
            command.arg("--headless=new");
        }
        // GitHub-hosted Linux runners and container CI lack a user namespace
        // for Chrome's sandbox. Keep the sandbox for interactive desktops.
        if std::env::var_os("CI").is_some() {
            command.arg("--no-sandbox");
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
            moodle_sso: (service == "moodle").then(|| moodle::Sso::new(origin)),
        };
        let until = Instant::now() + Duration::from_secs(15);
        let agent = ureq::AgentBuilder::new()
            .redirects(0)
            .timeout(Duration::from_secs(2))
            .build();
        loop {
            install::check_cancelled(cancelled)?;
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
        let home = if let Some(sso) = &browser.moodle_sso {
            let home = sso.launch_url();
            browser.call("Fetch.enable", json!({"patterns":[{"urlPattern":format!("{origin}/admin/tool/mobile/launch.php*"),"requestStage":"Response"}]}))?;
            home
        } else {
            browser.profile.home.clone()
        };
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
            if self.moodle_sso.is_some() && value["method"] == "Fetch.requestPaused" {
                let params = &value["params"];
                let callback = params["responseHeaders"]
                    .as_array()
                    .and_then(|headers| {
                        headers.iter().find(|h| {
                            h["name"]
                                .as_str()
                                .is_some_and(|n| n.eq_ignore_ascii_case("location"))
                        })
                    })
                    .and_then(|h| h["value"].as_str());
                if let Some(callback) = callback.filter(|s| s.starts_with("moodlemobile:")) {
                    let result = self.moodle_sso.as_mut().unwrap().accept(callback);
                    self.send(
                        "Fetch.failRequest",
                        json!({"requestId":params["requestId"],"errorReason":"Aborted"}),
                    )?;
                    result?;
                } else {
                    self.send(
                        "Fetch.continueRequest",
                        json!({"requestId":params["requestId"]}),
                    )?;
                }
            }
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
        Err(if self.moodle_sso.is_some() {
            moodle::UNAVAILABLE
        } else {
            RECONNECT
        }
        .into())
    }

    fn probe(&mut self) -> Result<(), String> {
        if self.moodle_sso.is_some() {
            // Also support Moodle's confirmation page, which renders a link
            // instead of an HTTP redirect. Read only the configured school page.
            let site = serde_json::to_string(&self.profile.origin).unwrap();
            let link = self.evaluate(format!("location.href.startsWith({site} + '/admin/tool/mobile/launch.php') ? document.querySelector('a[href^=\"moodlemobile://token=\"]')?.getAttribute('href') : null"))?;
            let sso = self.moodle_sso.as_mut().unwrap();
            if sso.token.is_none() {
                if let Some(link) = link.as_str() {
                    sso.accept(link)?;
                }
            }
            return if sso.token.is_some() {
                Ok(())
            } else {
                Err(RECONNECT.into())
            };
        }
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
        } else if self.profile.service == "canvas" {
            if origin != self.profile.origin
                || (url.scheme() != "https" && !origin.starts_with("http://127.0.0.1"))
            {
                return Err(RECONNECT.into());
            }
            if url.path().contains("login") {
                return Err(RECONNECT.into());
            }
            let user = self
                .request(
                    "GET",
                    "/api/v1/users/self",
                    None,
                    None,
                    false,
                    2 * 1024 * 1024,
                )?
                .json()?;
            if user.get("id").is_none_or(Value::is_null) {
                return Err(RECONNECT.into());
            }
            self.profile.home = format!("{origin}/");
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
        let input = self.request_input(method, path, body, content_type, binary, limit)?;
        self.perform_request(input, binary, limit)
    }

    fn mobile_write(&mut self, path: &str, body: &Value) -> Result<Reply, String> {
        if self.profile.service != "omnivox" {
            return Err("Mobile writes are only supported for Omnivox.".into());
        }
        let bytes = body.to_string();
        let limit = 2 * 1024 * 1024;
        let mut input = self.request_input(
            "POST",
            path,
            Some(bytes.as_bytes()),
            Some("application/json"),
            false,
            limit,
        )?;
        input["mobileWrite"] = json!(true);
        self.perform_request(input, false, limit)
    }

    fn request_input(
        &self,
        method: &str,
        path: &str,
        body: Option<&[u8]>,
        content_type: Option<&str>,
        binary: bool,
        limit: usize,
    ) -> Result<Value, String> {
        let url = url::Url::parse(&self.profile.origin)
            .map_err(|_| "Invalid account origin.")?
            .join(path)
            .map_err(|_| "Invalid request path.")?;
        let allowed = match self.profile.service.as_str() {
            "omnivox" => url.path().starts_with("/Mobl/"),
            "canvas" => url.path().starts_with("/api/v1/") || url.path().starts_with("/files/"),
            _ => url.path().starts_with("/d2l/api/"),
        };
        if url.origin().ascii_serialization() != self.profile.origin || !allowed {
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
            if method == "GET" && binary && body.is_none() {
                // Files are native GET downloads, not JSON calls. Keep the
                // profile's mobile UA/headers and same-origin redirect guard.
                input["mobile"] = json!(false);
            } else if method != "POST" || binary {
                return Err("Unsupported mobile request.".into());
            } else {
                input["body"] = serde_json::from_slice(body.unwrap_or(b"{}"))
                    .map_err(|_| "Invalid mobile request body.")?;
            }
        } else if let Some(body) = body {
            input["bodyBase64"] = json!(base64::engine::general_purpose::STANDARD.encode(body));
        }
        Ok(input)
    }

    fn perform_request(
        &mut self,
        input: Value,
        binary: bool,
        limit: usize,
    ) -> Result<Reply, String> {
        let value = self.evaluate(format!(
            "{}\nbrowserRequest({input})",
            include_str!("resources/browser/request.js")
        ))?;
        if let Some(error) = value["error"].as_str() {
            return Err(match error {
                "reconnect" | "redirect" => RECONNECT.into(),
                "challenge" => "Omnivox requested a new session challenge. The request was not replayed; refresh with a read tool before deciding whether to retry.".into(),
                "limit" => "School response exceeded the size limit.".into(),
                _ => "School request failed or timed out.".into(),
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
            link: value["link"].as_str().unwrap_or("").into(),
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

type LoginOutcome = Result<(String, String), String>;

pub struct Login {
    pub service: String,
    pub id: String,
    pub state: String,
    pub error: Option<String>,
    cancelled: Arc<AtomicBool>,
    result: mpsc::Receiver<LoginOutcome>,
    worker: Option<std::thread::JoinHandle<()>>,
    token: String,
    created: bool,
    origin: String,
    progress: Arc<Mutex<install::Progress>>,
}

impl Login {
    pub fn start(
        service: &str,
        id: &str,
        origin: &str,
        previous: Option<&str>,
        allow_download: bool,
    ) -> Result<Self, String> {
        let origin = match service {
            "moodle" => crate::config::normalize_moodle_url(origin)?,
            "brightspace" => crate::config::normalize_brightspace_url(origin)?,
            "omnivox" => {
                let origin = crate::config::normalize_omnivox_url(origin)?;
                if !crate::config::is_omnivox_host(&origin) {
                    return Err("Use your college's https://…omnivox.ca address.".into());
                }
                origin
            }
            "canvas" => crate::config::normalize_canvas_url(origin)?,
            _ => return Err("Unknown school service.".into()),
        };
        if !origin.starts_with("https://") {
            return Err("Browser login requires an HTTPS school address.".into());
        }
        // Validate an explicit override before starting; installation is done by
        // the login worker so the local UI and Cancel stay responsive.
        let system_browser = system_executable()?;
        let download = download_permitted(
            system_browser.or_else(install::available).is_some(),
            allow_download,
        )?;
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
        let progress = Arc::new(Mutex::new(install::Progress::default()));
        let worker_progress = progress.clone();
        let service_owned = service.to_string();
        let worker = std::thread::spawn(move || {
            let outcome = (|| {
                if download {
                    install::ensure(&stop, &worker_progress)?;
                }
                install::check_cancelled(&stop)?;
                worker_progress.lock().unwrap().phase = "opening";
                let mut browser = Browser::launch_cancellable(
                    &worker_token,
                    &service_owned,
                    &worker_origin,
                    true,
                    &stop,
                )?;
                worker_progress.lock().unwrap().phase = "signin";
                browser.wait_ready(&stop, Duration::from_secs(300))?;
                if let Some(sso) = browser.moodle_sso.as_mut() {
                    let token = sso.token.take().ok_or(moodle::UNAVAILABLE)?;
                    let client = crate::moodle::MoodleClient::new(&crate::config::MoodleConfig {
                        base_url: worker_origin.clone(),
                        token: token.clone(),
                        allow_write: false,
                    });
                    client.site_info()?;
                    install::check_cancelled(&stop)?;
                    drop(browser);
                    remove(&worker_token)?;
                    return Ok((token, worker_origin));
                }
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
            progress,
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
                "moodle" => {
                    let allow = settings.changes_allowed("moodle");
                    settings.moodle = Some(crate::config::MoodleSettings {
                        url: Some(url),
                        token: Some(token),
                        allow_write: Some(allow),
                    });
                    settings.remember_changes_allowed("moodle", allow);
                }
                "brightspace" => {
                    let allow = settings.changes_allowed("brightspace");
                    settings.brightspace = Some(crate::config::BrightspaceSettings {
                        url: Some(url),
                        token: Some(token),
                        allow_write: Some(allow),
                    });
                    settings.remember_changes_allowed("brightspace", allow);
                }
                "omnivox" => {
                    settings.omnivox = Some(crate::config::OmnivoxSettings {
                        url: Some(url),
                        token: Some(token),
                        ..Default::default()
                    })
                }
                "canvas" => {
                    settings.canvas = Some(crate::config::CanvasSettings {
                        url: Some(url),
                        token: Some(token),
                    });
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
        json!({"service":self.service,"id":self.id,"state":self.state,"error":self.error,"origin":self.origin,"progress":*self.progress.lock().unwrap()})
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
        if self.created && (self.state != "connected" || self.service == "moodle") {
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
    fn installed_browser_is_preferred_and_download_requires_explicit_consent() {
        assert!(!download_permitted(true, false).unwrap());
        assert!(!download_permitted(true, true).unwrap());
        assert!(download_permitted(false, false).is_err());
        assert!(download_permitted(false, true).unwrap());
    }

    #[test]
    fn profile_wait_serializes_callers_and_can_be_cancelled_or_expire() {
        crate::config::tests::with_temp_config(|| {
            let path = crate::config::config_path().with_extension("profile-lock-test");
            let first = OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .truncate(false)
                .open(&path)
                .unwrap();
            first.lock_exclusive().unwrap();
            let second = OpenOptions::new()
                .read(true)
                .write(true)
                .open(&path)
                .unwrap();
            let (tx, rx) = mpsc::channel();
            let worker = std::thread::spawn(move || {
                let result =
                    wait_for_profile(&second, &AtomicBool::new(false), Duration::from_secs(2));
                tx.send(result).unwrap();
            });
            assert!(rx.recv_timeout(Duration::from_millis(80)).is_err());
            drop(first);
            assert!(rx.recv_timeout(Duration::from_secs(2)).unwrap().is_ok());
            worker.join().unwrap();

            let owner = OpenOptions::new()
                .read(true)
                .write(true)
                .open(&path)
                .unwrap();
            owner.lock_exclusive().unwrap();
            let waiting = OpenOptions::new()
                .read(true)
                .write(true)
                .open(&path)
                .unwrap();
            let start = Instant::now();
            assert!(
                wait_for_profile(&waiting, &AtomicBool::new(true), Duration::from_secs(120))
                    .unwrap_err()
                    .contains("cancelled")
            );
            assert!(start.elapsed() < Duration::from_secs(1));
            assert!(
                wait_for_profile(&waiting, &AtomicBool::new(false), Duration::from_millis(20))
                    .unwrap_err()
                    .contains("busy")
            );
        });
    }

    #[test]
    fn default_browser_detection_accepts_chromium_family_only() {
        assert!(is_chromium_browser(Path::new("/usr/bin/helium-browser")));
        assert!(is_chromium_browser(Path::new(
            r"C:\Program Files\Microsoft\Edge\Application\msedge.exe"
        )));
        assert!(is_chromium_browser(Path::new(
            "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"
        )));
        assert!(!is_chromium_browser(Path::new("/usr/bin/firefox")));
        assert!(!is_chromium_browser(Path::new(
            "/Applications/Safari.app/Contents/MacOS/Safari"
        )));
        assert_eq!(
            parse_desktop_exec("[Desktop Entry]\nName=Browser\nExec=helium-browser %U\n"),
            Some("helium-browser".into())
        );
        assert_eq!(
            parse_desktop_exec("Exec=\"/opt/helium-browser-bin/helium-wrapper\" %U\n"),
            Some("/opt/helium-browser-bin/helium-wrapper".into())
        );
        assert_eq!(
            parse_desktop_exec("Name=Browser\nExec=omarchy-launch-browser %U\n"),
            Some("omarchy-launch-browser".into())
        );
        assert_eq!(
            parse_reg_sz("    ProgId    REG_SZ    ChromeHTML\r\n"),
            Some("ChromeHTML".into())
        );
        assert_eq!(
            split_command(
                r#""C:\Program Files\Google\Chrome\Application\chrome.exe" --single-argument %1"#
            ),
            Some(r"C:\Program Files\Google\Chrome\Application\chrome.exe".into())
        );
        if let Some(path) = default_https_browser() {
            assert!(path.is_file(), "{path:?}");
            assert!(is_chromium_browser(&path), "{path:?}");
        }
    }

    #[test]
    fn browser_login_rejects_incompatible_services_and_origins_before_launch() {
        for (service, origin) in [
            ("unknown", "https://school.example"),
            ("omnivox", "https://other.example"),
            ("brightspace", "http://school.example"),
            ("brightspace", "https://user:password@school.example"),
        ] {
            assert!(Login::start(service, "test", origin, None, false).is_err());
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
    #[ignore = "requires Chromium; synthetic local Moodle SSO server, no credentials"]
    fn chromium_moodle_sso_captures_redirect_and_confirmation_link() {
        use md5::{Digest, Md5};
        use tiny_http::{Header, Response, Server};
        crate::config::tests::with_temp_config(|| {
            let server = Server::http("127.0.0.1:0").unwrap();
            let origin = format!("http://{}", server.server_addr());
            let base = origin.clone();
            let stop = Arc::new(AtomicBool::new(false));
            let finished = stop.clone();
            let worker = std::thread::spawn(move || {
                while !finished.load(Ordering::SeqCst) {
                    let Some(request) = server.recv_timeout(Duration::from_millis(50)).unwrap()
                    else {
                        continue;
                    };
                    let url = url::Url::parse(&format!("{base}{}", request.url())).unwrap();
                    let passport = url
                        .query_pairs()
                        .find(|(k, _)| k == "passport")
                        .map(|(_, v)| v.into_owned())
                        .unwrap_or_default();
                    let site = if url.path().starts_with("/confirm/") {
                        format!("{base}/confirm")
                    } else {
                        base.clone()
                    };
                    let hash = format!("{:x}", Md5::digest(format!("{site}{passport}")));
                    let callback = format!(
                        "moodlemobile://token={}",
                        base64::engine::general_purpose::STANDARD
                            .encode(format!("{hash}:::{}:::discard-private", "a".repeat(32)))
                    );
                    let response = if site.ends_with("/confirm") {
                        Response::from_string(format!("<a href=\"{callback}\">Launch app</a>"))
                            .with_header(Header::from_bytes("Content-Type", "text/html").unwrap())
                    } else {
                        Response::from_string("")
                            .with_status_code(302)
                            .with_header(Header::from_bytes("Location", callback).unwrap())
                    };
                    let _ = request.respond(response);
                }
            });
            let result = std::panic::catch_unwind(|| {
                for site in [origin.clone(), format!("{origin}/confirm")] {
                    let token = format!("{PREFIX}{}", crate::config::random_hex(16));
                    let dir = profile_path(&token).unwrap();
                    private_dir(&dir).unwrap();
                    save(
                        &dir.join("session.json"),
                        &Profile {
                            service: "moodle".into(),
                            origin: site.clone(),
                            home: "about:blank".into(),
                            mobile: json!({}),
                            identity: None,
                        },
                    )
                    .unwrap();
                    let mut browser = Browser::launch(&token, "moodle", &site, false).unwrap();
                    browser
                        .wait_ready(&AtomicBool::new(false), Duration::from_secs(10))
                        .unwrap();
                    assert_eq!(
                        browser.moodle_sso.as_ref().unwrap().token.as_deref(),
                        Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
                    );
                    drop(browser);
                    remove(&token).unwrap();
                    assert_eq!(fs::read_dir(&dir).unwrap().count(), 1);
                    assert!(dir.join("profile.lock").is_file());
                }
            });
            stop.store(true, Ordering::SeqCst);
            worker.join().unwrap();
            if let Err(panic) = result {
                std::panic::resume_unwind(panic);
            }
        });
    }

    #[test]
    fn cancelled_login_cannot_save_account() {
        crate::config::tests::with_temp_config(|| {
            let (tx, result) = mpsc::channel();
            let cancelled = Arc::new(AtomicBool::new(false));
            let login = Login {
                progress: Arc::new(Mutex::new(install::Progress::default())),
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
                progress: Arc::new(Mutex::new(install::Progress::default())),
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
                        "/d2l/api/file" | "/Mobl/file" => {
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
                        "/Mobl/redirect" => {
                            status = 302;
                            headers.push(("Location", "/d2l/api/should-not-replay"));
                            String::new()
                        }
                        "/Mobl/write" | "/Mobl/write-challenge" | "/Mobl/write-redirect" => {
                            counted.fetch_add(1, Ordering::SeqCst);
                            assert_eq!(request.method().as_str(), "POST");
                            assert!(request
                                .headers()
                                .iter()
                                .any(|h| h.field.equiv("X-K")
                                    && h.value.as_str() == "site-challenge"));
                            assert!(request
                                .headers()
                                .iter()
                                .any(|h| h.field.equiv("X-User-Agent-Ovx")
                                    && h.value.as_str().starts_with("OVX ")));
                            if request.url() == "/Mobl/write-challenge" {
                                headers.push(("X-KE", "expired"));
                            } else if request.url() == "/Mobl/write-redirect" {
                                status = 307;
                                headers.push(("Location", "/d2l/api/should-not-replay"));
                            }
                            headers.push(("Content-Type", "application/json"));
                            "true".into()
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
                // Mobile JSON stays on the official worker; only binary GET
                // downloads may use fetch in the same profile and origin.
                browser.profile.service = "omnivox".into();
                let file = browser
                    .request("GET", "/Mobl/file", None, None, true, 1024)
                    .unwrap();
                assert_eq!(file.bytes, b"%PDF-fixture");
                assert!(browser
                    .request("GET", "/Mobl/file", None, None, true, 2)
                    .is_err());
                assert!(browser
                    .request("GET", "/Mobl/file", None, None, false, 1024)
                    .is_err());
                assert!(browser
                    .request("POST", "/Mobl/file", Some(b"{}"), None, true, 1024)
                    .is_err());
                assert!(browser
                    .request(
                        "GET",
                        "https://other.example/Mobl/file",
                        None,
                        None,
                        true,
                        1024
                    )
                    .is_err());
                assert!(browser
                    .request("GET", "/Mobl/redirect", None, None, true, 1024)
                    .is_err());
                browser.evaluate("window.userAgentRequete='OVX fixture';window.Skytech={Commun:{Utils:{LocalStorage:{GetData:(key,fallback)=>key==='X-K'?'site-challenge':fallback}}}};true".into()).unwrap();
                assert_eq!(
                    browser
                        .mobile_write("/Mobl/write", &json!({"action":"fixture"}))
                        .unwrap()
                        .json()
                        .unwrap(),
                    true
                );
                assert!(browser
                    .mobile_write("/Mobl/write-challenge", &json!({}))
                    .err()
                    .unwrap()
                    .contains("not replayed"));
                assert!(browser
                    .mobile_write("/Mobl/write-redirect", &json!({}))
                    .is_err());
                assert!(browser
                    .mobile_write("https://other.example/Mobl/write", &json!({}))
                    .is_err());
                browser.profile.service = "brightspace".into();
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
                    5,
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
