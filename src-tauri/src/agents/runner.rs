//! Runs one agent until it finishes, stops at a limit, fails or is cancelled.
//! Every limit is enforced here in Rust: steps, tool calls, time, the agent,
//! batch and daily budgets, repeat and no-progress detection, and step
//! retries. The model is never trusted to stop by itself.
//!
//! The runner talks to the outside world only through `Env`, so the tests
//! below drive it with a scripted fake provider and fake tools.

use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use futures_util::future::BoxFuture;
use serde_json::Value;
use tokio_util::sync::CancellationToken;

use super::model::*;
use crate::ai::call::Progress;
use crate::ai::error::{ErrorKind, ProviderError};
use crate::ai::limits::Policy;
use crate::ai::types::{ChatRequest, Completion, Message, Part, Role, ToolDef};
use crate::settings::schema::OnFailure;

/// Tool output kept in the conversation, in characters.
pub(crate) const MAX_TOOL_OUTPUT: usize = 8_000;
/// Status lines change at most this often.
const STATUS_EVERY_MS: i64 = 3_000;
/// Messages kept verbatim when older work is summarized.
const KEEP_RECENT: usize = 6;
/// Tool results sent to the model in full; older, longer ones are cleared.
const KEEP_RESULTS: usize = 2;
/// Tool results up to this many characters are always sent in full.
const SMALL_RESULT: usize = 800;
pub const ASK_USER: &str = "ask_user";
/// The agent's checklist and scratchpad: handled here, since they change
/// the agent itself.
pub const PLAN: &str = "plan";
pub const NOTE: &str = "note";
/// Most text kept in an agent's notes; the oldest go first.
const MAX_NOTES: usize = 8_000;
/// Hands parts of the work to helper agents and waits for their results.
pub const DELEGATE: &str = "delegate";

#[derive(Clone, Debug)]
pub struct Limits {
    pub max_steps: u32,
    pub max_tool_calls: u32,
    pub repeat: u32,
    pub no_progress: u32,
    pub time: Duration,
    /// 0 is off.
    pub agent_tokens: u64,
    pub agent_cost: Option<f64>,
    pub batch_tokens: u64,
    pub batch_cost: Option<f64>,
    pub context_tokens: u64,
    pub on_failure: OnFailure,
    /// Retries and backoff for tool errors that may pass next time.
    pub policy: Policy,
    pub max_response_tokens: u32,
    pub temperature: f64,
}

/// Whether an action may go ahead.
pub enum Gate {
    Allow,
    Ask {
        kind: ActionKind,
        source: String,
        summary: String,
        detail: String,
        /// Fields of the input the user may change before approving.
        editable: Vec<String>,
    },
    Never(String),
}

#[derive(Debug)]
pub enum ToolOutcome {
    Ok {
        text: String,
        ops: Vec<FileOp>,
    },
    /// May pass on a retry (a timeout, a network error).
    Transient(String),
    /// Will fail the same way again (not found, not allowed).
    Permanent(String),
}

pub trait Env: Sync {
    /// One model call with its own retries and fallbacks.
    fn call<'a>(
        &'a self,
        agent: &'a Agent,
        req: ChatRequest,
        on: &'a (dyn Fn(Progress) + Sync),
    ) -> BoxFuture<'a, Result<Completion, ProviderError>>;
    fn gate(&self, agent: &Agent, tool: &str, args: &Value) -> Gate;
    fn run_tool<'a>(
        &'a self,
        agent: &'a Agent,
        tool: &'a str,
        args: &'a Value,
    ) -> BoxFuture<'a, ToolOutcome>;
    /// Waits for the user's reply to `agent.pending`.
    fn answer<'a>(&'a self, agent: &'a Agent) -> BoxFuture<'a, Answer>;
    /// Tokens and cost spent by the other agents of this agent's batch.
    fn batch_spent(&self, agent: &Agent) -> (u64, f64);
    /// Worst-case cost of `tokens` on the agent's model; None if unpriced.
    fn price(&self, agent: &Agent, tokens: u64) -> Option<f64>;
    fn system(&self, agent: &Agent) -> String;
    fn tools(&self, agent: &Agent) -> Vec<ToolDef>;
    /// Saves the agent and shows the change.
    fn save(&self, agent: &Agent);
    /// Shows a passing status (a retry) without saving.
    fn live(&self, agent: &Agent, line: &str);
    fn now_ms(&self) -> i64;
    fn sleep(&self, d: Duration) -> BoxFuture<'static, ()>;
    /// What the user said to the agent while it works (steering), oldest
    /// first. Each message is returned once.
    fn steering(&self, agent: &Agent) -> Vec<String>;
    /// Starts helper agents for `tasks` (or picks up the ones this call
    /// already started) and waits for them. The text is their results.
    fn delegate<'a>(
        &'a self,
        agent: &'a Agent,
        call_id: &'a str,
        tasks: &'a Value,
    ) -> BoxFuture<'a, Result<String, String>>;
}

/// The plan tool's items, checked: text on each, at most MAX_PLAN of them.
fn plan_items(items: &Value) -> Result<Vec<PlanItem>, String> {
    let bad = || {
        "Give the plan as a list of items, each with text and a status (todo, doing or done)."
            .to_string()
    };
    let mut plan: Vec<PlanItem> = serde_json::from_value(items.clone()).map_err(|_| bad())?;
    plan.retain_mut(|i| {
        i.text = i.text.trim().to_string();
        !i.text.is_empty()
    });
    if plan.is_empty() {
        return Err(bad());
    }
    plan.truncate(super::tools::MAX_PLAN);
    Ok(plan)
}

/// Adds text for the model as the user, joining the last message when it's
/// already the user's (providers want the roles to alternate).
fn push_user_text(agent: &mut Agent, text: String) {
    match agent.messages.last_mut() {
        Some(m) if m.role == Role::User => m.parts.push(Part::Text(text)),
        _ => agent.messages.push(Message::user_text(text)),
    }
}

