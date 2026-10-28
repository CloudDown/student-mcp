//! Shared helpers for school-site sessions: cookie auth, HTML forms, redirects.
use std::collections::BTreeMap;
use url::Url;

pub const BROWSER_UA: &str = "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/128.0.0.0 Safari/537.36";

pub fn form(pairs: &[(&str, &str)]) -> String {
    let mut serializer = url::form_urlencoded::Serializer::new(String::new());
    for (key, value) in pairs {
        serializer.append_pair(key, value);
    }
    serializer.finish()
}

pub fn parse_cookie_header(header: &str) -> BTreeMap<String, String> {
    let mut jar = BTreeMap::new();
    for part in header.split(';') {
        let part = part.trim();
        if let Some((name, value)) = part.split_once('=') {
            let name = name.trim();
            if !name.is_empty() {
                jar.insert(name.to_string(), value.trim().to_string());
            }
        }
    }
    jar
}

pub fn csrf_from_cookies(header: &str) -> Option<String> {
    parse_cookie_header(header)
        .remove("d2lSessionVal")
        .filter(|value| !value.is_empty())
}

pub fn session_cookie_auth(token: &str) -> bool {
    let token = token.trim();
    token.contains("d2lSessionVal=") || token.contains(';')
}

pub fn html_form_action(html: &str) -> Option<String> {
    let lower = html.to_ascii_lowercase();
    let start = lower.find("<form")?;
    let rest = &html[start..];
    let end = rest.find('>')?;
    html_attr(&rest[..=end], "action").map(|action| {
        // Attribute values are HTML, not URL, encoded in the source.
        action
            .replace("&amp;", "&")
            .replace("&#38;", "&")
            .replace("&#x26;", "&")
    })
}

pub fn html_form_inputs(html: &str) -> Vec<(String, String)> {
    let lower = html.to_ascii_lowercase();
    let mut inputs = Vec::new();
    let mut from = 0;
    while let Some(rel) = lower[from..].find("<input") {
        let start = from + rel;
        let Some(end) = html[start..].find('>') else {
            break;
        };
        let tag = &html[start..=start + end];
        if let Some(name) = html_attr(tag, "name") {
            inputs.push((name, html_attr(tag, "value").unwrap_or_default()));
        }
        from = start + end + 1;
    }
    inputs
}

pub fn html_has_password_field(html: &str) -> bool {
    html_form_inputs(html).iter().any(|(name, _)| {
        name.eq_ignore_ascii_case("password") || name.to_ascii_lowercase().contains("password")
    })
}

pub(crate) fn html_attr(tag: &str, key: &str) -> Option<String> {
    let lower = tag.to_ascii_lowercase();
    let needle = format!("{key}=");
    let pos = lower.find(&needle)?;
    let rest = tag[pos + needle.len()..].trim_start();
    if let Some(quote) = rest.chars().next().filter(|ch| *ch == '"' || *ch == '\'') {
        let inner = rest.get(1..)?;
        let end = inner.find(quote)?;
        return Some(inner[..end].to_string());
    }
    let end = rest
        .find(|ch: char| ch.is_ascii_whitespace() || ch == '>')
        .unwrap_or(rest.len());
    Some(rest[..end].to_string())
}

