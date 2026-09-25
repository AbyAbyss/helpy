//! Outlook mail and calendar through Microsoft Graph, for work, school and
//! personal Microsoft accounts.

use chrono::{DateTime, NaiveDate};
use futures_util::future::BoxFuture;
use serde_json::{json, Value};

use super::api::{arg, done, limit, opt, Api, ApiResult};
use super::google::{cut, mail_detail, mail_properties};
use super::oauth::{ClientAuth, Tokens};
use super::{Action, Connector, OAuthSpec};
use crate::agents::runner::ToolOutcome;
use crate::settings::schema::Rule;

const GRAPH: &str = "https://graph.microsoft.com/v1.0/me";

pub struct Outlook;

fn recipients(list: &str) -> Value {
    list.split(',')
        .map(str::trim)
        .filter(|a| !a.is_empty())
        .map(|a| json!({ "emailAddress": { "address": a } }))
        .collect()
}

fn address(v: &Value) -> String {
    let e = &v["emailAddress"];
    match (e["name"].as_str(), e["address"].as_str()) {
        (Some(n), Some(a)) if !n.is_empty() && n != a => format!("{n} <{a}>"),
        (_, Some(a)) => a.to_string(),
        _ => String::new(),
    }
}

fn addresses(v: &Value) -> String {
    v.as_array()
        .into_iter()
        .flatten()
        .map(address)
        .collect::<Vec<_>>()
        .join(", ")
}

/// Graph wants a time without an offset plus its zone; everything goes as UTC.
fn graph_time(v: &str) -> Result<Value, ToolOutcome> {
    if let Ok(d) = NaiveDate::parse_from_str(v, "%Y-%m-%d") {
        return Ok(json!({ "dateTime": format!("{d}T00:00:00"), "timeZone": "UTC" }));
    }
    let t = DateTime::parse_from_rfc3339(v).map_err(|_| {
        ToolOutcome::Permanent(format!(
            "\"{v}\" isn't a time like 2026-05-01T09:00:00+02:00."
        ))
    })?;
    Ok(
        json!({ "dateTime": t.naive_utc().format("%Y-%m-%dT%H:%M:%S").to_string(), "timeZone": "UTC" }),
    )
}

fn event_line(e: &Value) -> String {
    let when = |t: &Value| {
        format!(
            "{} {}",
            t["dateTime"].as_str().unwrap_or(""),
            t["timeZone"].as_str().unwrap_or("")
        )
    };
    let mut line = format!(
        "{} to {}: {}",
        when(&e["start"]),
        when(&e["end"]),
        e["subject"].as_str().unwrap_or("(no title)")
    );
    if let Some(l) = e["location"]["displayName"]
        .as_str()
        .filter(|l| !l.is_empty())
    {
        line += &format!(" at {l}");
    }
    let people = addresses(&e["attendees"]);
    if !people.is_empty() {
        line += &format!(" with {people}");
    }
    line
}

fn message(args: &Value) -> ApiResult<Value> {
    let mut m = json!({
        "subject": opt(args, "subject").unwrap_or(""),
        "body": { "contentType": "Text", "content": args["body"].as_str().unwrap_or("") },
        "toRecipients": recipients(arg(args, "to")?),
    });
    if let Some(cc) = opt(args, "cc") {
        m["ccRecipients"] = recipients(cc);
    }
    Ok(m)
}

impl Outlook {
    async fn search(&self, api: &Api, args: &Value) -> ApiResult<String> {
        let q = arg(args, "query")?.replace('"', "");
        let list = api
            .get(
                &format!("{GRAPH}/messages"),
                &[
                    ("$search", format!("\"{q}\"")),
                    ("$top", limit(args, 10, 25).to_string()),
                    (
                        "$select",
                        "id,subject,from,receivedDateTime,bodyPreview".into(),
                    ),
                ],
            )
            .await?;
        let lines: Vec<String> = list["value"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|m| {
                format!(
                    "id {}\nFrom: {}\nDate: {}\nSubject: {}\n{}",
                    m["id"].as_str().unwrap_or(""),
                    address(&m["from"]),
                    m["receivedDateTime"].as_str().unwrap_or(""),
                    m["subject"].as_str().unwrap_or(""),
                    m["bodyPreview"].as_str().unwrap_or("")
                )
            })
            .collect();
        Ok(if lines.is_empty() {
            format!("No emails match \"{q}\".")
        } else {
            lines.join("\n\n")
        })
    }

