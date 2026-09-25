//! Shell commands, under the policy in Settings → Agents.

use std::path::Path;
use std::time::Duration;

use crate::agents::runner::ToolOutcome;
use crate::settings::schema::ShellPolicy;

const TIMEOUT: Duration = Duration::from_secs(120);
const MAX_OUTPUT: usize = 20_000;

/// What the policy says about a command, before any approval.
#[derive(Debug, PartialEq)]
pub enum Verdict {
    Run,
    Ask,
    Refuse(String),
}

/// Characters that chain or redirect commands; an allowlisted command
/// containing them could do something else entirely.
fn chains(cmd: &str) -> bool {
    cmd.contains([';', '|', '&', '`', '>', '<', '\n']) || cmd.contains("$(")
}

pub fn verdict(policy: ShellPolicy, allowlist: &[String], cmd: &str) -> Verdict {
    let cmd = cmd.trim();
    if cmd.is_empty() {
        return Verdict::Refuse("The command is empty.".into());
    }
    match policy {
        ShellPolicy::Never => {
            Verdict::Refuse("shell commands are turned off in Settings → Agents.".into())
        }
        ShellPolicy::Ask => Verdict::Ask,
        ShellPolicy::Allowlist => {
            let listed = allowlist
                .iter()
                .map(|a| a.trim())
                .filter(|a| !a.is_empty())
                .any(|a| {
                    cmd == a
                        || cmd
                            .strip_prefix(a)
                            .is_some_and(|rest| rest.starts_with(' '))
                });
            if listed && !chains(cmd) {
                Verdict::Run
            } else {
                Verdict::Refuse(format!(
                    "only these commands are allowed: {}.",
                    allowlist.join(", ")
                ))
            }
        }
    }
}

pub async fn run(cmd: &str, cwd: &Path) -> ToolOutcome {
    let mut c = if cfg!(windows) {
        let mut c = tokio::process::Command::new("cmd");
        c.arg("/C").arg(cmd);
        c
    } else {
        let mut c = tokio::process::Command::new("sh");
        c.arg("-c").arg(cmd);
        c
    };
    c.current_dir(cwd)
        .kill_on_drop(true)
        .stdin(std::process::Stdio::null());
    let out = match tokio::time::timeout(TIMEOUT, c.output()).await {
        Err(_) => {
            return ToolOutcome::Permanent(format!(
                "The command ran over {} seconds and was stopped.",
                TIMEOUT.as_secs()
            ))
        }
        Ok(Err(e)) => return ToolOutcome::Permanent(format!("Couldn't run the command: {e}")),
        Ok(Ok(o)) => o,
    };
    let mut text = format!("Exit code {}\n", out.status.code().unwrap_or(-1));
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    if !stdout.trim().is_empty() {
        text += &format!("Output:\n{}\n", stdout.trim_end());
    }
    if !stderr.trim().is_empty() {
        text += &format!("Errors:\n{}\n", stderr.trim_end());
    }
    if text.len() > MAX_OUTPUT {
        let cut = text
            .char_indices()
            .nth(MAX_OUTPUT)
            .map(|(i, _)| i)
            .unwrap_or(text.len());
        text.truncate(cut);
        text += "\n[output cut]";
    }
    ToolOutcome::Ok {
        text,
        ops: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn policy() {
        let list = vec!["ls".to_string(), "git status".to_string()];
        assert_eq!(verdict(ShellPolicy::Ask, &list, "rm -rf x"), Verdict::Ask);
        assert!(matches!(
            verdict(ShellPolicy::Never, &list, "ls"),
            Verdict::Refuse(_)
        ));
        assert_eq!(
            verdict(ShellPolicy::Allowlist, &list, "ls -la"),
            Verdict::Run
        );
        assert_eq!(
            verdict(ShellPolicy::Allowlist, &list, "git status"),
            Verdict::Run
        );
        for sneaky in [
            "lsblk",
            "ls; rm -rf ~",
            "ls && curl x",
            "ls | sh",
            "ls $(rm x)",
            "ls > f",
            "git statusx",
        ] {
            assert!(
                matches!(
                    verdict(ShellPolicy::Allowlist, &list, sneaky),
                    Verdict::Refuse(_)
                ),
                "{sneaky}"
            );
        }
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn runs_and_reports_the_exit_code() {
        let dir = tempfile::tempdir().unwrap();
        let ToolOutcome::Ok { text, .. } = run("echo hi; echo oops >&2; exit 3", dir.path()).await
        else {
            panic!()
        };
        assert!(text.contains("Exit code 3") && text.contains("hi") && text.contains("oops"));
    }
}
