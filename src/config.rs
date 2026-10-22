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
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct MoodleSettings {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allow_write: Option<bool>,
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
    pub fn load() -> Self {
        let text = std::fs::read_to_string(config_path()).unwrap_or_default();
        serde_json::from_str(&text).unwrap_or_default()
    }

    pub fn save(&self) -> std::io::Result<()> {
        let path = config_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let text = serde_json::to_string_pretty(self).unwrap_or_else(|_| "{}".to_string()) + "\n";
        write_private(&path, text.as_bytes())
    }

    /// Gateway settings; generates and persists the public token on first run.
    pub fn gateway(&self) -> Gateway {
        let token = self.gateway.token.clone().unwrap_or_else(|| {
            let generated = random_hex(24);
            let mut saved = self.clone();
            saved.gateway.token = Some(generated.clone());
            let _ = saved.save();
            generated
        });
        Gateway {
            port: self.gateway.port.unwrap_or(DEFAULT_PORT),
            token,
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
            .filter(|id| match *id {
                "moodle" => self.moodle().is_some(),
                "omnivox" => self.omnivox().is_some(),
                "brightspace" => self.brightspace().is_some(),
                _ => false,
            })
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
    getrandom(&mut buffer).ok();
    buffer.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// Public MCP endpoint for a given origin: the token in the path is the secret.
pub fn mcp_endpoint(origin: &str, token: &str) -> String {
    format!("{}/mcp/{}", origin.trim_end_matches('/'), token)
}

/// Constant-time comparison so request timing leaks nothing.
pub fn token_matches(expected: &str, provided: Option<&str>) -> bool {
    let Some(provided) = provided else {
        return false;
    };
    if provided.is_empty() || expected.is_empty() {
        return false;
    }
    let a = expected.as_bytes();
    let b = provided.as_bytes();
    a.len() == b.len()
        && a.iter()
            .zip(b.iter())
            .fold(0u8, |acc, (x, y)| acc | (x ^ y))
            == 0
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
    let localhost = matches!(
        parsed.host_str(),
        Some("localhost") | Some("127.0.0.1") | Some("[::1]")
    );
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
    if path.is_empty() {
        form.push_str(&path);
    }
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

    pub(crate) fn with_temp_config<T>(test: impl FnOnce() -> T) -> T {
        // The config path is process-global: serialize env-touching tests.
        static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let _guard = ENV_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let dir = std::env::temp_dir().join(format!(
            "student-mcp-{}-{}",
            std::process::id(),
            random_hex(4)
        ));
        std::fs::create_dir_all(&dir).expect("temp dir");
        std::env::set_var("STUDENT_MCP_CONFIG_FILE", dir.join("settings.json"));
        let result = test();
        std::env::remove_var("STUDENT_MCP_CONFIG_FILE");
        std::fs::remove_dir_all(dir).ok();
        result
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
}
