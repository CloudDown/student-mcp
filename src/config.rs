use getrandom::getrandom;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use url::Url;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const DEFAULT_PORT: u16 = 8420;
static SAVE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

// Keep this file in place: unlinking it would let two processes lock different inodes.
fn lock_config() -> std::io::Result<std::fs::File> {
    let path = config_path().with_extension("lock");
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut options = std::fs::OpenOptions::new();
    options.read(true).write(true).create(true).truncate(false);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let file = options.open(path)?;
    fs2::FileExt::lock_exclusive(&file)?;
    Ok(file)
}

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
    /// Legacy input only. Migrated into tools.allow_changes on load; omit on next save.
    #[serde(skip_serializing)]
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
    /// Mobile app device id used to rebuild the `OVX` user agent.
    pub device_id: Option<String>,
    /// Mobile session auth hash returned by /Mobl/App/Initialisation.
    pub auth_hash: Option<String>,
}

// Never include access keys when debugging settings.
impl std::fmt::Debug for OmnivoxSettings {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OmnivoxSettings")
            .field("url", &self.url)
            .field("token", &"[redacted]")
            .field("device_id", &self.device_id)
            .field("auth_hash", &"[redacted]")
            .finish()
    }
}

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct MailSettings {
    pub url: Option<String>,
    pub token: Option<String>,
    pub mailbox: Option<String>,
    pub forward_to: Option<String>,
    #[serde(default)]
    pub forwarding: bool,
}

impl std::fmt::Debug for MailSettings {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MailSettings")
            .field("url", &self.url)
            .field("token", &"[redacted]")
            .field("mailbox", &self.mailbox)
            .field("forward_to", &self.forward_to)
            .field("forwarding", &self.forwarding)
            .finish()
    }
}

#[derive(Clone)]
pub struct MailConfig {
    pub base_url: String,
    pub token: String,
    pub mailbox: Option<String>,
    pub forward_to: Option<String>,
    pub forwarding: bool,
}

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct BrightspaceSettings {
    pub url: Option<String>,
    pub token: Option<String>,
    /// Legacy input only. Migrated into tools.allow_changes on load; omit on next save.
    #[serde(skip_serializing)]
    pub allow_write: Option<bool>,
}

// Never include access tokens when debugging settings.
impl std::fmt::Debug for BrightspaceSettings {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BrightspaceSettings")
            .field("url", &self.url)
            .field("token", &"[redacted]")
            .field("allow_write", &self.allow_write)
            .finish()
    }
}

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct CanvasSettings {
    pub url: Option<String>,
    pub token: Option<String>,
}

impl std::fmt::Debug for CanvasSettings {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CanvasSettings")
            .field("url", &self.url)
            .field("token", &"[redacted]")
            .finish()
    }
}

#[derive(Clone)]
pub struct CanvasConfig {
    pub base_url: String,
    pub token: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ToolSettings {
    pub advanced: bool,
    /// Explicit overrides keyed by service/account ID. Missing means read-only,
    /// except for a legacy account that explicitly opted into allow_write.
    pub allow_changes: std::collections::BTreeMap<String, bool>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Accepted from older installs and ignored. Approval belongs to the AI client.
    #[serde(default, skip_serializing)]
    pub tools: ToolSettings,
    pub gateway: GatewaySettings,
    pub moodle: Option<MoodleSettings>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub omnivox: Option<OmnivoxSettings>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub brightspace: Option<BrightspaceSettings>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub outlook: Option<MailSettings>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub canvas: Option<CanvasSettings>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scale: Option<f32>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub accounts: Vec<Account>,
    /// Connected services can be temporarily hidden from MCP without deleting credentials.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub disabled_services: Vec<String>,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outlook: Option<MailSettings>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub canvas: Option<CanvasSettings>,
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
    // Legacy permission; explicit modern account overrides take precedence.
    pub allow_write: bool,
}

#[derive(Clone)]
pub struct OmnivoxConfig {
    pub base_url: String,
    pub token: String,
    pub device_id: Option<String>,
    pub auth_hash: Option<String>,
}

#[derive(Clone)]
pub struct BrightspaceConfig {
    pub base_url: String,
    pub token: String,
    // Legacy permission; explicit modern account overrides take precedence.
    pub allow_write: bool,
}

impl Settings {
    pub fn changes_allowed(&self, service: &str) -> bool {
        let id = self
            .account_scope
            .as_ref()
            .filter(|(s, _)| s == service)
            .map(|(_, id)| id.as_str())
            .unwrap_or(service);
        self.tools
            .allow_changes
            .get(&format!("{service}/{id}"))
            .copied()
            .unwrap_or_else(|| match service {
                "moodle" => self.moodle().is_some_and(|c| c.allow_write),
                "brightspace" => self.brightspace().is_some_and(|c| c.allow_write),
                _ => false,
            })
    }

