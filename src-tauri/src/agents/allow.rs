//! "Always allow": the standing rule an approval can turn on, so the same
//! kind of action stops asking, and the list in Settings → Permissions
//! that turns each one back to asking.

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};
use ts_rs::TS;

use crate::settings::schema::{Approvals, Rule, ShellPolicy};
use crate::settings::{Settings, SettingsStore};

/// A setting that "Always allow" changes.
#[derive(Serialize, Deserialize, TS, Clone, Debug, PartialEq, Eq, Hash)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[ts(export)]
pub enum AllowTarget {
    /// One of Settings → Permissions' rules, by its camelCase name
    /// ("fileDeletes", "deepResearch").
    Approval { key: String },
    /// A built-in connector's action ("notion", "create_page").
    Connector { id: String, action: String },
    /// An MCP server's tool, by the server's id.
    Mcp { server: String, tool: String },
    /// Shell commands starting with this ("git status").
    Command { prefix: String },
    /// Looking at the screen without asking each time.
    Screen,
}

/// What an approval offers to allow from now on.
#[derive(Serialize, Deserialize, TS, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AlwaysAllow {
    pub target: AllowTarget,
    /// What it covers, for the button: "deleting files", "Notion: Create a page".
    pub label: String,
}

impl AlwaysAllow {
    pub fn approval(key: &str) -> Option<Self> {
        let label = approval_label(key)?;
        Some(Self {
            target: AllowTarget::Approval { key: key.into() },
            label: label.into(),
        })
    }
}

/// The approval rules, in the order Settings shows them, with what each covers.
pub const APPROVALS: &[(&str, &str)] = &[
    ("fileChanges", "changing files"),
    ("fileDeletes", "deleting files"),
    ("reminders", "adding reminders and events"),
    ("browserForms", "typing into websites"),
    ("builds", "building apps"),
    ("deepResearch", "deep research with Tavily"),
    ("openLinks", "opening links in your browser"),
    ("appsAndWindows", "opening apps and arranging windows"),
    ("quitApps", "quitting apps"),
    ("screenControl", "clicking and typing on your screen"),
];

fn approval_label(key: &str) -> Option<&'static str> {
    APPROVALS.iter().find(|(k, _)| *k == key).map(|(_, l)| *l)
}

fn set_approval(a: &Approvals, key: &str, rule: Rule) -> Approvals {
    let mut v = serde_json::to_value(a).expect("approvals serialize");
    if v.get(key).is_some() {
        v[key] = serde_json::to_value(rule).expect("rule serializes");
    }
    serde_json::from_value(v).unwrap_or_else(|_| a.clone())
}

pub fn approval_rule(a: &Approvals, key: &str) -> Option<Rule> {
    let v = serde_json::to_value(a).ok()?;
    serde_json::from_value(v.get(key)?.clone()).ok()
}

/// Programs whose every use can't be allowed at once: they delete, change
/// owners, run other code, or need the administrator.
const NEVER_STANDING: &[&str] = &[
    "rm", "rmdir", "del", "erase", "sudo", "su", "doas", "dd", "mkfs", "fdisk", "diskutil", "format",
    "chmod", "chown", "kill", "killall", "pkill", "shutdown", "reboot", "halt", "launchctl", "sh", "bash",
    "zsh", "fish", "dash", "cmd", "powershell", "pwsh", "python", "python3", "node", "deno", "bun", "ruby",
    "perl", "php", "osascript", "eval", "exec", "env", "xargs", "nohup", "curl", "wget", "ssh", "scp",
    "mv", "cp", "find", "git-filter-repo", "crontab",
];

/// The start of a command that "Always allow" may allowlist: the program,
/// and its subcommand when it has one ("git status", "npm test"). None for
/// chained commands and for programs that are never allowed outright.
pub fn command_prefix(cmd: &str) -> Option<String> {
    let cmd = cmd.trim();
    if cmd.is_empty() || cmd.contains([';', '|', '&', '`', '>', '<', '\n']) || cmd.contains("$(") {
        return None;
    }
    let mut words = cmd.split_whitespace();
    let program = words.next()?;
    let name = program.rsplit('/').next().unwrap_or(program).to_lowercase();
    if NEVER_STANDING.contains(&name.as_str()) || program.contains(['=', '$', '*', '~']) {
        return None;
    }
    let sub = words
        .next()
        .filter(|w| w.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == ':') && !w.starts_with('-'));
    Some(match sub {
        Some(sub) => format!("{program} {sub}"),
        None => program.to_string(),
    })
}

