//! Templates: tasks to start again and again, with blanks to fill in. The
//! built-in ones live here; the user's own are in settings. Starting one
//! fills in the blanks and shows the usual plan card, without asking a
//! model to plan it.

use std::collections::HashMap;

use serde::Serialize;
use tauri::{AppHandle, Manager};
use ts_rs::TS;

use super::planner::{self, Plan, PlanAgent};
use super::tools::files::expand_home;
use super::{AgentsState, RunMode};
use crate::settings::schema::{ParamKind, Template, TemplateAgent, TemplateParam};
use crate::settings::{Settings, SettingsStore};

fn param(key: &str, label: &str, kind: ParamKind, default: &str, required: bool) -> TemplateParam {
    TemplateParam {
        key: key.into(),
        label: label.into(),
        kind,
        default: default.into(),
        options: Vec::new(),
        required,
    }
}

fn agent(name: &str, goal: &str, tools: &[&str], after: &[&str]) -> TemplateAgent {
    TemplateAgent {
        name: name.into(),
        goal: goal.into(),
        tools: tools.iter().map(|t| t.to_string()).collect(),
        keep_open: false,
        after: after.iter().map(|t| t.to_string()).collect(),
    }
}

/// The templates Helpy comes with.
pub fn builtins() -> Vec<Template> {
    vec![
        Template {
            id: "tidy-folder".into(),
            name: "Tidy a folder".into(),
            description: "Sorts loose files into folders by type. Every move can be undone in one click.".into(),
            params: vec![param("folder", "Folder", ParamKind::Folder, "~/Desktop", true)],
            agents: vec![agent(
                "Tidy",
                "Sort the loose files directly in {folder} into subfolders by type: Documents, Images, Videos, Audio, \
                 Archives, Installers, Code and Other (make only the ones needed; reuse existing folders with those \
                 names). Leave existing folders and their contents alone. Delete only obvious temporary files \
                 (.tmp, .crdownload, .part, ~$ lock files). Finish with a short list of what went where.",
                &["files"],
                &[],
            )],
            folders: vec!["{folder}".into()],
        },
        Template {
            id: "research-shortlist".into(),
            name: "Research a shortlist".into(),
            description: "Searches the web and comes back with a short, compared list with prices and links.".into(),
            params: vec![
                param("topic", "What to find", ParamKind::Text, "", true),
                param("count", "How many", ParamKind::Number, "5", true),
                param("criteria", "What matters (budget, size, where…)", ParamKind::LongText, "", false),
            ],
            agents: vec![agent(
                "Research",
                "Find the {count} best options for: {topic}. What matters: {criteria}. Compare them in a short table \
                 with price, the main pros and cons, and a link for each, then say which you'd pick and why.",
                &["search", "web"],
                &[],
            )],
            folders: Vec::new(),
        },
        Template {
            id: "compare-two".into(),
            name: "Compare two options".into(),
            description: "Two agents research both sides at the same time, then a third writes up the comparison.".into(),
            params: vec![
                param("a", "First option", ParamKind::Text, "", true),
                param("b", "Second option", ParamKind::Text, "", true),
                param("focus", "What matters", ParamKind::LongText, "", false),
            ],
            agents: vec![
                agent("Option A", "Research {a}: strengths, weaknesses, cost and what users say. Focus on: {focus}.", &["search", "web"], &[]),
                agent("Option B", "Research {b}: strengths, weaknesses, cost and what users say. Focus on: {focus}.", &["search", "web"], &[]),
                agent(
                    "Comparison",
                    "Compare {a} and {b} from the research you're given: a side-by-side table, then a clear \
                     recommendation for someone who cares about: {focus}.",
                    &[],
                    &["Option A", "Option B"],
                ),
            ],
            folders: Vec::new(),
        },
        Template {
            id: "morning-briefing".into(),
            name: "Morning briefing".into(),
            description: "Today's calendar and the email that needs you, in one short summary.".into(),
            params: Vec::new(),
            agents: vec![agent(
                "Briefing",
                "Give me a short morning briefing: today's calendar events with times, then unread email from the last \
                 day that needs a reply or action (sender, subject, what's needed). Skip newsletters and notifications.",
                &["calendar", "gmail"],
                &[],
            )],
            folders: Vec::new(),
        },
    ]
}

