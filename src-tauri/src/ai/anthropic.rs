//! Anthropic Messages API (raw HTTP; there is no official Rust SDK).

use std::time::Duration;

use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;

use super::error::{ErrorKind, ProviderError};
use super::sse;
use super::types::*;

const VERSION: &str = "2023-06-01";
/// Server-side refusal fallback, opted into for the models that support it.
const FALLBACK_BETA: &str = "server-side-fallback-2026-07-01";
const FALLBACK_MODELS: &[&str] = &["claude-opus-5", "claude-fable-5-1"];

fn url(base: &str, path: &str) -> String {
    format!(
        "{}/v1/{path}",
        base.trim_end_matches('/').trim_end_matches("/v1")
    )
}

pub fn body(req: &ChatRequest) -> Value {
    let messages: Vec<Value> = req
        .messages
        .iter()
        .map(|m| {
            json!({
                "role": if m.role == Role::User { "user" } else { "assistant" },
                "content": m.parts.iter().filter_map(block).collect::<Vec<_>>(),
            })
        })
        .collect();
    // Sampling parameters are left out: current Claude models reject them.
    // Automatic prompt caching: the unchanged start of a conversation (system,
    // tools, earlier turns) is read from cache on the next call.
    let mut body = json!({
        "model": req.model,
        "max_tokens": req.max_tokens,
        "stream": true,
        "messages": messages,
        "cache_control": { "type": "ephemeral" },
    });
    if !req.system.is_empty() {
        body["system"] = json!(req.system);
    }
    if !req.tools.is_empty() {
        body["tools"] = req
            .tools
            .iter()
            .map(|t| {
                json!({
                    "name": t.name,
                    "description": t.description,
                    "input_schema": t.schema,
                    "eager_input_streaming": true,
                })
            })
            .collect();
    }
    if FALLBACK_MODELS.contains(&req.model.as_str()) {
        body["fallbacks"] = json!("default");
    }
    body
}

fn block(p: &Part) -> Option<Value> {
    Some(match p {
        Part::Text(t) => json!({ "type": "text", "text": t }),
        Part::Image { media_type, data } => json!({
            "type": "image",
            "source": { "type": "base64", "media_type": media_type, "data": data },
        }),
        Part::ToolUse {
            id, name, input, ..
        } => json!({ "type": "tool_use", "id": id, "name": name, "input": input }),
        Part::ToolResult {
            id,
            parts,
            is_error,
            ..
        } => json!({
            "type": "tool_result",
            "tool_use_id": id,
            "content": parts.iter().filter_map(block).collect::<Vec<_>>(),
            "is_error": is_error,
        }),
        Part::AnthropicBlock(block) => block.clone(),
    })
}

enum Building {
    Text(String),
    Tool {
        id: String,
        name: String,
        json: String,
    },
    /// thinking / redacted_thinking and any other block we pass back as-is.
    Raw(Value),
    Fallback,
}

