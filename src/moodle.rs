use crate::config::MoodleConfig;
use serde_json::{json, Value};
use std::time::Duration;

const MOBILE_SERVICE: &str = "moodle_mobile_app";
const TIMEOUT: Duration = Duration::from_secs(30);
const SENSITIVE_KEYS: [&str; 6] = ["wstoken", "token", "userprivateaccesskey", "privateaccesskey", "accesskey", "password"];

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new().timeout(TIMEOUT).build()
}

/// Encode key/value pairs as application/x-www-form-urlencoded (Moodle expects that).
fn urlencoded(pairs: &[(String, String)]) -> String {
    let mut serializer = url::form_urlencoded::Serializer::new(String::new());
    for (key, value) in pairs {
        serializer.append_pair(key, value);
    }
    serializer.finish()
}

/// POST the standard mobile-app token request. The password is never stored.
pub fn request_mobile_token(site: &str, username: &str, password: &str) -> Result<String, String> {
    let base = crate::config::normalize_moodle_url(site)?;
    if username.trim().is_empty() || password.is_empty() {
        return Err("Entrez votre identifiant et votre mot de passe Moodle.".into());
    }
    let agent = agent();
    let body = urlencoded(&[
        ("username".into(), username.trim().to_string()),
        ("password".into(), password.to_string()),
        ("service".into(), MOBILE_SERVICE.to_string()),
    ]);
    let response = agent
        .post(&format!("{base}/login/token.php"))
        .set("Content-Type", "application/x-www-form-urlencoded")
        .send_string(&body)
        .map_err(|_| "Connexion à Moodle impossible. Vérifiez l'adresse du site et votre connexion Internet.".to_string())?;
    let result: Value = response.into_json().map_err(|_| "Moodle n'a pas renvoyé de token.".to_string())?;
    if let Some(token) = result.get("token").and_then(Value::as_str).filter(|t| !t.trim().is_empty()) {
        return Ok(token.trim().to_string());
    }
    if result.get("errorcode").and_then(Value::as_str) == Some("invalidlogin") {
        return Err("Moodle refuse la connexion. Vérifiez vos identifiants; avec SSO ou double authentification, demandez un token au soutien Moodle de votre école.".into());
    }
    Err("Moodle n'a pas fourni de token mobile. Vérifiez l'accès au service mobile avec votre école, ou choisissez l'option token existant.".into())
}

/// Blocking Moodle Web Services client (one instance per request is fine).
pub struct MoodleClient {
    base_url: String,
    token: String,
    agent: ureq::Agent,
}

impl MoodleClient {
    pub fn new(config: &MoodleConfig) -> Self {
        MoodleClient {
            base_url: config.base_url.clone(),
            token: config.token.clone(),
            agent: agent(),
        }
    }

    /// One Web Service call; arrays become courseids[0]= form entries.
    pub fn call(&self, function: &str, args: &Value) -> Result<Value, String> {
        let mut form: Vec<(String, String)> = vec![
            ("wstoken".into(), self.token.clone()),
            ("wsfunction".into(), function.to_string()),
            ("moodlewsrestformat".into(), "json".into()),
        ];
        append_params(args, &mut form, "");
        let response = self
            .agent
            .post(&format!("{}/webservice/rest/server.php", self.base_url))
            .set("Content-Type", "application/x-www-form-urlencoded")
            .send_string(&urlencoded(&form))
            .map_err(error_for_transport)?;
        let value: Value = response.into_json().map_err(|_| "Moodle a renvoyé une réponse non JSON.".to_string())?;
        Ok(scrub(value, &self.token))
    }

    pub fn site_info(&self) -> Result<Value, String> {
        let info = self.call("core_webservice_get_site_info", &json!({}))?;
        if info.get("exception").is_some() || info.get("userid").and_then(Value::as_i64).is_none() {
            return Err("Ce token ne peut pas accéder à ce site Moodle. Vérifiez ses permissions et l'adresse du site.".into());
        }
        Ok(info)
    }
}

fn error_for_transport(error: ureq::Error) -> String {
    match error {
        ureq::Error::Status(code, _) => format!("Moodle a renvoyé une erreur HTTP {code}."),
        _ => "Connexion à Moodle impossible. Vérifiez l'adresse du site et votre connexion Internet.".into(),
    }
}

/// Moodle REST expects form fields such as courseids[0] and options[name].
pub fn append_params(value: &Value, form: &mut Vec<(String, String)>, key: &str) {
    match value {
        Value::Null => {}
        Value::Array(items) => {
            for (index, item) in items.iter().enumerate() {
                append_params(item, form, &format!("{key}[{index}]"));
            }
        }
        Value::Object(map) => {
            for (child, item) in map {
                let child_key = if key.is_empty() { child.clone() } else { format!("{key}[{child}]") };
                append_params(item, form, &child_key);
            }
        }
        other => {
            let text = match other {
                Value::Bool(true) => "1".to_string(),
                Value::Bool(false) => "0".to_string(),
                Value::Number(number) => number.to_string(),
                Value::String(text) => text.clone(),
                _ => return,
            };
            let key = if key.is_empty() { "unnamed".to_string() } else { key.to_string() };
            form.push((key, text));
        }
    }
}

/// Remove credential-like fields and literal token occurrences from results.
pub fn scrub(value: Value, token: &str) -> Value {
    match value {
        Value::String(text) => Value::String(text.replace(token, "[redacted]")),
        Value::Array(items) => Value::Array(items.into_iter().map(|item| scrub(item, token)).collect()),
        Value::Object(map) => Value::Object(
            map.into_iter()
                .map(|(key, item)| {
                    let cleaned = if SENSITIVE_KEYS.iter().any(|sensitive| key.eq_ignore_ascii_case(sensitive)) {
                        Value::String("[redacted]".into())
                    } else {
                        scrub(item, token)
                    };
                    (key, cleaned)
                })
                .collect(),
        ),
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn params_flatten_like_moodle_expects() {
        let mut form = Vec::new();
        append_params(&json!({ "courseid": 42, "courseids": [7, 8] }), &mut form, "");
        assert!(form.contains(&("courseid".to_string(), "42".to_string())));
        assert!(form.contains(&("courseids[0]".to_string(), "7".to_string())));
        assert!(form.contains(&("courseids[1]".to_string(), "8".to_string())));
    }

    #[test]
    fn scrub_hides_tokens_and_passwords() {
        let value = scrub(json!({ "wstoken": "SECRET", "other": "contains SECRET here", "ok": 1 }), "SECRET");
        assert_eq!(value["wstoken"], json!("[redacted]"));
        assert_eq!(value["other"], json!("contains [redacted] here"));
        assert_eq!(value["ok"], json!(1));
    }
}