/// Built-in templates first, then the user's.
pub fn all(s: &Settings) -> Vec<Template> {
    let mut out = builtins();
    out.extend(s.agents.templates.iter().cloned());
    out
}

/// The {key}s a text uses.
fn placeholders(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(i) = rest.find('{') {
        rest = &rest[i + 1..];
        let Some(j) = rest.find('}') else { break };
        let key = &rest[..j];
        if !key.is_empty() && key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            out.push(key.to_string());
        }
        rest = &rest[j + 1..];
    }
    out
}

/// Whether a template can be used; the message says what's wrong.
pub fn check(t: &Template) -> Result<(), String> {
    let name = if t.name.trim().is_empty() {
        t.id.as_str()
    } else {
        t.name.as_str()
    };
    if t.id.trim().is_empty() || t.name.trim().is_empty() {
        return Err(format!("{name}: every template needs a name"));
    }
    if t.agents.is_empty() || t.agents.len() > 5 {
        return Err(format!("{name}: a template has 1 to 5 agents"));
    }
    let mut keys: Vec<&str> = Vec::new();
    for p in &t.params {
        if p.key.is_empty() || !p.key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            return Err(format!(
                "{name}: \"{}\" can't be a blank's name (letters, digits and _ only)",
                p.key
            ));
        }
        if keys.contains(&p.key.as_str()) {
            return Err(format!("{name}: the blank \"{}\" is there twice", p.key));
        }
        if p.kind == ParamKind::Choice && p.options.is_empty() {
            return Err(format!("{name}: the choice \"{}\" has no options", p.label));
        }
        keys.push(&p.key);
    }
    let texts = t
        .agents
        .iter()
        .flat_map(|a| [&a.name, &a.goal])
        .chain(&t.folders);
    for text in texts {
        if let Some(k) = placeholders(text)
            .into_iter()
            .find(|k| !keys.contains(&k.as_str()))
        {
            return Err(format!(
                "{name}: {{{k}}} is used but there's no blank called {k}"
            ));
        }
    }
    for (i, a) in t.agents.iter().enumerate() {
        if a.goal.trim().is_empty() {
            return Err(format!("{name}: every agent needs a goal"));
        }
        for dep in &a.after {
            if !t.agents[..i].iter().any(|o| &o.name == dep) {
                return Err(format!(
                    "{name}: {} can only wait for agents listed before it",
                    a.name
                ));
            }
        }
    }
    Ok(())
}

/// The user's values with defaults filled in and checked.
pub fn values(
    t: &Template,
    given: &HashMap<String, String>,
) -> Result<HashMap<String, String>, String> {
    let mut out = HashMap::new();
    for p in &t.params {
        let v = given
            .get(&p.key)
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| p.default.clone());
        if v.is_empty() && p.required {
            return Err(format!("Fill in \"{}\"", p.label));
        }
        match p.kind {
            ParamKind::Number if !v.is_empty() && v.parse::<f64>().is_err() => {
                return Err(format!("\"{}\" should be a number", p.label));
            }
            ParamKind::Choice if !v.is_empty() && !p.options.contains(&v) => {
                return Err(format!(
                    "\"{}\" should be one of: {}",
                    p.label,
                    p.options.join(", ")
                ));
            }
            _ => {}
        }
        let v = if p.kind == ParamKind::Folder && !v.is_empty() {
            expand_home(&v).display().to_string()
        } else {
            v
        };
        out.insert(p.key.clone(), v);
    }
    Ok(out)
}

