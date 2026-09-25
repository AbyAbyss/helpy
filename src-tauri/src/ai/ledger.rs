//! Token and cost spending per day, saved to disk after every call so budgets
//! survive restarts.

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::error::{ErrorKind, ProviderError};
use super::types::Usage;
use crate::settings::schema::{Limits, ModelConfig, ProviderKind};

const KEEP_DAYS: usize = 90;

#[derive(Serialize, Deserialize, Default, Clone, Debug, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct DayUsage {
    pub tokens: u64,
    /// Dollars, counting only calls whose model has a price.
    pub cost: f64,
    /// Per "feature · provider · model".
    pub breakdown: BTreeMap<String, Entry>,
}

#[derive(Serialize, Deserialize, Default, Clone, Debug, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Entry {
    pub calls: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_write_tokens: u64,
    pub cost: f64,
}

/// Today's spending, for the settings page.
#[derive(Serialize, TS, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct UsageToday {
    #[ts(type = "number")]
    pub calls: u64,
    /// Every token processed, cached ones included.
    #[ts(type = "number")]
    pub tokens: u64,
    #[ts(type = "number")]
    pub input_tokens: u64,
    #[ts(type = "number")]
    pub output_tokens: u64,
    #[ts(type = "number")]
    pub cache_read_tokens: u64,
    #[ts(type = "number")]
    pub cache_write_tokens: u64,
    pub cost: f64,
}

/// Spending over the last days, for the usage chart.
#[derive(Serialize, TS, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct UsageHistory {
    /// One per day, oldest first, days without calls included.
    pub days: Vec<DayTotal>,
    /// Biggest first.
    pub features: Vec<Total>,
    pub models: Vec<Total>,
}

#[derive(Serialize, TS, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct DayTotal {
    pub date: String,
    #[ts(type = "number")]
    pub tokens: u64,
    pub cost: f64,
}

#[derive(Serialize, TS, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Total {
    pub name: String,
    #[ts(type = "number")]
    pub calls: u64,
    #[ts(type = "number")]
    pub tokens: u64,
    #[ts(type = "number")]
    pub input_tokens: u64,
    #[ts(type = "number")]
    pub output_tokens: u64,
    #[ts(type = "number")]
    pub cache_read_tokens: u64,
    #[ts(type = "number")]
    pub cache_write_tokens: u64,
    pub cost: f64,
}

/// What a ledger key's feature is called in the chart. Agents are named by
/// the agent, so they're counted together.
fn feature_name(feature: &str) -> &'static str {
    match feature {
        "ask" => "Questions and guidance",
        "circle" => "Circle to explain",
        "agent planning" => "Planning agents",
        f if f.starts_with("agent ") => "Agents",
        _ => "Other",
    }
}

pub struct Ledger {
    path: Option<PathBuf>,
    days: Mutex<BTreeMap<String, DayUsage>>,
}

/// What a call cost, from the model's prices per million tokens. Cached
/// input is billed at each provider's published fraction of the input price:
/// Anthropic reads at 0.1 and writes at 1.25, OpenAI reads at 0.5, Gemini
/// reads at 0.25. Other servers are local or unknown, so cached tokens cost
/// the same as the rest.
pub fn price(model: &ModelConfig, kind: ProviderKind, usage: Usage) -> Option<f64> {
    let (read, write) = match kind {
        ProviderKind::Anthropic => (0.1, 1.25),
        ProviderKind::OpenAi => (0.5, 1.0),
        ProviderKind::Gemini => (0.25, 1.0),
        _ => (1.0, 1.0),
    };
    let input = model.input_price?;
    Some(
        (usage.input_tokens as f64 * input
            + usage.cache_read_tokens as f64 * input * read
            + usage.cache_write_tokens as f64 * input * write
            + usage.output_tokens as f64 * model.output_price?)
            / 1_000_000.0,
    )
}

fn today() -> String {
    chrono::Local::now().format("%Y-%m-%d").to_string()
}

