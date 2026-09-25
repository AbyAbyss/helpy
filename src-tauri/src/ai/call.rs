//! One streamed model call through the retry, fallback and budget limits.
//! Every feature that talks to a model goes through here, so the limits hold
//! everywhere.

use std::time::Duration;

use tauri::{AppHandle, Manager};
use tokio_util::sync::CancellationToken;

use super::ask::AiState;
use super::error::{ErrorKind, ProviderError};
use super::limits::{self, Policy};
use super::types::{ChatRequest, Completion, Usage};
use super::{ledger, provider, secrets};
use crate::settings::schema::{Ai, ModelConfig, ModelRef, ProviderConfig};
use crate::settings::Settings;

/// Progress of a call, as it happens.
pub enum Progress<'a> {
    /// An attempt started on this model ("model · provider"). Text from an
    /// earlier attempt is void.
    Started(&'a str),
    Text(&'a str),
    /// An attempt was billed (or may have been): tokens and cost, if the
    /// model has a price. Failed attempts count too.
    Spent {
        tokens: u64,
        cost: Option<f64>,
    },
    /// The last attempt failed and another follows after `wait`.
    Retry {
        retry: u32,
        limit: u32,
        reason: String,
        wait: Duration,
        model: String,
    },
}

/// "model · provider" for messages.
pub fn describe(ai: &Ai, r: &ModelRef) -> String {
    match ai.model(r) {
        Some((p, _)) => format!("{} · {}", r.model, p.name),
        None => r.model.clone(),
    }
}

/// Streams `base` to the first of `models` that works. `feature` names the
/// spending in the usage ledger. `base.model` is filled in per attempt.
pub async fn stream(
    app: &AppHandle,
    settings: &Settings,
    feature: &str,
    models: &[ModelRef],
    base: ChatRequest,
    cancel: &CancellationToken,
    on: &(dyn Fn(Progress) + Sync),
) -> Result<Completion, ProviderError> {
    let models = &crate::privacy::allowed_models(settings, models)?;
    let ai_state: &AiState = app.state::<AiState>().inner();
    let ai = &settings.ai;
    let estimate = base.estimated_tokens();
    let idle = Duration::from_secs(ai.timeout_secs as u64);
    let lookup = |r: &ModelRef| -> (ProviderConfig, ModelConfig) {
        let (p, m) = ai.model(r).expect("callers pass configured models");
        (p.clone(), m.clone())
    };

    let result = limits::run_step(
        &Policy::from(&settings.limits),
        models,
        cancel,
        |r| {
            ai_state
                .ledger
                .check(&settings.limits, estimate, &lookup(r).1)
        },
        |r| {
            let (p, m) = lookup(r);
            let req = ChatRequest {
                model: m.id.clone(),
                ..base.clone()
            };
            let key_name = format!("{feature} · {} · {}", p.name, m.id);
            async move {
                let key = secrets::key_for(&p)?;
                on(Progress::Started(&format!("{} · {}", m.id, p.name)));
                let mut on_text = |piece: &str| on(Progress::Text(piece));
                let result = provider::stream_chat(
                    &ai_state.http,
                    &p,
                    key.as_deref(),
                    &req,
                    idle,
                    cancel,
                    &mut on_text,
                )
                .await;
                match &result {
                    Ok(c) => {
                        let cost = ledger::price(&m, p.kind, c.usage);
                        ai_state.ledger.record(&key_name, c.usage, cost);
                        on(Progress::Spent {
                            tokens: c.usage.total(),
                            cost,
                        });
                    }
                    // The provider may have processed (and billed) the input.
                    Err(e)
                        if matches!(
                            e.kind,
                            ErrorKind::Timeout
                                | ErrorKind::Network
                                | ErrorKind::Server
                                | ErrorKind::Malformed
                                | ErrorKind::Refused
                        ) =>
                    {
                        let usage = Usage {
                            input_tokens: estimate - req.max_tokens as u64,
                            ..Default::default()
                        };
                        let cost = ledger::price(&m, p.kind, usage);
                        ai_state.ledger.record(&key_name, usage, cost);
                        on(Progress::Spent {
                            tokens: usage.total(),
                            cost,
                        });
                    }
                    Err(_) => {}
                }
                result
            }
        },
        |n| {
            on(Progress::Retry {
                retry: n.retry,
                limit: n.max_retries,
                reason: n.reason,
                wait: n.wait,
                model: describe(ai, &n.next),
            })
        },
    )
    .await;

    result.map_err(|f| {
        let mut e = f.error;
        if f.attempts > 1 {
            e.message = format!("Tried {} times. {}", f.attempts, e.message);
        }
        e
    })
}