/// Tool calls in the last message that never got a result: the agent was
/// paused or Helpy closed while they ran.
fn dangling_calls(agent: &Agent) -> Vec<(String, String, Value)> {
    match agent.messages.last() {
        Some(m) if m.role == Role::Assistant => m
            .parts
            .iter()
            .filter_map(|p| match p {
                Part::ToolUse {
                    id, name, input, ..
                } => Some((id.clone(), name.clone(), input.clone())),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

/// FNV-1a: a hash that stays the same across runs, for saved progress checks.
pub fn stable_hash(s: &str) -> u64 {
    s.bytes().fold(0xcbf2_9ce4_8422_2325, |h, b| {
        (h ^ b as u64).wrapping_mul(0x0100_0000_01b3)
    })
}

/// JSON with object keys sorted, so equal inputs compare equal.
pub fn canonical(v: &Value) -> String {
    fn sort(v: &Value) -> Value {
        match v {
            Value::Object(m) => {
                let mut keys: Vec<_> = m.keys().collect();
                keys.sort();
                let mut out = serde_json::Map::new();
                for k in keys {
                    out.insert(k.clone(), sort(&m[k]));
                }
                Value::Object(out)
            }
            Value::Array(a) => Value::Array(a.iter().map(sort).collect()),
            other => other.clone(),
        }
    }
    sort(v).to_string()
}

pub(crate) fn truncate(s: &str, max: usize) -> String {
    match s.char_indices().nth(max) {
        Some((i, _)) => format!("{}\n[… {} more characters cut]", &s[..i], s.len() - i),
        None => s.to_string(),
    }
}

/// The first sentence or two of what the agent said, for its status line.
pub fn status_from(text: &str) -> Option<String> {
    let t = text.trim();
    if t.is_empty() {
        return None;
    }
    let mut end = t.len();
    let mut sentences = 0;
    for (i, c) in t.char_indices() {
        if matches!(c, '.' | '!' | '?' | '\n') {
            sentences += 1;
            if sentences == 2 || c == '\n' {
                end = i + c.len_utf8();
                break;
            }
        }
    }
    let line = t[..end].trim();
    (!line.is_empty()).then(|| truncate(line, 180))
}

/// Splits "NEXT: a | b" suggestions off the end of a final answer.
pub fn split_suggestions(text: &str) -> (String, Vec<String>) {
    let t = text.trim_end();
    if let Some(i) = t.rfind("NEXT:") {
        if t[i..].lines().count() == 1 {
            let list = t[i + 5..]
                .split('|')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .take(3)
                .collect();
            return (t[..i].trim_end().to_string(), list);
        }
    }
    (t.to_string(), Vec::new())
}

enum Flow {
    Continue,
    Exit,
}

struct Run<'a> {
    env: &'a dyn Env,
    lim: &'a Limits,
    cancel: &'a CancellationToken,
    /// When this session's time runs out, in env milliseconds, before
    /// adding the time spent waiting for the user.
    deadline: i64,
    session_start: i64,
    base_active: u64,
    /// Waiting for the user doesn't count toward the time limit.
    waited: AtomicI64,
}

impl Run<'_> {
    /// Time left before the time limit.
    fn left(&self) -> Duration {
        let end = self.deadline + self.waited.load(Ordering::SeqCst);
        Duration::from_millis((end - self.env.now_ms()).max(0) as u64)
    }

    fn stop(&self, agent: &mut Agent, limit: Limit, message: String) -> Flow {
        let now = self.env.now_ms();
        agent.log(now, LogKind::Error, message.clone());
        agent.status = Status::Stopped;
        agent.status_line = message.clone();
        agent.stop = Some(Stop { limit, message });
        agent.finished = Some(now);
        agent.unseen = true;
        self.save(agent);
        Flow::Exit
    }

    fn fail(&self, agent: &mut Agent, message: String) -> Flow {
        let now = self.env.now_ms();
        agent.log(now, LogKind::Error, message.clone());
        agent.status = Status::Failed;
        agent.status_line = message.clone();
        agent.error = Some(message);
        agent.finished = Some(now);
        agent.unseen = true;
        self.save(agent);
        Flow::Exit
    }

    fn save(&self, agent: &mut Agent) {
        let ran = self.env.now_ms() - self.session_start - self.waited.load(Ordering::SeqCst);
        agent.active_ms = self.base_active + ran.max(0) as u64;
        self.env.save(agent);
    }

    fn set_status_line(&self, agent: &mut Agent, line: String) {
        let now = self.env.now_ms();
        if agent.status_line.is_empty() || now - agent.status_at >= STATUS_EVERY_MS {
            agent.status_line = line;
            agent.status_at = now;
        }
    }

    /// Waits for the user, but not past the time limit or a cancel.
    async fn wait(&self, agent: &mut Agent, pending: Pending, status: Status) -> Option<Answer> {
        agent.pending = Some(pending);
        agent.status = status;
        agent.unseen = true;
        self.save(agent);
        let from = self.env.now_ms();
        let answer = tokio::select! {
            _ = self.cancel.cancelled() => None,
            a = self.env.answer(agent) => Some(a),
        };
        self.waited
            .fetch_add(self.env.now_ms() - from, Ordering::SeqCst);
        agent.pending = None;
        agent.status = Status::Running;
        answer
    }

    /// A model call inside the time limit, with its spending recorded.
    async fn call(&self, agent: &mut Agent, req: ChatRequest) -> Result<Completion, ProviderError> {
        let spent = Mutex::new((0u64, 0f64, true));
        let on = |p: Progress| match p {
            Progress::Spent { tokens, cost } => {
                let mut s = spent.lock().unwrap();
                s.0 += tokens;
                match cost {
                    Some(c) => s.1 += c,
                    None => s.2 = false,
                }
            }
            Progress::Retry {
                retry,
                limit,
                reason,
                ..
            } => self
                .env
                .live(agent, &format!("Retry {retry} of {limit}: {reason}")),
            _ => {}
        };
        let left = self.left();
        let result = tokio::select! {
            r = self.env.call(agent, req, &on) => r,
            _ = self.env.sleep(left) => Err(ProviderError::new(ErrorKind::Timeout, "time limit")),
        };
        let (tokens, cost, known) = *spent.lock().unwrap();
        agent.counters.tokens += tokens;
        agent.counters.cost += cost;
        agent.counters.cost_known &= known;
        result
    }

    /// Stops the agent if a call of `estimate` tokens could pass a budget.
    fn over_budget(&self, agent: &mut Agent, estimate: u64) -> Option<Flow> {
        let c = &agent.counters;
        let lim = self.lim;
        if lim.agent_tokens > 0 && c.tokens + estimate > lim.agent_tokens + c.extra_tokens {
            let msg = format!(
                "Stopped at its token budget: {} of {} tokens used, and the next step could take up to {}.",
                c.tokens,
                lim.agent_tokens + c.extra_tokens,
                estimate
            );
            return Some(self.stop(agent, Limit::AgentBudget, msg));
        }
        let (batch_tokens, batch_cost) = self.env.batch_spent(agent);
        if lim.batch_tokens > 0
            && batch_tokens + c.tokens + estimate > lim.batch_tokens + c.extra_tokens
        {
            let msg = format!(
                "Stopped at the batch's token budget: {} of {} tokens used by this batch.",
                batch_tokens + c.tokens,
                lim.batch_tokens + c.extra_tokens
            );
            return Some(self.stop(agent, Limit::BatchBudget, msg));
        }
        if lim.agent_cost.is_some() || lim.batch_cost.is_some() {
            let Some(next) = self.env.price(agent, estimate).filter(|_| c.cost_known) else {
                let msg = "A cost budget is on, but this agent's model has no price set, so Helpy can't keep to it. Add the price in Settings → AI providers.".to_string();
                return Some(self.stop(agent, Limit::AgentBudget, msg));
            };
            if let Some(b) = lim.agent_cost {
                if c.cost + next > b + c.extra_cost {
                    let msg = format!(
                        "Stopped at its cost budget: ${:.2} of ${:.2} spent.",
                        c.cost,
                        b + c.extra_cost
                    );
                    return Some(self.stop(agent, Limit::AgentBudget, msg));
                }
            }
            if let Some(b) = lim.batch_cost {
                if batch_cost + c.cost + next > b + c.extra_cost {
                    let msg = format!(
                        "Stopped at the batch's cost budget: ${:.2} of ${:.2} spent.",
                        batch_cost + c.cost,
                        b + c.extra_cost
                    );
                    return Some(self.stop(agent, Limit::BatchBudget, msg));
                }
            }
        }
        None
    }

    fn request(&self, agent: &Agent, tools: Vec<ToolDef>) -> ChatRequest {
        ChatRequest {
            model: String::new(),
            system: self.env.system(agent),
            messages: clear_stale_results(&agent.messages),
            tools,
            max_tokens: self.lim.max_response_tokens,
            temperature: self.lim.temperature,
        }
    }

    /// Replaces older work with a summary once the conversation gets long,
    /// so long-lived agents never overflow their model.
    async fn compact(&self, agent: &mut Agent) -> Result<(), Flow> {
        let size = self.request(agent, Vec::new()).estimated_tokens();
        if size <= self.lim.context_tokens {
            return Ok(());
        }
        // Keep the latest messages, and cut before an assistant message so no
        // tool result loses its call. Index 0 is the goal; summarize at least
        // one step beyond it.
        let Some(cut) = (2..=agent.messages.len().saturating_sub(KEEP_RECENT))
            .rev()
            .find(|&i| agent.messages[i].role == Role::Assistant)
        else {
            return Ok(());
        };
        let req = crate::ai::context::summary_request(
            "Summarize this agent's work so far for the agent itself to continue from: the goal, what was \
             done, what was found (keep names, numbers, paths and links), and what is left. Be concise.",
            crate::ai::context::transcript(&agent.messages[..cut]),
            self.lim.max_response_tokens,
        );
        if let Some(flow) = self.over_budget(agent, req.estimated_tokens()) {
            return Err(flow);
        }
        match self.call(agent, req).await {
            Ok(c) => {
                let summary = c.text();
                let mut kept = agent.messages.split_off(cut);
                agent.messages = vec![Message::user_text(format!(
                    "Your goal: {}\n\nSummary of your earlier work:\n{summary}",
                    agent.goal
                ))];
                agent.messages.append(&mut kept);
                let now = self.env.now_ms();
                agent.log(
                    now,
                    LogKind::Note,
                    "Summarized earlier work to keep the context small.",
                );
                Ok(())
            }
            Err(e) if e.kind == ErrorKind::Cancelled => Err(Flow::Exit),
            Err(e) => Err(self.step_failed(agent, e).await),
        }
    }

    /// A model step failed after its retries. Stop, or ask what to do.
    async fn step_failed(&self, agent: &mut Agent, e: ProviderError) -> Flow {
        if e.kind == ErrorKind::Budget {
            return self.stop(agent, Limit::DailyBudget, e.message);
        }
        if e.kind == ErrorKind::Timeout && e.message == "time limit" {
            return self.stop(agent, Limit::Time, self.time_message());
        }
        self.failure(agent, e.message, None).await
    }

    fn time_message(&self) -> String {
        format!(
            "Stopped at its time limit of {} minutes.",
            self.lim.time.as_secs() / 60
        )
    }

    /// After the last retry: fail, or (if set) ask whether to retry, skip or
    /// cancel. `on_skip` is what the model hears when the user skips.
    async fn failure(
        &self,
        agent: &mut Agent,
        message: String,
        on_skip: Option<&mut Vec<Part>>,
    ) -> Flow {
        if self.lim.on_failure == OnFailure::Stop {
            return self.fail(agent, message);
        }
        let id = format!("f{}", self.env.now_ms());
        let pending = Pending::Failure {
            id,
            message: message.clone(),
        };
        match self.wait(agent, pending, Status::Question).await {
            None | Some(Answer::Cancel) => {
                agent.status = Status::Cancelled;
                agent.finished = Some(self.env.now_ms());
                self.save(agent);
                Flow::Exit
            }
            Some(Answer::Skip) => {
                let note = format!(
                    "That step failed ({message}). The user chose to skip it; carry on without it."
                );
                match on_skip {
                    Some(parts) => parts.push(Part::Text(note)),
                    None => agent.messages.push(Message::user_text(note)),
                }
                Flow::Continue
            }
            // Retry (or anything else): go round again.
            _ => Flow::Continue,
        }
    }

    /// Runs one tool call, retrying errors that may pass next time.
    async fn tool(
        &self,
        agent: &mut Agent,
        call_id: &str,
        name: &str,
        args: &Value,
    ) -> Result<(String, bool), Flow> {
        // The user may edit the input while approving.
        let mut args = args.clone();
        let now = self.env.now_ms();
        if name == DELEGATE {
            // Helpers have their own limits; waiting for them doesn't count
            // toward this agent's time.
            agent.status_line = "Waiting for its helpers to finish.".into();
            self.save(agent);
            let result = tokio::select! {
                _ = self.cancel.cancelled() => return Err(Flow::Exit),
                r = self.env.delegate(agent, call_id, &args) => r,
            };
            self.waited
                .fetch_add(self.env.now_ms() - now, Ordering::SeqCst);
            return Ok(match result {
                Ok(text) => (text, false),
                Err(e) => (e, true),
            });
        }
        if name == PLAN {
            return Ok(match plan_items(&args["items"]) {
                Ok(items) => {
                    agent.plan = items;
                    (agent.plan_text(), false)
                }
                Err(e) => (e, true),
            });
        }
        if name == NOTE {
            let text = args["text"].as_str().unwrap_or("").trim();
            if text.is_empty() {
                return Ok(("Give the note some text.".into(), true));
            }
            agent.notes.push(text.to_string());
            while agent.notes.len() > 1
                && agent.notes.iter().map(String::len).sum::<usize>() > MAX_NOTES
            {
                agent.notes.remove(0);
            }
            return Ok((format!("Noted ({} notes kept).", agent.notes.len()), false));
        }
        if name == ASK_USER {
            let question = args["question"].as_str().unwrap_or("").to_string();
            let options = args["options"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .take(4)
                        .collect()
                })
                .unwrap_or_default();
            let pending = Pending::Question {
                id: format!("q{now}"),
                question: question.clone(),
                options,
            };
            agent.status_line = question;
            return match self.wait(agent, pending, Status::Question).await {
                Some(Answer::Choice { text }) => Ok((format!("The user answered: {text}"), false)),
                Some(Answer::Cancel) | None => Err(Flow::Exit),
                Some(_) => Ok((
                    "The user didn't answer. Decide sensibly yourself.".into(),
                    false,
                )),
            };
        }

        match self.env.gate(agent, name, &args) {
            Gate::Never(reason) => {
                agent.log(now, LogKind::Approval, format!("Not allowed: {reason}"));
                return Ok((format!("Not allowed: {reason} Don't try this again."), true));
            }
            Gate::Ask {
                kind,
                source,
                summary,
                detail,
                editable,
            } => {
                let pending = Pending::Approval {
                    id: format!("a{now}"),
                    kind,
                    source,
                    summary: summary.clone(),
                    detail,
                    args: args.clone(),
                    editable: editable.clone(),
                };
                agent.status_line = format!("Needs your OK: {summary}");
                match self.wait(agent, pending, Status::Approval).await {
                    Some(Answer::Approve) => {
                        agent.log(
                            self.env.now_ms(),
                            LogKind::Approval,
                            format!("Approved: {summary}"),
                        );
                    }
                    Some(Answer::Edit { args: edited }) => {
                        // Only the fields offered for editing change.
                        for key in &editable {
                            if let Some(v) = edited.get(key) {
                                args[key.as_str()] = v.clone();
                            }
                        }
                        agent.log(
                            self.env.now_ms(),
                            LogKind::Approval,
                            format!("Approved with your changes: {summary}"),
                        );
                    }
                    None => return Err(Flow::Exit),
                    Some(Answer::Cancel) => return Err(Flow::Exit),
                    Some(answer) => {
                        let note = match answer {
                            Answer::Reject { note: Some(n) } if !n.trim().is_empty() => {
                                format!(" They said: {n}")
                            }
                            _ => String::new(),
                        };
                        agent.log(
                            self.env.now_ms(),
                            LogKind::Approval,
                            format!("Rejected: {summary}"),
                        );
                        return Ok((
                            format!("The user rejected this action.{note} Don't try it again; carry on without it or finish."),
                            true,
                        ));
                    }
                }
            }
            Gate::Allow => {}
        }

        let mut attempt = 0;
        loop {
            attempt += 1;
            let left = self.left();
            let outcome = tokio::select! {
                _ = self.cancel.cancelled() => return Err(Flow::Exit),
                _ = self.env.sleep(left) => return Err(self.stop(agent, Limit::Time, self.time_message())),
                o = self.env.run_tool(agent, name, &args) => o,
            };
            match outcome {
                ToolOutcome::Ok { text, ops } => {
                    agent.journal.extend(ops);
                    return Ok((truncate(&text, MAX_TOOL_OUTPUT), false));
                }
                ToolOutcome::Permanent(msg) => return Ok((msg, true)),
                ToolOutcome::Transient(msg) if attempt <= self.lim.policy.max_retries => {
                    let line = format!("Retry {attempt} of {}: {msg}", self.lim.policy.max_retries);
                    agent.log(self.env.now_ms(), LogKind::Retry, line.clone());
                    self.env.live(agent, &line);
                    let wait = self.lim.policy.backoff(attempt);
                    tokio::select! {
                        _ = self.cancel.cancelled() => return Err(Flow::Exit),
                        _ = self.env.sleep(wait) => {}
                    }
                }
                ToolOutcome::Transient(msg) => {
                    let message = format!("{name} kept failing: {msg}");
                    let mut skip = Vec::new();
                    return match self.failure(agent, message.clone(), Some(&mut skip)).await {
                        Flow::Exit => Err(Flow::Exit),
                        Flow::Continue if skip.is_empty() => {
                            attempt = 0;
                            continue;
                        }
                        Flow::Continue => Ok((message + ". The user chose to skip it.", true)),
                    };
                }
            }
        }
    }

    fn finish(&self, agent: &mut Agent, text: &str) {
        let (result, suggestions) = split_suggestions(text);
        let now = self.env.now_ms();
        agent.status_line = status_from(&result).unwrap_or_else(|| "Done.".into());
        agent.result = Some(result);
        agent.suggestions = suggestions;
        agent.status = if agent.keep_open {
            Status::Ready
        } else {
            Status::Done
        };
        agent.finished = Some(now);
        agent.unseen = true;
        agent.log(now, LogKind::Result, "Finished.");
        self.save(agent);
    }
}

/// Runs the agent. On return its status says how it ended, except after a
/// cancel, where the caller decides (paused or cancelled).
pub async fn run(agent: &mut Agent, env: &dyn Env, lim: &Limits, cancel: &CancellationToken) {
    let now = env.now_ms();
    let run = Run {
        env,
        lim,
        cancel,
        deadline: now + lim.time.as_millis() as i64 - agent.active_ms as i64,
        session_start: now,
        base_active: agent.active_ms,
        waited: AtomicI64::new(0),
    };
    agent.status = Status::Running;
    agent.pending = None;
    if agent.messages.is_empty() {
        let mut parts = Vec::new();
        if let Some(img) = &agent.image {
            parts.push(Part::Image {
                media_type: "image/jpeg".into(),
                data: img.clone(),
            });
        }
        parts.push(Part::Text(agent.goal.clone()));
        if let Some(h) = &agent.handoff {
            parts.push(Part::Text(h.clone()));
        }
        agent.messages.push(Message {
            role: Role::User,
            parts,
        });
    }
    run.save(agent);

    // Calls cut off by a pause or a restart get a result before going on.
    // Helpers are picked up again; anything else may or may not have
    // happened, so the agent is told to check.
    let dangling = dangling_calls(agent);
    if !dangling.is_empty() {
        let mut results = Vec::new();
        for (id, name, args) in dangling {
            let (text, is_error) = if name == DELEGATE {
                match run.tool(agent, &id, &name, &args).await {
                    Ok(r) => r,
                    Err(_) => return,
                }
            } else {
                (
                    "This action was interrupted (the agent was paused or Helpy closed) before its result came \
                     back. It may or may not have happened; check before doing it again."
                        .to_string(),
                    true,
                )
            };
            results.push(Part::ToolResult {
                id,
                name,
                parts: vec![Part::Text(text)],
                is_error,
            });
        }
        agent.messages.push(Message {
            role: Role::User,
            parts: results,
        });
        run.save(agent);
    }

    loop {
        if cancel.is_cancelled() {
            return;
        }
        if run.left().is_zero() {
            run.stop(agent, Limit::Time, run.time_message());
            return;
        }
        if agent.counters.steps >= lim.max_steps {
            let msg = format!("Stopped after {} steps, its step limit.", lim.max_steps);
            run.stop(agent, Limit::Steps, msg);
            return;
        }
        for said in env.steering(agent) {
            agent.log(env.now_ms(), LogKind::Note, format!("You said: {said}"));
            push_user_text(
                agent,
                format!("The user says this while you work (follow it): {said}"),
            );
        }
        match run.compact(agent).await {
            Ok(()) => {}
            Err(Flow::Exit) => return,
            Err(Flow::Continue) => continue,
        }
        let req = run.request(agent, env.tools(agent));
        if run.over_budget(agent, req.estimated_tokens()).is_some() {
            return;
        }
        let completion = match run.call(agent, req).await {
            Ok(c) => c,
            Err(e) if e.kind == ErrorKind::Cancelled || cancel.is_cancelled() => return,
            Err(e) => match run.step_failed(agent, e).await {
                Flow::Continue => continue,
                Flow::Exit => return,
            },
        };
        agent.counters.steps += 1;
        agent.messages.push(Message {
            role: Role::Assistant,
            parts: completion.parts.clone(),
        });

        let text = completion.text();
        let mut progress = false;
        if !text.trim().is_empty() {
            progress |= agent.seen.insert(stable_hash(text.trim()));
            agent.recent_outputs.push(text.trim().to_string());
            let n = agent.recent_outputs.len();
            if n > lim.repeat as usize {
                agent.recent_outputs.drain(..n - lim.repeat as usize);
            }
            if agent.recent_outputs.len() == lim.repeat as usize
                && agent
                    .recent_outputs
                    .iter()
                    .all(|o| *o == agent.recent_outputs[0])
            {
                let msg = format!(
                    "Stopped as stuck: it said the same thing {} times in a row.",
                    lim.repeat
                );
                run.stop(agent, Limit::Repeat, msg);
                return;
            }
        }

        let calls: Vec<(String, String, Value)> = completion
            .tool_uses()
            .map(|(id, name, input)| (id.to_string(), name.to_string(), input.clone()))
            .collect();
        if calls.is_empty() {
            run.finish(agent, &text);
            return;
        }
        if let Some(line) = status_from(&text) {
            agent.log(env.now_ms(), LogKind::Status, line.clone());
            run.set_status_line(agent, line);
        }
        run.save(agent);

        let mut results = Vec::new();
        for (id, name, args) in calls {
            if agent.counters.tool_calls >= lim.max_tool_calls {
                let msg = format!(
                    "Stopped after {} tool calls, its limit.",
                    lim.max_tool_calls
                );
                run.stop(agent, Limit::ToolCalls, msg);
                return;
            }
            let key = format!("{name} {}", canonical(&args));
            let seen = agent.repeats.get(&key).copied().unwrap_or(0) + 1;
            if seen >= lim.repeat {
                let msg = format!(
                    "Stopped as stuck: it tried {name} with the same input {} times.",
                    lim.repeat
                );
                run.stop(agent, Limit::Repeat, msg);
                return;
            }
            agent.repeats.insert(key, seen);
            agent.counters.tool_calls += 1;
            agent.log(
                env.now_ms(),
                LogKind::Tool,
                format!("{name} {}", truncate(&canonical(&args), 300)),
            );

            let (output, is_error) = match run.tool(agent, &id, &name, &args).await {
                Ok(r) => r,
                Err(_) => return,
            };
            progress |= agent
                .seen
                .insert(stable_hash(&format!("{name}\u{0}{output}")));
            let kind = if is_error {
                LogKind::Error
            } else {
                LogKind::Result
            };
            agent.log(env.now_ms(), kind, truncate(&output, 300));
            results.push(Part::ToolResult {
                id,
                name,
                parts: vec![Part::Text(output)],
                is_error,
            });
        }
        agent.messages.push(Message {
            role: Role::User,
            parts: results,
        });

        agent.no_progress = if progress { 0 } else { agent.no_progress + 1 };
        if agent.no_progress >= lim.no_progress {
            let msg = format!(
                "Stopped as stuck: {} steps in a row brought nothing new.",
                lim.no_progress
            );
            run.stop(agent, Limit::NoProgress, msg);
            return;
        }
        run.save(agent);
    }
}

/// The conversation as sent to the model: the latest tool results in full,
/// older long ones replaced by a note, since the agent has already used them
/// and they would otherwise be paid for again on every step. The stored
/// history keeps everything.
pub(crate) fn clear_stale_results(messages: &[Message]) -> Vec<Message> {
    let mut out = messages.to_vec();
    let mut seen = 0;
    for m in out.iter_mut().rev() {
        for p in m.parts.iter_mut().rev() {
            let Part::ToolResult { name, parts, .. } = p else { continue };
            seen += 1;
            let size: usize = parts
                .iter()
                .map(|p| match p {
                    Part::Text(t) => t.len(),
                    _ => SMALL_RESULT + 1,
                })
                .sum();
            if seen > KEEP_RESULTS && size > SMALL_RESULT {
                *parts = vec![Part::Text(format!(
                    "[Earlier result of {name} ({size} characters), cleared to save space. Call it again if you \
                     need it.]"
                ))];
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stale_tool_results_are_cleared_in_requests_only() {
        let result = |id: &str, text: &str| Message {
            role: Role::User,
            parts: vec![Part::ToolResult {
                id: id.into(),
                name: "notion_read".into(),
                parts: vec![Part::Text(text.into())],
                is_error: false,
            }],
        };
        let big = "x".repeat(5000);
        let history = vec![
            Message::user_text("goal"),
            result("1", &big),
            result("2", "short"),
            result("3", &big),
            result("4", &big),
        ];
        let sent = clear_stale_results(&history);
        let text = |m: &Message| match &m.parts[0] {
            Part::ToolResult { parts, .. } => match &parts[0] {
                Part::Text(t) => t.clone(),
                _ => String::new(),
            },
            _ => String::new(),
        };
        assert!(text(&sent[1]).starts_with("[Earlier result of notion_read (5000 characters)"));
        assert_eq!(text(&sent[2]), "short");
        assert_eq!(text(&sent[3]), big);
        assert_eq!(text(&sent[4]), big);
        assert_eq!(text(&history[1]), big);
    }
    use crate::ai::types::{StopReason, Usage};
    use serde_json::json;
    use std::collections::VecDeque;
    use std::sync::atomic::{AtomicI64, Ordering};

    type GateFn = fn(&str) -> Gate;

    /// A scripted provider and toolbox. Time only moves when told to.
    #[derive(Default)]
    struct Fake {
        replies: Mutex<VecDeque<Result<Completion, ProviderError>>>,
        tools: Mutex<VecDeque<ToolOutcome>>,
        answers: Mutex<VecDeque<Answer>>,
        gate: Mutex<Option<GateFn>>,
        /// Tokens each call bills, including failed ones.
        bill: u64,
        batch: (u64, f64),
        clock: AtomicI64,
        /// How far the clock moves per model call, and while the user answers.
        call_ms: i64,
        answer_ms: i64,
        calls: AtomicI64,
        tool_runs: Mutex<Vec<String>>,
        tool_args: Mutex<Vec<Value>>,
        saves: AtomicI64,
        /// Handed to the agent at its next step.
        steer: Mutex<Vec<String>>,
        /// Delegate calls seen: (call id, tasks).
        delegations: Mutex<Vec<(String, Value)>>,
        /// How far the clock moves while helpers work.
        delegate_ms: i64,
    }

    fn text(t: &str) -> Result<Completion, ProviderError> {
        Ok(Completion {
            model: "fake".into(),
            parts: vec![Part::Text(t.into())],
            stop: StopReason::EndTurn,
            usage: Usage::default(),
        })
    }

    fn tool_call(say: &str, name: &str, args: Value) -> Result<Completion, ProviderError> {
        Ok(Completion {
            model: "fake".into(),
            parts: vec![
                Part::Text(say.into()),
                Part::ToolUse {
                    id: format!("t{}", rand_id()),
                    name: name.into(),
                    input: args,
                    signature: None,
                },
            ],
            stop: StopReason::ToolUse,
            usage: Usage::default(),
        })
    }

    fn rand_id() -> u64 {
        static N: AtomicI64 = AtomicI64::new(0);
        N.fetch_add(1, Ordering::Relaxed) as u64
    }

    impl Env for Fake {
        fn call<'a>(
            &'a self,
            _: &'a Agent,
            _: ChatRequest,
            on: &'a (dyn Fn(Progress) + Sync),
        ) -> BoxFuture<'a, Result<Completion, ProviderError>> {
            Box::pin(async move {
                self.calls.fetch_add(1, Ordering::SeqCst);
                self.clock.fetch_add(self.call_ms, Ordering::SeqCst);
                if self.bill > 0 {
                    on(Progress::Spent {
                        tokens: self.bill,
                        cost: Some(self.bill as f64 / 1e6),
                    });
                }
                self.replies
                    .lock()
                    .unwrap()
                    .pop_front()
                    .unwrap_or_else(|| text("out of script"))
            })
        }
        fn gate(&self, _: &Agent, tool: &str, _: &Value) -> Gate {
            match *self.gate.lock().unwrap() {
                Some(f) => f(tool),
                None => Gate::Allow,
            }
        }
        fn run_tool<'a>(
            &'a self,
            _: &'a Agent,
            tool: &'a str,
            args: &'a Value,
        ) -> BoxFuture<'a, ToolOutcome> {
            Box::pin(async move {
                self.tool_runs.lock().unwrap().push(tool.to_string());
                self.tool_args.lock().unwrap().push(args.clone());
                self.tools
                    .lock()
                    .unwrap()
                    .pop_front()
                    .unwrap_or(ToolOutcome::Ok {
                        text: format!("result {}", rand_id()),
                        ops: Vec::new(),
                    })
            })
        }
        fn answer<'a>(&'a self, _: &'a Agent) -> BoxFuture<'a, Answer> {
            Box::pin(async move {
                self.clock.fetch_add(self.answer_ms, Ordering::SeqCst);
                self.answers
                    .lock()
                    .unwrap()
                    .pop_front()
                    .expect("scripted answer")
            })
        }
        fn batch_spent(&self, _: &Agent) -> (u64, f64) {
            self.batch
        }
        fn price(&self, _: &Agent, tokens: u64) -> Option<f64> {
            Some(tokens as f64 / 1e6)
        }
        fn system(&self, _: &Agent) -> String {
            "system".into()
        }
        fn tools(&self, _: &Agent) -> Vec<ToolDef> {
            Vec::new()
        }
        fn save(&self, _: &Agent) {
            self.saves.fetch_add(1, Ordering::SeqCst);
        }
        fn live(&self, _: &Agent, _: &str) {}
        fn now_ms(&self) -> i64 {
            self.clock.load(Ordering::SeqCst)
        }
        fn sleep(&self, d: Duration) -> BoxFuture<'static, ()> {
            // Backoff waits (milliseconds here) pass at once so tests are
            // fast. Time-limit timers (a minute or more) never fire: the
            // fake clock only moves with calls, and the runner checks it.
            Box::pin(async move {
                if d >= Duration::from_secs(60) {
                    std::future::pending::<()>().await
                }
            })
        }
        fn steering(&self, _: &Agent) -> Vec<String> {
            std::mem::take(&mut *self.steer.lock().unwrap())
        }
        fn delegate<'a>(
            &'a self,
            _: &'a Agent,
            call_id: &'a str,
            tasks: &'a Value,
        ) -> BoxFuture<'a, Result<String, String>> {
            Box::pin(async move {
                self.clock.fetch_add(self.delegate_ms, Ordering::SeqCst);
                self.delegations
                    .lock()
                    .unwrap()
                    .push((call_id.to_string(), tasks.clone()));
                Ok("## Helper\nFound 3 desks.".to_string())
            })
        }
    }

    fn limits() -> Limits {
        Limits {
            max_steps: 25,
            max_tool_calls: 50,
            repeat: 3,
            no_progress: 8,
            time: Duration::from_secs(1800),
            agent_tokens: 0,
            agent_cost: None,
            batch_tokens: 0,
            batch_cost: None,
            context_tokens: 1_000_000,
            on_failure: OnFailure::Stop,
            policy: Policy {
                max_retries: 3,
                base: Duration::from_millis(1),
                max: Duration::from_millis(4),
            },
            max_response_tokens: 1000,
            temperature: 0.5,
        }
    }

    fn agent() -> Agent {
        Agent::new(
            "a1".into(),
            "b1".into(),
            0,
            "Research".into(),
            "Find things".into(),
            0,
        )
    }

    fn script(f: &Fake, replies: Vec<Result<Completion, ProviderError>>) {
        *f.replies.lock().unwrap() = replies.into();
    }

    async fn go(f: &Fake, a: &mut Agent, lim: &Limits) {
        run(a, f, lim, &CancellationToken::new()).await
    }

    #[tokio::test]
    async fn finishes_with_a_result_and_suggestions() {
        let f = Fake::default();
        script(
            &f,
            vec![
                tool_call(
                    "I'm searching for accountants now.",
                    "web_search",
                    json!({"q": "uk"}),
                ),
                text("Found 5 accountants. Details follow.\nNEXT: Compare prices | Draft an email"),
            ],
        );
        let mut a = agent();
        go(&f, &mut a, &limits()).await;
        assert_eq!(a.status, Status::Done);
        assert_eq!(
            a.result.as_deref(),
            Some("Found 5 accountants. Details follow.")
        );
        assert_eq!(a.suggestions, ["Compare prices", "Draft an email"]);
        assert_eq!((a.counters.steps, a.counters.tool_calls), (2, 1));
        assert!(a
            .log
            .iter()
            .any(|l| l.text == "I'm searching for accountants now."));
    }

    #[tokio::test]
    async fn a_plan_and_notes_stay_with_the_agent() {
        let f = Fake::default();
        script(
            &f,
            vec![
                tool_call(
                    "Planning.",
                    PLAN,
                    json!({"items": [{"text": "Find sources"}, {"text": " Write it up ", "status": "doing"}]}),
                ),
                tool_call("Noting.", NOTE, json!({"text": "Budget is £400"})),
                tool_call("Bad plan.", PLAN, json!({"items": "later"})),
                text("Done."),
            ],
        );
        let mut a = agent();
        go(&f, &mut a, &limits()).await;
        assert_eq!(a.status, Status::Done);
        assert_eq!(
            a.plan,
            vec![
                PlanItem { text: "Find sources".into(), status: PlanStatus::Todo },
                PlanItem { text: "Write it up".into(), status: PlanStatus::Doing },
            ]
        );
        assert_eq!(a.plan_text(), "Plan: 0 of 2 done.\n[ ] Find sources\n[>] Write it up");
        assert_eq!(a.notes, ["Budget is £400"]);
        // The bad plan was refused and the old one kept; the results reached the model.
        let results: Vec<String> = a
            .messages
            .iter()
            .flat_map(|m| m.parts.iter())
            .filter_map(|p| match p {
                Part::ToolResult { parts, is_error, .. } => match &parts[0] {
                    Part::Text(t) => Some(format!("{is_error} {t}")),
                    _ => None,
                },
                _ => None,
            })
            .collect();
        assert_eq!(results[0], "false Plan: 0 of 2 done.\n[ ] Find sources\n[>] Write it up");
        assert_eq!(results[1], "false Noted (1 notes kept).");
        assert!(results[2].starts_with("true Give the plan as a list"));
    }

    #[tokio::test]
    async fn open_ended_work_waits_for_changes() {
        let f = Fake::default();
        script(&f, vec![text("Built the site.")]);
        let mut a = agent();
        a.keep_open = true;
        go(&f, &mut a, &limits()).await;
        assert_eq!(a.status, Status::Ready);
    }

    #[tokio::test]
    async fn step_limit_stops_it() {
        let f = Fake::default();
        let calls = (0..30)
            .map(|i| tool_call("", "fetch", json!({ "url": format!("https://x/{i}") })))
            .collect();
        script(&f, calls);
        let mut lim = limits();
        lim.max_steps = 5;
        let mut a = agent();
        go(&f, &mut a, &lim).await;
        assert_eq!(a.status, Status::Stopped);
        assert_eq!(a.stop.as_ref().unwrap().limit, Limit::Steps);
        assert_eq!(f.calls.load(Ordering::SeqCst), 5);
    }

    #[tokio::test]
    async fn tool_call_limit_stops_it_mid_step() {
        let f = Fake::default();
        let many = Completion {
            model: "fake".into(),
            parts: (0..10)
                .map(|i| Part::ToolUse {
                    id: format!("{i}"),
                    name: "fetch".into(),
                    input: json!({ "url": i }),
                    signature: None,
                })
                .collect(),
            stop: StopReason::ToolUse,
            usage: Usage::default(),
        };
        script(&f, vec![Ok(many)]);
        let mut lim = limits();
        lim.max_tool_calls = 4;
        let mut a = agent();
        go(&f, &mut a, &lim).await;
        assert_eq!(a.stop.as_ref().unwrap().limit, Limit::ToolCalls);
        assert_eq!(f.tool_runs.lock().unwrap().len(), 4);
    }

    #[tokio::test]
    async fn the_same_call_three_times_is_stuck_even_with_keys_reordered() {
        let f = Fake::default();
        script(
            &f,
            vec![
                tool_call("", "search", json!({"q": "x", "n": 5})),
                tool_call("", "search", json!({"n": 5, "q": "x"})),
                tool_call("", "search", json!({"q": "x", "n": 5})),
            ],
        );
        let mut a = agent();
        go(&f, &mut a, &limits()).await;
        assert_eq!(a.stop.as_ref().unwrap().limit, Limit::Repeat);
        // The third identical call never ran.
        assert_eq!(f.tool_runs.lock().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn saying_the_same_thing_three_times_is_stuck() {
        let f = Fake::default();
        script(
            &f,
            (0..5)
                .map(|i| tool_call("Checking again.", "fetch", json!({ "u": i })))
                .collect(),
        );
        let mut a = agent();
        go(&f, &mut a, &limits()).await;
        assert_eq!(a.stop.as_ref().unwrap().limit, Limit::Repeat);
        assert_eq!(f.calls.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn steps_with_nothing_new_are_stuck() {
        let f = Fake::default();
        script(
            &f,
            (0..20)
                .map(|i| tool_call("", "fetch", json!({ "u": i })))
                .collect(),
        );
        // Every tool returns the same text, and the agent says nothing new.
        *f.tools.lock().unwrap() = (0..20)
            .map(|_| ToolOutcome::Ok {
                text: "same page".into(),
                ops: Vec::new(),
            })
            .collect();
        let mut lim = limits();
        lim.no_progress = 4;
        let mut a = agent();
        go(&f, &mut a, &lim).await;
        assert_eq!(a.stop.as_ref().unwrap().limit, Limit::NoProgress);
        // The first step brought something new, the next four didn't.
        assert_eq!(f.calls.load(Ordering::SeqCst), 5);
    }

    #[tokio::test]
    async fn the_agent_budget_is_checked_before_each_call_and_counts_failures() {
        let f = Fake {
            bill: 3000,
            ..Default::default()
        };
        script(
            &f,
            vec![
                Err(ProviderError::new(ErrorKind::Server, "boom")),
                tool_call("", "fetch", json!({"u": 1})),
                tool_call("", "fetch", json!({"u": 2})),
            ],
        );
        let mut lim = limits();
        lim.agent_tokens = 7000;
        lim.on_failure = OnFailure::Ask;
        *f.answers.lock().unwrap() = vec![Answer::Retry].into();
        let mut a = agent();
        go(&f, &mut a, &lim).await;
        // The failed call was billed too, so after two calls 6000 are used,
        // and the next (up to ~1000 more) could pass 7000: it never happens.
        assert_eq!(a.stop.as_ref().unwrap().limit, Limit::AgentBudget);
        assert_eq!(f.calls.load(Ordering::SeqCst), 2);
        assert_eq!(a.counters.tokens, 6000);
        assert!(a.stop.unwrap().limit.raisable());
    }

    #[tokio::test]
    async fn batch_and_cost_budgets_stop_it_too() {
        let f = Fake {
            batch: (990_000, 0.0),
            ..Default::default()
        };
        script(&f, vec![text("never")]);
        let mut lim = limits();
        lim.batch_tokens = 1_000_000;
        let mut a = agent();
        a.goal = "x".repeat(40_000);
        go(&f, &mut a, &lim).await;
        assert_eq!(a.stop.as_ref().unwrap().limit, Limit::BatchBudget);
        assert_eq!(f.calls.load(Ordering::SeqCst), 0);

        let f = Fake::default();
        let mut lim = limits();
        lim.agent_cost = Some(0.001);
        let mut a = agent();
        a.counters.cost = 0.0009;
        go(&f, &mut a, &lim).await;
        assert_eq!(a.stop.as_ref().unwrap().limit, Limit::AgentBudget);
    }

    #[tokio::test]
    async fn the_daily_budget_error_stops_without_retrying() {
        let f = Fake::default();
        script(
            &f,
            vec![Err(ProviderError::new(ErrorKind::Budget, "Today's limit"))],
        );
        let mut lim = limits();
        lim.on_failure = OnFailure::Ask;
        let mut a = agent();
        go(&f, &mut a, &lim).await;
        assert_eq!(a.stop.as_ref().unwrap().limit, Limit::DailyBudget);
        assert_eq!(f.calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn the_time_limit_counts_time_from_earlier_sessions() {
        let f = Fake {
            call_ms: 60_000,
            ..Default::default()
        };
        script(
            &f,
            (0..50)
                .map(|i| tool_call(&format!("step {i}"), "fetch", json!({ "u": i })))
                .collect(),
        );
        let mut lim = limits();
        lim.time = Duration::from_secs(600);
        let mut a = agent();
        // Eight minutes were used before a restart.
        a.active_ms = 480_000;
        go(&f, &mut a, &lim).await;
        assert_eq!(a.stop.as_ref().unwrap().limit, Limit::Time);
        assert_eq!(f.calls.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn tool_errors_that_may_pass_are_retried_then_fail() {
        let f = Fake::default();
        script(&f, vec![tool_call("", "fetch", json!({}))]);
        *f.tools.lock().unwrap() = (0..10)
            .map(|_| ToolOutcome::Transient("timed out".into()))
            .collect();
        let mut a = agent();
        go(&f, &mut a, &limits()).await;
        assert_eq!(a.status, Status::Failed);
        // One try plus three retries, then it stops instead of carrying on.
        assert_eq!(f.tool_runs.lock().unwrap().len(), 4);
        assert!(a.log.iter().any(|l| l.text == "Retry 2 of 3: timed out"));
        assert_eq!(f.calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn on_failure_ask_offers_retry_skip_and_cancel() {
        let f = Fake::default();
        script(
            &f,
            vec![tool_call("", "fetch", json!({})), text("Done without it.")],
        );
        *f.tools.lock().unwrap() = (0..4)
            .map(|_| ToolOutcome::Transient("timed out".into()))
            .collect();
        *f.answers.lock().unwrap() = vec![Answer::Skip].into();
        let mut lim = limits();
        lim.on_failure = OnFailure::Ask;
        let mut a = agent();
        go(&f, &mut a, &lim).await;
        assert_eq!(a.status, Status::Done);

        let f = Fake::default();
        script(&f, vec![Err(ProviderError::new(ErrorKind::Server, "down"))]);
        *f.answers.lock().unwrap() = vec![Answer::Cancel].into();
        let mut a = agent();
        go(&f, &mut a, &lim).await;
        assert_eq!(a.status, Status::Cancelled);
    }

    #[tokio::test]
    async fn approvals_gate_actions_and_rejections_never_run() {
        let f = Fake::default();
        *f.gate.lock().unwrap() = Some(|tool| match tool {
            "shell" => Gate::Ask {
                kind: ActionKind::Shell,
                source: "Commands".into(),
                summary: "Run ls".into(),
                detail: "ls -la".into(),
                editable: vec!["c".into()],
            },
            "delete_file" => Gate::Never("deleting is turned off.".into()),
            _ => Gate::Allow,
        });
        script(
            &f,
            vec![
                tool_call("", "shell", json!({"c": 1})),
                tool_call("", "shell", json!({"c": 2})),
                tool_call("", "delete_file", json!({})),
                text("Finished."),
            ],
        );
        *f.answers.lock().unwrap() = vec![
            Answer::Edit {
                args: json!({"c": "edited", "other": "ignored"}),
            },
            Answer::Reject {
                note: Some("not that one".into()),
            },
        ]
        .into();
        let mut a = agent();
        go(&f, &mut a, &limits()).await;
        assert_eq!(a.status, Status::Done);
        // Only the approved command ran.
        assert_eq!(*f.tool_runs.lock().unwrap(), ["shell"]);
        // It ran with the user's edit, and only editable fields changed.
        assert_eq!(f.tool_args.lock().unwrap()[0], json!({"c": "edited"}));
        let said: String = a
            .messages
            .iter()
            .flat_map(|m| &m.parts)
            .filter_map(|p| match p {
                Part::ToolResult { parts, .. } => match &parts[0] {
                    Part::Text(t) => Some(t.clone()),
                    _ => None,
                },
                _ => None,
            })
            .collect();
        assert!(
            said.contains("rejected")
                && said.contains("not that one")
                && said.contains("Not allowed")
        );
    }

    #[tokio::test]
    async fn counters_survive_a_restart_and_a_follow_up_keeps_the_budget() {
        let f = Fake {
            bill: 1000,
            ..Default::default()
        };
        script(
            &f,
            (0..3)
                .map(|i| tool_call(&format!("s{i}"), "fetch", json!({ "u": i })))
                .collect(),
        );
        // Helpy quits during the fourth call.
        f.replies
            .lock()
            .unwrap()
            .push_back(Err(ProviderError::new(ErrorKind::Cancelled, "quit")));
        let mut lim = limits();
        lim.max_steps = 10;
        lim.agent_tokens = 6000;
        let mut a = agent();
        go(&f, &mut a, &lim).await;
        assert_eq!((a.counters.steps, a.counters.tokens), (3, 4000));
        let saved = serde_json::to_string(&a).unwrap();
        let mut back: Agent = serde_json::from_str(&saved).unwrap();
        assert_eq!(back.counters, a.counters);
        // After the restart 4000 of 6000 are used: one more step fits, then
        // the budget stops it.
        script(
            &f,
            vec![tool_call("s9", "fetch", json!({"u": 9})), text("done")],
        );
        go(&f, &mut back, &lim).await;
        assert_eq!(back.stop.as_ref().unwrap().limit, Limit::AgentBudget);
        assert_eq!(back.counters.steps, 4);

        // A follow-up round resets steps, but not the spending.
        back.new_round();
        assert_eq!(back.counters.steps, 0);
        assert_eq!(back.counters.tokens, 5000);
    }

    #[tokio::test]
    async fn long_work_is_summarized_and_the_summary_is_paid_for() {
        let f = Fake {
            bill: 100,
            ..Default::default()
        };
        script(
            &f,
            vec![
                tool_call("one", "fetch", json!({"u": 1})),
                tool_call("two", "fetch", json!({"u": 2})),
                tool_call("three", "fetch", json!({"u": 3})),
                tool_call("four", "fetch", json!({"u": 4})),
                text("Summary: fetched 1 and 2."),
                text("All done."),
            ],
        );
        *f.tools.lock().unwrap() = (0..4)
            .map(|i| ToolOutcome::Ok {
                text: format!("{i} {}", "page ".repeat(2000)),
                ops: Vec::new(),
            })
            .collect();
        let mut lim = limits();
        lim.context_tokens = 5000;
        let mut a = agent();
        go(&f, &mut a, &lim).await;
        assert_eq!(a.status, Status::Done);
        match &a.messages[0].parts[0] {
            Part::Text(t) => assert!(t.contains("Summary: fetched 1 and 2.")),
            other => panic!("{other:?}"),
        }
        // Every call, the summary included, was billed.
        assert_eq!(
            a.counters.tokens,
            100 * f.calls.load(Ordering::SeqCst) as u64
        );
    }

    #[tokio::test]
    async fn questions_wait_for_the_user() {
        let f = Fake::default();
        script(
            &f,
            vec![
                tool_call(
                    "",
                    ASK_USER,
                    json!({"question": "Which folder?", "options": ["Desktop", "Downloads"]}),
                ),
                text("Sorted Downloads."),
            ],
        );
        *f.answers.lock().unwrap() = vec![Answer::Choice {
            text: "Downloads".into(),
        }]
        .into();
        let mut a = agent();
        go(&f, &mut a, &limits()).await;
        assert_eq!(a.status, Status::Done);
        assert!(f.tool_runs.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn waiting_for_the_user_doesnt_use_up_the_time_limit() {
        let f = Fake {
            call_ms: 60_000,
            answer_ms: 3_600_000,
            ..Default::default()
        };
        script(
            &f,
            vec![
                tool_call("", ASK_USER, json!({"question": "Which one?"})),
                text("Done."),
            ],
        );
        *f.answers.lock().unwrap() = vec![Answer::Choice { text: "A".into() }].into();
        let mut lim = limits();
        lim.time = Duration::from_secs(300);
        let mut a = agent();
        go(&f, &mut a, &lim).await;
        // An hour went by while the user thought; the agent ran two minutes.
        assert_eq!(a.status, Status::Done);
        assert_eq!(a.active_ms, 120_000);
    }

    #[test]
    fn helpers() {
        assert_eq!(
            canonical(&json!({"b": [{"d": 1, "c": 2}], "a": 1})),
            r#"{"a":1,"b":[{"c":2,"d":1}]}"#
        );
        assert_eq!(stable_hash("abc"), stable_hash("abc"));
        assert_ne!(stable_hash("abc"), stable_hash("abd"));
        assert_eq!(
            status_from("I'm on it. Next I'll check. Then more."),
            Some("I'm on it. Next I'll check.".into())
        );
        assert_eq!(status_from("  "), None);
        assert_eq!(
            split_suggestions("Done.\nNEXT: a|b | c | d"),
            ("Done.".into(), vec!["a".into(), "b".into(), "c".into()])
        );
        assert_eq!(split_suggestions("No list"), ("No list".into(), vec![]));
    }

    fn texts(a: &Agent) -> String {
        a.messages
            .iter()
            .flat_map(|m| m.parts.iter())
            .filter_map(|p| match p {
                Part::Text(t) => Some(t.clone()),
                Part::ToolResult { parts, .. } => parts.iter().find_map(|p| match p {
                    Part::Text(t) => Some(t.clone()),
                    _ => None,
                }),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[tokio::test]
    async fn helpers_results_come_back_and_their_time_doesnt_count() {
        // Helpers take 40 minutes; the agent's own limit is 30.
        let f = Fake {
            delegate_ms: 40 * 60_000,
            ..Default::default()
        };
        script(
            &f,
            vec![
                tool_call(
                    "I'm splitting this up.",
                    DELEGATE,
                    json!({"tasks": [{"name": "Desks", "goal": "Find desks"}]}),
                ),
                text("Here are 3 desks."),
            ],
        );
        let mut a = agent();
        go(&f, &mut a, &limits()).await;
        assert_eq!(a.status, Status::Done);
        assert_eq!(f.delegations.lock().unwrap().len(), 1);
        assert!(texts(&a).contains("Found 3 desks."));
    }

    #[tokio::test]
    async fn steering_reaches_the_agent_at_its_next_step() {
        let f = Fake::default();
        script(
            &f,
            vec![
                tool_call("Looking.", "fetch", json!({"u": 1})),
                text("Done."),
            ],
        );
        f.steer.lock().unwrap().push("Only desks under £300".into());
        let mut a = agent();
        go(&f, &mut a, &limits()).await;
        assert!(texts(&a).contains("Only desks under £300"));
        assert!(a
            .log
            .iter()
            .any(|l| l.text == "You said: Only desks under £300"));
        // Roles still alternate: no two user messages in a row.
        assert!(a.messages.windows(2).all(|w| w[0].role != w[1].role));
    }

    #[tokio::test]
    async fn a_handoff_goes_with_the_task() {
        let f = Fake::default();
        script(&f, vec![text("Done.")]);
        let mut a = agent();
        a.handoff = Some("Results from Research: 3 desks".into());
        go(&f, &mut a, &limits()).await;
        assert!(texts(&a).starts_with("Find things\nResults from Research: 3 desks"));
    }

    #[tokio::test]
    async fn calls_cut_off_by_a_pause_are_closed_on_resume() {
        let f = Fake::default();
        script(&f, vec![text("Done.")]);
        let mut a = agent();
        a.messages = vec![
            Message::user_text("Find things"),
            Message {
                role: Role::Assistant,
                parts: vec![
                    Part::ToolUse {
                        id: "t1".into(),
                        name: "send_email".into(),
                        input: json!({}),
                        signature: None,
                    },
                    Part::ToolUse {
                        id: "t2".into(),
                        name: DELEGATE.into(),
                        input: json!({"tasks": []}),
                        signature: None,
                    },
                ],
            },
        ];
        go(&f, &mut a, &limits()).await;
        assert_eq!(a.status, Status::Done);
        // The email isn't sent again; the helpers are picked up again.
        assert!(f.tool_runs.lock().unwrap().is_empty());
        assert_eq!(f.delegations.lock().unwrap()[0].0, "t2");
        let t = texts(&a);
        assert!(t.contains("may or may not have happened") && t.contains("Found 3 desks."));
    }
}