impl Ledger {
    pub fn load(path: PathBuf) -> Self {
        if let Some(dir) = path.parent() {
            let _ = fs::create_dir_all(dir);
        }
        let days = fs::read_to_string(&path)
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default();
        Self {
            path: Some(path),
            days: Mutex::new(days),
        }
    }

    #[cfg(test)]
    pub fn in_memory() -> Self {
        Self {
            path: None,
            days: Mutex::new(BTreeMap::new()),
        }
    }

    pub fn today(&self) -> UsageToday {
        let d = self
            .days
            .lock()
            .unwrap()
            .get(&today())
            .cloned()
            .unwrap_or_default();
        let mut t = UsageToday {
            tokens: d.tokens,
            cost: d.cost,
            ..Default::default()
        };
        for e in d.breakdown.values() {
            t.calls += e.calls;
            t.input_tokens += e.input_tokens;
            t.output_tokens += e.output_tokens;
            t.cache_read_tokens += e.cache_read_tokens;
            t.cache_write_tokens += e.cache_write_tokens;
        }
        t
    }

    /// The last `n` days up to `last` (a YYYY-MM-DD date).
    pub fn history(&self, last: chrono::NaiveDate, n: u32) -> UsageHistory {
        let days = self.days.lock().unwrap();
        let mut out = UsageHistory {
            days: Vec::new(),
            features: Vec::new(),
            models: Vec::new(),
        };
        let add = |list: &mut Vec<Total>, name: String, e: &Entry| {
            let t = match list.iter_mut().find(|t| t.name == name) {
                Some(t) => t,
                None => {
                    list.push(Total {
                        name,
                        calls: 0,
                        tokens: 0,
                        input_tokens: 0,
                        output_tokens: 0,
                        cache_read_tokens: 0,
                        cache_write_tokens: 0,
                        cost: 0.0,
                    });
                    list.last_mut().unwrap()
                }
            };
            t.calls += e.calls;
            t.tokens += e.input_tokens + e.output_tokens + e.cache_read_tokens + e.cache_write_tokens;
            t.input_tokens += e.input_tokens;
            t.output_tokens += e.output_tokens;
            t.cache_read_tokens += e.cache_read_tokens;
            t.cache_write_tokens += e.cache_write_tokens;
            t.cost += e.cost;
        };
        for i in (0..n.max(1)).rev() {
            let date = (last - chrono::Days::new(i as u64))
                .format("%Y-%m-%d")
                .to_string();
            let d = days.get(&date).cloned().unwrap_or_default();
            for (key, e) in &d.breakdown {
                let mut parts = key.split(" · ");
                let feature = parts.next().unwrap_or_default();
                let provider = parts.next().unwrap_or_default();
                let model = parts.next().unwrap_or_default();
                add(&mut out.features, feature_name(feature).to_string(), e);
                add(&mut out.models, format!("{model} · {provider}"), e);
            }
            out.days.push(DayTotal {
                date,
                tokens: d.tokens,
                cost: d.cost,
            });
        }
        for list in [&mut out.features, &mut out.models] {
            list.sort_by(|a, b| b.tokens.cmp(&a.tokens));
        }
        out
    }

    pub fn record(&self, key: &str, usage: Usage, cost: Option<f64>) {
        self.record_on(&today(), key, usage, cost)
    }

    fn record_on(&self, day: &str, key: &str, usage: Usage, cost: Option<f64>) {
        let mut days = self.days.lock().unwrap();
        let d = days.entry(day.to_string()).or_default();
        d.tokens += usage.total();
        d.cost += cost.unwrap_or(0.0);
        let e = d.breakdown.entry(key.to_string()).or_default();
        e.calls += 1;
        e.input_tokens += usage.input_tokens;
        e.output_tokens += usage.output_tokens;
        e.cache_read_tokens += usage.cache_read_tokens;
        e.cache_write_tokens += usage.cache_write_tokens;
        e.cost += cost.unwrap_or(0.0);
        while days.len() > KEEP_DAYS {
            let oldest = days.keys().next().cloned().unwrap();
            days.remove(&oldest);
        }
        if let Some(path) = &self.path {
            if let Ok(bytes) = serde_json::to_vec(&*days) {
                let tmp = path.with_extension("json.tmp");
                if fs::write(&tmp, bytes)
                    .and_then(|_| fs::rename(&tmp, path))
                    .is_err()
                {
                    log::error!("couldn't save usage to {}", path.display());
                }
            }
        }
    }