/// Fills in {key}s; an empty optional value reads as "no preference".
fn fill(text: &str, values: &HashMap<String, String>) -> String {
    let mut out = text.to_string();
    for (k, v) in values {
        let v = if v.is_empty() {
            "no preference"
        } else {
            v.as_str()
        };
        out = out.replace(&format!("{{{k}}}"), v);
    }
    out
}

/// The template's agents, folders and run mode for these values.
pub fn build(
    t: &Template,
    given: &HashMap<String, String>,
) -> Result<(RunMode, Vec<PlanAgent>, Vec<String>), String> {
    check(t)?;
    let v = values(t, given)?;
    let agents: Vec<PlanAgent> = t
        .agents
        .iter()
        .map(|a| PlanAgent {
            name: fill(&a.name, &v),
            goal: fill(&a.goal, &v),
            tools: a.tools.clone(),
            keep_open: a.keep_open,
            after: a.after.iter().map(|n| fill(n, &v)).collect(),
        })
        .collect();
    let mode = if agents.len() == 1 {
        RunMode::Single
    } else {
        RunMode::Parallel
    };
    Ok((
        mode,
        agents,
        t.folders.iter().map(|f| fill(f, &v)).collect(),
    ))
}

/// A template for the planner's prompt: its id, what it's for, its blanks.
pub fn for_planner(t: &Template) -> String {
    let blanks: Vec<String> = t
        .params
        .iter()
        .map(|p| {
            format!(
                "{} ({}{})",
                p.key,
                p.label,
                if p.required { "" } else { ", optional" }
            )
        })
        .collect();
    format!(
        "- \"{}\": {} {}{}\n",
        t.id,
        t.name,
        t.description,
        if blanks.is_empty() {
            String::new()
        } else {
            format!(" Blanks: {}.", blanks.join(", "))
        }
    )
}

// ---------- Commands ----------

#[derive(Serialize, TS, Clone, Debug)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct TemplateInfo {
    pub template: Template,
    pub builtin: bool,
}

/// Every tool group agents can be given, for the template editor.
#[tauri::command]
pub fn agents_tool_groups(app: AppHandle) -> Vec<super::tools::ToolGroup> {
    let s = app.state::<SettingsStore>().get();
    let mut groups = super::tools::groups(&s.agents);
    groups.extend(crate::connectors::external(&app, &s).groups());
    groups
}

#[tauri::command]
pub fn agents_templates(app: AppHandle) -> Vec<TemplateInfo> {
    let s = app.state::<SettingsStore>().get();
    let n = builtins().len();
    all(&s)
        .into_iter()
        .enumerate()
        .map(|(i, template)| TemplateInfo {
            template,
            builtin: i < n,
        })
        .collect()
}

/// Starts a template: shows its plan card (or starts it when settings allow).
#[tauri::command]
pub fn agents_template_plan(
    app: AppHandle,
    id: String,
    values: HashMap<String, String>,
) -> Result<Plan, String> {
    let s = app.state::<SettingsStore>().get();
    let t = all(&s)
        .into_iter()
        .find(|t| t.id == id)
        .ok_or("That template is gone")?;
    let (mode, agents, folders) = build(&t, &values)?;
    let request = match t
        .params
        .first()
        .and_then(|p| values.get(&p.key))
        .filter(|v| !v.trim().is_empty())
    {
        Some(v) => format!("{}: {}", t.name, v.trim()),
        None => t.name.clone(),
    };
    planner::offer(
        &app,
        &s,
        &request,
        mode,
        agents,
        folders,
        String::new(),
        None,
    )
}