pub async fn stream(
    http: &reqwest::Client,
    base_url: &str,
    api_key: &str,
    req: &ChatRequest,
    idle: Duration,
    cancel: &CancellationToken,
    on_text: &mut (dyn FnMut(&str) + Send),
) -> Result<Completion, ProviderError> {
    let mut request = http
        .post(url(base_url, "messages"))
        .header("x-api-key", api_key)
        .header("anthropic-version", VERSION)
        .json(&body(req));
    if FALLBACK_MODELS.contains(&req.model.as_str()) {
        request = request.header("anthropic-beta", FALLBACK_BETA);
    }
    let mut events = sse::open(request, idle, cancel).await?;

    let mut blocks: Vec<Building> = Vec::new();
    let mut usage = Usage::default();
    let mut stop = StopReason::Other("incomplete".into());
    let mut model = req.model.clone();

    while let Some(ev) = events.next().await? {
        let v: Value = serde_json::from_str(&ev.data).map_err(|e| {
            ProviderError::new(
                ErrorKind::Malformed,
                format!("Unreadable stream event: {e}"),
            )
        })?;
        match v["type"].as_str().unwrap_or_default() {
            "message_start" => {
                let m = &v["message"];
                if let Some(id) = m["model"].as_str() {
                    model = id.to_string();
                }
                let u = &m["usage"];
                usage.input_tokens = u["input_tokens"].as_u64().unwrap_or(0);
                usage.cache_write_tokens = u["cache_creation_input_tokens"].as_u64().unwrap_or(0);
                usage.cache_read_tokens = u["cache_read_input_tokens"].as_u64().unwrap_or(0);
            }
            "content_block_start" => {
                let b = &v["content_block"];
                blocks.push(match b["type"].as_str().unwrap_or_default() {
                    "text" => Building::Text(b["text"].as_str().unwrap_or_default().to_string()),
                    "tool_use" => Building::Tool {
                        id: b["id"].as_str().unwrap_or_default().into(),
                        name: b["name"].as_str().unwrap_or_default().into(),
                        json: String::new(),
                    },
                    "fallback" => Building::Fallback,
                    _ => Building::Raw(b.clone()),
                });
            }
            "content_block_delta" => {
                let d = &v["delta"];
                let Some(current) = blocks.last_mut() else {
                    continue;
                };
                match (d["type"].as_str().unwrap_or_default(), current) {
                    ("text_delta", Building::Text(t)) => {
                        let piece = d["text"].as_str().unwrap_or_default();
                        t.push_str(piece);
                        on_text(piece);
                    }
                    ("input_json_delta", Building::Tool { json, .. }) => {
                        json.push_str(d["partial_json"].as_str().unwrap_or_default())
                    }
                    ("thinking_delta", Building::Raw(b)) => append(b, "thinking", &d["thinking"]),
                    ("signature_delta", Building::Raw(b)) => {
                        b["signature"] = d["signature"].clone()
                    }
                    _ => {}
                }
            }
            "message_delta" => {
                if let Some(s) = v["delta"]["stop_reason"].as_str() {
                    stop = match s {
                        "end_turn" | "stop_sequence" => StopReason::EndTurn,
                        "tool_use" => StopReason::ToolUse,
                        "max_tokens" => StopReason::MaxTokens,
                        "refusal" => {
                            return Err(ProviderError::new(
                                ErrorKind::Refused,
                                "The model declined to answer this",
                            ))
                        }
                        other => StopReason::Other(other.into()),
                    };
                }
                if let Some(o) = v["usage"]["output_tokens"].as_u64() {
                    usage.output_tokens = o;
                }
            }
            "error" => {
                let e = &v["error"];
                let message = e["message"].as_str().unwrap_or("Stream error").to_string();
                let kind = match e["type"].as_str().unwrap_or_default() {
                    "rate_limit_error" => ErrorKind::RateLimited { retry_after: None },
                    "overloaded_error" | "api_error" => ErrorKind::Server,
                    "authentication_error" => ErrorKind::Auth,
                    "permission_error" => ErrorKind::Permission,
                    "not_found_error" => ErrorKind::NotFound,
                    _ => ErrorKind::BadRequest,
                };
                return Err(ProviderError::new(kind, message));
            }
            "message_stop" => break,
            _ => {} // ping, content_block_stop
        }
    }

    if stop == StopReason::Other("incomplete".into()) {
        return Err(ProviderError::new(
            ErrorKind::Network,
            "The answer stopped partway through",
        ));
    }
    Ok(Completion {
        parts: finish(blocks)?,
        usage,
        stop,
        model,
    })
}

fn append(b: &mut Value, key: &str, piece: &Value) {
    let s = format!(
        "{}{}",
        b[key].as_str().unwrap_or_default(),
        piece.as_str().unwrap_or_default()
    );
    b[key] = json!(s);
}

/// Converts streamed blocks to parts. After a mid-output server-side
/// fallback, only text survives from before the last fallback marker; the
/// declined model's thinking and tool calls must not be sent back.
fn finish(blocks: Vec<Building>) -> Result<Vec<Part>, ProviderError> {
    let boundary = blocks.iter().rposition(|b| matches!(b, Building::Fallback));
    let mut parts = Vec::new();
    for (i, b) in blocks.into_iter().enumerate() {
        let before_boundary = boundary.is_some_and(|f| i < f);
        match b {
            Building::Text(t) if !t.is_empty() => parts.push(Part::Text(t)),
            Building::Text(_) | Building::Fallback => {}
            Building::Tool { .. } | Building::Raw(_) if before_boundary => {}
            Building::Tool { id, name, json } => {
                let input = if json.trim().is_empty() {
                    json!({})
                } else {
                    serde_json::from_str(&json).map_err(|e| {
                        ProviderError::new(
                            ErrorKind::Malformed,
                            format!("The model sent a broken tool call: {e}"),
                        )
                    })?
                };
                parts.push(Part::ToolUse {
                    id,
                    name,
                    input,
                    signature: None,
                });
            }
            Building::Raw(v) => parts.push(Part::AnthropicBlock(v)),
        }
    }
    Ok(parts)
}

