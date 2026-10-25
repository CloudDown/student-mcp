use getrandom::getrandom;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use url::Url;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const DEFAULT_PORT: u16 = 8420;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GatewaySettings {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub public_enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub local_enabled: Option<bool>,
}

impl GatewaySettings {
    pub fn public_enabled(&self) -> bool {
        self.public_enabled.unwrap_or(true)
    }

    pub fn local_enabled(&self) -> bool {
        self.local_enabled.unwrap_or(true)
    }
}

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct MoodleSettings {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allow_write: Option<bool>,
}

// Never include the web-service token when debugging settings.
impl std::fmt::Debug for MoodleSettings {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MoodleSettings")
            .field("url", &self.url)
            .field("token", &self.token.as_ref().map(|_| "[redacted]"))
            .field("allow_write", &self.allow_write)
            .finish()
    }
}

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct OmnivoxSettings {
    pub url: Option<String>,
    pub token: Option<String>,
}

// Never include access keys when debugging settings.
impl std::fmt::Debug for OmnivoxSettings {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OmnivoxSettings")
            .field("url", &self.url)
            .field("token", &"[redacted]")
            .finish()
    }
}

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct BrightspaceSettings {
    pub url: Option<String>,
    pub token: Option<String>,
}

// Never include access tokens when debugging settings.
impl std::fmt::Debug for BrightspaceSettings {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BrightspaceSettings")
            .field("url", &self.url)
            .field("token", &"[redacted]")
            .finish()
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub gateway: GatewaySettings,
    pub moodle: Option<MoodleSettings>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub omnivox: Option<OmnivoxSettings>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub brightspace: Option<BrightspaceSettings>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scale: Option<f32>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub accounts: Vec<Account>,
    #[serde(skip)]
    pub account_scope: Option<(String, String)>,
    /// Set when the file on disk could not be read or parsed. Saves refuse to
    /// replace that file, so a bad edit cannot wipe stored credentials.
    #[serde(skip)]
    pub config_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Account {
    pub id: String,
    pub service: String,
    pub moodle: Option<MoodleSettings>,
    pub omnivox: Option<OmnivoxSettings>,
    pub brightspace: Option<BrightspaceSettings>,
}

/// Interface zoom persisted in the config; 1.0 keeps the platform default.
pub const DEFAULT_SCALE: f32 = 1.2;
pub const SCALE_MIN: f32 = 0.8;
pub const SCALE_MAX: f32 = 2.0;
pub const SCALE_STEP: f32 = 0.1;

impl Settings {
    pub fn scale(&self) -> f32 {
        self.scale
            .unwrap_or(DEFAULT_SCALE)
            .clamp(SCALE_MIN, SCALE_MAX)
    }
}

#[derive(Debug, Clone)]
pub struct Gateway {
    pub port: u16,
    pub token: String,
}

#[derive(Debug, Clone)]
pub struct MoodleConfig {
    pub base_url: String,
    pub token: String,
    pub allow_write: bool,
}

#[derive(Clone)]
pub struct OmnivoxConfig {
    pub base_url: String,
    pub token: String,
}

#[derive(Clone)]
pub struct BrightspaceConfig {
    pub base_url: String,
    pub token: String,
}

impl Settings {
    pub fn account_list(&self, service: &str) -> Vec<(String, String)> {
        let mut list = Vec::new();
        let url = match service {
            "moodle" => self.moodle().map(|c| c.base_url),
            "omnivox" => self.omnivox().map(|c| c.base_url),
            "brightspace" => self.brightspace().map(|c| c.base_url),
            _ => None,
        };
        if let Some(url) = url {
            list.push((service.to_string(), url));
        }
        for account in &self.accounts {
            if account.service == service {
                let url = match service {
                    "moodle" => account.moodle.as_ref().and_then(|c| c.url.clone()),
                    "omnivox" => account.omnivox.as_ref().and_then(|c| c.url.clone()),
                    "brightspace" => account.brightspace.as_ref().and_then(|c| c.url.clone()),
                    _ => None,
                };
                if let Some(url) = url {
                    list.push((account.id.clone(), url));
                }
            }
        }
        list
    }