/// Makes a template from an agent's request: its agents (not helpers),
/// their goals, tools and order. Blanks can be added in settings.
#[tauri::command]
pub fn agents_save_template(app: AppHandle, id: String) -> Result<Template, String> {
    let state = app.state::<AgentsState>();
    let (batch, request, agents) = {
        let agents = state.agents.lock().unwrap();
        let a = agents.get(&id).ok_or("That agent is gone")?;
        let request = state
            .batches
            .lock()
            .unwrap()
            .get(&a.batch)
            .map(|b| b.request.clone())
            .unwrap_or_default();
        let mut list: Vec<super::model::Agent> = agents
            .values()
            .filter(|o| o.batch == a.batch && o.parent.is_none())
            .cloned()
            .collect();
        list.sort_by_key(|o| o.order);
        (a.batch.clone(), request, list)
    };
    let names: HashMap<String, String> = agents
        .iter()
        .map(|a| (a.id.clone(), a.name.clone()))
        .collect();
    let mut s = app.state::<SettingsStore>().get();
    let base: String = request
        .chars()
        .take(40)
        .collect::<String>()
        .trim()
        .to_string();
    let name = if base.is_empty() {
        agents[0].name.clone()
    } else {
        base
    };
    let mut chars = name.chars();
    let name: String = chars
        .next()
        .map(|c| c.to_uppercase().chain(chars).collect())
        .unwrap_or_default();
    let t = Template {
        id: super::new_id("t"),
        name,
        description: format!("Saved from \"{}\".", request.trim()),
        params: Vec::new(),
        agents: agents
            .iter()
            .map(|a| TemplateAgent {
                name: a.name.clone(),
                goal: a.goal.clone(),
                tools: a.tools.clone(),
                keep_open: a.keep_open,
                after: a
                    .after
                    .iter()
                    .filter_map(|d| names.get(d).cloned())
                    .collect(),
            })
            .collect(),
        folders: Vec::new(),
    };
    let _ = batch;
    s.agents.templates.push(t.clone());
    let value = serde_json::to_value(&s.agents.templates).unwrap_or_default();
    crate::settings::settings_set(app.clone(), "agents.templates".into(), value)
        .map_err(|e| e.first().map(|e| e.message.clone()).unwrap_or_default())?;
    Ok(t)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn built_in_templates_are_valid() {
        for t in builtins() {
            check(&t).unwrap();
        }
    }

    #[test]
    fn fills_blanks_with_values_and_defaults() {
        let t = builtins()
            .into_iter()
            .find(|t| t.id == "research-shortlist")
            .unwrap();
        let mut given = HashMap::new();
        assert_eq!(build(&t, &given).unwrap_err(), "Fill in \"What to find\"");
        given.insert("topic".to_string(), "standing desks".to_string());
        let (mode, agents, _) = build(&t, &given).unwrap();
        assert_eq!(mode, RunMode::Single);
        assert!(agents[0].goal.starts_with(
            "Find the 5 best options for: standing desks. What matters: no preference."
        ));
        given.insert("count".to_string(), "lots".to_string());
        assert!(build(&t, &given).is_err());
    }

    #[test]
    fn folders_expand_and_graphs_keep_their_order() {
        let tidy = builtins()
            .into_iter()
            .find(|t| t.id == "tidy-folder")
            .unwrap();
        let (_, agents, folders) = build(&tidy, &HashMap::new()).unwrap();
        assert!(folders[0].ends_with("Desktop") && !folders[0].starts_with('~'));
        assert!(agents[0].goal.contains(&folders[0]));
        let cmp = builtins()
            .into_iter()
            .find(|t| t.id == "compare-two")
            .unwrap();
        let given = HashMap::from([
            ("a".to_string(), "Mac".to_string()),
            ("b".to_string(), "PC".to_string()),
        ]);
        let (mode, agents, _) = build(&cmp, &given).unwrap();
        assert_eq!(mode, RunMode::Parallel);
        assert_eq!(agents[2].after, vec!["Option A", "Option B"]);
    }

    #[test]
    fn checks_blanks_and_order() {
        let mut t = builtins().remove(0);
        t.agents[0].goal = "Tidy {place}".into();
        assert!(check(&t).unwrap_err().contains("{place}"));
        let mut t = builtins().remove(2);
        t.agents.swap(0, 2);
        assert!(check(&t).unwrap_err().contains("listed before"));
        let mut t = builtins().remove(0);
        t.params.push(t.params[0].clone());
        assert!(check(&t).unwrap_err().contains("twice"));
    }
}