pub async fn list_models(
    http: &reqwest::Client,
    base_url: &str,
    api_key: &str,
) -> Result<Vec<ModelInfo>, ProviderError> {
    let response = http
        .get(url(base_url, "models?limit=1000"))
        .header("x-api-key", api_key)
        .header("anthropic-version", VERSION)
        .send()
        .await
        .map_err(|e| ProviderError::from_reqwest(&e))?;
    let v: Value = sse::check_status(response)
        .await?
        .json()
        .await
        .map_err(|e| ProviderError::from_reqwest(&e))?;
    Ok(v["data"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|m| {
            Some(ModelInfo {
                id: m["id"].as_str()?.to_string(),
                vision: m["capabilities"]["image_input"]["supported"]
                    .as_bool()
                    .or(Some(true)),
                tools: Some(true),
            })
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req() -> ChatRequest {
        ChatRequest {
            model: "claude-opus-5".into(),
            system: "Be brief.".into(),
            messages: vec![
                Message::user_text("What's this?"),
                Message {
                    role: Role::Assistant,
                    parts: vec![
                        Part::AnthropicBlock(
                            json!({"type":"thinking","thinking":"","signature":"s"}),
                        ),
                        Part::ToolUse {
                            id: "t1".into(),
                            name: "view_screen".into(),
                            input: json!({}),
                            signature: None,
                        },
                    ],
                },
                Message {
                    role: Role::User,
                    parts: vec![Part::ToolResult {
                        id: "t1".into(),
                        name: "view_screen".into(),
                        parts: vec![Part::Image {
                            media_type: "image/jpeg".into(),
                            data: "AAA".into(),
                        }],
                        is_error: false,
                    }],
                },
            ],
            tools: vec![ToolDef {
                name: "view_screen".into(),
                description: "Look".into(),
                schema: json!({"type":"object"}),
            }],
            max_tokens: 1000,
            temperature: 0.7,
        }
    }

    #[test]
    fn request_body_matches_the_messages_api() {
        let b = body(&req());
        assert_eq!(b["system"], "Be brief.");
        assert!(b.get("temperature").is_none());
        assert_eq!(b["fallbacks"], "default");
        assert_eq!(b["messages"][1]["content"][0]["type"], "thinking");
        assert_eq!(b["messages"][1]["content"][1]["type"], "tool_use");
        let result = &b["messages"][2]["content"][0];
        assert_eq!(result["type"], "tool_result");
        assert_eq!(result["tool_use_id"], "t1");
        assert_eq!(result["content"][0]["source"]["media_type"], "image/jpeg");
        assert_eq!(b["tools"][0]["input_schema"]["type"], "object");
        assert_eq!(b["tools"][0]["eager_input_streaming"], true);
    }

    #[test]
    fn other_models_get_no_fallbacks() {
        let mut r = req();
        r.model = "claude-sonnet-5".into();
        assert!(body(&r).get("fallbacks").is_none());
    }

    #[test]
    fn fallback_boundary_drops_declined_thinking_and_tools() {
        let parts = finish(vec![
            Building::Raw(json!({"type":"thinking"})),
            Building::Text("partial ".into()),
            Building::Tool {
                id: "x".into(),
                name: "view_screen".into(),
                json: "{}".into(),
            },
            Building::Fallback,
            Building::Text("rest".into()),
        ])
        .unwrap();
        assert_eq!(
            parts,
            vec![Part::Text("partial ".into()), Part::Text("rest".into())]
        );
    }

    #[test]
    fn broken_tool_json_is_malformed() {
        let e = finish(vec![Building::Tool {
            id: "x".into(),
            name: "n".into(),
            json: "{\"a\":".into(),
        }])
        .unwrap_err();
        assert_eq!(e.kind, ErrorKind::Malformed);
    }
}
