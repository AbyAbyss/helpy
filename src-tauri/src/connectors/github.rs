//! GitHub issues, pull requests, repositories and files.

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use futures_util::future::BoxFuture;
use serde_json::{json, Value};

use super::api::{arg, done, limit, opt, Api, ApiResult};
use super::google::cut;
use super::oauth::{ClientAuth, Tokens};
use super::{Action, Connector, OAuthSpec, TokenHelp};
use crate::agents::runner::ToolOutcome;
use crate::settings::schema::Rule;

const API: &str = "https://api.github.com";

pub struct GitHub;

/// "owner/name", checked so it can't reach another API path.
fn repo(args: &Value) -> ApiResult<&str> {
    let r = arg(args, "repo")?;
    let mut parts = r.split('/');
    let ok_part = |p: Option<&str>| {
        p.is_some_and(|p| {
            !p.is_empty()
                && p != ".."
                && p.chars()
                    .all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c))
        })
    };
    if ok_part(parts.next()) && ok_part(parts.next()) && parts.next().is_none() {
        Ok(r)
    } else {
        Err(ToolOutcome::Permanent(format!(
            "\"{r}\" isn't a repository like owner/name."
        )))
    }
}

fn number(args: &Value) -> ApiResult<u64> {
    args["number"]
        .as_u64()
        .or_else(|| {
            args["number"]
                .as_str()
                .and_then(|s| s.trim_start_matches('#').parse().ok())
        })
        .ok_or_else(|| ToolOutcome::Permanent("\"number\" is missing.".into()))
}

fn issue_line(i: &Value) -> String {
    let repo = i["repository_url"]
        .as_str()
        .and_then(|u| u.strip_prefix("https://api.github.com/repos/"))
        .unwrap_or("");
    let kind = if i["pull_request"].is_object() {
        "PR"
    } else {
        "issue"
    };
    format!(
        "{repo}#{} ({kind}, {}) {} by {} | updated {} | {}",
        i["number"],
        i["state"].as_str().unwrap_or(""),
        i["title"].as_str().unwrap_or(""),
        i["user"]["login"].as_str().unwrap_or(""),
        i["updated_at"].as_str().unwrap_or(""),
        i["html_url"].as_str().unwrap_or("")
    )
}

impl GitHub {
    async fn read_issue(&self, api: &Api, args: &Value) -> ApiResult<String> {
        let (r, n) = (repo(args)?, number(args)?);
        let issue = api.get(&format!("{API}/repos/{r}/issues/{n}"), &[]).await?;
        let comments = api
            .get(
                &format!("{API}/repos/{r}/issues/{n}/comments"),
                &[("per_page", "50".into())],
            )
            .await?;
        let mut text = format!(
            "{} ({})\nby {}, {}\n\n{}",
            issue["title"].as_str().unwrap_or(""),
            issue["state"].as_str().unwrap_or(""),
            issue["user"]["login"].as_str().unwrap_or(""),
            issue["created_at"].as_str().unwrap_or(""),
            issue["body"].as_str().unwrap_or("")
        );
        for c in comments.as_array().into_iter().flatten() {
            text += &format!(
                "\n\n--- {} ({}):\n{}",
                c["user"]["login"].as_str().unwrap_or(""),
                c["created_at"].as_str().unwrap_or(""),
                c["body"].as_str().unwrap_or("")
            );
        }
        Ok(cut(text))
    }