    pub fn remember_changes_allowed(&mut self, service: &str, enabled: bool) {
        let id = self
            .account_scope
            .as_ref()
            .filter(|(known, _)| known == service)
            .map(|(_, id)| id.as_str())
            .unwrap_or(service);
        if enabled {
            self.tools
                .allow_changes
                .insert(format!("{service}/{id}"), true);
        }
    }

    pub fn account_list(&self, service: &str) -> Vec<(String, String)> {
        let mut list = Vec::new();
        let url = match service {
            "moodle" => self.moodle().map(|c| c.base_url),
            "omnivox" => self.omnivox().map(|c| c.base_url),
            "brightspace" => self.brightspace().map(|c| c.base_url),
            "outlook" => self.outlook().map(|c| c.base_url),
            "canvas" => self.canvas().map(|c| c.base_url),
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
                    "outlook" => account.outlook.as_ref().and_then(|c| c.url.clone()),
                    "canvas" => account.canvas.as_ref().and_then(|c| c.url.clone()),
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
                "outlook" => scoped.outlook = account.and_then(|a| a.outlook.clone()),
                "canvas" => scoped.canvas = account.and_then(|a| a.canvas.clone()),
                _ => {}
            }
        }
        scoped.account_scope = Some((service.into(), id.into()));
        scoped
    }

    fn migrate_permissions(&mut self) {
        self.promote_write_permission(
            "moodle",
            "moodle",
            self.moodle.as_ref().and_then(|saved| saved.allow_write),
        );
        self.promote_write_permission(
            "brightspace",
            "brightspace",
            self.brightspace
                .as_ref()
                .and_then(|saved| saved.allow_write),
        );
        let extra: Vec<_> = self
            .accounts
            .iter()
            .map(|account| {
                let allow = match account.service.as_str() {
                    "moodle" => account.moodle.as_ref().and_then(|saved| saved.allow_write),
                    "brightspace" => account
                        .brightspace
                        .as_ref()
                        .and_then(|saved| saved.allow_write),
                    _ => None,
                };
                (account.service.clone(), account.id.clone(), allow)
            })
            .collect();
        for (service, id, allow) in extra {
            self.promote_write_permission(&service, &id, allow);
        }
    }

