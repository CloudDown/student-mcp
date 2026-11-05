//! Outlook mailbox forwarding. Sign-in uses the same browser profile as other
//! school sites. Forwarding keeps a copy in Outlook. Error 5.7.520 is a school
//! policy block, never reported as success.
use crate::config::MailConfig;
use serde_json::{json, Value};
use std::time::Duration;

pub const BLOCKED: &str = "Your school blocks automatic forwarding.";

const RULE_NAME: &str = "student-mcp-forward";
const TEST_SUBJECT: &str = "student-mcp mail transfer test";

pub fn forwarding_blocked(status: u16, body: &str) -> bool {
    let lower = body.to_ascii_lowercase();
    body.contains("5.7.520")
        || lower.contains("5.7.520")
        || lower.contains("remote domain")
        || (status >= 400 && lower.contains("forwarding") && lower.contains("denied"))
}

pub fn normalize_forward_to(input: &str) -> Result<String, String> {
    let address = input.trim().to_ascii_lowercase();
    let (user, host) = address
        .split_once('@')
        .ok_or("Enter the address that should receive forwarded mail, e.g. you@gmail.com.")?;
    if user.is_empty()
        || host.len() < 3
        || !host.contains('.')
        || address.len() > 254
        || !address.bytes().all(|b| b.is_ascii_graphic())
    {
        return Err("Enter a valid email address.".into());
    }
    Ok(address)
}

pub struct MailClient {
    config: MailConfig,
    agent: ureq::Agent,
    browser: crate::browser::Connection,
}

impl MailClient {
    pub fn new(config: MailConfig) -> Self {
        Self {
            browser: crate::browser::Connection::new("outlook", &config.base_url, &config.token),
            agent: ureq::AgentBuilder::new()
                .timeout(Duration::from_secs(30))
                .redirects(0)
                .user_agent(crate::session::BROWSER_UA)
                .build(),
            config,
        }
    }

    fn request(
        &self,
        method: &str,
        path: &str,
        body: Option<&Value>,
    ) -> Result<(u16, String), String> {
        if self.browser.enabled() {
            let bytes = body.map(|value| value.to_string());
            let reply = self.browser.request(
                method,
                path,
                bytes.as_ref().map(|s| s.as_bytes()),
                body.map(|_| "application/json"),
                false,
                2 * 1024 * 1024,
            )?;
            return Ok((
                reply.status,
                String::from_utf8_lossy(&reply.bytes).into_owned(),
            ));
        }
        let url = format!("{}{path}", self.config.base_url);
        let mut request = match method {
            "GET" => self.agent.get(&url),
            "POST" => self.agent.post(&url),
            "PATCH" => self.agent.request("PATCH", &url),
            "DELETE" => self.agent.request("DELETE", &url),
            _ => return Err("Unsupported mail request.".into()),
        };
        request = request
            .set("Accept", "application/json")
            .set("Cookie", &self.config.token);
        let result = if let Some(body) = body {
            request
                .set("Content-Type", "application/json")
                .send_string(&body.to_string())
        } else {
            request.call()
        };
        match result {
            Ok(response) | Err(ureq::Error::Status(_, response)) => {
                let status = response.status();
                let text = response.into_string().unwrap_or_default();
                Ok((status, text))
            }
            Err(_) => Err("Cannot reach Outlook. Check your Internet connection.".into()),
        }
    }

    fn json(&self, method: &str, path: &str, body: Option<&Value>) -> Result<Value, String> {
        let (status, text) = self.request(method, path, body)?;
        if forwarding_blocked(status, &text) {
            return Err(BLOCKED.into());
        }
        if !(200..300).contains(&status) {
            return Err(format!("Outlook returned HTTP {status}."));
        }
        if text.trim().is_empty() {
            return Ok(json!({}));
        }
        serde_json::from_str(&text).map_err(|_| "Outlook did not return JSON.".to_string())
    }

    pub fn me(&self) -> Result<Value, String> {
        let user = self.json("GET", "/api/v2.0/me", None)?;
        if user.get("EmailAddress").and_then(Value::as_str).is_none()
            && user.get("Id").and_then(Value::as_str).is_none()
        {
            return Err("Outlook did not return the signed-in mailbox.".into());
        }
        Ok(json!({
            "email": user.get("EmailAddress"),
            "id": user.get("Id"),
            "displayName": user.get("DisplayName")
        }))
    }

    fn rules(&self) -> Result<Vec<Value>, String> {
        let value = self.json("GET", "/api/v2.0/me/MailFolders/Inbox/MessageRules", None)?;
        Ok(value
            .get("value")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default())
    }

