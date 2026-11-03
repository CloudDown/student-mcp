//! Shared response checks, not an authorization gate. Each API retains its own
//! success contract (e.g. Brightspace empty 2xx, Omnivox explicit confirmation).
use serde_json::Value;

pub(crate) fn explicit_failure(value: &Value) -> bool {
    value == &Value::Bool(false)
        || ["Error", "error", "Errors", "errors"].iter().any(|key| {
            value.get(key).is_some_and(|v| {
                !matches!(v, Value::Null | Value::Bool(false))
                    && v.as_str() != Some("")
                    && v.as_array().is_none_or(|a| !a.is_empty())
            })
        })
        || value.get("HasError").and_then(Value::as_bool) == Some(true)
        || ["success", "Success"]
            .iter()
            .any(|key| value.get(key).and_then(Value::as_bool) == Some(false))
}

/// Brightspace permits an empty successful HTTP response. Nonempty bodies must
/// be JSON and must not explicitly reject the action. Never turn HTML into success.
pub(crate) fn brightspace(body: &[u8]) -> Result<Option<Value>, String> {
    if body.iter().all(u8::is_ascii_whitespace) {
        return Ok(None);
    }
    let value: Value = serde_json::from_slice(body).map_err(|_| {
        "Action returned a non-JSON response; success is not confirmed. Verify state before retrying."
    })?;
    if value.is_null() || explicit_failure(&value) {
        return Err("Action returned an error or ambiguous response; success is not confirmed. Verify state before retrying.".into());
    }
    Ok(Some(value))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn success_contract_distinguishes_empty_http_from_error_and_invalid_body() {
        assert_eq!(brightspace(b" \r\n").unwrap(), None);
        assert_eq!(brightspace(br#"{"Id":42}"#).unwrap().unwrap()["Id"], 42);
        for body in [
            b"<html>Sign in</html>".as_slice(),
            b"{",
            b"null",
            b"false",
            br#"{"Errors":[{"Message":"denied"}]}"#,
            br#"{"success":false}"#,
        ] {
            assert!(brightspace(body).is_err(), "{body:?}");
        }
        assert!(!explicit_failure(
            &serde_json::json!({"Errors":[],"Success":true})
        ));
    }
}
