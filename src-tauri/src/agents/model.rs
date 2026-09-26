//! What an agent is: its task, its conversation, its counters and its log.
//! The whole struct is saved after every step, so a restart resumes it with
//! its limits exactly where they were.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::ai::types::Message;

#[derive(Serialize, Deserialize, TS, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum Status {
    Queued,
    Running,
    /// Waiting for the user to approve an action.
    Approval,
    /// Waiting for the user to answer a question or make a choice.
    Question,
    Paused,
    /// Done with this round, open for follow-up changes.
    Ready,
    Done,
    Failed,
    /// A limit stopped it.
    Stopped,
    Cancelled,
}

impl Status {
    pub fn is_finished(self) -> bool {
        matches!(
            self,
            Status::Done | Status::Failed | Status::Stopped | Status::Cancelled
        )
    }
}

#[derive(Serialize, Deserialize, TS, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum RunMode {
    Single,
    Parallel,
    Sequential,
}

/// Which limit stopped an agent.
#[derive(Serialize, Deserialize, TS, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum Limit {
    Steps,
    ToolCalls,
    Time,
    AgentBudget,
    BatchBudget,
    DailyBudget,
    Repeat,
    NoProgress,
}

impl Limit {
    /// Budgets can be raised once from the card to let the agent continue.
    pub fn raisable(self) -> bool {
        matches!(self, Limit::AgentBudget | Limit::BatchBudget)
    }
}

#[derive(Serialize, Deserialize, TS, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Stop {
    pub limit: Limit,
    pub message: String,
}

#[derive(Serialize, Deserialize, TS, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum ActionKind {
    FileChange,
    FileDelete,
    Shell,
    Reminder,
    /// Something a connector or MCP server does (send, post, create, delete).
    Connector,
    /// Typing into a website or sending its form.
    Browser,
    /// A builder agent's coding round, project command or launch.
    Build,
}

/// Something the agent is waiting on the user for.
#[derive(Serialize, Deserialize, TS, Clone, Debug, PartialEq)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[ts(export)]
pub enum Pending {
    Approval {
        id: String,
        kind: ActionKind,
        /// What does it: "Files", "Gmail", an MCP server's name.
        source: String,
        /// One line: what will happen.
        summary: String,
        /// Everything: the full command, the email, the page content.
        detail: String,
        /// The action's input, for editing before approving.
        #[ts(type = "Record<string, unknown>")]
        args: serde_json::Value,
        /// Fields of `args` the user may edit.
        editable: Vec<String>,
    },
    Question {
        id: String,
        question: String,
        options: Vec<String>,
    },
    /// A step failed for good and the settings say to ask what to do.
    Failure { id: String, message: String },
}

/// The user's reply to a Pending.
#[derive(Deserialize, TS, Clone, Debug, PartialEq)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[ts(export)]
pub enum Answer {
    Approve,
    /// Approve with changes to the editable fields.
    Edit {
        #[ts(type = "Record<string, unknown>")]
        args: serde_json::Value,
    },
    Reject {
        note: Option<String>,
    },
    Choice {
        text: String,
    },
    Retry,
    Skip,
    Cancel,
}

/// A file change an agent made, with what's needed to undo it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "op", rename_all = "camelCase")]
pub enum FileOp {
    /// A new file or folder; undo removes it.
    Created {
        path: String,
    },
    /// An existing file was overwritten; `backup` holds the old content.
    Replaced {
        path: String,
        backup: String,
    },
    Moved {
        from: String,
        to: String,
    },
    /// Deleted; `backup` holds the old content.
    Deleted {
        path: String,
        backup: String,
    },
}

#[derive(Serialize, Deserialize, TS, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum PlanStatus {
    #[default]
    Todo,
    Doing,
    Done,
}

/// One step of the checklist an agent keeps with its plan tool.
#[derive(Serialize, Deserialize, TS, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PlanItem {
    pub text: String,
    #[serde(default)]
    pub status: PlanStatus,
}

#[derive(Serialize, Deserialize, TS, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum LogKind {
    /// What the agent said it's doing.
    Status,
    Tool,
    Result,
    Retry,
    Approval,
    Error,
    Note,
}

