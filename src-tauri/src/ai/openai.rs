//! OpenAI chat completions, also spoken by Ollama, LM Studio, llama.cpp
//! server and most self-hosted endpoints.

use std::time::Duration;

use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;

use super::error::{ErrorKind, ProviderError};
use super::sse;
use super::types::*;

#[derive(Clone, Copy, PartialEq)]
pub enum Dialect {
    /// api.openai.com: newer models take `max_completion_tokens` and some
    /// reject custom temperatures.
    OpenAi,
    /// Everything else speaking the same API.
    Compatible,
}

pub fn url(base: &str, path: &str) -> String {
    format!("{}/{path}", base.trim_end_matches('/'))
}

/// OpenAI reasoning models only accept the default temperature.
fn fixed_temperature(model: &str) -> bool {
    model.starts_with('o') || model.starts_with("gpt-5")
}

pub fn body(req: &ChatRequest, dialect: Dialect) -> Value {
    let mut messages = Vec::new();
    if !req.system.is_empty() {
        messages.push(json!({ "role": "system", "content": req.system }));
    }
    for m in &req.messages {
        match m.role {
            Role::User => push_user(&mut messages, &m.parts),
            Role::Assistant => messages.push(assistant(&m.parts)),
        }
    }
    let mut body = json!({
        "model": req.model,
        "messages": messages,
        "stream": true,
        "stream_options": { "include_usage": true },
    });
    match dialect {
        Dialect::OpenAi => {
            body["max_completion_tokens"] = json!(req.max_tokens);
            if !fixed_temperature(&req.model) {
                body["temperature"] = json!(req.temperature);
            }
        }
        Dialect::Compatible => {
            body["max_tokens"] = json!(req.max_tokens);
            body["temperature"] = json!(req.temperature);
        }
    }
    if !req.tools.is_empty() {
        body["tools"] = req
            .tools
            .iter()
            .map(|t| json!({ "type": "function", "function": { "name": t.name, "description": t.description, "parameters": t.schema } }))
            .collect();
    }
    body
}