/// Settings with `target` set to allow.
pub fn apply(s: &Settings, target: &AllowTarget) -> Settings {
    let mut next = s.clone();
    match target {
        AllowTarget::Approval { key } => {
            next.agents.approvals = set_approval(&s.agents.approvals, key, Rule::Allow)
        }
        AllowTarget::Connector { id, action } => {
            next.connectors
                .builtin
                .entry(id.clone())
                .or_default()
                .rules
                .insert(action.clone(), Rule::Allow);
        }
        AllowTarget::Mcp { server, tool } => {
            if let Some(m) = next.connectors.mcp.iter_mut().find(|m| &m.id == server) {
                m.rules.insert(tool.clone(), Rule::Allow);
            }
        }
        AllowTarget::Command { prefix } => {
            if !next.agents.shell_allowlist.contains(prefix) {
                next.agents.shell_allowlist.push(prefix.clone());
            }
        }
        AllowTarget::Screen => {
            next.answer_style.screen_access = crate::settings::schema::ScreenAccess::WhenNeeded
        }
    }
    next
}

/// Settings with `target` asking again.
pub fn revert(s: &Settings, target: &AllowTarget) -> Settings {
    let mut next = s.clone();
    match target {
        AllowTarget::Approval { key } => {
            next.agents.approvals = set_approval(&s.agents.approvals, key, Rule::Ask)
        }
        AllowTarget::Connector { id, action } => {
            next.connectors
                .builtin
                .entry(id.clone())
                .or_default()
                .rules
                .insert(action.clone(), Rule::Ask);
        }
        AllowTarget::Mcp { server, tool } => {
            if let Some(m) = next.connectors.mcp.iter_mut().find(|m| &m.id == server) {
                m.rules.insert(tool.clone(), Rule::Ask);
            }
        }
        AllowTarget::Command { prefix } => next.agents.shell_allowlist.retain(|p| p != prefix),
        AllowTarget::Screen => {
            next.answer_style.screen_access = crate::settings::schema::ScreenAccess::Ask
        }
    }
    next
}

/// Whether the settings allow `target` without asking.
pub fn is_allowed(s: &Settings, target: &AllowTarget) -> bool {
    match target {
        AllowTarget::Approval { key } => approval_rule(&s.agents.approvals, key) == Some(Rule::Allow),
        AllowTarget::Connector { id, action } => {
            s.connectors.config(id).rules.get(action) == Some(&Rule::Allow)
        }
        AllowTarget::Mcp { server, tool } => s
            .connectors
            .mcp
            .iter()
            .any(|m| &m.id == server && m.rules.get(tool) == Some(&Rule::Allow)),
        AllowTarget::Command { prefix } => {
            s.agents.shell_policy != ShellPolicy::Never && s.agents.shell_allowlist.contains(prefix)
        }
        AllowTarget::Screen => {
            s.answer_style.screen_access != crate::settings::schema::ScreenAccess::Ask
        }
    }
}

/// Everything set to run without asking, for Settings → Permissions.
pub fn allowed(s: &Settings) -> Vec<AlwaysAllow> {
    let mut out = Vec::new();
    for (key, label) in APPROVALS {
        if approval_rule(&s.agents.approvals, key) == Some(Rule::Allow) {
            out.push(AlwaysAllow {
                target: AllowTarget::Approval { key: (*key).into() },
                label: capitalize(label),
            });
        }
    }
    for (id, config) in &s.connectors.builtin {
        let name = crate::connectors::find(id).map(|c| c.name().to_string()).unwrap_or_else(|| id.clone());
        for (action, rule) in &config.rules {
            if *rule == Rule::Allow {
                out.push(AlwaysAllow {
                    target: AllowTarget::Connector { id: id.clone(), action: action.clone() },
                    label: format!("{name}: {}", action.replace('_', " ")),
                });
            }
        }
    }
    for m in &s.connectors.mcp {
        for (tool, rule) in &m.rules {
            if *rule == Rule::Allow {
                out.push(AlwaysAllow {
                    target: AllowTarget::Mcp { server: m.id.clone(), tool: tool.clone() },
                    label: format!("{}: {tool}", m.name),
                });
            }
        }
    }
    if s.agents.shell_policy != ShellPolicy::Never {
        for prefix in &s.agents.shell_allowlist {
            out.push(AlwaysAllow {
                target: AllowTarget::Command { prefix: prefix.clone() },
                label: format!("Commands starting \"{prefix}\""),
            });
        }
    }
    out
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
}