#[derive(Serialize, Deserialize, TS, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct LogEntry {
    #[ts(type = "number")]
    pub at: i64,
    pub kind: LogKind,
    pub text: String,
}

#[derive(Serialize, Deserialize, TS, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Counters {
    /// Model calls this round.
    pub steps: u32,
    /// Tool calls this round.
    pub tool_calls: u32,
    /// Spending over the agent's whole life; never reset.
    #[ts(type = "number")]
    pub tokens: u64,
    pub cost: f64,
    /// False once any call had no known price.
    pub cost_known: bool,
    /// Budget added by "raise the limit and continue once".
    #[ts(type = "number")]
    pub extra_tokens: u64,
    pub extra_cost: f64,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Agent {
    pub id: String,
    pub batch: String,
    /// Position in its batch, for sequential runs.
    pub order: u32,
    pub name: String,
    pub goal: String,
    /// Tool groups it may use.
    pub tools: Vec<String>,
    /// Open-ended work (an app, a site): stays open for changes when done.
    pub keep_open: bool,
    /// Extra context, e.g. what the user circled (a JPEG, base64).
    pub image: Option<String>,
    /// Agents (ids) this one waits for; it starts with their results.
    #[serde(default)]
    pub after: Vec<String>,
    /// The results it was handed at the start, added to its task.
    #[serde(default)]
    pub handoff: Option<String>,
    /// Set on a helper started by an orchestrating agent (one level only).
    #[serde(default)]
    pub parent: Option<String>,
    /// The parent's tool call that started this helper.
    #[serde(default)]
    pub delegation: Option<String>,
    /// Its checklist, kept with the plan tool; shown to it every step.
    #[serde(default)]
    pub plan: Vec<PlanItem>,
    /// Things it wrote down with the note tool; shown to it every step.
    #[serde(default)]
    pub notes: Vec<String>,

    pub status: Status,
    pub status_line: String,
    pub status_at: i64,
    pub stop: Option<Stop>,
    pub result: Option<String>,
    pub suggestions: Vec<String>,
    pub error: Option<String>,
    pub pending: Option<Pending>,
    /// Something new happened that the user hasn't looked at.
    pub unseen: bool,
    /// Taken off the dock by the user; still in the panel.
    #[serde(default)]
    pub dismissed: bool,

    pub created: i64,
    pub finished: Option<i64>,
    /// Time spent running, across restarts, for the time limit.
    pub active_ms: u64,

    pub counters: Counters,
    /// How often each tool+input was called this round.
    pub repeats: HashMap<String, u32>,
    /// The last few things the agent said, to spot it repeating itself.
    pub recent_outputs: Vec<String>,
    /// Hashes of every output and tool result seen, to spot no progress.
    pub seen: HashSet<u64>,
    pub no_progress: u32,

    pub log: Vec<LogEntry>,
    pub messages: Vec<Message>,
    pub journal: Vec<FileOp>,
}

impl Agent {
    pub fn new(
        id: String,
        batch: String,
        order: u32,
        name: String,
        goal: String,
        now: i64,
    ) -> Self {
        Self {
            id,
            batch,
            order,
            name,
            goal,
            tools: Vec::new(),
            keep_open: false,
            image: None,
            after: Vec::new(),
            handoff: None,
            parent: None,
            delegation: None,
            plan: Vec::new(),
            notes: Vec::new(),
            status: Status::Queued,
            status_line: String::new(),
            status_at: 0,
            stop: None,
            result: None,
            suggestions: Vec::new(),
            error: None,
            pending: None,
            unseen: false,
            dismissed: false,
            created: now,
            finished: None,
            active_ms: 0,
            counters: Counters {
                cost_known: true,
                ..Default::default()
            },
            repeats: HashMap::new(),
            recent_outputs: Vec::new(),
            seen: HashSet::new(),
            no_progress: 0,
            log: Vec::new(),
            messages: Vec::new(),
            journal: Vec::new(),
        }
    }

