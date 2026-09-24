//! Server-sent events: a byte-level parser plus a reader that applies the
//! idle timeout and cancellation to every chunk.

use std::time::Duration;

use futures_util::{Stream, StreamExt};
use tokio_util::sync::CancellationToken;

use super::error::{parse_retry_after, ErrorKind, ProviderError};

#[derive(Debug, Clone, PartialEq)]
pub struct SseEvent {
    pub event: Option<String>,
    pub data: String,
}

#[derive(Default)]
pub struct SseParser {
    buf: Vec<u8>,
}

impl SseParser {
    /// Feeds raw bytes; returns every event completed by them. Handles events
    /// split across chunks, multi-byte characters split across chunks, CRLF
    /// line endings and multi-line `data:` fields.
    pub fn push(&mut self, bytes: &[u8]) -> Vec<SseEvent> {
        self.buf
            .extend(bytes.iter().copied().filter(|b| *b != b'\r'));
        let mut out = Vec::new();
        while let Some(end) = self.buf.windows(2).position(|w| w == b"\n\n") {
            let raw: Vec<u8> = self.buf.drain(..end + 2).collect();
            let text = String::from_utf8_lossy(&raw[..end]);
            let mut event = None;
            let mut data: Vec<&str> = Vec::new();
            for line in text.lines() {
                if let Some(v) = line.strip_prefix("data:") {
                    data.push(v.strip_prefix(' ').unwrap_or(v));
                } else if let Some(v) = line.strip_prefix("event:") {
                    event = Some(v.trim().to_string());
                }
            }
            if !data.is_empty() {
                out.push(SseEvent {
                    event,
                    data: data.join("\n"),
                });
            }
        }
        out
    }
}

/// Reads events from a streaming HTTP response.
pub struct SseReader<S> {
    stream: S,
    parser: SseParser,
    pending: std::collections::VecDeque<SseEvent>,
    idle: Duration,
    cancel: CancellationToken,
}

impl<S, B> SseReader<S>
where
    S: Stream<Item = Result<B, reqwest::Error>> + Unpin,
    B: AsRef<[u8]>,
{
    /// Next event, None at the end of the stream.
    pub async fn next(&mut self) -> Result<Option<SseEvent>, ProviderError> {
        loop {
            if let Some(e) = self.pending.pop_front() {
                return Ok(Some(e));
            }
            let chunk = tokio::select! {
                _ = self.cancel.cancelled() => return Err(cancelled()),
                r = tokio::time::timeout(self.idle, self.stream.next()) => r.map_err(|_| idle_timeout(self.idle))?,
            };
            match chunk {
                None => return Ok(None),
                Some(Err(e)) => return Err(ProviderError::from_reqwest(&e)),
                Some(Ok(bytes)) => self.pending.extend(self.parser.push(bytes.as_ref())),
            }
        }
    }
}

pub fn cancelled() -> ProviderError {
    ProviderError::new(ErrorKind::Cancelled, "Stopped")
}

fn idle_timeout(idle: Duration) -> ProviderError {
    ProviderError::new(
        ErrorKind::Timeout,
        format!("No response for {} seconds", idle.as_secs()),
    )
}

/// Sends a request and, if it succeeds, returns an event reader for its body.
/// HTTP errors become classified `ProviderError`s.
pub async fn open(
    request: reqwest::RequestBuilder,
    idle: Duration,
    cancel: &CancellationToken,
) -> Result<
    SseReader<impl Stream<Item = Result<bytes::Bytes, reqwest::Error>> + Unpin>,
    ProviderError,
> {
    let response = tokio::select! {
        _ = cancel.cancelled() => return Err(cancelled()),
        r = tokio::time::timeout(idle, request.send()) => r.map_err(|_| idle_timeout(idle))?,
    }
    .map_err(|e| ProviderError::from_reqwest(&e))?;
    let response = check_status(response).await?;
    Ok(SseReader {
        stream: response.bytes_stream(),
        parser: SseParser::default(),
        pending: Default::default(),
        idle,
        cancel: cancel.clone(),
    })
}

/// Turns a non-2xx response into a classified error.
pub async fn check_status(response: reqwest::Response) -> Result<reqwest::Response, ProviderError> {
    let status = response.status();
    if status.is_success() {
        return Ok(response);
    }
    let retry_after = parse_retry_after(
        response
            .headers()
            .get("retry-after")
            .and_then(|v| v.to_str().ok()),
    );
    let body = response.text().await.unwrap_or_default();
    Err(ProviderError::from_status(
        status.as_u16(),
        retry_after,
        &body,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_events_split_across_chunks() {
        let mut p = SseParser::default();
        assert!(p.push(b"event: message_start\ndata: {\"a\"").is_empty());
        let out = p.push(b":1}\n\ndata: [DONE]\n\n");
        assert_eq!(
            out,
            vec![
                SseEvent {
                    event: Some("message_start".into()),
                    data: "{\"a\":1}".into()
                },
                SseEvent {
                    event: None,
                    data: "[DONE]".into()
                },
            ]
        );
    }

    #[test]
    fn handles_crlf_multiline_data_and_split_utf8() {
        let mut p = SseParser::default();
        let text = "data: héllo\r\ndata: world\r\n\r\n".as_bytes();
        let (a, b) = text.split_at(8); // splits inside "é"
        assert!(p.push(a).is_empty());
        assert_eq!(p.push(b)[0].data, "héllo\nworld");
    }

    #[test]
    fn ignores_comments_and_keepalives() {
        let mut p = SseParser::default();
        assert!(p.push(b": ping\n\n").is_empty());
    }
}