    async fn read(&self, api: &Api, args: &Value) -> ApiResult<String> {
        let id = arg(args, "id")?;
        let m = api
            .get(
                &format!("{GRAPH}/messages/{id}"),
                &[(
                    "$select",
                    "subject,from,toRecipients,ccRecipients,receivedDateTime,body,hasAttachments"
                        .into(),
                )],
            )
            .await?;
        let body = m["body"]["content"].as_str().unwrap_or("");
        let body = if m["body"]["contentType"].as_str() == Some("html") {
            html2text::from_read(body.as_bytes(), 100)
        } else {
            body.to_string()
        };
        Ok(cut(format!(
            "From: {}\nTo: {}\nCc: {}\nDate: {}\nSubject: {}\n{}\n{}",
            address(&m["from"]),
            addresses(&m["toRecipients"]),
            addresses(&m["ccRecipients"]),
            m["receivedDateTime"].as_str().unwrap_or(""),
            m["subject"].as_str().unwrap_or(""),
            if m["hasAttachments"] == json!(true) {
                "Has attachments\n"
            } else {
                ""
            },
            body.trim()
        )))
    }

    async fn draft(&self, api: &Api, args: &Value) -> ApiResult<String> {
        let msg = message(args)?;
        match opt(args, "reply_to") {
            Some(id) => {
                let draft = api
                    .post(
                        &format!("{GRAPH}/messages/{id}/createReply"),
                        &json!({ "message": msg }),
                    )
                    .await?;
                // createReply quotes the original; put the reply text on top.
                let quoted = draft["body"]["content"].as_str().unwrap_or("").to_string();
                let did = draft["id"].as_str().unwrap_or("");
                let text = args["body"].as_str().unwrap_or("");
                let content = if draft["body"]["contentType"].as_str() == Some("html") {
                    format!("<p>{}</p>{quoted}", html_escape(text).replace('\n', "<br>"))
                } else {
                    format!("{text}\n\n{quoted}")
                };
                api.patch(
                    &format!("{GRAPH}/messages/{did}"),
                    &json!({ "body": { "contentType": draft["body"]["contentType"], "content": content } }),
                )
                .await?;
            }
            None => {
                api.post(&format!("{GRAPH}/messages"), &msg).await?;
            }
        }
        Ok("Saved the draft in Outlook.".into())
    }

    async fn send(&self, api: &Api, args: &Value) -> ApiResult<String> {
        let msg = message(args)?;
        match opt(args, "reply_to") {
            Some(id) => {
                let text = args["body"].as_str().unwrap_or("");
                api.post(
                    &format!("{GRAPH}/messages/{id}/reply"),
                    &json!({ "message": msg, "comment": text }),
                )
                .await?;
            }
            None => {
                api.post(
                    &format!("{GRAPH}/sendMail"),
                    &json!({ "message": msg, "saveToSentItems": true }),
                )
                .await?;
            }
        }
        Ok(format!(
            "Sent the email to {}.",
            args["to"].as_str().unwrap_or("")
        ))
    }

    async fn events(&self, api: &Api, args: &Value) -> ApiResult<String> {
        let list = api
            .get(
                &format!("{GRAPH}/calendarView"),
                &[
                    ("startDateTime", arg(args, "from")?.into()),
                    ("endDateTime", arg(args, "to")?.into()),
                    ("$top", limit(args, 25, 100).to_string()),
                    ("$orderby", "start/dateTime".into()),
                    ("$select", "subject,start,end,location,attendees".into()),
                ],
            )
            .await?;
        let words = opt(args, "query").map(str::to_lowercase);
        let lines: Vec<String> = list["value"]
            .as_array()
            .into_iter()
            .flatten()
            .map(event_line)
            .filter(|l| words.as_ref().is_none_or(|w| l.to_lowercase().contains(w)))
            .collect();
        Ok(if lines.is_empty() {
            "No events then.".into()
        } else {
            lines.join("\n")
        })
    }

