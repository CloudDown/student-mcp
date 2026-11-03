//! Secret removal shared by all school connectors and document delivery.
use serde_json::Value;

const SENSITIVE_KEYS: [&str; 6] = [
    "wstoken",
    "token",
    "userprivateaccesskey",
    "privateaccesskey",
    "accesskey",
    "password",
];

pub(crate) fn scrub_secret(text: String, token: &str) -> String {
    if token.is_empty() {
        text
    } else {
        text.replace(token, "[redacted]")
    }
}

pub(crate) fn scrub(value: Value, token: &str) -> Value {
    match value {
        Value::String(text) => Value::String(scrub_secret(text, token)),
        Value::Array(items) => {
            Value::Array(items.into_iter().map(|item| scrub(item, token)).collect())
        }
        Value::Object(map) => Value::Object(
            map.into_iter()
                .map(|(key, item)| {
                    let cleaned = if SENSITIVE_KEYS
                        .iter()
                        .any(|sensitive| key.eq_ignore_ascii_case(sensitive))
                    {
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