/// Every permission back to its default: the rules, connector and MCP
/// overrides, and allowlisted commands.
pub fn reset(s: &Settings) -> Settings {
    let mut next = s.clone();
    next.agents.approvals = Approvals::default();
    for c in next.connectors.builtin.values_mut() {
        c.rules.clear();
    }
    for m in next.connectors.mcp.iter_mut() {
        m.rules.clear();
    }
    next.agents.shell_allowlist.clear();
    next
}

/// Saves `target` as allowed.
pub fn commit_allow(app: &AppHandle, target: &AllowTarget) -> Result<(), String> {
    let s = app.state::<SettingsStore>().get();
    crate::settings::commit(app, apply(&s, target)).map(|_| ()).map_err(errors)
}

fn errors(e: Vec<crate::settings::FieldError>) -> String {
    e.into_iter().map(|e| e.message).collect::<Vec<_>>().join("; ")
}

#[tauri::command]
pub fn permissions_allowed(store: tauri::State<SettingsStore>) -> Vec<AlwaysAllow> {
    allowed(&store.get())
}

#[tauri::command]
pub fn permissions_allow(app: AppHandle, target: AllowTarget) -> Result<(), String> {
    commit_allow(&app, &target)
}

#[tauri::command]
pub fn permissions_ask_again(app: AppHandle, target: AllowTarget) -> Result<(), String> {
    let s = app.state::<SettingsStore>().get();
    crate::settings::commit(&app, revert(&s, &target)).map(|_| ()).map_err(errors)
}

#[tauri::command]
pub fn permissions_reset(app: AppHandle) -> Result<Settings, String> {
    let s = app.state::<SettingsStore>().get();
    crate::settings::commit(&app, reset(&s)).map_err(errors)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allowing_and_asking_again_round_trip() {
        let s = Settings::default();
        for target in [
            AllowTarget::Approval { key: "deepResearch".into() },
            AllowTarget::Connector { id: "notion".into(), action: "create_page".into() },
            AllowTarget::Command { prefix: "git status".into() },
        ] {
            assert!(!is_allowed(&s, &target), "{target:?}");
            let on = apply(&s, &target);
            assert!(is_allowed(&on, &target), "{target:?}");
            assert!(allowed(&on).iter().any(|a| a.target == target));
            assert!(!is_allowed(&revert(&on, &target), &target), "{target:?}");
        }
        // Unknown keys change nothing.
        let same = apply(&s, &AllowTarget::Approval { key: "nope".into() });
        assert_eq!(same.agents.approvals, s.agents.approvals);
    }

    #[test]
    fn reset_puts_everything_back() {
        let s = apply(
            &apply(&Settings::default(), &AllowTarget::Approval { key: "fileDeletes".into() }),
            &AllowTarget::Command { prefix: "ls".into() },
        );
        let r = reset(&s);
        assert_eq!(r.agents.approvals, Approvals::default());
        assert!(r.agents.shell_allowlist.is_empty());
    }

    #[test]
    fn only_plain_safe_commands_get_a_standing_rule() {
        assert_eq!(command_prefix("git status --short").as_deref(), Some("git status"));
        assert_eq!(command_prefix("npm test").as_deref(), Some("npm test"));
        assert_eq!(command_prefix("ls -la ~/Desktop").as_deref(), Some("ls"));
        assert_eq!(command_prefix("rm -rf build"), None);
        assert_eq!(command_prefix("sudo ls"), None);
        assert_eq!(command_prefix("python3 x.py"), None);
        assert_eq!(command_prefix("git status && rm x"), None);
        assert_eq!(command_prefix("cat a > b"), None);
    }
}
