//! Routes calls to the right adapter for a provider kind.

use std::time::Duration;

use serde::Serialize;
use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;
use ts_rs::TS;

use super::error::{ErrorKind, ProviderError};
use super::openai::Dialect;
use super::types::*;
use super::{anthropic, gemini, openai, sse};
use crate::settings::schema::{ProviderConfig, ProviderKind};

pub fn http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .user_agent(concat!("Helpy/", env!("CARGO_PKG_VERSION")))
        .build()
        .expect("HTTP client")
}

fn require_key<'a>(p: &ProviderConfig, key: Option<&'a str>) -> Result<&'a str, ProviderError> {
    key.filter(|k| !k.is_empty()).ok_or_else(|| {
        ProviderError::new(
            ErrorKind::Auth,
            format!(
                "{} has no API key yet. Add one in Settings → AI providers",
                p.name
            ),
        )
    })
}

pub async fn stream_chat(
    http: &reqwest::Client,
    p: &ProviderConfig,
    key: Option<&str>,
    req: &ChatRequest,
    idle: Duration,
    cancel: &CancellationToken,
    on_text: &mut (dyn FnMut(&str) + Send),
) -> Result<Completion, ProviderError> {
    match p.kind {
        ProviderKind::Anthropic => {
            anthropic::stream(
                http,
                &p.base_url,
                require_key(p, key)?,
                req,
                idle,
                cancel,
                on_text,
            )
            .await
        }
        ProviderKind::Gemini => {
            gemini::stream(
                http,
                &p.base_url,
                require_key(p, key)?,
                req,
                idle,
                cancel,
                on_text,
            )
            .await
        }
        ProviderKind::OpenAi => {
            openai::stream(
                http,
                &p.base_url,
                Some(require_key(p, key)?),
                Dialect::OpenAi,
                req,
                idle,
                cancel,
                on_text,
            )
            .await
        }
        _ => {
            openai::stream(
                http,
                &p.base_url,
                key,
                Dialect::Compatible,
                req,
                idle,
                cancel,
                on_text,
            )
            .await
        }
    }
}

pub async fn list_models(
    http: &reqwest::Client,
    p: &ProviderConfig,
    key: Option<&str>,
) -> Result<Vec<ModelInfo>, ProviderError> {
    let mut models = match p.kind {
        ProviderKind::Anthropic => {
            anthropic::list_models(http, &p.base_url, require_key(p, key)?).await?
        }
        ProviderKind::Gemini => {
            gemini::list_models(http, &p.base_url, require_key(p, key)?).await?
        }
        ProviderKind::OpenAi => {
            openai::list_models(
                http,
                &p.base_url,
                Some(require_key(p, key)?),
                Dialect::OpenAi,
            )
            .await?
        }
        ProviderKind::Nebius => nebius_models(http, &p.base_url, require_key(p, key)?).await?,
        ProviderKind::Ollama => ollama_models(http, &p.base_url).await?,
        ProviderKind::LmStudio => match lmstudio_models(http, &p.base_url).await {
            Ok(m) => m,
            Err(_) => openai::list_models(http, &p.base_url, key, Dialect::Compatible).await?,
        },
        _ => openai::list_models(http, &p.base_url, key, Dialect::Compatible).await?,
    };
    for m in &mut models {
        if m.vision.is_none() && p.kind == ProviderKind::OpenAi {
            m.vision = Some(openai_has_vision(&m.id));
            m.tools = Some(true);
        }
    }
    models.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(models)
}

/// OpenAI's list doesn't say which models read images.
fn openai_has_vision(id: &str) -> bool {
    [
        "gpt-4o",
        "gpt-4.1",
        "gpt-4-turbo",
        "gpt-5",
        "o1",
        "o3",
        "o4",
    ]
    .iter()
    .any(|p| id.starts_with(p))
}

/// ".../v1" → "...", for the native (non-OpenAI) endpoints of local servers.
fn root(base: &str) -> &str {
    base.trim_end_matches('/').trim_end_matches("/v1")
}

async fn get_json(http: &reqwest::Client, url: &str) -> Result<Value, ProviderError> {
    let r = http
        .get(url)
        .send()
        .await
        .map_err(|e| ProviderError::from_reqwest(&e))?;
    sse::check_status(r)
        .await?
        .json()
        .await
        .map_err(|e| ProviderError::from_reqwest(&e))
}

/// Ollama's native API reports each model's capabilities.
async fn ollama_models(
    http: &reqwest::Client,
    base: &str,
) -> Result<Vec<ModelInfo>, ProviderError> {
    let tags = get_json(http, &format!("{}/api/tags", root(base))).await?;
    let mut out = Vec::new();
    for m in tags["models"].as_array().into_iter().flatten() {
        let Some(name) = m["name"].as_str() else {
            continue;
        };
        let caps = http
            .post(format!("{}/api/show", root(base)))
            .json(&json!({ "model": name }))
            .send()
            .await
            .ok()
            .and_then(|r| r.status().is_success().then_some(r));
        let caps: Option<Vec<String>> = match caps {
            Some(r) => r
                .json::<Value>()
                .await
                .ok()
                .and_then(|v| serde_json::from_value(v["capabilities"].clone()).ok()),
            None => None,
        };
        out.push(ModelInfo {
            id: name.to_string(),
            vision: caps.as_ref().map(|c| c.iter().any(|x| x == "vision")),
            tools: caps.as_ref().map(|c| c.iter().any(|x| x == "tools")),
            ..Default::default()
        });
    }
    Ok(out)
}

