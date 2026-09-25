//! Calling a service's HTTP API for a connector, with failures mapped to
//! what the agent runner understands: worth retrying, or not.

use reqwest::{Method, StatusCode};
use serde_json::Value;

use crate::agents::runner::ToolOutcome;

pub type ApiResult<T> = Result<T, ToolOutcome>;

/// An authorized client for one connector's service.
pub struct Api {
    pub http: reqwest::Client,
    pub token: String,
    /// The service's name, for messages ("Gmail").
    pub service: &'static str,
    /// Extra headers every call needs (Notion's API version).
    pub headers: Vec<(&'static str, String)>,
}

fn failure(service: &str, status: StatusCode, body: &str) -> ToolOutcome {
    // Services put their reason in different places; take the first found.
    let v: Value = serde_json::from_str(body).unwrap_or(Value::Null);
    let why = [
        &v["error"]["message"],
        &v["message"],
        &v["error_description"],
        &v["error"],
    ]
    .into_iter()
    .find_map(|x| x.as_str())
    .unwrap_or("")
    .to_string();
    let why = if why.is_empty() {
        String::new()
    } else {
        format!(": {why}")
    };
    match status.as_u16() {
        401 => ToolOutcome::Permanent(format!(
            "{service} no longer accepts Helpy's sign-in{why}. Reconnect it in Settings → Connectors."
        )),
        403 => ToolOutcome::Permanent(format!("{service} doesn't allow this{why}. The connection may be read-only.")),
        404 => ToolOutcome::Permanent(format!("{service} couldn't find that{why}.")),
        429 | 500..=599 => ToolOutcome::Transient(format!("{service} is busy or having trouble ({status})")),
        _ => ToolOutcome::Permanent(format!("{service} answered {status}{why}.")),
    }
}

impl Api {
    pub async fn send(
        &self,
        method: Method,
        url: &str,
        query: &[(&str, String)],
        body: Option<&Value>,
    ) -> ApiResult<Value> {
        let mut rb = self
            .http
            .request(method, url)
            .bearer_auth(&self.token)
            .header("Accept", "application/json")
            .query(query);
        for (k, v) in &self.headers {
            rb = rb.header(*k, v);
        }
        if let Some(b) = body {
            rb = rb.json(b);
        }
        let resp = rb.send().await.map_err(|e| {
            if e.is_timeout() || e.is_connect() {
                ToolOutcome::Transient(format!("{} didn't respond", self.service))
            } else {
                ToolOutcome::Permanent(format!("Couldn't reach {}: {e}", self.service))
            }
        })?;
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        if !status.is_success() {
            return Err(failure(self.service, status, &text));
        }
        if text.trim().is_empty() {
            return Ok(Value::Null);
        }
        let v: Value = serde_json::from_str(&text).map_err(|_| {
            ToolOutcome::Transient(format!(
                "{} sent something Helpy couldn't read",
                self.service
            ))
        })?;
        // Slack answers 200 with ok: false.
        if v["ok"] == Value::Bool(false) {
            let e = v["error"].as_str().unwrap_or("unknown error");
            return Err(match e {
                "ratelimited" => ToolOutcome::Transient(format!("{} is rate limiting", self.service)),
                "invalid_auth" | "token_revoked" | "not_authed" | "account_inactive" => ToolOutcome::Permanent(format!(
                    "{} no longer accepts Helpy's sign-in ({e}). Reconnect it in Settings → Connectors.",
                    self.service
                )),
                _ => ToolOutcome::Permanent(format!("{} refused: {e}", self.service)),
            });
        }
        Ok(v)
    }

    pub async fn get(&self, url: &str, query: &[(&str, String)]) -> ApiResult<Value> {
        self.send(Method::GET, url, query, None).await
    }

    pub async fn post(&self, url: &str, body: &Value) -> ApiResult<Value> {
        self.send(Method::POST, url, &[], Some(body)).await
    }

    pub async fn patch(&self, url: &str, body: &Value) -> ApiResult<Value> {
        self.send(Method::PATCH, url, &[], Some(body)).await
    }

    /// The body as text (file downloads, exports).
    pub async fn get_text(&self, url: &str, query: &[(&str, String)]) -> ApiResult<String> {
        let resp = self
            .http
            .get(url)
            .bearer_auth(&self.token)
            .query(query)
            .send()
            .await
            .map_err(|_| ToolOutcome::Transient(format!("{} didn't respond", self.service)))?;
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        if !status.is_success() {
            return Err(failure(self.service, status, &text));
        }
        Ok(text)
    }
}

/// A required string argument.
pub fn arg<'a>(args: &'a Value, key: &str) -> ApiResult<&'a str> {
    args[key]
        .as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| ToolOutcome::Permanent(format!("\"{key}\" is missing.")))
}

pub fn opt<'a>(args: &'a Value, key: &str) -> Option<&'a str> {
    args[key].as_str().map(str::trim).filter(|s| !s.is_empty())
}

pub fn limit(args: &Value, default: u64, max: u64) -> u64 {
    args["limit"].as_u64().unwrap_or(default).clamp(1, max)
}

pub fn ok(text: String) -> ToolOutcome {
    ToolOutcome::Ok {
        text,
        ops: Vec::new(),
    }
}

/// Runs a connector action written with `?`, turning its error into the outcome.
pub fn done(r: ApiResult<String>) -> ToolOutcome {
    match r {
        Ok(text) => ok(text),
        Err(e) => e,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_failures_to_retry_or_stop() {
        let body = r#"{"error": {"message": "Invalid Credentials"}}"#;
        assert!(
            matches!(failure("Gmail", StatusCode::UNAUTHORIZED, body), ToolOutcome::Permanent(m) if m.contains("Reconnect") && m.contains("Invalid Credentials"))
        );
        assert!(matches!(
            failure("Gmail", StatusCode::TOO_MANY_REQUESTS, ""),
            ToolOutcome::Transient(_)
        ));
        assert!(matches!(
            failure("Gmail", StatusCode::BAD_GATEWAY, ""),
            ToolOutcome::Transient(_)
        ));
        assert!(
            matches!(failure("Notion", StatusCode::NOT_FOUND, r#"{"message": "Could not find page"}"#), ToolOutcome::Permanent(m) if m.contains("Could not find page"))
        );
    }

    #[test]
    fn reads_arguments() {
        let a = serde_json::json!({"q": " x ", "empty": "", "limit": 500});
        assert_eq!(arg(&a, "q").ok(), Some("x"));
        assert!(arg(&a, "empty").is_err());
        assert_eq!(opt(&a, "missing"), None);
        assert_eq!(limit(&a, 10, 50), 50);
    }
}