    fn promote_write_permission(&mut self, service: &str, id: &str, allow: Option<bool>) {
        if allow == Some(true) {
            self.tools
                .allow_changes
                .entry(format!("{service}/{id}"))
                .or_insert(true);
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
        match serde_json::from_str::<Self>(&text) {
            Ok(mut settings) => {
                settings.migrate_permissions();
                settings
            }
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
        let _guard = SAVE_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        let _file_guard = lock_config()?;
        self.save_unlocked()
    }

    /// Apply a small change to the latest root state, under the same lock as saves.
    pub fn update(edit: impl FnOnce(&mut Self)) -> std::io::Result<Self> {
        let _guard = SAVE_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        let _file_guard = lock_config()?;
        let mut settings = Self::load();
        edit(&mut settings);
        settings.save_unlocked()?;
        Ok(settings)
    }

    /// Persist only this service/account; a login must not overwrite UI preferences
    /// or credentials saved by another worker while the network request was running.
    pub fn save_service(&self, service: &str) -> std::io::Result<()> {
        let mut scoped = self.clone();
        if scoped.account_scope.is_none() {
            scoped.account_scope = Some((service.into(), service.into()));
        }
        scoped.save()
    }

    fn save_unlocked(&self) -> std::io::Result<()> {
        if let Some(message) = &self.config_error {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                message.clone(),
            ));
        }
        if let Some((service, id)) = &self.account_scope {
            let mut root = Self::load();
            if let Some(message) = root.config_error {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    message,
                ));
            }
            if id == service {
                match service.as_str() {
                    "moodle" => root.moodle = self.moodle.clone(),
                    "omnivox" => root.omnivox = self.omnivox.clone(),
                    "brightspace" => root.brightspace = self.brightspace.clone(),
                    "outlook" => root.outlook = self.outlook.clone(),
                    "canvas" => root.canvas = self.canvas.clone(),
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
                    outlook: (service == "outlook")
                        .then(|| self.outlook.clone())
                        .flatten(),
                    canvas: (service == "canvas").then(|| self.canvas.clone()).flatten(),
                };
                if account.moodle.is_some()
                    || account.omnivox.is_some()
                    || account.brightspace.is_some()
                    || account.outlook.is_some()
                    || account.canvas.is_some()
                {
                    root.accounts.push(account);
                }
            }
            let key = format!("{service}/{id}");
            if let Some(value) = self.tools.allow_changes.get(&key) {
                root.tools.allow_changes.insert(key, *value);
            }
            return root.save_unlocked();
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
        if let Some(token) = self.gateway.token.clone().filter(|token| !token.is_empty()) {
            return Gateway { port, token };
        }
        let _guard = SAVE_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        let Ok(_file_guard) = lock_config() else {
            return Gateway {
                port,
                token: String::new(),
            };
        };
        let mut root = Self::load();
        root.account_scope = None;
        let port = root.gateway.port.unwrap_or(port);
        if root.config_error.is_some() {
            return Gateway {
                port,
                token: String::new(),
            };
        }
        if let Some(token) = root.gateway.token.clone().filter(|token| !token.is_empty()) {
            return Gateway { port, token };
        }
        let generated = random_hex(24);
        root.gateway.token = Some(generated.clone());
        if root.save_unlocked().is_ok() {
            return Gateway {
                port,
                token: generated,
            };
        }
        if let Some(token) = Self::load().gateway.token.filter(|token| !token.is_empty()) {
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
            "outlook" => self.outlook = None,
            "canvas" => self.canvas = None,
            _ => {}
        }
    }

    pub fn service_enabled(&self, service: &str) -> bool {
        !self
            .disabled_services
            .iter()
            .any(|disabled| disabled == service)
    }

    /// Persist service availability without touching any saved account credentials.
    pub fn set_service_enabled(service: &str, enabled: bool) -> Result<Self, String> {
        if !["moodle", "omnivox", "brightspace", "canvas"].contains(&service) {
            return Err("Unknown service".into());
        }
        Self::update(|settings| {
            if enabled {
                settings
                    .disabled_services
                    .retain(|disabled| disabled != service);
            } else if !settings
                .disabled_services
                .iter()
                .any(|disabled| disabled == service)
            {
                settings.disabled_services.push(service.to_string());
            }
        })
        .map_err(|error| error.to_string())
    }

    pub fn services_active(&self) -> Vec<String> {
        ["moodle", "omnivox", "brightspace", "canvas"]
            .into_iter()
            .filter(|id| self.service_enabled(id) && !self.account_list(id).is_empty())
            .map(str::to_string)
            .collect()
    }

    pub fn omnivox(&self) -> Option<OmnivoxConfig> {
        let saved = self.omnivox.as_ref()?;
        let token = saved.token.as_deref()?.trim();
        if token.is_empty() {
            return None;
        }
        let base_url = normalize_omnivox_url(saved.url.as_deref()?).ok()?;
        if !is_omnivox_host(&base_url) && token.chars().any(char::is_whitespace) {
            return None;
        }
        Some(OmnivoxConfig {
            base_url,
            token: token.to_string(),
            device_id: saved
                .device_id
                .as_deref()
                .map(str::trim)
                .filter(|id| valid_device_id(id))
                .map(str::to_string),
            auth_hash: saved
                .auth_hash
                .as_deref()
                .map(str::trim)
                .filter(|hash| !hash.is_empty() && hash.len() <= 256)
                .filter(|hash| hash.bytes().all(|byte| byte.is_ascii_graphic()))
                .map(str::to_string),
        })
    }

    pub fn brightspace(&self) -> Option<BrightspaceConfig> {
        let saved = self.brightspace.as_ref()?;
        let token = saved.token.as_deref()?.trim();
        if token.is_empty() {
            return None;
        }
        Some(BrightspaceConfig {
            base_url: normalize_brightspace_url(saved.url.as_deref()?).ok()?,
            token: token.to_string(),
            allow_write: saved.allow_write.unwrap_or(false),
        })
    }

    pub fn outlook(&self) -> Option<MailConfig> {
        let saved = self.outlook.as_ref()?;
        let token = saved.token.as_deref()?.trim();
        if token.is_empty() {
            return None;
        }
        Some(MailConfig {
            base_url: normalize_outlook_url(saved.url.as_deref()?).ok()?,
            token: token.to_string(),
            mailbox: saved.mailbox.clone(),
            forward_to: saved.forward_to.clone(),
            forwarding: saved.forwarding,
        })
    }

    pub fn canvas(&self) -> Option<CanvasConfig> {
        let saved = self.canvas.as_ref()?;
        let token = saved.token.as_deref()?.trim();
        if token.is_empty() {
            return None;
        }
        Some(CanvasConfig {
            base_url: normalize_canvas_url(saved.url.as_deref()?).ok()?,
            token: token.to_string(),
        })
    }
}