    async fn create_event(&self, api: &Api, args: &Value) -> ApiResult<String> {
        let start = arg(args, "start")?;
        let mut e = json!({
            "subject": arg(args, "title")?,
            "start": graph_time(start)?,
            "end": graph_time(arg(args, "end")?)?,
            "isAllDay": start.len() == 10,
        });
        if let Some(l) = opt(args, "location") {
            e["location"] = json!({ "displayName": l });
        }
        if let Some(d) = opt(args, "description") {
            e["body"] = json!({ "contentType": "Text", "content": d });
        }
        if let Some(a) = opt(args, "attendees") {
            e["attendees"] = recipients(a)
                .as_array()
                .into_iter()
                .flatten()
                .map(|r| json!({ "emailAddress": r["emailAddress"], "type": "required" }))
                .collect();
        }
        let made = api.post(&format!("{GRAPH}/events"), &e).await?;
        Ok(format!("Added: {}", event_line(&made)))
    }
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

impl Connector for Outlook {
    fn id(&self) -> &'static str {
        "outlook"
    }
    fn name(&self) -> &'static str {
        "Outlook"
    }
    fn about(&self) -> &'static str {
        "search and read the user's Outlook email, write drafts and send email, see and add Outlook calendar events"
    }
    fn oauth(&self) -> Option<OAuthSpec> {
        Some(OAuthSpec {
            provider: "microsoft",
            authorize_url: "https://login.microsoftonline.com/common/oauth2/v2.0/authorize",
            token_url: "https://login.microsoftonline.com/common/oauth2/v2.0/token",
            read_scopes: &["User.Read", "Mail.Read", "Calendars.Read", "offline_access"],
            write_scopes: &["Mail.ReadWrite", "Mail.Send", "Calendars.ReadWrite"],
            // A public client: PKCE, no secret.
            needs_secret: false,
            client_auth: ClientAuth::Body,
            scope_param: "scope",
            extra: &[],
            // Microsoft ignores the port only for "localhost", registered
            // once as http://localhost/callback.
            host: "localhost",
            port: 0,
        })
    }
    fn headers(&self) -> Vec<(&'static str, String)> {
        vec![("Prefer", "outlook.body-content-type=\"text\"".into())]
    }
    fn actions(&self) -> Vec<Action> {
        vec![
            Action::read(
                "search",
                "Search email by words (sender, subject, text). Returns ids, senders, subjects and previews.",
                json!({"query": {"type": "string"}, "limit": {"type": "integer"}}),
                &["query"],
            ),
            Action::read("read", "Read one email in full by id.", json!({"id": {"type": "string"}}), &["id"]),
            Action::write("draft", "Save an email as a draft for the user to send.", mail_properties(), &["to"], Rule::Allow, &["to", "cc", "subject", "body"]),
            Action::write("send", "Send an email.", mail_properties(), &["to", "subject", "body"], Rule::Ask, &["to", "cc", "subject", "body"]),
            Action::read(
                "events",
                "List calendar events between two times (RFC 3339, e.g. 2026-05-01T00:00:00+02:00), optionally matching words. Times come back in UTC.",
                json!({"from": {"type": "string"}, "to": {"type": "string"}, "query": {"type": "string"}, "limit": {"type": "integer"}}),
                &["from", "to"],
            ),
            Action::write(
                "create_event",
                "Add an event to the calendar. Times are RFC 3339 with the UTC offset, or YYYY-MM-DD for all day.",
                json!({
                    "title": {"type": "string"}, "start": {"type": "string"}, "end": {"type": "string"},
                    "location": {"type": "string"}, "description": {"type": "string"},
                    "attendees": {"type": "string", "description": "Emails to invite, comma separated"}
                }),
                &["title", "start", "end"],
                Rule::Ask,
                &["title", "start", "end", "location", "description", "attendees"],
            ),
        ]
    }
    fn describe(&self, action: &str, args: &Value) -> (String, String) {
        let to = args["to"].as_str().unwrap_or("someone");
        match action {
            "send" => (format!("Send an email to {to}"), mail_detail(args)),
            "draft" => (format!("Save a draft to {to}"), mail_detail(args)),
            _ => {
                let title = args["title"].as_str().unwrap_or("an event");
                let mut detail = format!(
                    "{title}\n{} to {}",
                    args["start"].as_str().unwrap_or(""),
                    args["end"].as_str().unwrap_or("")
                );
                for k in ["location", "attendees", "description"] {
                    if let Some(v) = opt(args, k) {
                        detail += &format!("\n{k}: {v}");
                    }
                }
                (format!("Add \"{title}\" to the Outlook calendar"), detail)
            }
        }
    }
    fn account<'a>(&'a self, api: &'a Api, _: &'a Tokens) -> BoxFuture<'a, ApiResult<String>> {
        Box::pin(async move {
            let me = api
                .get(GRAPH, &[("$select", "mail,userPrincipalName".into())])
                .await?;
            Ok(me["mail"]
                .as_str()
                .or(me["userPrincipalName"].as_str())
                .unwrap_or("Outlook")
                .to_string())
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
                "read" => self.read(api, args).await,
                "draft" => self.draft(api, args).await,
                "send" => self.send(api, args).await,
                "events" => self.events(api, args).await,
                "create_event" => self.create_event(api, args).await,
                other => Err(ToolOutcome::Permanent(format!(
                    "Outlook has no action {other}."
                ))),
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn times_go_to_graph_as_utc() {
        assert_eq!(
            graph_time("2026-05-01T09:30:00+02:00").unwrap(),
            json!({"dateTime": "2026-05-01T07:30:00", "timeZone": "UTC"})
        );
        assert_eq!(
            graph_time("2026-05-01").unwrap()["dateTime"],
            "2026-05-01T00:00:00"
        );
        assert!(graph_time("tomorrow at 9").is_err());
    }

    #[test]
    fn builds_messages_and_reads_people() {
        let m = message(
            &json!({"to": "a@x.com, b@x.com", "cc": "c@x.com", "subject": "Hi", "body": "Yo"}),
        )
        .unwrap();
        assert_eq!(m["toRecipients"][1]["emailAddress"]["address"], "b@x.com");
        assert_eq!(m["ccRecipients"][0]["emailAddress"]["address"], "c@x.com");
        assert!(message(&json!({"subject": "no one"})).is_err());
        let people = json!([{"emailAddress": {"name": "Ann", "address": "ann@x"}}, {"emailAddress": {"name": "bob@x", "address": "bob@x"}}]);
        assert_eq!(addresses(&people), "Ann <ann@x>, bob@x");
    }
}