    pub fn for_account(&self, service: &str, id: &str) -> Self {
        let mut scoped = self.clone();
        if id != service {
            let account = self
                .accounts
                .iter()
                .find(|a| a.id == id && a.service == service);
            match service {
                "moodle" => scoped.moodle = account.and_then(|a| a.moodle.clone()),
                "omnivox" => scoped.omnivox = account.and_then(|a| a.omnivox.clone()),
                "brightspace" => scoped.brightspace = account.and_then(|a| a.brightspace.clone()),
                _ => {}
            }
        }
        scoped.account_scope = Some((service.into(), id.into()));
        scoped
    }

    pub fn reload(&self) -> Self {
        let fresh = Self::load();
        match &self.account_scope {
            Some((service, id)) => fresh.for_account(service, id),
            None => fresh,
        }
    }

    pub fn load() -> Self {
        let path = config_path();
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Self::default(),
            Err(error) => {
                return Self {
                    config_error: Some(format!(
                        "Cannot read {}. It was left unchanged. {error}",
                        path.display()
                    )),
                    ..Self::default()
                };
            }
        };
        if text.trim().is_empty() {
            return Self::default();
        }
        match serde_json::from_str(&text) {
            Ok(settings) => settings,
            Err(error) => Self {
                config_error: Some(format!(
                    "Configuration file is not valid JSON and was left unchanged ({}): {error}",
                    path.display()
                )),
                ..Self::default()
            },
        }
    }

