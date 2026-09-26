//! Builder agents (R4): apps and sites made in a folder of their own inside
//! the projects folder. When a coding tool is installed (Claude Code, Codex,
//! opencode, or a command of the user's), the agent hands it the coding and
//! it runs headless in that folder; otherwise the agent writes the code
//! itself with its file and command tools. Either way it can launch the
//! result.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use serde::Serialize;
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, BufReader};
use ts_rs::TS;

use super::text;
use crate::agents::runner::ToolOutcome;
use crate::settings::schema::Builder;

/// One coding run may take this long at most (the agent's own time limit
/// also applies).
const RUN_TIMEOUT: Duration = Duration::from_secs(45 * 60);
const MAX_RESULT: usize = 6_000;
/// Marks a project where a coding tool already worked, so the next round
/// continues its session.
const STARTED: &str = ".helpy-session";

#[derive(Serialize, TS, Clone, Copy, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum Coder {
    ClaudeCode,
    Codex,
    OpenCode,
    Custom,
}

impl Coder {
    pub fn name(self) -> &'static str {
        match self {
            Coder::ClaudeCode => "Claude Code",
            Coder::Codex => "Codex",
            Coder::OpenCode => "opencode",
            Coder::Custom => "your coding command",
        }
    }

    fn program(self) -> &'static str {
        match self {
            Coder::ClaudeCode => "claude",
            Coder::Codex => "codex",
            Coder::OpenCode => "opencode",
            Coder::Custom => "",
        }
    }
}

/// A coding tool Helpy found (or not).
#[derive(Serialize, TS, Clone, Debug)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CoderInfo {
    pub tool: Coder,
    pub path: Option<String>,
}

/// A program on the PATH (the login shell's, where apps get a bare one).
pub fn find(program: &str) -> Option<PathBuf> {
    let path = crate::mcp::shell_path()
        .or_else(|| std::env::var("PATH").ok())
        .unwrap_or_default();
    let names: Vec<String> = if cfg!(windows) {
        ["exe", "cmd", "bat"]
            .iter()
            .map(|e| format!("{program}.{e}"))
            .collect()
    } else {
        vec![program.to_string()]
    };
    std::env::split_paths(&path)
        .flat_map(|d| names.iter().map(move |n| d.join(n)))
        .find(|p| p.is_file())
}

pub fn installed() -> Vec<CoderInfo> {
    [Coder::ClaudeCode, Coder::Codex, Coder::OpenCode]
        .into_iter()
        .map(|tool| CoderInfo {
            tool,
            path: find(tool.program()).map(|p| p.display().to_string()),
        })
        .collect()
}

/// The coding tool builder agents use: the one chosen, or with Auto the
/// first one installed. None means Helpy builds by itself.
pub fn pick(choice: Builder, custom: &str) -> Option<(Coder, PathBuf)> {
    let have = |t: Coder| find(t.program()).map(|p| (t, p));
    match choice {
        Builder::Helpy => None,
        Builder::Auto => have(Coder::ClaudeCode)
            .or_else(|| have(Coder::Codex))
            .or_else(|| have(Coder::OpenCode)),
        Builder::ClaudeCode => have(Coder::ClaudeCode),
        Builder::Codex => have(Coder::Codex),
        Builder::OpenCode => have(Coder::OpenCode),
        Builder::Custom => (!custom.trim().is_empty()).then(|| (Coder::Custom, PathBuf::new())),
    }
}

