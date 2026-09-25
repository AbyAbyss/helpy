//! Turning a request ("clean up my desktop", "research accountants, compare
//! prices, draft an email") into agents, and the plan card the user
//! confirms before anything starts.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager};
use tokio_util::sync::CancellationToken;
use ts_rs::TS;

use super::model::RunMode;
use super::tools::{self, files::expand_home, ToolGroup};
use super::{AgentsState, NewAgent};
use crate::ai::call;
use crate::ai::types::{ChatRequest, Message, Part, Role};
use crate::settings::schema::{Ai, ConfirmPlans, DefaultRunMode, ModelRef};
use crate::settings::{Settings, SettingsStore};

pub const PLAN_EVENT: &str = "agents://plan";
const MAX_AGENTS: usize = 5;

#[derive(Serialize, Deserialize, TS, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PlanAgent {
    pub name: String,
    pub goal: String,
    pub tools: Vec<String>,
    pub keep_open: bool,
    /// Names of earlier agents in the plan it waits for.
    #[serde(default)]
    pub after: Vec<String>,
}

/// A plan waiting for the user's go-ahead.
#[derive(Serialize, TS, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Plan {
    pub id: String,
    pub request: String,
    pub mode: RunMode,
    pub agents: Vec<PlanAgent>,
    /// Folders the agents need that aren't approved yet.
    pub new_folders: Vec<String>,
    /// Actions that will wait for the user's OK.
    pub asks: Vec<String>,
    /// Tool groups the plan uses, with their labels, for the card.
    pub groups: Vec<ToolGroup>,
    /// Something the user circled or had on screen goes along.
    pub has_image: bool,
    /// One sentence for the user about what's planned.
    pub reply: String,
    /// It started without the card (read-only work, settings allow it).
    pub started: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawPlan {
    #[serde(default)]
    mode: Option<String>,
    agents: Vec<RawAgent>,
    #[serde(default)]
    folders: Vec<String>,
    #[serde(default)]
    reply: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawAgent {
    name: String,
    goal: String,
    #[serde(default)]
    tools: Vec<String>,
    #[serde(default)]
    keep_open: bool,
    #[serde(default)]
    after: Vec<String>,
}

pub fn planning_models(ai: &Ai) -> Result<Vec<ModelRef>, String> {
    let primary = [&ai.routing.agent_planning, &ai.routing.ask]
        .into_iter()
        .flatten()
        .find(|r| ai.model(r).is_some())
        .cloned()
        .ok_or("Choose a model for agent planning (or questions) in Settings → AI providers")?;
    let mut out = vec![primary];
    for f in &ai.fallback_chain {
        if ai.model(f).is_some() && !out.contains(f) {
            out.push(f.clone());
        }
    }
    Ok(out)
}

pub fn system_prompt(s: &Settings, groups: &[ToolGroup]) -> String {
    let templates: String = super::templates::all(s)
        .iter()
        .map(super::templates::for_planner)
        .collect();
    let list: String = groups
        .iter()
        .map(|g| {
            let note = if g.connected {
                ""
            } else {
                " (not connected yet; still pick it if the task needs it)"
            };
            format!("- \"{}\": {}{note}\n", g.id, g.about)
        })
        .collect();
    let mode = match s.agents.default_mode {
        DefaultRunMode::Auto => "Pick the mode that fits",
        DefaultRunMode::Parallel => "Prefer \"parallel\" unless agents depend on each other",
        DefaultRunMode::Sequential => "Prefer \"sequential\"",
    };
    format!(
        "You plan background agents for Helpy, a desktop assistant. Turn the user's request into 1 to {MAX_AGENTS} \
         agents. Most requests need one agent; use several only when the user asks for separate pieces of work.\n\n\
         Tools agents can have (give each agent only what it needs):\n{list}\n\
         Modes: \"single\" (one agent), \"parallel\" (independent agents at once), \"sequential\" (one after another, in \
         order, each starting with what the one before found). {mode}. In a parallel plan, an agent can wait for \
         earlier agents and start with their results: list their names in its \"after\" (e.g. two researchers at once, \
         then a writer after both). For one big task with clearly separate parts, you may instead give a single agent \
         the \"team\" tool so it splits the work among helpers itself.\n\n\
         Write each goal as a complete, self-contained instruction with every detail from the request (names, dates, \
         numbers, places, format wanted). Names are one or two words, like \"Research\" or \"Desktop tidy\". Set \
         keepOpen to true for open-ended work the user will want to keep changing, such as building an app or a site.\n\
         If agents must work in folders, list them in \"folders\" as full paths (~ for the home folder); the user's \
         Desktop is ~/Desktop. Builder agents use the projects folder and don't need it listed.\n\n\
         Reply with only a JSON object: {{\"mode\": \"single\", \"agents\": [{{\"name\": \"...\", \"goal\": \"...\", \
         \"tools\": [\"...\"], \"keepOpen\": false, \"after\": []}}], \"folders\": [], \"reply\": \"one short sentence telling the user \
         what you set up\"}}\n\n\
         If the request is clearly one of these templates, reply instead with only {{\"template\": \"id\", \"params\": \
         {{\"blank\": \"value\"}}, \"reply\": \"one short sentence\"}}, filling the blanks from the request (leave out \
         ones it doesn't say):\n{templates}"
    )
}

/// Reads and checks the planner's JSON. Unknown tools are dropped; the mode
/// is single for one agent.
pub fn parse(
    text: &str,
    groups: &[ToolGroup],
    default_mode: DefaultRunMode,
) -> Result<(RunMode, Vec<PlanAgent>, Vec<String>, String), String> {
    let t = text.trim();
    let start = t.find('{').ok_or("The planner didn't send a plan")?;
    let end = t.rfind('}').ok_or("The planner didn't send a plan")?;
    let raw: RawPlan =
        serde_json::from_str(&t[start..=end]).map_err(|e| format!("The plan was garbled: {e}"))?;
    let known: Vec<&str> = groups.iter().map(|g| g.id.as_str()).collect();
    let mut agents: Vec<PlanAgent> = Vec::new();
    for a in raw.agents.into_iter().take(MAX_AGENTS) {
        let goal = a.goal.trim().to_string();
        if goal.is_empty() {
            continue;
        }
        let mut name: String = a.name.trim().chars().take(24).collect();
        if name.is_empty() {
            name = format!("Agent {}", agents.len() + 1);
        }
        while agents.iter().any(|o| o.name == name) {
            name = format!("{name} 2");
        }
        let mut tools: Vec<String> = a
            .tools
            .into_iter()
            .filter(|t| known.contains(&t.as_str()))
            .collect();
        tools.dedup();
        // Only agents listed before it, so a plan can't wait on itself.
        let after: Vec<String> = a
            .after
            .iter()
            .map(|n| n.trim())
            .filter_map(|n| {
                agents
                    .iter()
                    .find(|o| o.name.eq_ignore_ascii_case(n))
                    .map(|o| o.name.clone())
            })
            .collect();
        agents.push(PlanAgent {
            name,
            goal,
            tools,
            keep_open: a.keep_open,
            after,
        });
    }
    if agents.is_empty() {
        return Err("The planner didn't come up with any agents".into());
    }
    let mode = match (agents.len(), raw.mode.as_deref(), default_mode) {
        (1, _, _) => RunMode::Single,
        (_, Some("sequential"), _) => RunMode::Sequential,
        (_, Some("parallel"), _) => RunMode::Parallel,
        (_, _, DefaultRunMode::Sequential) => RunMode::Sequential,
        _ => RunMode::Parallel,
    };
    Ok((mode, agents, raw.folders, raw.reply))
}

/// Folders from the plan that aren't approved yet, as full paths.
fn unapproved(folders: &[String], approved: &[String]) -> Vec<String> {
    let approved: Vec<_> = approved.iter().map(|a| expand_home(a)).collect();
    let mut out: Vec<String> = Vec::new();
    for f in folders {
        // Rebuilding from components drops a trailing slash.
        let p: std::path::PathBuf = expand_home(f).components().collect();
        if !p.is_absolute() || approved.iter().any(|a| p.starts_with(a)) {
            continue;
        }
        let s = p.display().to_string();
        if !out.contains(&s) {
            out.push(s);
        }
    }
    out
}

/// Whether the plan can change anything (files, commands, reminders).
fn has_side_effects(agents: &[PlanAgent]) -> bool {
    agents
        .iter()
        .flat_map(|a| &a.tools)
        // Connectors and MCP servers can send, post and change things.
        .any(|t| !matches!(t.as_str(), "search" | "web"))
}

/// Plans a request with the planning model, then shows the plan card, or
/// starts right away when settings allow it. Returns the plan.
/// The address of the page the request is about, when it's about the page
/// in the user's browser and Helpy may look.
async fn open_page(s: &Settings, request: &str) -> Option<String> {
    if !crate::a11y::refers_to_page(request) || s.privacy.capture_paused {
        return None;
    }
    let url = crate::a11y::browser_url().await?;
    let lower = url.to_lowercase();
    let blocked = s
        .privacy
        .blocked_apps
        .iter()
        .map(|r| r.trim().to_lowercase())
        .any(|r| !r.is_empty() && lower.contains(&r));
    (!blocked).then_some(url)
}

pub async fn plan(app: &AppHandle, request: &str, image: Option<String>) -> Result<Plan, String> {
    let s = app.state::<SettingsStore>().get();
    let mut groups = tools::groups(&s.agents);
    groups.extend(crate::connectors::external(app, &s).groups());
    let models = planning_models(&s.ai)?;
    let mut parts = Vec::new();
    // The planner sees the picture only if its model can.
    let sees = s.ai.model(&models[0]).is_some_and(|(_, m)| m.vision);
    if let (Some(img), true) = (&image, sees) {
        parts.push(Part::Image {
            media_type: "image/jpeg".into(),
            data: img.clone(),
        });
    }
    let mut text = request.to_string();
    if let Some(url) = open_page(&s, request).await {
        text +=
            &format!("\n\n(The page open in the user's browser is {url}. \"This page\" means it.)");
    }
    if image.is_some() {
        text +=
            "\n\n(The agents will get a picture of what the user had on screen with their task.)";
    }
    parts.push(Part::Text(text));
    let req = ChatRequest {
        model: String::new(),
        system: system_prompt(&s, &groups),
        messages: vec![Message {
            role: Role::User,
            parts,
        }],
        tools: Vec::new(),
        max_tokens: s.ai.max_response_tokens.min(4000),
        temperature: 0.3,
    };
    let completion = call::stream(
        app,
        &s,
        "agent planning",
        &models,
        req,
        &CancellationToken::new(),
        &|_| {},
    )
    .await
    .map_err(|e| e.message)?;
    let text = completion.text();
    let (mode, agents, folders, reply) = match template_choice(&text) {
        Some((id, values, reply)) => {
            let t = super::templates::all(&s)
                .into_iter()
                .find(|t| t.id == id)
                .ok_or("The planner picked a template that doesn't exist")?;
            let (mode, agents, folders) = super::templates::build(&t, &values)?;
            (mode, agents, folders, reply)
        }
        None => parse(&text, &groups, s.agents.default_mode)?,
    };
    offer(app, &s, request, mode, agents, folders, reply, image)
}

/// A template the planner picked: {"template": id, "params": {...}}.
fn template_choice(
    text: &str,
) -> Option<(String, std::collections::HashMap<String, String>, String)> {
    let t = text.trim();
    let v: Value = serde_json::from_str(&t[t.find('{')?..=t.rfind('}')?]).ok()?;
    let id = v["template"].as_str()?.to_string();
    let values = v["params"]
        .as_object()
        .map(|m| {
            m.iter()
                .map(|(k, v)| {
                    (
                        k.clone(),
                        v.as_str()
                            .map(String::from)
                            .unwrap_or_else(|| v.to_string()),
                    )
                })
                .collect()
        })
        .unwrap_or_default();
    Some((id, values, v["reply"].as_str().unwrap_or("").to_string()))
}

/// Shows the plan card for these agents, or starts them when settings
/// allow it. Used by the planner and by templates.
#[allow(clippy::too_many_arguments)]
pub fn offer(
    app: &AppHandle,
    s: &Settings,
    request: &str,
    mode: RunMode,
    agents: Vec<PlanAgent>,
    folders: Vec<String>,
    reply: String,
    image: Option<String>,
) -> Result<Plan, String> {
    let mut groups = tools::groups(&s.agents);
    groups.extend(crate::connectors::external(app, s).groups());
    let used: Vec<ToolGroup> = groups
        .iter()
        .filter(|g| agents.iter().any(|a| a.tools.contains(&g.id)))
        .cloned()
        .collect();
    let mut asks: Vec<String> = used.iter().flat_map(|g| g.asks.clone()).collect();
    asks.dedup();
    let plan = Plan {
        id: super::new_id("p"),
        request: request.to_string(),
        mode,
        new_folders: unapproved(&folders, &s.agents.approved_folders),
        asks,
        groups: used,
        has_image: image.is_some(),
        reply: if reply.trim().is_empty() {
            format!(
                "I've planned {} agent{}.",
                agents.len(),
                if agents.len() == 1 { "" } else { "s" }
            )
        } else {
            reply.trim().to_string()
        },
        started: false,
        agents,
    };
    let quiet = s.agents.confirm_plans == ConfirmPlans::SideEffectsOnly
        && !has_side_effects(&plan.agents)
        && plan.new_folders.is_empty()
        && plan.groups.iter().all(|g| g.connected);
    let mut plan = plan;
    plan.started = quiet;
    *app.state::<AgentsState>().plan.lock().unwrap() = Some((plan.clone(), image));
    if quiet {
        start(app, &plan.id, plan.mode)?;
    } else {
        show_card(app, &plan);
    }
    Ok(plan)
}

fn show_card(app: &AppHandle, plan: &Plan) {
    let _ = app.emit(PLAN_EVENT, Some(plan.clone()));
    crate::windows::show_plan(app);
}

fn close_card(app: &AppHandle) {
    let _ = app.emit(PLAN_EVENT, None::<Plan>);
    crate::windows::hide_plan(app);
}

/// Starts the plan on the card: approves its new folders and queues agents.
fn start(app: &AppHandle, id: &str, mode: RunMode) -> Result<(), String> {
    let (plan, image) = {
        let state = app.state::<AgentsState>();
        let mut slot = state.plan.lock().unwrap();
        match slot.take() {
            Some((p, img)) if p.id == id => (p, img),
            other => {
                *slot = other;
                return Err("That plan was replaced".into());
            }
        }
    };
    if !plan.new_folders.is_empty() {
        let mut s = app.state::<SettingsStore>().get();
        for f in &plan.new_folders {
            let _ = std::fs::create_dir_all(f);
            if !s.agents.approved_folders.contains(f) {
                s.agents.approved_folders.push(f.clone());
            }
        }
        let value = serde_json::to_value(&s.agents.approved_folders).unwrap_or(Value::Null);
        crate::settings::settings_set(app.clone(), "agents.approvedFolders".into(), value)
            .map_err(|e| format!("Couldn't approve the folders: {e:?}"))?;
    }
    let mode = if plan.agents.len() == 1 {
        RunMode::Single
    } else {
        mode
    };
    let names: Vec<String> = plan.agents.iter().map(|a| a.name.clone()).collect();
    let agents = plan
        .agents
        .into_iter()
        .map(|a| NewAgent {
            after: a
                .after
                .iter()
                .filter_map(|n| names.iter().position(|x| x == n))
                .collect(),
            name: a.name,
            goal: a.goal,
            tools: a.tools,
            keep_open: a.keep_open,
        })
        .collect();
    super::create(app, &plan.request, mode, agents, image);
    close_card(app);
    Ok(())
}

/// Voice replies while a plan card is open: "yes, go" starts it, "no" or
/// "cancel" drops it. None when the words weren't for the card, otherwise
/// whether agents started.
pub fn voice_reply(app: &AppHandle, text: &str) -> Option<bool> {
    let plan = current(app)?;
    let t = text.to_lowercase();
    let words: Vec<&str> = t
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .collect();
    let short = words.len() <= 5;
    let has = |list: &[&str]| words.iter().any(|w| list.contains(w));
    if short && has(&["no", "cancel", "stop", "nope", "don't"]) {
        app.state::<AgentsState>().plan.lock().unwrap().take();
        close_card(app);
        return Some(false);
    }
    if short
        && has(&[
            "yes", "go", "start", "yeah", "yep", "ok", "okay", "sure", "do",
        ])
    {
        return Some(start(app, &plan.id, plan.mode).is_ok());
    }
    None
}

pub fn current(app: &AppHandle) -> Option<Plan> {
    app.state::<AgentsState>()
        .plan
        .lock()
        .unwrap()
        .as_ref()
        .map(|(p, _)| p.clone())
}

#[tauri::command]
pub fn agents_plan_current(app: AppHandle) -> Option<Plan> {
    current(&app)
}

#[tauri::command]
pub fn agents_plan_start(app: AppHandle, id: String, mode: RunMode) -> Result<(), String> {
    start(&app, &id, mode)
}

#[tauri::command]
pub fn agents_plan_cancel(app: AppHandle) {
    app.state::<AgentsState>().plan.lock().unwrap().take();
    close_card(&app);
}

/// A typed request from the agent panel.
#[tauri::command]
pub async fn agents_plan(app: AppHandle, request: String) -> Result<Plan, String> {
    if request.trim().is_empty() {
        return Err("Describe what the agent should do".into());
    }
    plan(&app, request.trim(), None).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::schema::Agents;

    fn groups() -> Vec<ToolGroup> {
        tools::groups(&Agents::default())
    }

    #[test]
    fn reads_a_plan_and_keeps_it_within_bounds() {
        let (mode, agents, folders, reply) = parse(
            r#"Here you go:
            ```json
            {"mode": "parallel", "agents": [
              {"name": "Research", "goal": "Find 5 UK contractor accountants", "tools": ["search", "web", "teleport"]},
              {"name": "Research", "goal": "Compare their prices", "tools": ["search"]},
              {"name": "", "goal": "  "}
            ], "folders": ["~/Desktop"], "reply": "Two agents are ready."}
            ```"#,
            &groups(),
            DefaultRunMode::Auto,
        )
        .unwrap();
        assert_eq!(mode, RunMode::Parallel);
        assert_eq!(agents.len(), 2);
        assert_eq!(agents[0].tools, ["search", "web"]);
        assert_eq!(agents[1].name, "Research 2");
        assert_eq!(folders, ["~/Desktop"]);
        assert_eq!(reply, "Two agents are ready.");
    }

    #[test]
    fn one_agent_is_single_and_garbage_is_an_error() {
        let (mode, ..) = parse(
            r#"{"mode": "parallel", "agents": [{"name": "Tidy", "goal": "Sort the desktop", "tools": ["files"]}]}"#,
            &groups(),
            DefaultRunMode::Auto,
        )
        .unwrap();
        assert_eq!(mode, RunMode::Single);
        let (mode, ..) = parse(
            r#"{"agents": [{"name": "A", "goal": "x"}, {"name": "B", "goal": "y"}]}"#,
            &groups(),
            DefaultRunMode::Sequential,
        )
        .unwrap();
        assert_eq!(mode, RunMode::Sequential);
        // "after" keeps only earlier agents, so there are no cycles.
        let (_, agents, _, _) = parse(
            r#"{"agents": [{"name": "A", "goal": "a", "after": ["B"]}, {"name": "B", "goal": "b", "after": ["a", "Nobody"]}]}"#,
            &groups(),
            DefaultRunMode::Auto,
        )
        .unwrap();
        assert!(agents[0].after.is_empty());
        assert_eq!(agents[1].after, vec!["A"]);
        assert!(parse("I can't do that", &groups(), DefaultRunMode::Auto).is_err());
        assert!(parse(r#"{"agents": []}"#, &groups(), DefaultRunMode::Auto).is_err());
        let many = format!(
            r#"{{"agents": [{}]}}"#,
            [r#"{"name": "A", "goal": "x"}"#; 9].join(",")
        );
        assert_eq!(
            parse(&many, &groups(), DefaultRunMode::Auto)
                .unwrap()
                .1
                .len(),
            MAX_AGENTS
        );
    }

    #[test]
    fn only_new_folders_need_approval() {
        let home = expand_home("~");
        let approved = vec![home.join("Documents").display().to_string()];
        let out = unapproved(
            &[
                "~/Desktop".into(),
                "~/Documents/Taxes".into(),
                "relative".into(),
                "~/Desktop/".into(),
            ],
            &approved,
        );
        assert_eq!(out, [home.join("Desktop").display().to_string()]);
        assert!(has_side_effects(&[PlanAgent {
            name: "a".into(),
            goal: "g".into(),
            tools: vec!["files".into()],
            keep_open: false,
            after: Vec::new(),
        }]));
        assert!(!has_side_effects(&[PlanAgent {
            name: "a".into(),
            goal: "g".into(),
            tools: vec!["search".into()],
            keep_open: false,
            after: Vec::new(),
        }]));
    }
}