    async fn read_file(&self, api: &Api, args: &Value) -> ApiResult<String> {
        let r = repo(args)?;
        let path = opt(args, "path").unwrap_or("").trim_start_matches('/');
        if path.split('/').any(|p| p == "..") {
            return Err(ToolOutcome::Permanent(
                "The path can't go up with \"..\".".into(),
            ));
        }
        let mut q = Vec::new();
        if let Some(b) = opt(args, "ref") {
            q.push(("ref", b.to_string()));
        }
        let v = api
            .get(&format!("{API}/repos/{r}/contents/{path}"), &q)
            .await?;
        // A folder lists its entries.
        if let Some(entries) = v.as_array() {
            let lines: Vec<String> = entries
                .iter()
                .map(|e| {
                    format!(
                        "{} {}",
                        if e["type"] == "dir" {
                            "folder"
                        } else {
                            "file  "
                        },
                        e["path"].as_str().unwrap_or("")
                    )
                })
                .collect();
            return Ok(lines.join("\n"));
        }
        let raw = v["content"].as_str().unwrap_or("").replace('\n', "");
        let bytes = STANDARD.decode(raw).unwrap_or_default();
        match String::from_utf8(bytes) {
            Ok(text) => Ok(cut(text)),
            Err(_) => Err(ToolOutcome::Permanent(format!("{path} isn't a text file."))),
        }
    }
}