/// Nebius lists what each model reads and costs only when asked for detail.
async fn nebius_models(
    http: &reqwest::Client,
    base: &str,
    key: &str,
) -> Result<Vec<ModelInfo>, ProviderError> {
    let r = http
        .get(format!("{}/models?verbose=true", base.trim_end_matches('/')))
        .bearer_auth(key)
        .send()
        .await
        .map_err(|e| ProviderError::from_reqwest(&e))?;
    let v: Value = sse::check_status(r)
        .await?
        .json()
        .await
        .map_err(|e| ProviderError::from_reqwest(&e))?;
    Ok(parse_nebius_models(&v))
}

/// Chat models only: `modality` is like "text+image->text", and embedding
/// models answer with something other than text. Prices are per token.
fn parse_nebius_models(v: &Value) -> Vec<ModelInfo> {
    let per_million = |p: &Value| {
        let usd: f64 = p.as_str()?.parse().ok()?;
        Some((usd * 1e12).round() / 1e6)
    };
    v["data"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|m| {
            let modality = m["architecture"]["modality"].as_str();
            let (input, output) = modality.map_or(("", "text"), |s| s.split_once("->").unwrap_or((s, "")));
            if output != "text" {
                return None;
            }
            Some(ModelInfo {
                id: m["id"].as_str()?.to_string(),
                vision: modality.map(|_| input.contains("image")),
                tools: None,
                input_price: per_million(&m["pricing"]["prompt"]),
                output_price: per_million(&m["pricing"]["completion"]),
            })
        })
        .collect()
}

/// LM Studio's REST API marks vision models as type "vlm".
async fn lmstudio_models(
    http: &reqwest::Client,
    base: &str,
) -> Result<Vec<ModelInfo>, ProviderError> {
    let v = get_json(http, &format!("{}/api/v0/models", root(base))).await?;
    Ok(v["data"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|m| m["type"] != "embeddings")
        .filter_map(|m| {
            Some(ModelInfo {
                id: m["id"].as_str()?.to_string(),
                vision: Some(m["type"] == "vlm"),
                tools: m["capabilities"]
                    .as_array()
                    .map(|c| c.iter().any(|x| x == "tool_use")),
                ..Default::default()
            })
        })
        .collect())
}

#[derive(Serialize, TS, Clone, Debug)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct LocalServer {
    pub kind: ProviderKind,
    pub base_url: String,
    pub models: Vec<ModelInfo>,
}

/// Looks for Ollama and LM Studio on their default ports.
pub async fn detect_local(http: &reqwest::Client) -> Vec<LocalServer> {
    let quick = reqwest::Client::builder()
        .timeout(Duration::from_millis(1500))
        .build()
        .unwrap_or_else(|_| http.clone());
    let candidates = [
        (ProviderKind::Ollama, "http://localhost:11434/v1"),
        (ProviderKind::LmStudio, "http://localhost:1234/v1"),
    ];
    let mut found = Vec::new();
    for (kind, base_url) in candidates {
        let p = ProviderConfig {
            kind,
            base_url: base_url.into(),
            ..Default::default()
        };
        if let Ok(models) = list_models(&quick, &p, None).await {
            found.push(LocalServer {
                kind,
                base_url: base_url.into(),
                models,
            });
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_nebius_vision_and_prices_and_skips_embeddings() {
        let v = json!({"data": [
            {"id": "moonshotai/Kimi-K3", "architecture": {"modality": "text+image->text"},
             "pricing": {"prompt": "0.000003", "completion": "0.000015"}},
            {"id": "nvidia/nemotron-3-super-120b-a12b", "architecture": {"modality": "text->text"},
             "pricing": {"prompt": "0.0000003", "completion": "0.0000009"}},
            {"id": "Qwen/Qwen3-Embedding-8B", "architecture": {"modality": "text->embedding"}},
            {"id": "new/model"}
        ]});
        let models = parse_nebius_models(&v);
        let ids: Vec<_> = models.iter().map(|m| m.id.as_str()).collect();
        assert_eq!(ids, ["moonshotai/Kimi-K3", "nvidia/nemotron-3-super-120b-a12b", "new/model"]);
        assert_eq!(models[0].vision, Some(true));
        assert_eq!((models[0].input_price, models[0].output_price), (Some(3.0), Some(15.0)));
        assert_eq!(models[1].vision, Some(false));
        assert_eq!((models[1].input_price, models[1].output_price), (Some(0.3), Some(0.9)));
        // Nothing said: leave it to the user.
        assert_eq!(models[2], ModelInfo { id: "new/model".into(), ..Default::default() });
    }
}