    pub fn save(&self) -> std::io::Result<()> {
        if let Some(message) = &self.config_error {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                message.clone(),
            ));
        }
        if let Some((service, id)) = &self.account_scope {
            let mut root = Self::load();
            if let Some(message) = root.config_error {
                return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, message));
            }
            root.scale = self.scale;
            if id == service {
                match service.as_str() {
                    "moodle" => root.moodle = self.moodle.clone(),
                    "omnivox" => root.omnivox = self.omnivox.clone(),
                    "brightspace" => root.brightspace = self.brightspace.clone(),
                    _ => {}
                }
            } else {
                root.accounts
                    .retain(|account| !(account.id == *id && account.service == *service));
                let account = Account {
                    id: id.clone(),
                    service: service.clone(),
                    moodle: (service == "moodle").then(|| self.moodle.clone()).flatten(),
                    omnivox: (service == "omnivox")
                        .then(|| self.omnivox.clone())
                        .flatten(),
                    brightspace: (service == "brightspace")
                        .then(|| self.brightspace.clone())
                        .flatten(),
                };
                if account.moodle.is_some()
                    || account.omnivox.is_some()
                    || account.brightspace.is_some()
                {
                    root.accounts.push(account);
                }
            }
            return root.save();
        }
        let path = config_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let text = serde_json::to_string_pretty(self).map_err(|error| {
            std::io::Error::new(std::io::ErrorKind::InvalidData, error.to_string())
        })? + "\n";
        write_private(&path, text.as_bytes())
    }

    /// Gateway settings. The public token is generated once and stored on the
    /// root config, including when this view is scoped to one account.
    /// A config that failed to load is never replaced.
    pub fn gateway(&self) -> Gateway {
        let port = self.gateway.port.unwrap_or(DEFAULT_PORT);
        if self.config_error.is_some() {
            return Gateway {
                port,
                token: String::new(),
            };
        }
        if let Some(token) = self
            .gateway
            .token
            .clone()
            .filter(|token| !token.is_empty())
        {
            return Gateway { port, token };
        }
        let mut root = Self::load();
        root.account_scope = None;
        let port = root.gateway.port.unwrap_or(port);
        if root.config_error.is_some() {
            return Gateway {
                port,
                token: String::new(),
            };
        }
        if let Some(token) = root
            .gateway
            .token
            .clone()
            .filter(|token| !token.is_empty())
        {
            return Gateway { port, token };
        }
        let generated = random_hex(24);
        root.gateway.token = Some(generated.clone());
        if root.save().is_ok() {
            return Gateway {
                port,
                token: generated,
            };
        }
        if let Some(token) = Self::load()
            .gateway
            .token
            .filter(|token| !token.is_empty())
        {
            return Gateway { port, token };
        }
        Gateway {
            port,
            token: generated,
        }
    }

    pub fn moodle(&self) -> Option<MoodleConfig> {
        let moodle = self.moodle.as_ref()?;
        let base_url = moodle.url.clone()?.trim().to_string();
        let token = moodle.token.clone()?.trim().to_string();
        if base_url.is_empty() || token.is_empty() {
            return None;
        }
        Some(MoodleConfig {
            base_url: normalize_moodle_url(&base_url).ok()?,
            token,
            allow_write: moodle.allow_write.unwrap_or(false),
        })
    }

    pub fn disable(&mut self, service: &str) {
        match service {
            "moodle" => self.moodle = None,
            "omnivox" => self.omnivox = None,
            "brightspace" => self.brightspace = None,
            _ => {}
        }
    }

    pub fn services_active(&self) -> Vec<String> {
        ["moodle", "omnivox", "brightspace"]
            .into_iter()
            .filter(|id| !self.account_list(id).is_empty())
            .map(str::to_string)
            .collect()
    }

    pub fn omnivox(&self) -> Option<OmnivoxConfig> {
        let saved = self.omnivox.as_ref()?;
        let token = saved.token.as_deref()?.trim();
        if token.is_empty() || token.chars().any(char::is_whitespace) {
            return None;
        }
        Some(OmnivoxConfig {
            base_url: normalize_omnivox_url(saved.url.as_deref()?).ok()?,
            token: token.to_string(),
        })
    }

    pub fn brightspace(&self) -> Option<BrightspaceConfig> {
        let saved = self.brightspace.as_ref()?;
        let token = saved.token.as_deref()?.trim();
        if token.is_empty() || token.chars().any(char::is_whitespace) {
            return None;
        }
        Some(BrightspaceConfig {
            base_url: normalize_brightspace_url(saved.url.as_deref()?).ok()?,
            token: token.to_string(),
        })
    }
}

pub fn config_path() -> PathBuf {
    // Test override is process-global and mutex-guarded (see with_temp_config).
    // Checked before env vars so parallel tests never race on set_var/remove_var.
    #[cfg(test)]
    if let Some(path) = tests::override_config_path() {
        return path;
    }
    for variable in ["STUDENT_MCP_CONFIG_FILE", "MOODLE_CONFIG_FILE"] {
        if let Ok(path) = std::env::var(variable) {
            if !path.trim().is_empty() {
                return PathBuf::from(path);
            }
        }
    }
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_else(|_| ".".to_string());
    Path::new(&home).join(".student-mcp.json")
}

