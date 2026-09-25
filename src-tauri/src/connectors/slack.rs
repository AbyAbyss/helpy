//! Slack, as the user (a user token), so agents see what the user sees and
//! post under the user's name.

use std::collections::HashMap;

use futures_util::future::BoxFuture;
use serde_json::{json, Value};

use super::api::{arg, done, limit, opt, Api, ApiResult};
use super::oauth::{ClientAuth, Tokens};
use super::{Action, Connector, OAuthSpec, TokenHelp};
use crate::agents::runner::ToolOutcome;
use crate::settings::schema::Rule;

const API: &str = "https://slack.com/api";

pub struct Slack;

fn message_line(m: &Value, who: &str) -> String {
    let mut line = format!(
        "[ts {}] {who}: {}",
        m["ts"].as_str().unwrap_or(""),
        m["text"].as_str().unwrap_or("")
    );
    if let Some(n) = m["reply_count"].as_u64().filter(|n| *n > 0) {
        line += &format!(" ({n} replies)");
    }
    line
}

impl Slack {
    /// People's names for their ids, looked up once each.
    async fn names(&self, api: &Api, ids: impl Iterator<Item = &str>) -> HashMap<String, String> {
        let mut out = HashMap::new();
        for id in ids {
            if out.contains_key(id) {
                continue;
            }
            let name = match api
                .get(&format!("{API}/users.info"), &[("user", id.into())])
                .await
            {
                Ok(u) => u["user"]["real_name"]
                    .as_str()
                    .or(u["user"]["name"].as_str())
                    .unwrap_or(id)
                    .to_string(),
                Err(_) => id.to_string(),
            };
            out.insert(id.to_string(), name);
        }
        out
    }

    async fn search(&self, api: &Api, args: &Value) -> ApiResult<String> {
        let found = api
            .get(
                &format!("{API}/search.messages"),
                &[
                    ("query", arg(args, "query")?.into()),
                    ("count", limit(args, 20, 100).to_string()),
                ],
            )
            .await?;
        let lines: Vec<String> = found["messages"]["matches"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|m| {
                format!(
                    "#{} (channel {}) {}\n{}",
                    m["channel"]["name"].as_str().unwrap_or(""),
                    m["channel"]["id"].as_str().unwrap_or(""),
                    m["permalink"].as_str().unwrap_or(""),
                    message_line(m, m["username"].as_str().unwrap_or(""))
                )
            })
            .collect();
        Ok(if lines.is_empty() {
            "No messages match.".into()
        } else {
            lines.join("\n\n")
        })
    }

    async fn channels(&self, api: &Api, args: &Value) -> ApiResult<String> {
        let list = api
            .get(
                &format!("{API}/conversations.list"),
                &[
                    ("types", "public_channel,private_channel,mpim,im".into()),
                    ("exclude_archived", "true".into()),
                    ("limit", limit(args, 200, 1000).to_string()),
                ],
            )
            .await?;
        let chans = list["channels"].as_array().cloned().unwrap_or_default();
        let dm_people = self
            .names(api, chans.iter().filter_map(|c| c["user"].as_str()))
            .await;
        let lines: Vec<String> = chans
            .iter()
            .map(|c| {
                let id = c["id"].as_str().unwrap_or("");
                match c["user"].as_str() {
                    Some(u) => format!(
                        "{id} | direct messages with {}",
                        dm_people.get(u).map(String::as_str).unwrap_or(u)
                    ),
                    None => format!("{id} | #{}", c["name"].as_str().unwrap_or("")),
                }
            })
            .collect();
        Ok(lines.join("\n"))
    }

    async fn history(&self, api: &Api, args: &Value) -> ApiResult<String> {
        let channel = arg(args, "channel")?;
        let mut q = vec![
            ("channel", channel.to_string()),
            ("limit", limit(args, 30, 200).to_string()),
        ];
        let url = match opt(args, "thread_ts") {
            Some(ts) => {
                q.push(("ts", ts.into()));
                format!("{API}/conversations.replies")
            }
            None => format!("{API}/conversations.history"),
        };
        let list = api.get(&url, &q).await?;
        let mut msgs = list["messages"].as_array().cloned().unwrap_or_default();
        // History comes newest first; read it in order.
        if opt(args, "thread_ts").is_none() {
            msgs.reverse();
        }
        let names = self
            .names(api, msgs.iter().filter_map(|m| m["user"].as_str()))
            .await;
        let lines: Vec<String> = msgs
            .iter()
            .map(|m| {
                let who = m["user"]
                    .as_str()
                    .and_then(|u| names.get(u))
                    .map(String::as_str)
                    .or(m["username"].as_str())
                    .unwrap_or("someone");
                message_line(m, who)
            })
            .collect();
        Ok(if lines.is_empty() {
            "No messages.".into()
        } else {
            lines.join("\n")
        })
    }
}