    /// Errors when a call of `estimated` tokens would likely go over today's
    /// limits. `model` is the one about to be called, for its price.
    pub fn check(
        &self,
        limits: &Limits,
        estimated: u64,
        model: &ModelConfig,
    ) -> Result<(), ProviderError> {
        self.check_on(&today(), limits, estimated, model)
    }

    fn check_on(
        &self,
        day: &str,
        limits: &Limits,
        estimated: u64,
        model: &ModelConfig,
    ) -> Result<(), ProviderError> {
        let spent = self
            .days
            .lock()
            .unwrap()
            .get(day)
            .cloned()
            .unwrap_or_default();
        if limits.daily_token_budget > 0 && spent.tokens + estimated > limits.daily_token_budget {
            return Err(ProviderError::new(
                ErrorKind::Budget,
                format!(
                    "Today's token limit would be passed: {} of {} tokens used, and this request could take up to {}. Raise the limit in Settings → Usage & budgets",
                    thousands(spent.tokens),
                    thousands(limits.daily_token_budget),
                    thousands(estimated)
                ),
            ));
        }
        if let Some(budget) = limits.daily_cost_budget {
            // Worst case: every estimated token priced at the higher rate.
            let (Some(i), Some(o)) = (model.input_price, model.output_price) else {
                return Err(ProviderError::new(
                    ErrorKind::Budget,
                    format!(
                        "A daily cost limit is on, but {} has no price set, so Helpy can't tell what it costs. Add its price in Settings → AI providers, or turn the cost limit off in Usage & budgets",
                        model.id
                    ),
                ));
            };
            let worst = estimated as f64 * i.max(o) / 1_000_000.0;
            if spent.cost + worst > budget {
                return Err(ProviderError::new(
                    ErrorKind::Budget,
                    format!(
                        "Today's cost limit would be passed: ${:.2} of ${budget:.2} spent",
                        spent.cost
                    ),
                ));
            }
        }
        Ok(())
    }
}