/// Tool results become `tool` messages. The chat completions API can't carry
/// images in them, so any images follow in a user message.
fn push_user(messages: &mut Vec<Value>, parts: &[Part]) {
    let mut content = Vec::new();
    let mut tool_images = Vec::new();
    for p in parts {
        match p {
            Part::Text(t) => content.push(json!({ "type": "text", "text": t })),
            Part::Image { .. } => content.push(image(p)),
            Part::ToolResult {
                id,
                parts,
                is_error,
                ..
            } => {
                let text: String = parts
                    .iter()
                    .filter_map(|p| {
                        if let Part::Text(t) = p {
                            Some(t.as_str())
                        } else {
                            None
                        }
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                let images: Vec<_> = parts
                    .iter()
                    .filter(|p| matches!(p, Part::Image { .. }))
                    .collect();
                let note = if images.is_empty() {
                    ""
                } else {
                    " The screenshot follows in the next message."
                };
                let prefix = if *is_error { "Error: " } else { "" };
                messages.push(json!({ "role": "tool", "tool_call_id": id, "content": format!("{prefix}{text}{note}") }));
                tool_images.extend(images.into_iter().map(image));
            }
            Part::ToolUse { .. } | Part::AnthropicBlock(_) => {}
        }
    }
    if !tool_images.is_empty() {
        let mut c =
            vec![json!({ "type": "text", "text": "Screenshot from the view_screen tool:" })];
        c.extend(tool_images);
        messages.push(json!({ "role": "user", "content": c }));
    }
    if content.is_empty() {
        return;
    }
    // Plain text as a string: some local servers only accept that form.
    if let [only] = content.as_slice() {
        if only["type"] == "text" {
            messages.push(json!({ "role": "user", "content": only["text"] }));
            return;
        }
    }
    messages.push(json!({ "role": "user", "content": content }));
}

fn image(p: &Part) -> Value {
    let Part::Image { media_type, data } = p else {
        unreachable!()
    };
    json!({ "type": "image_url", "image_url": { "url": format!("data:{media_type};base64,{data}") } })
}

fn assistant(parts: &[Part]) -> Value {
    let text: String = parts
        .iter()
        .filter_map(|p| {
            if let Part::Text(t) = p {
                Some(t.as_str())
            } else {
                None
            }
        })
        .collect();
    let calls: Vec<Value> = parts
        .iter()
        .filter_map(|p| match p {
            Part::ToolUse { id, name, input, .. } => Some(json!({
                "id": id, "type": "function", "function": { "name": name, "arguments": input.to_string() },
            })),
            _ => None,
        })
        .collect();
    let mut m = json!({ "role": "assistant", "content": if text.is_empty() { Value::Null } else { json!(text) } });
    if !calls.is_empty() {
        m["tool_calls"] = json!(calls);
    }
    m
}

#[derive(Default)]
struct Call {
    id: String,
    name: String,
    args: String,
}

#[allow(clippy::too_many_arguments)]
pub async fn stream(
    http: &reqwest::Client,
    base_url: &str,
    api_key: Option<&str>,
    dialect: Dialect,
    req: &ChatRequest,
    idle: Duration,
    cancel: &CancellationToken,
    on_text: &mut (dyn FnMut(&str) + Send),
) -> Result<Completion, ProviderError> {
    let mut request = http
        .post(url(base_url, "chat/completions"))
        .json(&body(req, dialect));
    if let Some(key) = api_key.filter(|k| !k.is_empty()) {
        request = request.bearer_auth(key);
    }
    let mut events = sse::open(request, idle, cancel).await?;

    let mut text = String::new();
    let mut calls: Vec<Call> = Vec::new();
    let mut usage: Option<Usage> = None;
    let mut stop: Option<StopReason> = None;
    let mut model = req.model.clone();
    let mut done = false;

    while let Some(ev) = events.next().await? {
        if ev.data.trim() == "[DONE]" {
            done = true;
            break;
        }
        let v: Value = serde_json::from_str(&ev.data).map_err(|e| {
            ProviderError::new(
                ErrorKind::Malformed,
                format!("Unreadable stream event: {e}"),
            )
        })?;
        if let Some(e) = v.get("error") {
            let message = e["message"]
                .as_str()
                .or(e.as_str())
                .unwrap_or("Stream error")
                .to_string();
            return Err(ProviderError::new(ErrorKind::Server, message));
        }
        if let Some(m) = v["model"].as_str() {
            model = m.to_string();
        }
        if let Some(u) = v.get("usage").filter(|u| u.is_object()) {
            usage = Some(Usage {
                input_tokens: u["prompt_tokens"].as_u64().unwrap_or(0),
                output_tokens: u["completion_tokens"].as_u64().unwrap_or(0),
            });
        }
        let Some(choice) = v["choices"].get(0) else {
            continue;
        };
        let delta = &choice["delta"];
        if let Some(piece) = delta["content"].as_str() {
            text.push_str(piece);
            on_text(piece);
        }
        for tc in delta["tool_calls"].as_array().into_iter().flatten() {
            let i = tc["index"].as_u64().unwrap_or(calls.len() as u64) as usize;
            while calls.len() <= i {
                calls.push(Call::default());
            }
            let c = &mut calls[i];
            if let Some(id) = tc["id"].as_str() {
                c.id = id.to_string();
            }
            if let Some(n) = tc["function"]["name"].as_str() {
                c.name.push_str(n);
            }
            if let Some(a) = tc["function"]["arguments"].as_str() {
                c.args.push_str(a);
            }
        }
        if let Some(f) = choice["finish_reason"].as_str() {
            stop = Some(match f {
                "stop" => StopReason::EndTurn,
                "tool_calls" | "function_call" => StopReason::ToolUse,
                "length" => StopReason::MaxTokens,
                "content_filter" => {
                    return Err(ProviderError::new(
                        ErrorKind::Refused,
                        "The provider's content filter blocked the answer",
                    ))
                }
                other => StopReason::Other(other.into()),
            });
        }
    }

    // Some servers end the stream without [DONE]; a finish reason is enough.
    let Some(mut stop) = stop else {
        if !done {
            return Err(ProviderError::new(
                ErrorKind::Network,
                "The answer stopped partway through",
            ));
        }
        return Err(ProviderError::new(
            ErrorKind::Malformed,
            "The provider ended the answer without saying why",
        ));
    };

    let mut parts = Vec::new();
    if !text.is_empty() {
        parts.push(Part::Text(text));
    }
    for (i, c) in calls.into_iter().enumerate() {
        let input = if c.args.trim().is_empty() {
            json!({})
        } else {
            serde_json::from_str(&c.args).map_err(|e| {
                ProviderError::new(
                    ErrorKind::Malformed,
                    format!("The model sent a broken tool call: {e}"),
                )
            })?
        };
        let id = if c.id.is_empty() {
            format!("call_{i}")
        } else {
            c.id
        };
        parts.push(Part::ToolUse {
            id,
            name: c.name,
            input,
            signature: None,
        });
        stop = StopReason::ToolUse;
    }
    let usage = usage.unwrap_or_else(|| estimate_usage(req, &parts));
    Ok(Completion {
        parts,
        usage,
        stop,
        model,
    })
}

/// Local servers often don't report usage; estimate so budgets still count.
fn estimate_usage(req: &ChatRequest, parts: &[Part]) -> Usage {
    let out: usize = parts
        .iter()
        .map(|p| match p {
            Part::Text(t) => t.len(),
            Part::ToolUse { input, .. } => input.to_string().len(),
            _ => 0,
        })
        .sum();
    Usage {
        input_tokens: req.estimated_tokens() - req.max_tokens as u64,
        output_tokens: out as u64 / 4 + 1,
    }
}

/// Model ids that can't chat, filtered out of OpenAI's list.
fn is_chat_model(id: &str) -> bool {
    ![
        "embedding",
        "tts",
        "whisper",
        "dall-e",
        "moderation",
        "davinci",
        "babbage",
        "transcribe",
        "image",
        "realtime",
        "audio",
        "search",
    ]
    .iter()
    .any(|x| id.contains(x))
}

pub async fn list_models(
    http: &reqwest::Client,
    base_url: &str,
    api_key: Option<&str>,
    dialect: Dialect,
) -> Result<Vec<ModelInfo>, ProviderError> {
    let mut request = http.get(url(base_url, "models"));
    if let Some(key) = api_key.filter(|k| !k.is_empty()) {
        request = request.bearer_auth(key);
    }
    let response = request
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
        .filter_map(|m| m["id"].as_str())
        .filter(|id| dialect == Dialect::Compatible || is_chat_model(id))
        .map(|id| ModelInfo {
            id: id.to_string(),
            vision: None,
            tools: None,
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_results_with_images_become_a_tool_message_and_a_user_image() {
        let req = ChatRequest {
            model: "gpt-4.1".into(),
            system: "sys".into(),
            messages: vec![
                Message::user_text("hi"),
                Message {
                    role: Role::Assistant,
                    parts: vec![Part::ToolUse {
                        id: "c1".into(),
                        name: "view_screen".into(),
                        input: json!({"reason":"x"}),
                        signature: None,
                    }],
                },
                Message {
                    role: Role::User,
                    parts: vec![Part::ToolResult {
                        id: "c1".into(),
                        name: "view_screen".into(),
                        parts: vec![
                            Part::Text("Display 1".into()),
                            Part::Image {
                                media_type: "image/jpeg".into(),
                                data: "QQ".into(),
                            },
                        ],
                        is_error: false,
                    }],
                },
            ],
            tools: vec![],
            max_tokens: 500,
            temperature: 0.2,
        };
        let b = body(&req, Dialect::OpenAi);
        let m = b["messages"].as_array().unwrap();
        assert_eq!(m[0]["role"], "system");
        assert_eq!(m[1]["content"], "hi");
        assert_eq!(
            m[2]["tool_calls"][0]["function"]["arguments"],
            "{\"reason\":\"x\"}"
        );
        assert_eq!(m[3]["role"], "tool");
        assert_eq!(m[3]["tool_call_id"], "c1");
        assert_eq!(m[4]["role"], "user");
        assert_eq!(
            m[4]["content"][1]["image_url"]["url"],
            "data:image/jpeg;base64,QQ"
        );
        assert_eq!(b["max_completion_tokens"], 500);
        assert_eq!(b["temperature"], 0.2);
    }

    #[test]
    fn dialects_differ_in_token_limit_and_temperature() {
        let mut req = ChatRequest {
            model: "gpt-5".into(),
            system: String::new(),
            messages: vec![Message::user_text("x")],
            tools: vec![],
            max_tokens: 100,
            temperature: 0.5,
        };
        let b = body(&req, Dialect::OpenAi);
        assert!(b.get("temperature").is_none());
        req.model = "llama3.2".into();
        let b = body(&req, Dialect::Compatible);
        assert_eq!(b["max_tokens"], 100);
        assert_eq!(b["temperature"], 0.5);
    }

    #[test]
    fn filters_non_chat_models() {
        assert!(is_chat_model("gpt-4.1-mini"));
        assert!(!is_chat_model("text-embedding-3-small"));
        assert!(!is_chat_model("whisper-1"));
    }
}