impl Connector for GitHub {
    fn id(&self) -> &'static str {
        "github"
    }
    fn name(&self) -> &'static str {
        "GitHub"
    }
    fn about(&self) -> &'static str {
        "search and read GitHub issues, pull requests, repositories and files; open issues and comment"
    }
    fn oauth(&self) -> Option<OAuthSpec> {
        Some(OAuthSpec {
            provider: "github",
            authorize_url: "https://github.com/login/oauth/authorize",
            token_url: "https://github.com/login/oauth/access_token",
            // GitHub has no read-only scope for private repositories; Helpy
            // enforces read-only itself.
            read_scopes: &["repo", "read:user"],
            write_scopes: &[],
            needs_secret: true,
            client_auth: ClientAuth::Body,
            scope_param: "scope",
            extra: &[],
            // GitHub ignores the port of a loopback redirect.
            host: "127.0.0.1",
            port: 0,
        })
    }
    fn token(&self) -> Option<TokenHelp> {
        Some(TokenHelp {
            label: "Personal access token".into(),
            help: "Make a fine-grained token with read access to issues, pull requests and contents (and write access to issues to let agents open issues and comment).".into(),
            url: "https://github.com/settings/personal-access-tokens/new".into(),
        })
    }
    fn headers(&self) -> Vec<(&'static str, String)> {
        vec![
            ("X-GitHub-Api-Version", "2022-11-28".into()),
            ("User-Agent", "Helpy".into()),
        ]
    }
    fn actions(&self) -> Vec<Action> {
        let issue = json!({"repo": {"type": "string", "description": "owner/name"}, "number": {"type": "integer"}});
        vec![
            Action::read(
                "search_issues",
                "Search issues and pull requests with GitHub search syntax (repo:owner/name is:open is:pr author:@me).",
                json!({"query": {"type": "string"}, "limit": {"type": "integer"}}),
                &["query"],
            ),
            Action::read("read_issue", "Read an issue or pull request with its comments.", issue, &["repo", "number"]),
            Action::read("list_repos", "List the user's repositories, most recently updated first.", json!({"limit": {"type": "integer"}}), &[]),
            Action::read(
                "read_file",
                "Read a file, or list a folder, in a repository.",
                json!({"repo": {"type": "string", "description": "owner/name"}, "path": {"type": "string"}, "ref": {"type": "string", "description": "Branch, tag or commit"}}),
                &["repo"],
            ),
            Action::write(
                "create_issue",
                "Open an issue.",
                json!({"repo": {"type": "string", "description": "owner/name"}, "title": {"type": "string"}, "body": {"type": "string"}}),
                &["repo", "title"],
                Rule::Ask,
                &["title", "body"],
            ),
            Action::write(
                "comment",
                "Comment on an issue or pull request.",
                json!({"repo": {"type": "string", "description": "owner/name"}, "number": {"type": "integer"}, "body": {"type": "string"}}),
                &["repo", "number", "body"],
                Rule::Ask,
                &["body"],
            ),
        ]
    }
    fn describe(&self, action: &str, args: &Value) -> (String, String) {
        let r = args["repo"].as_str().unwrap_or("a repository");
        let body = args["body"].as_str().unwrap_or("").to_string();
        match action {
            "create_issue" => (
                format!("Open an issue in {r}"),
                format!("{}\n\n{body}", args["title"].as_str().unwrap_or("")),
            ),
            _ => (format!("Comment on {r}#{}", args["number"]), body),
        }
    }
    fn account<'a>(&'a self, api: &'a Api, _: &'a Tokens) -> BoxFuture<'a, ApiResult<String>> {
        Box::pin(async move {
            let me = api.get(&format!("{API}/user"), &[]).await?;
            Ok(me["login"].as_str().unwrap_or("GitHub").to_string())
        })
    }
    fn call<'a>(
        &'a self,
        api: &'a Api,
        action: &'a str,
        args: &'a Value,
    ) -> BoxFuture<'a, ToolOutcome> {
        Box::pin(async move {
            done(match action {
                "search_issues" => async {
                    let found = api
                        .get(&format!("{API}/search/issues"), &[("q", arg(args, "query")?.into()), ("per_page", limit(args, 20, 100).to_string())])
                        .await?;
                    let lines: Vec<String> = found["items"].as_array().into_iter().flatten().map(issue_line).collect();
                    Ok(if lines.is_empty() { "Nothing matches.".into() } else { lines.join("\n") })
                }
                .await,
                "read_issue" => self.read_issue(api, args).await,
                "list_repos" => async {
                    let list = api
                        .get(&format!("{API}/user/repos"), &[("sort", "updated".into()), ("per_page", limit(args, 30, 100).to_string())])
                        .await?;
                    let lines: Vec<String> = list
                        .as_array()
                        .into_iter()
                        .flatten()
                        .map(|r| {
                            format!(
                                "{}{} | {}",
                                r["full_name"].as_str().unwrap_or(""),
                                if r["private"] == json!(true) { " (private)" } else { "" },
                                r["description"].as_str().unwrap_or("")
                            )
                        })
                        .collect();
                    Ok(lines.join("\n"))
                }
                .await,
                "read_file" => self.read_file(api, args).await,
                "create_issue" => async {
                    let r = repo(args)?;
                    let made = api
                        .post(&format!("{API}/repos/{r}/issues"), &json!({ "title": arg(args, "title")?, "body": args["body"].as_str().unwrap_or("") }))
                        .await?;
                    Ok(format!("Opened {}", made["html_url"].as_str().unwrap_or("")))
                }
                .await,
                "comment" => async {
                    let (r, n) = (repo(args)?, number(args)?);
                    let made = api.post(&format!("{API}/repos/{r}/issues/{n}/comments"), &json!({ "body": arg(args, "body")? })).await?;
                    Ok(format!("Commented: {}", made["html_url"].as_str().unwrap_or("")))
                }
                .await,
                other => Err(ToolOutcome::Permanent(format!("GitHub has no action {other}."))),
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checks_repositories_and_numbers() {
        assert_eq!(
            repo(&json!({"repo": "tauri-apps/tauri"})).ok(),
            Some("tauri-apps/tauri")
        );
        for bad in ["tauri", "a/b/c", "../x", "a/..", "a/b?x=1"] {
            assert!(repo(&json!({"repo": bad})).is_err(), "{bad}");
        }
        assert_eq!(number(&json!({"number": "#12"})).ok(), Some(12));
        assert_eq!(number(&json!({"number": 7})).ok(), Some(7));
    }

    #[test]
    fn writes_issues_as_lines() {
        let i = json!({"number": 5, "state": "open", "title": "Crash", "user": {"login": "ann"}, "updated_at": "2026-05-01",
                       "html_url": "https://github.com/a/b/pull/5", "repository_url": "https://api.github.com/repos/a/b", "pull_request": {}});
        assert_eq!(
            issue_line(&i),
            "a/b#5 (PR, open) Crash by ann | updated 2026-05-01 | https://github.com/a/b/pull/5"
        );
    }
}
