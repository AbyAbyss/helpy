//! Google Gemini API (generativelanguage.googleapis.com).

use std::time::Duration;

use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;

use super::error::{ErrorKind, ProviderError};
use super::sse;
use super::types::*;

fn url(base: &str, path: &str) -> String {
    format!("{}/{path}", base.trim_end_matches('/'))
}

pub fn body(req: &ChatRequest) -> Value {
    let contents: Vec<Value> = req
        .messages
        .iter()
        .map(|m| {
            let mut parts = Vec::new();
            let mut images = Vec::new();
            for p in &m.parts {
                match p {
                    Part::Text(t) => parts.push(json!({ "text": t })),
                    Part::Image { .. } => parts.push(inline(p)),
                    Part::ToolUse { name, input, signature, .. } => {
                        let mut part = json!({ "functionCall": { "name": name, "args": input } });
                        if let Some(sig) = signature {
                            part["thoughtSignature"] = json!(sig);
                        }
                        parts.push(part);
                    }
                    Part::ToolResult { name, parts: inner, is_error, .. } => {
                        let text: Vec<&str> =
                            inner.iter().filter_map(|p| if let Part::Text(t) = p { Some(t.as_str()) } else { None }).collect();
                        let key = if *is_error { "error" } else { "content" };
                        parts.push(json!({ "functionResponse": { "name": name, "response": { key: text.join("\n") } } }));
                        images.extend(inner.iter().filter(|p| matches!(p, Part::Image { .. })).map(inline));
                    }
                    Part::AnthropicBlock(_) => {}
                }
            }
            parts.extend(images);
            json!({ "role": if m.role == Role::User { "user" } else { "model" }, "parts": parts })
        })
        .collect();
    let mut body = json!({
        "contents": contents,
        "generationConfig": { "temperature": req.temperature, "maxOutputTokens": req.max_tokens },
    });
    if !req.system.is_empty() {
        body["systemInstruction"] = json!({ "parts": [{ "text": req.system }] });
    }
    if !req.tools.is_empty() {
        body["tools"] = json!([{
            "functionDeclarations": req.tools.iter().map(|t| json!({
                "name": t.name, "description": t.description, "parameters": t.schema,
            })).collect::<Vec<_>>(),
        }]);
    }
    body
}

fn inline(p: &Part) -> Value {
    let Part::Image { media_type, data } = p else {
        unreachable!()
    };
    json!({ "inlineData": { "mimeType": media_type, "data": data } })
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
    let request = http
        .post(url(
            base_url,
            &format!("models/{}:streamGenerateContent?alt=sse", req.model),
        ))
        .header("x-goog-api-key", api_key)
        .json(&body(req));
    let mut events = sse::open(request, idle, cancel).await?;

    let mut text = String::new();
    let mut parts = Vec::new();
    let mut usage = Usage::default();
    let mut stop: Option<StopReason> = None;

    while let Some(ev) = events.next().await? {
        let v: Value = serde_json::from_str(&ev.data).map_err(|e| {
            ProviderError::new(
                ErrorKind::Malformed,
                format!("Unreadable stream event: {e}"),
            )
        })?;
        if let Some(e) = v.get("error") {
            return Err(ProviderError::new(
                ErrorKind::Server,
                e["message"].as_str().unwrap_or("Stream error"),
            ));
        }
        if let Some(reason) = v["promptFeedback"]["blockReason"].as_str() {
            return Err(ProviderError::new(
                ErrorKind::Refused,
                format!("Gemini blocked the question ({reason})"),
            ));
        }
        let u = &v["usageMetadata"];
        if u.is_object() {
            // promptTokenCount includes the cached part.
            let prompt = u["promptTokenCount"].as_u64().unwrap_or(0);
            let cached = u["cachedContentTokenCount"].as_u64().unwrap_or(0).min(prompt);
            usage.input_tokens = prompt - cached;
            usage.cache_read_tokens = cached;
            usage.output_tokens = u["candidatesTokenCount"].as_u64().unwrap_or(0)
                + u["thoughtsTokenCount"].as_u64().unwrap_or(0);
        }
        let Some(candidate) = v["candidates"].get(0) else {
            continue;
        };
        for p in candidate["content"]["parts"]
            .as_array()
            .into_iter()
            .flatten()
        {
            if p["thought"].as_bool() == Some(true) {
                continue;
            }
            if let Some(t) = p["text"].as_str() {
                text.push_str(t);
                on_text(t);
            }
            if let Some(call) = p.get("functionCall") {
                let name = call["name"].as_str().unwrap_or_default().to_string();
                let id = call["id"]
                    .as_str()
                    .map(str::to_string)
                    .unwrap_or_else(|| format!("{name}-{}", parts.len()));
                parts.push(Part::ToolUse {
                    id,
                    name,
                    input: call.get("args").cloned().unwrap_or(json!({})),
                    signature: p["thoughtSignature"].as_str().map(str::to_string),
                });
            }
        }
        if let Some(f) = candidate["finishReason"].as_str() {
            stop = Some(match f {
                "STOP" => StopReason::EndTurn,
                "MAX_TOKENS" => StopReason::MaxTokens,
                "SAFETY" | "RECITATION" | "BLOCKLIST" | "PROHIBITED_CONTENT" | "SPII" => {
                    return Err(ProviderError::new(
                        ErrorKind::Refused,
                        format!("Gemini stopped the answer ({f})"),
                    ))
                }
                "MALFORMED_FUNCTION_CALL" => {
                    return Err(ProviderError::new(
                        ErrorKind::Malformed,
                        "The model sent a broken tool call",
                    ))
                }
                other => StopReason::Other(other.into()),
            });
        }
    }

    let Some(mut stop) = stop else {
        return Err(ProviderError::new(
            ErrorKind::Network,
            "The answer stopped partway through",
        ));
    };
    if parts.iter().any(|p| matches!(p, Part::ToolUse { .. })) {
        stop = StopReason::ToolUse;
    }
    if !text.is_empty() {
        parts.insert(0, Part::Text(text));
    }
    Ok(Completion {
        parts,
        usage,
        stop,
        model: req.model.clone(),
    })
}