pub fn resolve_url(base: &str, location: &str) -> Result<String, String> {
    Url::parse(base)
        .map_err(|_| "The site address is not a valid URL.".to_string())?
        .join(location)
        .map(|url| url.into())
        .map_err(|_| "The site returned an invalid redirect.".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use cookie_store::CookieStore;

    fn cookie_header(jar: &CookieStore, target: &str) -> Result<String, String> {
        let url = Url::parse(target).map_err(|_| "Invalid cookie address.")?;
        Ok(jar
            .get_request_values(&url)
            .map(|(name, value)| format!("{name}={value}"))
            .collect::<Vec<_>>()
            .join("; "))
    }

    fn same_host(left: &str, right: &str) -> bool {
        match (Url::parse(left), Url::parse(right)) {
            (Ok(left), Ok(right)) => {
                left.origin() == right.origin()
                    && right.username().is_empty()
                    && right.password().is_none()
            }
            _ => false,
        }
    }

    fn is_sso_host(url: &str) -> bool {
        let Ok(parsed) = Url::parse(url) else {
            return false;
        };
        let host = parsed.host_str().unwrap_or("").to_ascii_lowercase();
        host.contains("microsoftonline.")
            || host.contains("login.microsoft.")
            || host.contains("windows.net")
            || host.contains("okta.com")
            || host.contains("onelogin.com")
            || host.contains("pingidentity.")
            || host.contains("auth0.com")
            || host.contains("accounts.google.")
            || host.contains("duosecurity.com")
    }

    fn fill_login_form(
        html: &str,
        username: &str,
        password: &str,
        username_names: &[&str],
    ) -> Result<(String, String), String> {
        let action = html_form_action(html).unwrap_or_default();
        let mut pairs: Vec<(String, String)> = html_form_inputs(html);
        if pairs.is_empty() {
            return Err("The login page did not include a form.".into());
        }
        let mut user_set = false;
        let mut password_set = false;
        for (name, value) in &mut pairs {
            let lower = name.to_ascii_lowercase();
            if username_names
                .iter()
                .any(|candidate| lower == candidate.to_ascii_lowercase())
                || lower.contains("identifiant")
                || lower == "noda"
                || lower == "username"
                || lower == "userid"
                || lower == "loginid"
                || lower == "login"
            {
                *value = username.to_string();
                user_set = true;
            }
            if lower.contains("password") {
                *value = password.to_string();
                password_set = true;
            }
        }
        if !user_set || !password_set {
            return Err(
                "This school uses a login page this app cannot fill (often single sign-on). Paste a session cookie or access token instead."
                    .into(),
            );
        }
        let encoded = form(
            &pairs
                .iter()
                .map(|(name, value)| (name.as_str(), value.as_str()))
                .collect::<Vec<_>>(),
        );
        Ok((action, encoded))
    }

    #[test]
    fn parses_forms_cookies_and_sso_hosts() {
        let html = r#"<form action="/intr/Login.aspx" method="post">
            <input id="k" name="k" type="hidden" value="123" />
            <input name="Identifiant" value="" />
            <input name="Password" type="password" />
            </form>"#;
        assert_eq!(html_form_action(html).as_deref(), Some("/intr/Login.aspx"));
        assert!(html_has_password_field(html));
        let (action, body) = fill_login_form(html, "1111111", "secret", &["Identifiant"]).unwrap();
        assert_eq!(action, "/intr/Login.aspx");
        assert!(body.contains("Identifiant=1111111"));
        assert!(body.contains("Password=secret"));
        assert!(body.contains("k=123"));
        let mut jar = CookieStore::default();
        let url = Url::parse("https://mycourses.mcgill.ca/d2l/login").unwrap();
        jar.parse("d2lSessionVal=abc; Path=/", &url).unwrap();
        jar.parse("d2lSecureSessionVal=xyz; Path=/", &url).unwrap();
        let header = cookie_header(&jar, url.as_str()).unwrap();
        assert!(session_cookie_auth(&header));
        assert_eq!(csrf_from_cookies(&header).as_deref(), Some("abc"));
        jar.parse("d2lSessionVal=deleted; Path=/; Max-Age=0", &url)
            .unwrap();
        assert!(csrf_from_cookies(&cookie_header(&jar, url.as_str()).unwrap()).is_none());
        assert!(!same_host(
            url.as_str(),
            "https://mycourses.mcgill.ca:8443/d2l/home"
        ));
        assert!(!same_host(
            url.as_str(),
            "https://user@mycourses.mcgill.ca/d2l/home"
        ));
        assert!(is_sso_host("https://login.microsoftonline.com/common"));
        assert!(!is_sso_host("https://mycourses.mcgill.ca/d2l/login"));
        assert!(same_host(
            "https://mycourses.mcgill.ca/d2l/login",
            "https://mycourses.mcgill.ca/d2l/home"
        ));
        assert!(!same_host(
            "https://mycourses.mcgill.ca/d2l/login",
            "https://login.microsoftonline.com/"
        ));
    }
}