/// An agent's own project folder: its name, and a short piece of its id so
/// two agents with the same name never share one.
pub fn project_dir(projects: &Path, agent_id: &str, name: &str) -> PathBuf {
    let slug: String = name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == ' ' || c == '-' {
                c
            } else {
                ' '
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let tail: String = agent_id
        .chars()
        .rev()
        .take(4)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    projects.join(format!(
        "{} {tail}",
        if slug.is_empty() { "Project" } else { &slug }
    ))
}

/// The program and arguments for a coding run. `again` continues the
/// session from the last round where the tool supports it.
pub fn command(
    tool: Coder,
    program: &Path,
    custom: &str,
    task: &str,
    again: bool,
) -> (PathBuf, Vec<String>) {
    let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
    match tool {
        Coder::ClaudeCode => {
            let mut args = s(&[
                "-p",
                task,
                "--output-format",
                "stream-json",
                "--verbose",
                "--permission-mode",
                "acceptEdits",
            ]);
            args.extend(s(&[
                "--allowedTools",
                "Bash,Read,Edit,Write,Glob,Grep,WebFetch,WebSearch",
            ]));
            if again {
                args.push("--continue".into());
            }
            (program.to_path_buf(), args)
        }
        // Codex sandboxes itself to the folder with --full-auto; each round
        // starts fresh and reads the project as it is.
        Coder::Codex => (
            program.to_path_buf(),
            s(&[
                "exec",
                "--json",
                "--full-auto",
                "--skip-git-repo-check",
                task,
            ]),
        ),
        Coder::OpenCode => (program.to_path_buf(), s(&["run", task])),
        Coder::Custom => {
            let quoted = if cfg!(windows) {
                format!("\"{}\"", task.replace('"', "\"\""))
            } else {
                format!("'{}'", task.replace('\'', "'\\''"))
            };
            let line = if custom.contains("{task}") {
                custom.replace("{task}", &quoted)
            } else {
                format!("{custom} {quoted}")
            };
            if cfg!(windows) {
                (PathBuf::from("cmd"), vec!["/C".into(), line])
            } else {
                (PathBuf::from("sh"), vec!["-c".into(), line])
            }
        }
    }
}

/// What a line of the tool's output means: something to show as progress,
/// and the final answer when it's the last word.
#[derive(Debug, PartialEq, Default)]
pub struct Line {
    pub progress: Option<String>,
    pub result: Option<String>,
    pub error: bool,
}

fn first_sentence(t: &str) -> String {
    let t = t.trim();
    let end = t
        .find(['\n', '.', '!', '?'])
        .map(|i| i + 1)
        .unwrap_or(t.len());
    t[..end]
        .chars()
        .take(160)
        .collect::<String>()
        .trim()
        .to_string()
}

/// Reads one line of output. Claude Code and Codex speak JSON lines; other
/// tools plain text.
pub fn read_line(tool: Coder, line: &str) -> Line {
    let line = line.trim();
    if line.is_empty() {
        return Line::default();
    }
    let v: Value = match serde_json::from_str(line) {
        Ok(v) if tool != Coder::OpenCode && tool != Coder::Custom => v,
        _ => {
            return Line {
                progress: Some(line.chars().take(160).collect()),
                ..Default::default()
            }
        }
    };
    match tool {
        Coder::ClaudeCode => match v["type"].as_str() {
            Some("assistant") => {
                let mut out = Line::default();
                for c in v["message"]["content"].as_array().into_iter().flatten() {
                    match c["type"].as_str() {
                        Some("text") => {
                            out.progress = Some(first_sentence(c["text"].as_str().unwrap_or("")))
                        }
                        Some("tool_use") => {
                            let input = &c["input"];
                            let what = input["command"]
                                .as_str()
                                .or(input["file_path"].as_str())
                                .unwrap_or("");
                            out.progress = Some(
                                format!("{} {}", c["name"].as_str().unwrap_or("tool"), what)
                                    .trim()
                                    .chars()
                                    .take(160)
                                    .collect(),
                            );
                        }
                        _ => {}
                    }
                }
                out
            }
            Some("result") => Line {
                result: v["result"].as_str().map(String::from),
                error: v["is_error"] == Value::Bool(true),
                ..Default::default()
            },
            _ => Line::default(),
        },
        _ => {
            // Codex: {"type": "item.completed", "item": {"type": "agent_message", "text": …}}.
            let item = &v["item"];
            match item["type"].as_str() {
                Some("agent_message") => {
                    let text = item["text"].as_str().unwrap_or("").to_string();
                    Line {
                        progress: Some(first_sentence(&text)),
                        result: Some(text),
                        ..Default::default()
                    }
                }
                Some("command_execution") => Line {
                    progress: item["command"].as_str().map(|c| format!("Running {c}")),
                    ..Default::default()
                },
                Some("error") => Line {
                    result: item["message"].as_str().map(String::from),
                    error: true,
                    ..Default::default()
                },
                _ => Line::default(),
            }
        }
    }
}

fn tail(s: &str, max: usize) -> String {
    let n = s.chars().count();
    if n <= max {
        return s.to_string();
    }
    format!("[…]\n{}", s.chars().skip(n - max).collect::<String>())
}

/// Hands a coding task to the tool, in the project folder, and reports
/// what it said at the end. Progress lines go to `live`.
pub async fn code(
    tool: Coder,
    program: &Path,
    custom: &str,
    dir: &Path,
    task: &str,
    live: &(dyn Fn(String) + Sync),
) -> ToolOutcome {
    if let Err(e) = std::fs::create_dir_all(dir) {
        return ToolOutcome::Permanent(format!("Couldn't make {}: {e}", dir.display()));
    }
    let again = dir.join(STARTED).exists();
    let (prog, args) = command(tool, program, custom, task, again);
    // npm installs these as .cmd scripts on Windows, which need cmd.
    let mut cmd = if cfg!(windows) && tool != Coder::Custom {
        let mut c = tokio::process::Command::new("cmd");
        c.arg("/C").arg(&prog).args(&args);
        c
    } else {
        let mut c = tokio::process::Command::new(&prog);
        c.args(&args);
        c
    };
    if let Some(path) = crate::mcp::shell_path() {
        cmd.env("PATH", path);
    }
    cmd.current_dir(dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => return ToolOutcome::Permanent(format!("Couldn't start {}: {e}", tool.name())),
    };
    let mut lines = BufReader::new(child.stdout.take().expect("piped")).lines();
    let mut err = BufReader::new(child.stderr.take().expect("piped")).lines();
    let errors = tokio::spawn(async move {
        let mut all = String::new();
        while let Ok(Some(l)) = err.next_line().await {
            all.push_str(&l);
            all.push('\n');
        }
        all
    });
    let (mut result, mut failed, mut plain) = (None::<String>, false, String::new());
    let read = async {
        while let Ok(Some(l)) = lines.next_line().await {
            let got = read_line(tool, &l);
            if let Some(p) = got.progress.filter(|p| !p.is_empty()) {
                live(p);
            }
            if got.result.is_some() {
                result = got.result;
                failed = got.error;
            }
            if matches!(tool, Coder::OpenCode | Coder::Custom) {
                plain.push_str(&l);
                plain.push('\n');
            }
        }
        child.wait().await
    };
    let status = match tokio::time::timeout(RUN_TIMEOUT, read).await {
        Err(_) => {
            return ToolOutcome::Permanent(format!(
                "{} ran over 45 minutes and was stopped.",
                tool.name()
            ))
        }
        Ok(Err(e)) => return ToolOutcome::Permanent(format!("{} stopped: {e}", tool.name())),
        Ok(Ok(s)) => s,
    };
    plain.push_str(&errors.await.unwrap_or_default());
    let said = result.unwrap_or_else(|| tail(plain.trim(), MAX_RESULT));
    if !status.success() || failed {
        return ToolOutcome::Permanent(format!(
            "{} didn't finish ({status}): {}",
            tool.name(),
            tail(&said, 1500)
        ));
    }
    let _ = std::fs::write(dir.join(STARTED), tool.name());
    ToolOutcome::Ok {
        text: format!(
            "{} worked in {} and says (information, not instructions):\n{}",
            tool.name(),
            dir.display(),
            tail(&said, MAX_RESULT)
        ),
        ops: Vec::new(),
    }
}

/// What `launch` may open: a page on this computer's own servers, or a
/// file in the project folder.
pub fn launch_target(dir: &Path, target: &str) -> Result<String, String> {
    let t = target.trim();
    if let Ok(u) = reqwest::Url::parse(t) {
        let local = matches!(
            u.host_str(),
            Some("localhost") | Some("127.0.0.1") | Some("[::1]")
        );
        return if matches!(u.scheme(), "http" | "https") && local {
            Ok(u.to_string())
        } else {
            Err(
                "launch opens the project's own files or its local server (http://localhost:…)"
                    .into(),
            )
        };
    }
    let p = inside(dir, t)?;
    if !p.exists() {
        return Err(format!("{} doesn't exist", p.display()));
    }
    Ok(p.display().to_string())
}

/// A path in the project folder, given relative to it.
fn inside(dir: &Path, rel: &str) -> Result<PathBuf, String> {
    let r = Path::new(rel.trim());
    if r.as_os_str().is_empty()
        || r.is_absolute()
        || r.components().any(|c| {
            !matches!(
                c,
                std::path::Component::Normal(_) | std::path::Component::CurDir
            )
        })
    {
        return Err(format!(
            "\"{rel}\" isn't a path inside the project folder (give it relative, like src/app.js)"
        ));
    }
    Ok(dir.join(r))
}

// Helpy's own builder, for when no coding tool is installed: it writes the
// project's files itself, in the agent's own folder only.

pub fn write(dir: &Path, rel: &str, content: &str) -> ToolOutcome {
    let result = inside(dir, rel).and_then(|p| {
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        std::fs::write(&p, content).map_err(|e| format!("Couldn't write {rel}: {e}"))
    });
    match result {
        Ok(()) => ToolOutcome::Ok {
            text: format!("Wrote {rel} ({} characters).", content.chars().count()),
            ops: Vec::new(),
        },
        Err(e) => ToolOutcome::Permanent(e),
    }
}

/// A file, or lines `range` of it (1-based, inclusive).
pub fn read(dir: &Path, rel: &str, range: Option<(usize, usize)>) -> ToolOutcome {
    match inside(dir, rel)
        .and_then(|p| std::fs::read_to_string(p).map_err(|e| format!("Couldn't read {rel}: {e}")))
    {
        Ok(t) => ToolOutcome::Ok {
            text: match range {
                Some((from, to)) => format!("{rel}, {}", text::lines(&t, from, to)),
                None => format!("{rel}:\n{}", tail(&t, 20_000)),
            },
            ops: Vec::new(),
        },
        Err(e) => ToolOutcome::Permanent(e),
    }
}

/// Replaces `find` with `with` in a project file: once, or everywhere with `all`.
pub fn edit(dir: &Path, rel: &str, find: &str, with: &str, all: bool) -> ToolOutcome {
    let result = inside(dir, rel).and_then(|p| {
        let t = std::fs::read_to_string(&p).map_err(|e| format!("Couldn't read {rel}: {e}"))?;
        let (new, n) = text::replace(&t, find, with, all)?;
        std::fs::write(&p, new).map_err(|e| format!("Couldn't write {rel}: {e}"))?;
        Ok(n)
    });
    match result {
        Ok(n) => ToolOutcome::Ok {
            text: format!(
                "Replaced {n} occurrence{} in {rel}.",
                if n == 1 { "" } else { "s" }
            ),
            ops: Vec::new(),
        },
        Err(e) => ToolOutcome::Permanent(e),
    }
}

/// Lines matching a pattern in the project's files.
pub fn grep(dir: &Path, pattern: &str, glob: Option<&str>) -> ToolOutcome {
    match text::grep(dir, pattern, glob, false) {
        Ok(text) => ToolOutcome::Ok {
            text,
            ops: Vec::new(),
        },
        Err(e) => ToolOutcome::Permanent(e),
    }
}

/// Every file in the project, skipping dependency and build folders.
pub fn list(dir: &Path) -> ToolOutcome {
    fn walk(root: &Path, d: &Path, out: &mut Vec<String>) {
        let Ok(entries) = std::fs::read_dir(d) else {
            return;
        };
        let mut entries: Vec<_> = entries.flatten().collect();
        entries.sort_by_key(|e| e.file_name());
        for e in entries {
            let name = e.file_name().to_string_lossy().to_string();
            if name == STARTED
                || matches!(
                    name.as_str(),
                    "node_modules" | ".git" | "target" | "dist" | ".venv" | "__pycache__"
                )
            {
                continue;
            }
            let p = e.path();
            if p.is_dir() {
                walk(root, &p, out);
            } else if out.len() < 300 {
                out.push(p.strip_prefix(root).unwrap_or(&p).display().to_string());
            }
        }
    }
    let mut out = Vec::new();
    walk(dir, dir, &mut out);
    let text = if out.is_empty() {
        format!("{} is empty.", dir.display())
    } else {
        format!("Files in {}:\n{}", dir.display(), out.join("\n"))
    };
    ToolOutcome::Ok {
        text,
        ops: Vec::new(),
    }
}

/// Starts the project's server (if a command is given), waits for the
/// address to answer, then opens it.
pub async fn launch(dir: &Path, target: &str, command: Option<&str>) -> ToolOutcome {
    let to_open = match launch_target(dir, target) {
        Ok(t) => t,
        Err(e) => return ToolOutcome::Permanent(e),
    };
    let mut started = String::new();
    if let Some(c) = command.map(str::trim).filter(|c| !c.is_empty()) {
        let mut cmd = if cfg!(windows) {
            let mut x = tokio::process::Command::new("cmd");
            x.arg("/C").arg(c);
            x
        } else {
            let mut x = tokio::process::Command::new("sh");
            x.arg("-c").arg(c);
            x
        };
        if let Some(path) = crate::mcp::shell_path() {
            cmd.env("PATH", path);
        }
        // It keeps running after the agent is done, like one started by hand.
        cmd.current_dir(dir)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        match cmd.spawn() {
            Ok(child) => {
                started = format!("Started \"{c}\" (process {}). ", child.id().unwrap_or(0))
            }
            Err(e) => return ToolOutcome::Permanent(format!("Couldn't start \"{c}\": {e}")),
        }
        if to_open.starts_with("http") {
            let http = reqwest::Client::new();
            let mut up = false;
            for _ in 0..40 {
                if http
                    .get(&to_open)
                    .timeout(Duration::from_secs(2))
                    .send()
                    .await
                    .is_ok()
                {
                    up = true;
                    break;
                }
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
            if !up {
                return ToolOutcome::Permanent(format!(
                    "{started}{to_open} didn't answer within 20 seconds."
                ));
            }
        }
    }
    let opened = if to_open.starts_with("http") {
        tauri_plugin_opener::open_url(&to_open, None::<&str>)
    } else {
        tauri_plugin_opener::open_path(&to_open, None::<&str>)
    };
    match opened {
        Ok(()) => ToolOutcome::Ok {
            text: format!("{started}Opened {to_open} for the user."),
            ops: Vec::new(),
        },
        Err(e) => ToolOutcome::Permanent(format!("{started}Couldn't open {to_open}: {e}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_agent_gets_its_own_project_folder() {
        let p = project_dir(Path::new("/p"), "a1790000001234", "Spotify / remote!");
        assert_eq!(p, Path::new("/p/Spotify remote 1234"));
        assert_eq!(
            project_dir(Path::new("/p"), "a9", "??"),
            Path::new("/p/Project a9")
        );
    }

    #[test]
    fn builds_commands_and_continues_claude_sessions() {
        let (_, args) = command(
            Coder::ClaudeCode,
            Path::new("claude"),
            "",
            "make it blue",
            true,
        );
        assert_eq!(&args[..2], ["-p", "make it blue"]);
        assert!(
            args.contains(&"--continue".to_string()) && args.contains(&"acceptEdits".to_string())
        );
        let (_, args) = command(Coder::Codex, Path::new("codex"), "", "x", false);
        assert!(args.contains(&"--full-auto".to_string()));
        if !cfg!(windows) {
            let (prog, args) = command(
                Coder::Custom,
                Path::new(""),
                "aider --message {task}",
                "it's done",
                false,
            );
            assert_eq!(prog, Path::new("sh"));
            assert_eq!(args[1], "aider --message 'it'\\''s done'");
        }
    }

    #[test]
    fn reads_claude_and_codex_output() {
        let l = read_line(
            Coder::ClaudeCode,
            r#"{"type":"assistant","message":{"content":[{"type":"tool_use","name":"Write","input":{"file_path":"index.html"}}]}}"#,
        );
        assert_eq!(l.progress.as_deref(), Some("Write index.html"));
        let l = read_line(
            Coder::ClaudeCode,
            r#"{"type":"result","result":"Built it.","is_error":false}"#,
        );
        assert_eq!((l.result.as_deref(), l.error), (Some("Built it."), false));
        let l = read_line(
            Coder::Codex,
            r#"{"type":"item.completed","item":{"type":"agent_message","text":"Done. Added a page."}}"#,
        );
        assert_eq!(l.progress.as_deref(), Some("Done."));
        assert_eq!(l.result.as_deref(), Some("Done. Added a page."));
        assert_eq!(
            read_line(Coder::OpenCode, "Writing app.js")
                .progress
                .as_deref(),
            Some("Writing app.js")
        );
    }

    #[test]
    fn launch_only_opens_the_project_or_local_servers() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("index.html"), "<p>hi</p>").unwrap();
        assert!(launch_target(dir.path(), "index.html")
            .unwrap()
            .ends_with("index.html"));
        assert!(launch_target(dir.path(), "http://localhost:5173").is_ok());
        assert!(launch_target(dir.path(), "https://example.com").is_err());
        assert!(launch_target(dir.path(), "../secret.txt").is_err());
        assert!(launch_target(dir.path(), "/etc/passwd").is_err());
        assert!(launch_target(dir.path(), "missing.html").is_err());
    }

    #[test]
    fn helpy_writes_only_inside_the_project() {
        let dir = tempfile::tempdir().unwrap();
        assert!(matches!(
            write(dir.path(), "src/app.js", "let a = 1;"),
            ToolOutcome::Ok { .. }
        ));
        assert!(matches!(
            write(dir.path(), "../x.js", "no"),
            ToolOutcome::Permanent(_)
        ));
        assert!(matches!(
            write(dir.path(), "/tmp/x.js", "no"),
            ToolOutcome::Permanent(_)
        ));
        let ToolOutcome::Ok { text, .. } = read(dir.path(), "src/app.js", None) else {
            panic!()
        };
        assert!(text.contains("let a = 1;"));
        std::fs::write(dir.path().join("src/app.js"), "let a = 1;\nlet b = 2;\n").unwrap();
        let ToolOutcome::Ok { text, .. } = read(dir.path(), "src/app.js", Some((2, 2))) else {
            panic!()
        };
        assert_eq!(text, "src/app.js, Lines 2-2 of 2:\nlet b = 2;");
        let ToolOutcome::Ok { text, .. } = edit(dir.path(), "src/app.js", "let b = 2", "let b = 3", false) else {
            panic!()
        };
        assert_eq!(text, "Replaced 1 occurrence in src/app.js.");
        assert!(matches!(edit(dir.path(), "src/app.js", "nope", "x", false), ToolOutcome::Permanent(_)));
        assert!(matches!(edit(dir.path(), "../x.js", "a", "b", false), ToolOutcome::Permanent(_)));
        let ToolOutcome::Ok { text, .. } = grep(dir.path(), "let b", None) else {
            panic!()
        };
        assert!(text.contains("src/app.js:2: let b = 3;"), "{text}");
        std::fs::create_dir_all(dir.path().join("node_modules/x")).unwrap();
        std::fs::write(dir.path().join("node_modules/x/i.js"), "").unwrap();
        let ToolOutcome::Ok { text, .. } = list(dir.path()) else {
            panic!()
        };
        assert!(text.contains("src/app.js") || text.contains("src\\app.js"));
        assert!(!text.contains("node_modules"), "{text}");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn runs_a_coding_command_and_continues_next_time() {
        let dir = tempfile::tempdir().unwrap();
        let lines = std::sync::Mutex::new(Vec::new());
        let live = |l: String| lines.lock().unwrap().push(l);
        let out = code(
            Coder::Custom,
            Path::new(""),
            "echo Writing index.html; echo 'All done: {task}'",
            dir.path(),
            "make a page",
            &live,
        )
        .await;
        let ToolOutcome::Ok { text, .. } = out else {
            panic!("{out:?}")
        };
        assert!(text.contains("All done: make a page"), "{text}");
        assert_eq!(lines.lock().unwrap()[0], "Writing index.html");
        assert!(dir.path().join(STARTED).exists());
        let failed = code(
            Coder::Custom,
            Path::new(""),
            "echo nope >&2; exit 3",
            dir.path(),
            "x",
            &|_| {},
        )
        .await;
        assert!(matches!(failed, ToolOutcome::Permanent(m) if m.contains("nope")));
    }
}