pub async fn list_models(
    http: &reqwest::Client,
    base_url: &str,
    api_key: &str,
) -> Result<Vec<ModelInfo>, ProviderError> {
    let response = http
        .get(url(base_url, "models?pageSize=1000"))
        .header("x-goog-api-key", api_key)
        .send()
        .await
        .map_err(|e| ProviderError::from_reqwest(&e))?;
    let v: Value = sse::check_status(response)
        .await?
        .json()
        .await
        .map_err(|e| ProviderError::from_reqwest(&e))?;
    Ok(v["models"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|m| {
            m["supportedGenerationMethods"]
                .as_array()
                .is_some_and(|a| a.iter().any(|x| x == "generateContent"))
        })
        .filter_map(|m| m["name"].as_str())
        .map(|n| ModelInfo {
            id: n.trim_start_matches("models/").to_string(),
            vision: Some(true),
            tools: Some(true),
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_roles_tools_and_keeps_thought_signatures() {
        let req = ChatRequest {
            model: "gemini-2.5-flash".into(),
            system: "sys".into(),
            messages: vec![
                Message::user_text("hi"),
                Message {
                    role: Role::Assistant,
                    parts: vec![Part::ToolUse {
                        id: "view_screen-0".into(),
                        name: "view_screen".into(),
                        input: json!({}),
                        signature: Some("sig".into()),
                    }],
                },
                Message {
                    role: Role::User,
                    parts: vec![Part::ToolResult {
                        id: "view_screen-0".into(),
                        name: "view_screen".into(),
                        parts: vec![
                            Part::Text("ok".into()),
                            Part::Image {
                                media_type: "image/jpeg".into(),
                                data: "QQ".into(),
                            },
                        ],
                        is_error: false,
                    }],
                },
            ],
            tools: vec![ToolDef {
                name: "view_screen".into(),
                description: "d".into(),
                schema: json!({"type":"object"}),
            }],
            max_tokens: 800,
            temperature: 0.3,
        };
        let b = body(&req);
        assert_eq!(b["systemInstruction"]["parts"][0]["text"], "sys");
        assert_eq!(b["contents"][1]["role"], "model");
        assert_eq!(b["contents"][1]["parts"][0]["thoughtSignature"], "sig");
        assert_eq!(
            b["contents"][2]["parts"][0]["functionResponse"]["response"]["content"],
            "ok"
        );
        assert_eq!(
            b["contents"][2]["parts"][1]["inlineData"]["mimeType"],
            "image/jpeg"
        );
        assert_eq!(b["generationConfig"]["maxOutputTokens"], 800);
        assert_eq!(
            b["tools"][0]["functionDeclarations"][0]["name"],
            "view_screen"
        );
    }
}