impl Connector for Slack {
    fn id(&self) -> &'static str {
        "slack"
    }
    fn name(&self) -> &'static str {
        "Slack"
    }
    fn about(&self) -> &'static str {
        "search and read the user's Slack messages and channels, and post messages as the user"
    }
    fn oauth(&self) -> Option<OAuthSpec> {
        Some(OAuthSpec {
            provider: "slack",
            authorize_url: "https://slack.com/oauth/v2/authorize",
            token_url: "https://slack.com/api/oauth.v2.access",
            read_scopes: &[
                "search:read",
                "channels:read",
                "groups:read",
                "im:read",
                "mpim:read",
                "channels:history",
                "groups:history",
                "im:history",
                "mpim:history",
                "users:read",
            ],
            write_scopes: &["chat:write"],
            // With PKCE turned on for the app, no secret is needed; one is
            // still sent if the user saved it.
            needs_secret: false,
            client_auth: ClientAuth::Basic,
            scope_param: "user_scope",
            extra: &[],
            // Slack treats http://localhost redirects as desktop ones under
            // PKCE, matched with the port.
            host: "localhost",
            port: 53172,
        })
    }
    fn token(&self) -> Option<TokenHelp> {
        Some(TokenHelp {
            label: "User OAuth token (xoxp-…)".into(),
            help: "Create a Slack app, add the user token scopes Helpy needs under OAuth & Permissions, install it, and copy the User OAuth Token.".into(),
            url: "https://api.slack.com/apps".into(),
        })
    }
    fn actions(&self) -> Vec<Action> {
        vec![
            Action::read(
                "search",
                "Search messages with Slack search syntax (in:#channel, from:@name, after:2026-05-01).",
                json!({"query": {"type": "string"}, "limit": {"type": "integer"}}),
                &["query"],
            ),
            Action::read("channels", "List the channels and direct messages the user is in, with their ids.", json!({"limit": {"type": "integer"}}), &[]),
            Action::read(
                "history",
                "Read recent messages in a channel by id, or one thread when thread_ts is given.",
                json!({"channel": {"type": "string"}, "thread_ts": {"type": "string"}, "limit": {"type": "integer"}}),
                &["channel"],
            ),
            Action::write(
                "post",
                "Post a message as the user in a channel (by id), or as a reply in a thread.",
                json!({"channel": {"type": "string"}, "text": {"type": "string"}, "thread_ts": {"type": "string"}}),
                &["channel", "text"],
                Rule::Ask,
                &["text"],
            ),
        ]
    }
    fn describe(&self, _: &str, args: &Value) -> (String, String) {
        let channel = args["channel"].as_str().unwrap_or("a channel");
        let where_ = if opt(args, "thread_ts").is_some() {
            format!("a thread in {channel}")
        } else {
            channel.to_string()
        };
        (
            format!("Post in Slack to {where_}"),
            args["text"].as_str().unwrap_or("").to_string(),
        )
    }
    fn account<'a>(&'a self, api: &'a Api, _: &'a Tokens) -> BoxFuture<'a, ApiResult<String>> {
        Box::pin(async move {
            let me = api.get(&format!("{API}/auth.test"), &[]).await?;
            Ok(format!(
                "{} in {}",
                me["user"].as_str().unwrap_or("you"),
                me["team"].as_str().unwrap_or("Slack")
            ))
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
                "search" => self.search(api, args).await,
                "channels" => self.channels(api, args).await,
                "history" => self.history(api, args).await,
                "post" => {
                    async {
                        let mut body =
                            json!({ "channel": arg(args, "channel")?, "text": arg(args, "text")? });
                        if let Some(ts) = opt(args, "thread_ts") {
                            body["thread_ts"] = json!(ts);
                        }
                        let r = api.post(&format!("{API}/chat.postMessage"), &body).await?;
                        Ok(format!("Posted (ts {}).", r["ts"].as_str().unwrap_or("")))
                    }
                    .await
                }
                other => Err(ToolOutcome::Permanent(format!(
                    "Slack has no action {other}."
                ))),
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_messages_as_lines() {
        let m = json!({"ts": "1714.1", "text": "ship it", "reply_count": 2});
        assert_eq!(
            message_line(&m, "Ann"),
            "[ts 1714.1] Ann: ship it (2 replies)"
        );
    }
}
