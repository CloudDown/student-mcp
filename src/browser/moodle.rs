//! Moodle mobile SSO wire format. MD5 is required by Moodle's protocol, not
//! used for password storage. Never persist the passport or private app token.
use base64::{engine::general_purpose::STANDARD, Engine};
use md5::{Digest, Md5};

pub(super) struct Sso {
    site: String,
    passport: String,
    pub token: Option<String>,
}

const INVALID: &str =
    "Moodle returned an invalid or unrelated sign-in response. Start browser sign-in again.";
pub(super) const UNAVAILABLE: &str = "Moodle browser sign-in did not finish. Check that your school allows the Moodle mobile app, or choose Manual sign-in to try your password.";

impl Sso {
    pub fn new(site: &str) -> Self {
        Self {
            site: site.into(),
            passport: crate::config::random_hex(32),
            token: None,
        }
    }

    pub fn launch_url(&self) -> String {
        let mut url =
            url::Url::parse(&format!("{}/admin/tool/mobile/launch.php", self.site)).unwrap();
        url.query_pairs_mut()
            .append_pair("service", "moodle_mobile_app")
            .append_pair("passport", &self.passport)
            .append_pair("urlscheme", "moodlemobile");
        url.into()
    }

    pub fn accept(&mut self, callback: &str) -> Result<(), String> {
        let encoded = callback
            .strip_prefix("moodlemobile://token=")
            .ok_or(INVALID)?;
        if self.token.is_some() || encoded.len() > 2048 {
            return Err(INVALID.into());
        }
        let decoded = STANDARD.decode(encoded).map_err(|_| INVALID)?;
        let text = std::str::from_utf8(&decoded).map_err(|_| INVALID)?;
        let fields: Vec<_> = text.split(":::").collect();
        let expected = format!(
            "{:x}",
            Md5::digest(format!("{}{}", self.site, self.passport))
        );
        if !(2..=3).contains(&fields.len())
            || !crate::config::token_matches(&expected, fields.first().copied())
            || fields[1].len() != 32
            || !fields[1].bytes().all(|b| b.is_ascii_hexdigit())
        {
            return Err(INVALID.into());
        }
        self.token = Some(fields[1].into());
        self.passport.clear();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn callback(sso: &Sso, private: bool) -> String {
        let hash = format!("{:x}", Md5::digest(format!("{}{}", sso.site, sso.passport)));
        format!(
            "moodlemobile://token={}",
            STANDARD.encode(format!(
                "{hash}:::{}{}",
                "a".repeat(32),
                if private { ":::private-secret" } else { "" }
            ))
        )
    }

    #[test]
    fn sso_launch_and_one_time_callback_for_target_sites_and_subdirectories() {
        for site in [
            "https://studium.umontreal.ca",
            "https://moodle.polymtl.ca",
            "https://school.example/moodle",
        ] {
            for private in [false, true] {
                let mut sso = Sso::new(site);
                let url = url::Url::parse(&sso.launch_url()).unwrap();
                assert_eq!(
                    url.path(),
                    format!(
                        "{}/admin/tool/mobile/launch.php",
                        url::Url::parse(site).unwrap().path().trim_end_matches('/')
                    )
                );
                assert!(url
                    .query_pairs()
                    .any(|(k, v)| k == "service" && v == "moodle_mobile_app"));
                let reply = callback(&sso, private);
                sso.accept(&reply).unwrap();
                assert_eq!(
                    sso.token.as_deref(),
                    Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
                );
                assert!(sso.passport.is_empty());
                assert!(sso.accept(&reply).is_err());
            }
        }
    }

    #[test]
    fn sso_rejects_foreign_passports_sites_and_malformed_callbacks_without_leaking_secrets() {
        let sso = Sso::new("https://school.example");
        let foreign = callback(&sso, true);
        for reply in [
            foreign,
            "moodlemobile://token=bad-private-secret".into(),
            format!(
                "moodlemobile://token={}",
                STANDARD.encode("bad:::private-secret")
            ),
            "https://school.example/?token=private-secret".into(),
            format!("moodlemobile://token={}", "a".repeat(4096)),
        ] {
            let mut target = Sso::new("https://other.example");
            let error = target.accept(&reply).unwrap_err();
            assert!(!error.contains("private-secret"));
            assert!(target.token.is_none());
        }
    }
}