    /// The plan as a checklist, for the agent and its tool result.
    pub fn plan_text(&self) -> String {
        let done = self
            .plan
            .iter()
            .filter(|i| i.status == PlanStatus::Done)
            .count();
        let mut out = format!("Plan: {done} of {} done.", self.plan.len());
        for i in &self.plan {
            let mark = match i.status {
                PlanStatus::Todo => " ",
                PlanStatus::Doing => ">",
                PlanStatus::Done => "x",
            };
            out += &format!("\n[{mark}] {}", i.text);
        }
        out
    }

    pub fn log(&mut self, at: i64, kind: LogKind, text: impl Into<String>) {
        const KEEP: usize = 400;
        self.log.push(LogEntry {
            at,
            kind,
            text: text.into(),
        });
        if self.log.len() > KEEP {
            self.log.drain(..self.log.len() - KEEP);
        }
    }

    /// A follow-up after the agent finished a round: the step and tool-call
    /// counters start over, spending never does.
    pub fn new_round(&mut self) {
        self.counters.steps = 0;
        self.counters.tool_calls = 0;
        self.repeats.clear();
        self.recent_outputs.clear();
        self.no_progress = 0;
        self.stop = None;
        self.error = None;
        self.result = None;
        self.suggestions.clear();
        self.finished = None;
    }
}

#[derive(Serialize, Deserialize, TS, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Batch {
    pub id: String,
    pub mode: RunMode,
    /// What the user asked for, in their words.
    pub request: String,
    #[ts(type = "number")]
    pub created: i64,
    /// The trigger that started it, if it started by itself.
    #[serde(default)]
    pub trigger: Option<String>,
}

/// What the dock, cards and panel show about an agent.
#[derive(Serialize, TS, Clone, Debug)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AgentView {
    pub id: String,
    pub batch: String,
    pub order: u32,
    pub name: String,
    pub goal: String,
    pub tools: Vec<String>,
    pub keep_open: bool,
    /// Agents it waits for, by id.
    pub after: Vec<String>,
    /// The orchestrating agent, for a helper.
    pub parent: Option<String>,
    pub status: Status,
    pub status_line: String,
    pub plan: Vec<PlanItem>,
    pub stop: Option<Stop>,
    pub result: Option<String>,
    pub suggestions: Vec<String>,
    pub error: Option<String>,
    pub pending: Option<Pending>,
    pub unseen: bool,
    pub dismissed: bool,
    #[ts(type = "number")]
    pub created: i64,
    #[ts(type = "number | null")]
    pub finished: Option<i64>,
    #[ts(type = "number")]
    pub active_ms: u64,
    pub counters: Counters,
    pub max_steps: u32,
    pub log: Vec<LogEntry>,
    /// File changes that can be undone.
    pub changes: u32,
    /// Files it made (a CSV, a report), newest last.
    pub files: Vec<String>,
}

impl Agent {
    pub fn view(&self, max_steps: u32) -> AgentView {
        AgentView {
            id: self.id.clone(),
            batch: self.batch.clone(),
            order: self.order,
            name: self.name.clone(),
            goal: self.goal.clone(),
            tools: self.tools.clone(),
            keep_open: self.keep_open,
            after: self.after.clone(),
            parent: self.parent.clone(),
            status: self.status,
            status_line: self.status_line.clone(),
            plan: self.plan.clone(),
            stop: self.stop.clone(),
            result: self.result.clone(),
            suggestions: self.suggestions.clone(),
            error: self.error.clone(),
            pending: self.pending.clone(),
            unseen: self.unseen,
            dismissed: self.dismissed,
            created: self.created,
            finished: self.finished,
            active_ms: self.active_ms,
            counters: self.counters.clone(),
            max_steps,
            log: self.log.clone(),
            changes: self.journal.len() as u32,
            files: self
                .journal
                .iter()
                .filter_map(|op| match op {
                    FileOp::Created { path }
                        if std::path::Path::new(path).extension().is_some() =>
                    {
                        Some(path.clone())
                    }
                    _ => None,
                })
                .collect(),
        }
    }
}