    fn delete_rule(&self, id: &str) -> Result<(), String> {
        if id.is_empty()
            || id.len() > 128
            || !id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        {
            return Err("Invalid Outlook rule.".into());
        }
        let (status, text) = self.request(
            "DELETE",
            &format!("/api/v2.0/me/MailFolders/Inbox/MessageRules/{id}"),
            None,
        )?;
        if forwarding_blocked(status, &text) {
            return Err(BLOCKED.into());
        }
        if !(200..300).contains(&status) && status != 404 {
            return Err(format!("Outlook returned HTTP {status}."));
        }
        Ok(())
    }

    pub fn disable(&self) -> Result<(), String> {
        for rule in self.rules()? {
            if rule["DisplayName"] == RULE_NAME {
                if let Some(id) = rule.get("Id").and_then(Value::as_str) {
                    self.delete_rule(id)?;
                }
            }
        }
        Ok(())
    }

    pub fn enable(&self, forward_to: &str) -> Result<String, String> {
        let forward_to = normalize_forward_to(forward_to)?;
        self.disable()?;
        let created = self.json(
            "POST",
            "/api/v2.0/me/MailFolders/Inbox/MessageRules",
            Some(&json!({
                "DisplayName": RULE_NAME,
                "Sequence": 1,
                "IsEnabled": true,
                "Actions": {
                    "ForwardTo": [{"EmailAddress": {"Address": forward_to}}],
                    "StopProcessingRules": false
                }
            })),
        )?;
        if created.get("Id").and_then(Value::as_str).is_none() && created.get("error").is_some() {
            let text = created.to_string();
            if forwarding_blocked(400, &text) {
                return Err(BLOCKED.into());
            }
            return Err("Outlook did not enable forwarding.".into());
        }
        Ok(forward_to)
    }

    pub fn send_test(&self, mailbox: &str) -> Result<String, String> {
        let subject = format!("{TEST_SUBJECT} {}", crate::config::random_hex(4));
        let (status, text) = self.request(
            "POST",
            "/api/v2.0/me/sendmail",
            Some(&json!({
                "Message": {
                    "Subject": subject,
                    "Body": {"ContentType": "Text", "Content": "student-mcp forwarding test. Keep this copy in Outlook."},
                    "ToRecipients": [{"EmailAddress": {"Address": mailbox}}]
                },
                "SaveToSentItems": true
            })),
        )?;
        if forwarding_blocked(status, &text) {
            return Err(BLOCKED.into());
        }
        if !(200..300).contains(&status) {
            return Err(format!("Outlook returned HTTP {status}."));
        }
        Ok(subject)
    }

    pub fn verify_test(&self, subject: &str) -> Result<(), String> {
        let (status, text) = self.request(
            "GET",
            "/api/v2.0/me/messages?$top=20&$select=Subject,BodyPreview",
            None,
        )?;
        if forwarding_blocked(status, &text) {
            return Err(BLOCKED.into());
        }
        if text.contains("5.7.520") {
            return Err(BLOCKED.into());
        }
        if !(200..300).contains(&status) {
            return Err(format!("Outlook returned HTTP {status}."));
        }
        if !text.contains(TEST_SUBJECT) && !text.contains(subject) {
            return Err("The test message was not found in Outlook. Check forwarding in Outlook, then try again.".into());
        }
        Ok(())
    }
}

pub fn activate(
    settings: &mut crate::config::Settings,
    url: &str,
    token: &str,
) -> Result<String, String> {
    let base_url = crate::config::normalize_outlook_url(url)?;
    let client = MailClient::new(crate::config::MailConfig {
        base_url: base_url.clone(),
        token: token.into(),
        mailbox: None,
        forward_to: None,
        forwarding: false,
    });
    let me = client.me()?;
    let mailbox = me.get("email").and_then(Value::as_str).map(str::to_string);
    let mut updated = settings.clone();
    updated.outlook = Some(crate::config::MailSettings {
        url: Some(base_url),
        token: Some(token.into()),
        mailbox: mailbox.clone(),
        forward_to: None,
        forwarding: false,
    });
    updated.save_service("outlook").map_err(|error| {
        format!("Cannot save Outlook credentials to the private config file: {error}")
    })?;
    *settings = updated;
    Ok(format!(
        "Connected to Outlook as {}.",
        mailbox.as_deref().unwrap_or("mailbox")
    ))
}