fn thousands(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model(price: Option<f64>) -> ModelConfig {
        ModelConfig {
            id: "m".into(),
            input_price: price,
            output_price: price,
            ..Default::default()
        }
    }

    fn limits(tokens: u64, cost: Option<f64>) -> Limits {
        Limits {
            daily_token_budget: tokens,
            daily_cost_budget: cost,
            ..Default::default()
        }
    }

    #[test]
    fn history_fills_empty_days_and_groups_agents() {
        let l = Ledger::in_memory();
        let u = |i, o| Usage {
            input_tokens: i,
            output_tokens: o,
            ..Default::default()
        };
        l.record_on(
            "2026-09-20",
            "ask · Anthropic · sonnet",
            u(100, 50),
            Some(0.01),
        );
        l.record_on(
            "2026-09-22",
            "agent Desks · Ollama · qwen",
            u(300, 100),
            None,
        );
        l.record_on(
            "2026-09-22",
            "agent Chairs · Ollama · qwen",
            u(200, 0),
            None,
        );
        let last = chrono::NaiveDate::from_ymd_opt(2026, 9, 22).unwrap();
        let h = l.history(last, 3);
        let days: Vec<_> = h.days.iter().map(|d| (d.date.as_str(), d.tokens)).collect();
        assert_eq!(
            days,
            [("2026-09-20", 150), ("2026-09-21", 0), ("2026-09-22", 600)]
        );
        assert_eq!(
            (
                h.features[0].name.as_str(),
                h.features[0].calls,
                h.features[0].tokens
            ),
            ("Agents", 2, 600)
        );
        assert_eq!(h.models[1].name, "sonnet · Anthropic");
        assert_eq!(l.history(last, 2).days.len(), 2);
    }

    #[test]
    fn blocks_a_call_that_would_pass_the_token_limit() {
        let l = Ledger::in_memory();
        l.record_on(
            "2026-09-24",
            "ask",
            Usage {
                input_tokens: 900,
                output_tokens: 50,
                ..Default::default()
            },
            None,
        );
        let lim = limits(1000, None);
        assert!(l.check_on("2026-09-24", &lim, 50, &model(None)).is_ok());
        let e = l
            .check_on("2026-09-24", &lim, 51, &model(None))
            .unwrap_err();
        assert_eq!(e.kind, ErrorKind::Budget);
        assert!(e.message.contains("950 of 1,000"), "{}", e.message);
        // A new day starts fresh.
        assert!(l.check_on("2026-09-25", &lim, 900, &model(None)).is_ok());
    }

    #[test]
    fn zero_turns_the_token_limit_off() {
        let l = Ledger::in_memory();
        l.record_on(
            "d",
            "ask",
            Usage {
                input_tokens: 10_000_000,
                output_tokens: 0,
                ..Default::default()
            },
            None,
        );
        assert!(l
            .check_on("d", &limits(0, None), 1_000_000, &model(None))
            .is_ok());
    }

    #[test]
    fn cost_limit_uses_prices_and_refuses_unpriced_models() {
        let l = Ledger::in_memory();
        let lim = limits(0, Some(1.0));
        // $10 per million tokens: 90k tokens = $0.90.
        l.record_on(
            "d",
            "ask",
            Usage {
                input_tokens: 90_000,
                output_tokens: 0,
                ..Default::default()
            },
            Some(0.9),
        );
        assert!(l.check_on("d", &lim, 10_000, &model(Some(10.0))).is_ok());
        assert_eq!(
            l.check_on("d", &lim, 10_001, &model(Some(10.0)))
                .unwrap_err()
                .kind,
            ErrorKind::Budget
        );
        let e = l.check_on("d", &lim, 1, &model(None)).unwrap_err();
        assert!(e.message.contains("no price"));
        // Free local models pass with a price of 0.
        assert!(l.check_on("d", &lim, 1_000_000, &model(Some(0.0))).is_ok());
    }

    #[test]
    fn spending_survives_a_restart() {
        let path = std::env::temp_dir().join(format!("helpy-ledger-{}.json", std::process::id()));
        let _ = fs::remove_file(&path);
        {
            let l = Ledger::load(path.clone());
            l.record(
                "ask · local · llama",
                Usage {
                    input_tokens: 100,
                    output_tokens: 20,
                ..Default::default()
            },
                Some(0.5),
            );
            l.record(
                "ask · local · llama",
                Usage {
                    input_tokens: 10,
                    output_tokens: 2,
                ..Default::default()
            },
                None,
            );
        }
        let l = Ledger::load(path.clone());
        let t = l.today();
        assert_eq!(t.tokens, 132);
        assert_eq!(t.cost, 0.5);
        assert_eq!(
            l.days.lock().unwrap()[&today()].breakdown["ask · local · llama"].calls,
            2
        );
        let _ = fs::remove_file(path);
    }

    #[test]
    fn prices_are_per_million_tokens() {
        let m = ModelConfig {
            input_price: Some(5.0),
            output_price: Some(25.0),
            ..Default::default()
        };
        assert_eq!(
            price(
                &m,
                ProviderKind::OpenAiCompatible,
                Usage {
                    input_tokens: 1_000_000,
                    output_tokens: 100_000,
                    ..Default::default()
                }
            ),
            Some(7.5)
        );
        assert_eq!(price(&model(None), ProviderKind::Anthropic, Usage::default()), None);
    }

    #[test]
    fn cached_input_is_priced_at_the_providers_fraction() {
        let m = ModelConfig {
            input_price: Some(10.0),
            output_price: Some(0.0),
            ..Default::default()
        };
        let u = Usage {
            cache_read_tokens: 1_000_000,
            cache_write_tokens: 1_000_000,
            ..Default::default()
        };
        assert_eq!(price(&m, ProviderKind::Anthropic, u), Some(1.0 + 12.5));
        assert_eq!(price(&m, ProviderKind::OpenAi, u), Some(5.0 + 10.0));
        assert_eq!(price(&m, ProviderKind::Ollama, u), Some(20.0));
    }
}