pub fn random_hex(bytes: usize) -> String {
    let mut buffer = vec![0u8; bytes];
    // A failed read used to leave the buffer full of zeros and publish that
    // value as the connector secret. Refuse to run rather than mint a guessable token.
    if let Err(error) = getrandom(&mut buffer) {
        panic!("OS random number generator unavailable: {error}");
    }
    buffer.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// Public MCP endpoint for a given origin: the token in the path is the secret.
pub fn mcp_endpoint(origin: &str, token: &str) -> String {
    format!("{}/mcp/{}", origin.trim_end_matches('/'), token)
}

/// Compare the whole token. A mismatch does not return at the first different byte.
pub fn token_matches(expected: &str, provided: Option<&str>) -> bool {
    let Some(provided) = provided else {
        return false;
    };
    let a = expected.as_bytes();
    let b = provided.as_bytes();
    if a.is_empty() || b.is_empty() {
        return false;
    }
    // Bound the scan. Connector tokens are short; a multi-megabyte guess must
    // not pin the accept thread. Length still participates, so a longer equal
    // prefix cannot match.
    let mut diff = a.len() ^ b.len();
    let compared = a.len().max(b.len()).min(4096);
    for index in 0..compared {
        let left = a.get(index).copied().unwrap_or(0);
        let right = b.get(index).copied().unwrap_or(0);
        diff |= usize::from(left ^ right);
    }
    if a.len().max(b.len()) > 4096 {
        diff |= 1;
    }
    diff == 0
}

fn is_loopback_host(host: &str) -> bool {
    // `Url::host_str` returns IPv6 without brackets (`::1`, not `[::1]`).
    let host = host.trim_matches(['[', ']']);
    host.eq_ignore_ascii_case("localhost") || host == "127.0.0.1" || host == "::1"
}

fn parse_site_url(input: &str) -> Result<Url, String> {
    let trimmed = input.trim().trim_end_matches('/');
    let parsed = Url::parse(trimmed)
        .or_else(|_| Url::parse(&format!("https://{trimmed}")))
        .map_err(|_| "The site address is not a valid URL.".to_string())?;
    if parsed.username() != ""
        || parsed.password().is_some()
        || parsed.query().is_some()
        || parsed.fragment().is_some()
    {
        return Err("The address must not contain credentials, a query or a fragment.".to_string());
    }
    let localhost = parsed
        .host_str()
        .is_some_and(is_loopback_host);
    if parsed.scheme() != "https" && !(parsed.scheme() == "http" && localhost) {
        return Err("The address must use HTTPS (HTTP is only allowed for localhost).".to_string());
    }
    Ok(parsed)
}

pub fn normalize_moodle_url(input: &str) -> Result<String, String> {
    let mut url = parse_site_url(input)?;
    let lower = url.as_str().to_lowercase();
    let suffix = "/webservice/rest/server.php";
    if lower.ends_with(suffix) && url.as_str().len() >= suffix.len() {
        // Case-insensitive suffix cut: rebuild from the prefix.
        let original = url.as_str();
        let cut = &original[..original.len() - suffix.len()];
        url = Url::parse(cut).map_err(|_| "The site address is not a valid URL.".to_string())?;
    }
    let mut form = url.origin().ascii_serialization();
    let path = url.path().trim_end_matches('/').to_string();
    form.push_str(&path);
    Ok(form)
}

pub fn normalize_omnivox_url(input: &str) -> Result<String, String> {
    let url = parse_site_url(input)?;
    let host = url.host_str().unwrap_or("").to_ascii_lowercase();
    if host == "omnivox.ca" || host.ends_with(".omnivox.ca") {
        return Err(
            "Use the address of your Omnivox MCP gateway (for example http://127.0.0.1:3000), not the college Omnivox site."
                .into(),
        );
    }
    match url.path().trim_end_matches('/') {
        "" | "/mcp" | "/tools" => {}
        _ => return Err("Use the Omnivox MCP root address, e.g. http://127.0.0.1:3000.".into()),
    }
    Ok(url.origin().ascii_serialization())
}

pub fn normalize_brightspace_url(input: &str) -> Result<String, String> {
    let url = parse_site_url(input)?;
    if !matches!(url.path().trim_end_matches('/'), "" | "/d2l" | "/d2l/home") {
        return Err("Use your Brightspace site address, e.g. https://mycourses.mcgill.ca.".into());
    }
    Ok(url.origin().ascii_serialization())
}

fn write_private(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let temp = path.with_extension(format!("json.tmp-{}", std::process::id()));
    {
        let mut file = std::fs::File::create(&temp)?;
        file.write_all(bytes)?;
        file.sync_all().ok();
    }
    set_private(&temp);
    std::fs::rename(&temp, path)?;
    Ok(())
}

fn set_private(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
    }
    #[cfg(windows)]
    {
        let _ = path;
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    // Process-global so worker threads spawned during a test still hit the
    // same temp file as the test body (save/load from jobs, checks, …).
    static CONFIG_OVERRIDE: std::sync::Mutex<Option<PathBuf>> = std::sync::Mutex::new(None);
    static CONFIG_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    pub(crate) fn override_config_path() -> Option<PathBuf> {
        CONFIG_OVERRIDE
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    pub(crate) fn with_temp_config<T>(test: impl FnOnce() -> T) -> T {
        let _guard = CONFIG_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let dir = std::env::temp_dir().join(format!(
            "student-mcp-{}-{}",
            std::process::id(),
            random_hex(8)
        ));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let path = dir.join("settings.json");
        *CONFIG_OVERRIDE
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(path);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(test));
        *CONFIG_OVERRIDE
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
        std::fs::remove_dir_all(dir).ok();
        match result {
            Ok(value) => value,
            Err(panic) => std::panic::resume_unwind(panic),
        }
    }

    #[test]
    fn scale_clamps_and_persists() {
        with_temp_config(|| {
            let mut settings = Settings::load();
            assert!((settings.scale() - DEFAULT_SCALE).abs() < f32::EPSILON);
            settings.scale = Some(9.9);
            assert_eq!(settings.scale(), SCALE_MAX);
            settings.scale = Some(0.1);
            assert_eq!(settings.scale(), SCALE_MIN);
            settings.scale = Some(1.5);
            let _ = settings.save();
            let reloaded = Settings::load();
            assert_eq!(reloaded.scale, Some(1.5));
        });
    }

    #[test]
    fn token_generates_once_and_persists() {
        with_temp_config(|| {
            let first = Settings::load().gateway();
            assert_eq!(first.token.len(), 48);
            let again = Settings::load().gateway();
            assert_eq!(first.token, again.token);
            assert_eq!(first.port, DEFAULT_PORT);
            // it is also on disk
            let text = std::fs::read_to_string(config_path()).unwrap();
            assert!(text.contains(&first.token));
        });
    }

    #[test]
    fn settings_roundtrip_and_disable() {
        with_temp_config(|| {
            let mut settings = Settings::load();
            settings.moodle = Some(MoodleSettings {
                url: Some("https://moodle.polymtl.ca".into()),
                token: Some("tok".into()),
                allow_write: Some(false),
            });
            settings.save().unwrap();
            let loaded = Settings::load();
            assert!(loaded.moodle().is_some());
            let mut settings = loaded;
            settings.disable("moodle");
            settings.save().unwrap();
            assert!(Settings::load().moodle().is_none());
        });
    }

    #[test]
    fn timing_safe_tokens() {
        assert!(!token_matches("abc", Some("ab")));
        assert!(!token_matches("abc", Some("abd")));
        assert!(token_matches("abc", Some("abc")));
        assert!(!token_matches("abc", None));
    }

    #[test]
    fn urls_normalize() {
        assert_eq!(
            normalize_moodle_url("moodle.polymtl.ca/").unwrap(),
            "https://moodle.polymtl.ca"
        );
        assert_eq!(
            normalize_moodle_url("https://moodle.polymtl.ca").unwrap(),
            "https://moodle.polymtl.ca"
        );
        assert!(normalize_moodle_url("ftp://moodle.polymtl.ca").is_err());
    }

    #[test]
    fn moodle_urls_preserve_installation_paths() {
        for input in [
            "https://school.example/moodle/",
            "school.example/moodle",
            "https://school.example/moodle/webservice/rest/server.php",
            "https://school.example/moodle/WEBSERVICE/REST/SERVER.PHP",
        ] {
            assert_eq!(
                normalize_moodle_url(input).unwrap(),
                "https://school.example/moodle"
            );
        }
    }

    #[test]
    fn multiple_accounts_keep_credentials_permissions_and_gateway_separate() {
        with_temp_config(|| {
            let mut original = Settings::default();
            original.gateway.token = Some("shared-link-token".into());
            original.moodle = Some(MoodleSettings {
                url: Some("https://first.example".into()),
                token: Some("first-token".into()),
                allow_write: Some(false),
            });
            original.save().unwrap();
            let mut second = original.for_account("moodle", "second-account");
            second.moodle = Some(MoodleSettings {
                url: Some("https://second.example".into()),
                token: Some("second-token".into()),
                allow_write: Some(true),
            });
            second.save().unwrap();
            let loaded = Settings::load();
            assert_eq!(loaded.account_list("moodle").len(), 2);
            assert_eq!(loaded.moodle().unwrap().token, "first-token");
            assert!(!loaded.moodle().unwrap().allow_write);
            let mut selected = loaded.for_account("moodle", "second-account");
            assert_eq!(selected.moodle().unwrap().token, "second-token");
            assert!(selected.moodle().unwrap().allow_write);
            selected.disable("moodle");
            selected.save().unwrap();
            let loaded = Settings::load();
            assert_eq!(loaded.account_list("moodle").len(), 1);
            assert_eq!(loaded.gateway.token.as_deref(), Some("shared-link-token"));
            assert_eq!(loaded.moodle().unwrap().token, "first-token");
        });
    }

    #[test]
    fn brightspace_settings_validate_roundtrip_and_disconnect() {
        for input in [
            "mycourses.mcgill.ca",
            "https://mycourses.mcgill.ca/d2l/home/",
        ] {
            assert_eq!(
                normalize_brightspace_url(input).unwrap(),
                "https://mycourses.mcgill.ca"
            );
        }
        for input in [
            "http://school.example",
            "https://user:password@school.example",
            "https://school.example/?token=secret",
            "https://school.example/unexpected",
        ] {
            assert!(normalize_brightspace_url(input).is_err());
        }
        with_temp_config(|| {
            let settings = Settings {
                brightspace: Some(BrightspaceSettings {
                    url: Some("https://school.example".into()),
                    token: Some("private-token".into()),
                }),
                ..Default::default()
            };
            settings.save().unwrap();
            let mut loaded = Settings::load();
            assert_eq!(loaded.brightspace().unwrap().token, "private-token");
            assert_eq!(loaded.services_active(), vec!["brightspace"]);
            assert!(!format!("{:?}", loaded.brightspace).contains("private-token"));
            loaded.disable("brightspace");
            loaded.save().unwrap();
            assert!(Settings::load().brightspace().is_none());
        });
    }

    #[test]
    fn omnivox_settings_validate_roundtrip_and_disconnect() {
        for (input, origin) in [
            ("http://127.0.0.1:3000", "http://127.0.0.1:3000"),
            ("http://127.0.0.1:3000/tools/", "http://127.0.0.1:3000"),
            ("http://localhost:3000/mcp", "http://localhost:3000"),
            ("https://omnivox.example.com", "https://omnivox.example.com"),
        ] {
            assert_eq!(normalize_omnivox_url(input).unwrap(), origin);
        }
        for input in [
            "https://cegepmontpetit.omnivox.ca",
            "http://school.example",
            "https://127.0.0.1:3000/unexpected",
            "https://user:password@127.0.0.1:3000",
            "http://127.0.0.1:3000/?key=secret",
        ] {
            assert!(normalize_omnivox_url(input).is_err());
        }
        with_temp_config(|| {
            let settings = Settings {
                omnivox: Some(OmnivoxSettings {
                    url: Some("http://127.0.0.1:3000".into()),
                    token: Some("private-token".into()),
                }),
                ..Default::default()
            };
            settings.save().unwrap();
            let mut loaded = Settings::load();
            assert_eq!(loaded.omnivox().unwrap().token, "private-token");
            assert_eq!(loaded.services_active(), vec!["omnivox"]);
            assert!(!format!("{:?}", loaded.omnivox).contains("private-token"));
            loaded.disable("omnivox");
            loaded.save().unwrap();
            assert!(Settings::load().omnivox().is_none());
        });
    }

    #[test]
    fn moodle_debug_redacts_the_token() {
        let settings = MoodleSettings {
            url: Some("https://moodle.example".into()),
            token: Some("private-token".into()),
            allow_write: Some(false),
        };
        let rendered = format!("{settings:?}");
        assert!(rendered.contains("https://moodle.example"));
        assert!(rendered.contains("[redacted]"));
        assert!(!rendered.contains("private-token"));
    }

    #[test]
    fn corrupt_config_is_left_unchanged() {
        with_temp_config(|| {
            let path = config_path();
            let original = b"{not json, \"token\":\"keep-me\"}";
            std::fs::write(&path, original).unwrap();
            let loaded = Settings::load();
            assert!(loaded.config_error.is_some());
            assert!(loaded.moodle().is_none());
            assert!(loaded.save().is_err());
            let _ = loaded.gateway();
            assert_eq!(std::fs::read(&path).unwrap(), original);
        });
    }

    #[test]
    fn empty_config_file_is_a_fresh_install() {
        with_temp_config(|| {
            std::fs::write(config_path(), " \n").unwrap();
            let loaded = Settings::load();
            assert!(loaded.config_error.is_none());
            assert!(loaded.gateway().token.len() == 48);
        });
    }

    #[test]
    fn gateway_token_from_a_scoped_view_is_stored_on_the_root() {
        with_temp_config(|| {
            let scoped = Settings::default().for_account("moodle", "extra");
            let gateway = scoped.gateway();
            assert_eq!(gateway.token.len(), 48);
            assert_eq!(
                Settings::load().gateway.token.as_deref(),
                Some(gateway.token.as_str())
            );
        });
    }

    #[test]
    fn saving_one_service_keeps_another_account_with_the_same_id() {
        with_temp_config(|| {
            let mut settings = Settings::default();
            settings.accounts.push(Account {
                id: "shared".into(),
                service: "omnivox".into(),
                moodle: None,
                omnivox: Some(OmnivoxSettings {
                    url: Some("http://127.0.0.1:3000".into()),
                    token: Some("ox-token".into()),
                }),
                brightspace: None,
            });
            settings.save().unwrap();
            let mut moodle = Settings::load().for_account("moodle", "shared");
            moodle.moodle = Some(MoodleSettings {
                url: Some("https://moodle.example".into()),
                token: Some("md-token".into()),
                allow_write: Some(false),
            });
            moodle.save().unwrap();
            let loaded = Settings::load();
            assert_eq!(
                loaded.for_account("omnivox", "shared").omnivox().unwrap().token,
                "ox-token"
            );
            assert_eq!(
                loaded.for_account("moodle", "shared").moodle().unwrap().token,
                "md-token"
            );
            assert_eq!(loaded.account_list("omnivox").len(), 1);
            assert_eq!(loaded.account_list("moodle").len(), 1);
        });
    }

    #[test]
    fn ipv6_loopback_is_a_local_http_address() {
        assert_eq!(
            normalize_omnivox_url("http://[::1]:3000").unwrap(),
            "http://[::1]:3000"
        );
        assert_eq!(
            normalize_omnivox_url("http://[::1]:3000/mcp").unwrap(),
            "http://[::1]:3000"
        );
        assert!(normalize_moodle_url("http://[::1]/moodle").is_ok());
        assert!(normalize_moodle_url("http://10.0.0.8").is_err());
    }

    #[test]
    fn token_compare_rejects_a_different_length_and_a_long_guess() {
        assert!(!token_matches("abcd", Some("abc")));
        assert!(!token_matches("abcd", Some(&"a".repeat(5000))));
        assert!(token_matches("abcd", Some("abcd")));
    }
}