pub fn is_omnivox_host(url: &str) -> bool {
    Url::parse(url)
        .ok()
        .and_then(|parsed| parsed.host_str().map(str::to_ascii_lowercase))
        .is_some_and(|host| host == "omnivox.ca" || host.ends_with(".omnivox.ca"))
}

fn valid_device_id(id: &str) -> bool {
    id.len() == 40 && id.bytes().all(|byte| byte.is_ascii_hexdigit())
}

pub fn config_path() -> PathBuf {
    // Test override is process-global and mutex-guarded (see with_temp_config).
    // Checked before env vars so parallel tests never race on set_var/remove_var.
    #[cfg(test)]
    return tests::override_config_path()
        .expect("config access requires with_temp_config in tests");
    #[cfg(not(test))]
    {
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
    let localhost = parsed.host_str().is_some_and(is_loopback_host);
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
        return Ok(url.origin().ascii_serialization());
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

pub fn normalize_outlook_url(input: &str) -> Result<String, String> {
    let url = parse_site_url(input)?;
    let host = url.host_str().unwrap_or("").to_ascii_lowercase();
    let outlook = host == "outlook.office.com"
        || host == "outlook.cloud.microsoft"
        || host == "outlook.office365.com"
        || host == "outlook.live.com"
        || host.ends_with(".office.com")
        || host.ends_with(".office365.com");
    if outlook
        || parse_site_url(input).ok().is_some_and(|u| {
            u.host_str()
                .is_some_and(|h| h == "127.0.0.1" || h == "localhost" || h == "::1")
        })
    {
        return Ok(url.origin().ascii_serialization());
    }
    Err("Use https://outlook.office.com or your school Outlook address.".into())
}

pub fn normalize_canvas_url(input: &str) -> Result<String, String> {
    let url = parse_site_url(input)?;
    let host = url.host_str().unwrap_or("").to_ascii_lowercase();
    if host == "127.0.0.1" || host == "localhost" || host == "::1" {
        return Ok(url.origin().ascii_serialization());
    }
    match url.path().trim_end_matches('/') {
        "" | "/login" | "/login/canvas" | "/courses" => {}
        _ => return Err("Use your Canvas site address, e.g. https://zonecours2.hec.ca.".into()),
    }
    Ok(url.origin().ascii_serialization())
}

fn write_private(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let temp = path.with_extension(format!("json.tmp-{}", random_hex(8)));
    let result = (|| {
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        std::fs::rename(&temp, path)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    result
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
            settings.scale = Some(1.5);
            let _ = settings.save();
            let reloaded = Settings::load();
            assert_eq!(reloaded.scale, Some(1.5));
        });
    }

    #[test]
    fn legacy_permissions_are_removed_without_losing_credentials() {
        with_temp_config(|| {
            std::fs::write(config_path(), r#"{"moodle":{"url":"https://school.example","token":"fixture","allow_write":false},"brightspace":{"url":"https://school.example","token":"fixture2","allow_write":true}}"#).unwrap();
            let settings = Settings::load();
            assert_eq!(settings.moodle.as_ref().unwrap().allow_write, Some(false));
            settings.save().unwrap();
            let text = std::fs::read_to_string(config_path()).unwrap();
            assert!(!text.contains("allow_write"));
            assert_eq!(Settings::load().moodle().unwrap().token, "fixture");
            assert_eq!(Settings::load().brightspace().unwrap().token, "fixture2");
        });
    }

    #[test]
    fn legacy_allow_write_migrates_into_allow_changes_without_dropping_tokens() {
        with_temp_config(|| {
            std::fs::write(
                config_path(),
                r#"{"moodle":{"url":"https://school.example","token":"fixture","allow_write":true},"tools":{"advanced":false,"allow_changes":{"moodle/moodle":true}}}"#,
            )
            .unwrap();
            let settings = Settings::load();
            assert_eq!(settings.moodle().unwrap().token, "fixture");
            settings.save().unwrap();
            let text = std::fs::read_to_string(config_path()).unwrap();
            assert!(!text.contains("allow_write"));
            assert!(!text.contains("allow_changes"));
            assert!(!text.contains("\"tools\""));
            assert_eq!(Settings::load().moodle().unwrap().token, "fixture");
            assert!(crate::mcp::tool_definitions(&Settings::load())
                .iter()
                .any(|tool| tool["name"] == "ws_call"));
        });
    }

    #[test]
    fn accounts_without_an_explicit_write_permission_stay_read_only() {
        with_temp_config(|| {
            std::fs::write(
                config_path(),
                r#"{"moodle":{"url":"https://school.example","token":"fixture"}}"#,
            )
            .unwrap();
            let loaded = Settings::load();
            assert_eq!(loaded.moodle().unwrap().token, "fixture");
            assert!(crate::mcp::tool_definitions(&loaded)
                .iter()
                .any(|tool| tool["name"] == "ws_call"));
            loaded.save().unwrap();
            assert!(!std::fs::read_to_string(config_path())
                .unwrap()
                .contains("allow_changes"));
        });
    }

    // Invoked in a separate test process by the regression below.
    #[test]
    fn config_writer_process() {
        let Ok(path) = std::env::var("STUDENT_MCP_TEST_LOCK_CONFIG") else {
            return;
        };
        let index = std::env::var("STUDENT_MCP_TEST_LOCK_INDEX").unwrap();
        let path = PathBuf::from(path);
        *CONFIG_OVERRIDE.lock().unwrap() = Some(path.clone());
        std::fs::write(path.with_extension(format!("ready-{index}")), b"ready").unwrap();
        Settings::update(|settings| {
            settings.accounts.push(Account {
                id: index,
                service: "moodle".into(),
                moodle: Some(MoodleSettings {
                    url: Some("https://school.example".into()),
                    token: Some("fixture".into()),
                    ..Default::default()
                }),
                omnivox: None,
                brightspace: None,
                outlook: None,
                canvas: None,
            });
        })
        .unwrap();
    }

    #[test]
    fn independent_processes_wait_for_config_transaction_and_preserve_updates() {
        with_temp_config(|| {
            Settings::default().save().unwrap();
            let lock = lock_config().unwrap();
            let mut children: Vec<_> = (0..4)
                .map(|index| {
                    std::process::Command::new(std::env::current_exe().unwrap())
                        .args([
                            "--exact",
                            "config::tests::config_writer_process",
                            "--nocapture",
                        ])
                        .env("STUDENT_MCP_TEST_LOCK_CONFIG", config_path())
                        .env("STUDENT_MCP_TEST_LOCK_INDEX", index.to_string())
                        .stdout(std::process::Stdio::null())
                        .spawn()
                        .unwrap()
                })
                .collect();
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
            for index in 0..4 {
                while !config_path()
                    .with_extension(format!("ready-{index}"))
                    .exists()
                {
                    assert!(
                        std::time::Instant::now() < deadline,
                        "child startup timeout"
                    );
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
            let blocked = children
                .iter_mut()
                .all(|child| child.try_wait().unwrap().is_none());
            drop(lock);
            for mut child in children {
                assert!(child.wait().unwrap().success());
            }
            assert!(blocked, "a writer bypassed the inter-process lock");
            assert_eq!(Settings::load().accounts.len(), 4);
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
    fn service_can_be_toggled_without_removing_saved_credentials() {
        with_temp_config(|| {
            let mut settings = Settings::load();
            settings.omnivox = Some(OmnivoxSettings {
                url: Some("https://college.omnivox.ca".into()),
                token: Some("browser-profile:0123456789abcdef0123456789abcdef".into()),
                device_id: Some("a".repeat(40)),
                auth_hash: Some("private-auth-hash".into()),
            });
            settings.save().unwrap();

            let disabled = Settings::set_service_enabled("omnivox", false).unwrap();
            assert!(!disabled.service_enabled("omnivox"));
            assert!(disabled.services_active().is_empty());
            assert_eq!(
                disabled.omnivox.as_ref().unwrap().token.as_deref(),
                Some("browser-profile:0123456789abcdef0123456789abcdef")
            );
            assert!(disabled.omnivox().is_some());

            let reenabled = Settings::set_service_enabled("omnivox", true).unwrap();
            assert!(reenabled.service_enabled("omnivox"));
            assert_eq!(reenabled.services_active(), vec!["omnivox"]);
            assert_eq!(
                reenabled.omnivox.as_ref().unwrap().token,
                settings.omnivox.as_ref().unwrap().token
            );
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
    fn outlook_urls_accept_office_hosts_and_localhost() {
        assert_eq!(
            normalize_outlook_url("https://outlook.cloud.microsoft/mail/").unwrap(),
            "https://outlook.cloud.microsoft"
        );
        assert_eq!(
            normalize_outlook_url("outlook.office.com").unwrap(),
            "https://outlook.office.com"
        );
        assert_eq!(
            normalize_outlook_url("https://outlook.office365.com/mail/").unwrap(),
            "https://outlook.office365.com"
        );
        assert_eq!(
            normalize_outlook_url("http://127.0.0.1:1234/mail/").unwrap(),
            "http://127.0.0.1:1234"
        );
        assert!(normalize_outlook_url("https://evil.example").is_err());
    }

    #[test]
    fn canvas_urls_accept_the_site_root_and_zonecours() {
        assert_eq!(
            normalize_canvas_url("zonecours2.hec.ca").unwrap(),
            "https://zonecours2.hec.ca"
        );
        assert_eq!(
            normalize_canvas_url("https://school.instructure.com/login/canvas").unwrap(),
            "https://school.instructure.com"
        );
        assert!(normalize_canvas_url("https://evil.example/not-canvas").is_err());
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
    fn multiple_accounts_keep_credentials_and_gateway_separate() {
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
            assert_eq!(loaded.moodle.as_ref().unwrap().allow_write, None);
            let mut selected = loaded.for_account("moodle", "second-account");
            assert_eq!(selected.moodle().unwrap().token, "second-token");
            assert_eq!(selected.moodle.as_ref().unwrap().allow_write, None);
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
                    allow_write: Some(true),
                }),
                ..Default::default()
            };
            settings.save().unwrap();
            let mut loaded = Settings::load();
            assert_eq!(loaded.brightspace().unwrap().token, "private-token");
            assert_eq!(loaded.brightspace.as_ref().unwrap().allow_write, None);
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
        assert_eq!(
            normalize_omnivox_url("https://cegepmontpetit.omnivox.ca/Login/Account/Login").unwrap(),
            "https://cegepmontpetit.omnivox.ca"
        );
        for input in [
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
                    ..Default::default()
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
        with_temp_config(|| {
            let settings = Settings {
                omnivox: Some(OmnivoxSettings {
                    url: Some("https://cegepmontpetit.omnivox.ca".into()),
                    token: Some("SidLea=abc; Mobile=tok".into()),
                    ..Default::default()
                }),
                ..Default::default()
            };
            settings.save().unwrap();
            assert_eq!(
                Settings::load().omnivox().unwrap().token,
                "SidLea=abc; Mobile=tok"
            );
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
    fn concurrent_account_saves_preserve_preferences_and_other_accounts() {
        with_temp_config(|| {
            let stale = Settings::default();
            Settings::update(|settings| settings.scale = Some(1.7)).unwrap();
            std::thread::scope(|scope| {
                for index in 0..12 {
                    let mut account = stale.for_account("moodle", &format!("account-{index}"));
                    scope.spawn(move || {
                        account.moodle = Some(MoodleSettings {
                            url: Some("https://school.example".into()),
                            token: Some(format!("token-{index}")),
                            ..Default::default()
                        });
                        account.save_service("moodle").unwrap();
                    });
                }
            });
            let saved = Settings::load();
            assert_eq!(saved.scale, Some(1.7));
            assert_eq!(saved.account_list("moodle").len(), 12);
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
                    ..Default::default()
                }),
                brightspace: None,
                outlook: None,
                canvas: None,
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
                loaded
                    .for_account("omnivox", "shared")
                    .omnivox()
                    .unwrap()
                    .token,
                "ox-token"
            );
            assert_eq!(
                loaded
                    .for_account("moodle", "shared")
                    .moodle()
                    .unwrap()
                    .token,
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