pub fn enable(settings: &mut crate::config::Settings, forward_to: &str) -> Result<String, String> {
    let config = settings
        .outlook()
        .ok_or("Connect Outlook before enabling forwarding.")?;
    if config.base_url == "https://outlook.cloud.microsoft" {
        return Err(
            "Reconnect Outlook with both addresses to configure forwarding in the sign-in browser."
                .into(),
        );
    }
    let client = MailClient::new(config.clone());
    let me = client.me()?;
    let mailbox = me
        .get("email")
        .and_then(Value::as_str)
        .ok_or("Outlook did not return the signed-in mailbox.")?;
    let forward_to = client.enable(forward_to)?;
    let subject = client.send_test(mailbox)?;
    if let Err(error) = client.verify_test(&subject) {
        let _ = client.disable();
        return Err(error);
    }
    let mut updated = settings.clone();
    if let Some(saved) = updated.outlook.as_mut() {
        if saved.mailbox.as_deref() != Some(mailbox) {
            saved.mailbox = Some(mailbox.into());
        }
        saved.forward_to = Some(forward_to.clone());
        saved.forwarding = true;
    }
    updated
        .save_service("outlook")
        .map_err(|error| format!("Cannot save forwarding settings: {error}"))?;
    *settings = updated;
    Ok(format!(
        "Forwarding to {forward_to}. A copy stays in Outlook."
    ))
}

pub fn disable(settings: &mut crate::config::Settings) -> Result<String, String> {
    let config = settings.outlook().ok_or("Outlook is not connected.")?;
    if config.base_url == "https://outlook.cloud.microsoft" {
        return Err("Outlook forwarding cannot be switched off here yet. Use Outlook Settings → Mail → Forwarding.".into());
    }
    MailClient::new(config).disable()?;
    let mut updated = settings.clone();
    if let Some(saved) = updated.outlook.as_mut() {
        saved.forwarding = false;
    }
    updated
        .save_service("outlook")
        .map_err(|error| format!("Cannot save forwarding settings: {error}"))?;
    *settings = updated;
    Ok("Forwarding turned off.".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tiny_http::{Header, Response, Server};

    fn server(replies: Vec<(u16, String)>) -> (MailConfig, std::thread::JoinHandle<()>) {
        let http = Server::http("127.0.0.1:0").unwrap();
        let origin = format!("http://{}", http.server_addr());
        let worker = std::thread::spawn(move || {
            for (status, body) in replies {
                let request = http.recv().unwrap();
                let mut response = Response::from_string(body).with_status_code(status);
                response
                    .add_header(Header::from_bytes("Content-Type", "application/json").unwrap());
                let _ = request.respond(response);
            }
        });
        (
            MailConfig {
                base_url: origin,
                token: "session=fixture".into(),
                mailbox: None,
                forward_to: None,
                forwarding: false,
            },
            worker,
        )
    }

    #[test]
    fn school_block_5_7_520_is_never_a_success() {
        assert!(forwarding_blocked(
            400,
            r#"{"error":{"code":"ErrorSendAsDenied","message":"5.7.520 Access denied"}}"#
        ));
        assert!(!forwarding_blocked(200, r#"{"Id":"rule"}"#));
        assert_eq!(
            normalize_forward_to(" You@Gmail.COM ").unwrap(),
            "you@gmail.com"
        );
        assert!(normalize_forward_to("not-mail").is_err());
    }

    #[test]
    fn enable_sends_a_test_and_keeps_a_copy_then_disable_removes_the_rule() {
        let (config, worker) = server(vec![
            (200, r#"{"value":[]}"#.into()),
            (
                201,
                r#"{"Id":"rule-1","DisplayName":"student-mcp-forward"}"#.into(),
            ),
            (202, String::new()),
            (
                200,
                format!(
                    r#"{{"value":[{{"Subject":"{TEST_SUBJECT}","BodyPreview":"Keep this copy in Outlook."}}]}}"#
                ),
            ),
            (
                200,
                r#"{"value":[{"Id":"rule-1","DisplayName":"student-mcp-forward"}]}"#.into(),
            ),
            (204, String::new()),
        ]);
        let client = MailClient::new(config);
        assert_eq!(client.enable("you@gmail.com").unwrap(), "you@gmail.com");
        client.send_test("me@polymtl.ca").unwrap();
        client.verify_test(TEST_SUBJECT).unwrap();
        client.disable().unwrap();
        worker.join().unwrap();
    }

    #[test]
    fn a_delivery_report_with_5_7_520_is_the_school_block() {
        let (config, worker) = server(vec![(
            200,
            r#"{"value":[{"Subject":"Undeliverable","BodyPreview":"5.7.520 Access denied"}]}"#
                .into(),
        )]);
        let error = MailClient::new(config)
            .verify_test(TEST_SUBJECT)
            .unwrap_err();
        assert_eq!(error, BLOCKED);
        worker.join().unwrap();
    }

    #[test]
    fn enable_reports_the_school_block_instead_of_success() {
        let (config, worker) = server(vec![
            (200, r#"{"value":[]}"#.into()),
            (
                403,
                r#"{"error":{"message":"5.7.520 Access denied, Your organization does not allow external forwarding"}}"#.into(),
            ),
        ]);
        let error = MailClient::new(config).enable("you@gmail.com").unwrap_err();
        assert_eq!(error, BLOCKED);
        worker.join().unwrap();
    }
}
