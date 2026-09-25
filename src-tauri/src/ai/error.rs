//! Provider errors and how the retry logic treats each kind.

use std::time::Duration;

#[derive(Clone, Debug, PartialEq, thiserror::Error)]
#[error("{message}")]
pub struct ProviderError {
    pub kind: ErrorKind,
    pub message: String,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ErrorKind {
    // Worth retrying: the next attempt can plausibly succeed.
    Timeout,
    Network,
    RateLimited {
        retry_after: Option<Duration>,
    },
    Server,
    /// The model's output couldn't be parsed (bad JSON, broken tool call).
    Malformed,

    // Specific to this provider or model: retrying the same one fails the same
    // way, but a different model in the fallback chain may work.
    Auth,
    Permission,
    NotFound,
    BadRequest,
    Billing,
    /// The model declined to answer.
    Refused,

    // Stop everything.
    Budget,
    Cancelled,
    /// Something outside the providers blocks the request (no provider set
    /// up, screen capture paused, keychain unavailable).
    Setup,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Disposition {
    Retry,
    TryNextModel,
    Stop,
}

impl ErrorKind {
    pub fn disposition(&self) -> Disposition {
        use ErrorKind::*;
        match self {
            Timeout | Network | RateLimited { .. } | Server | Malformed => Disposition::Retry,
            Auth | Permission | NotFound | BadRequest | Billing | Refused => {
                Disposition::TryNextModel
            }
            Budget | Cancelled | Setup => Disposition::Stop,
        }
    }

    /// A short plain-language reason, for "Retry 2 of 3: …".
    pub fn short(&self) -> &'static str {
        use ErrorKind::*;
        match self {
            Timeout => "the provider timed out",
            Network => "couldn't reach the provider",
            RateLimited { .. } => "rate limited",
            Server => "the provider had an error",
            Malformed => "the answer came back garbled",
            Auth => "the API key was rejected",
            Permission => "this key isn't allowed to use that model",
            NotFound => "the model wasn't found",
            BadRequest => "the provider rejected the request",
            Billing => "a billing problem with the provider",
            Refused => "the model declined",
            Budget => "a spending limit was reached",
            Cancelled => "cancelled",
            Setup => "Helpy isn't set up for this",
        }
    }
}

impl ProviderError {
    pub fn new(kind: ErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }

    /// Maps an HTTP error status (and body text) to an error kind.
    pub fn from_status(status: u16, retry_after: Option<Duration>, body: &str) -> Self {
        let detail = extract_message(body);
        let kind = match status {
            401 => ErrorKind::Auth,
            402 => ErrorKind::Billing,
            403 => ErrorKind::Permission,
            404 => ErrorKind::NotFound,
            408 => ErrorKind::Timeout,
            429 => ErrorKind::RateLimited { retry_after },
            400 | 409 | 413 | 422 => ErrorKind::BadRequest,
            500..=599 => ErrorKind::Server,
            _ => ErrorKind::BadRequest,
        };
        let message = match detail {
            Some(d) => format!("{} ({status}): {d}", capitalize(kind.short())),
            None => format!("{} ({status})", capitalize(kind.short())),
        };
        Self { kind, message }
    }

    pub fn from_reqwest(e: &reqwest::Error) -> Self {
        if e.is_timeout() {
            Self::new(ErrorKind::Timeout, "The provider didn't respond in time")
        } else if e.is_decode() || e.is_body() {
            Self::new(ErrorKind::Network, format!("The connection broke off: {e}"))
        } else {
            Self::new(
                ErrorKind::Network,
                format!("Couldn't reach the provider: {}", root_cause(e)),
            )
        }
    }
}

fn root_cause(e: &(dyn std::error::Error + 'static)) -> String {
    let mut cur = e;
    while let Some(next) = cur.source() {
        cur = next;
    }
    cur.to_string()
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    c.next()
        .map(|f| f.to_uppercase().chain(c).collect())
        .unwrap_or_default()
}

/// Pulls a human-readable message out of the error bodies the providers use:
/// `{"error":{"message":…}}` (Anthropic, OpenAI, Gemini, most local servers).
fn extract_message(body: &str) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(body).ok()?;
    let m = v["error"]["message"]
        .as_str()
        .or_else(|| v["error"].as_str())
        .or_else(|| v["message"].as_str())?;
    Some(m.chars().take(300).collect())
}

pub fn parse_retry_after(value: Option<&str>) -> Option<Duration> {
    value?
        .trim()
        .parse::<f64>()
        .ok()
        .filter(|s| *s >= 0.0)
        .map(Duration::from_secs_f64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn statuses_map_to_the_right_disposition() {
        let d = |s| ProviderError::from_status(s, None, "").kind.disposition();
        assert_eq!(d(429), Disposition::Retry);
        assert_eq!(d(500), Disposition::Retry);
        assert_eq!(d(529), Disposition::Retry);
        assert_eq!(d(401), Disposition::TryNextModel);
        assert_eq!(d(403), Disposition::TryNextModel);
        assert_eq!(d(404), Disposition::TryNextModel);
        assert_eq!(d(400), Disposition::TryNextModel);
    }

    #[test]
    fn keeps_retry_after_and_provider_message() {
        let e = ProviderError::from_status(
            429,
            parse_retry_after(Some("7")),
            r#"{"type":"error","error":{"type":"rate_limit_error","message":"Slow down"}}"#,
        );
        assert_eq!(
            e.kind,
            ErrorKind::RateLimited {
                retry_after: Some(Duration::from_secs(7))
            }
        );
        assert_eq!(e.message, "Rate limited (429): Slow down");
    }
}
