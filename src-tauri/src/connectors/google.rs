//! Gmail, Google Calendar and Google Drive. Each connects on its own, with
//! only the scopes it needs, through the user's Google OAuth app.

use base64::engine::general_purpose::{STANDARD, URL_SAFE, URL_SAFE_NO_PAD};
use base64::Engine;
use futures_util::future::BoxFuture;
use serde_json::{json, Value};

use super::api::{arg, done, limit, opt, Api, ApiResult};
use super::oauth::{ClientAuth, Tokens};
use super::{Action, Connector, OAuthSpec};
use crate::agents::runner::ToolOutcome;
use crate::settings::schema::Rule;

const GMAIL: &str = "https://gmail.googleapis.com/gmail/v1/users/me";
const CALENDAR: &str = "https://www.googleapis.com/calendar/v3";
const DRIVE: &str = "https://www.googleapis.com/drive/v3";
/// The most text of one email or file handed to an agent.
const MAX_TEXT: usize = 20_000;

fn spec(read: &'static [&'static str], write: &'static [&'static str]) -> OAuthSpec {
    OAuthSpec {
        provider: "google",
        authorize_url: "https://accounts.google.com/o/oauth2/v2/auth",
        token_url: "https://oauth2.googleapis.com/token",
        read_scopes: read,
        write_scopes: write,
        // Google gives desktop apps a client secret and wants it back.
        needs_secret: true,
        client_auth: ClientAuth::Body,
        scope_param: "scope",
        // A refresh token every time, not just on the first consent.
        extra: &[("access_type", "offline"), ("prompt", "consent")],
        host: "127.0.0.1",
        port: 0,
    }
}

pub(super) fn cut(mut s: String) -> String {
    if s.len() > MAX_TEXT {
        let at = (0..=MAX_TEXT)
            .rev()
            .find(|i| s.is_char_boundary(*i))
            .unwrap_or(0);
        s.truncate(at);
        s += "\n[cut]";
    }
    s
}

// ---------- Gmail ----------

pub struct Gmail;

fn header<'a>(msg: &'a Value, name: &str) -> &'a str {
    msg["payload"]["headers"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|h| {
            h["name"]
                .as_str()
                .is_some_and(|n| n.eq_ignore_ascii_case(name))
        })
        .and_then(|h| h["value"].as_str())
        .unwrap_or("")
}

fn decode(data: &str) -> String {
    let bytes = URL_SAFE
        .decode(data)
        .or_else(|_| URL_SAFE_NO_PAD.decode(data))
        .unwrap_or_default();
    String::from_utf8_lossy(&bytes).into_owned()
}

/// A message's readable text: its plain part, or its HTML part as text.
fn body_text(payload: &Value) -> String {
    fn find(p: &Value, mime: &str) -> Option<String> {
        if p["mimeType"].as_str() == Some(mime) {
            if let Some(d) = p["body"]["data"].as_str() {
                return Some(decode(d));
            }
        }
        p["parts"]
            .as_array()
            .into_iter()
            .flatten()
            .find_map(|q| find(q, mime))
    }
    find(payload, "text/plain")
        .or_else(|| find(payload, "text/html").map(|h| html2text::from_read(h.as_bytes(), 100)))
        .unwrap_or_default()
}

fn attachments(payload: &Value) -> Vec<String> {
    let mut out = Vec::new();
    fn walk(p: &Value, out: &mut Vec<String>) {
        if let Some(name) = p["filename"].as_str().filter(|n| !n.is_empty()) {
            out.push(name.to_string());
        }
        for q in p["parts"].as_array().into_iter().flatten() {
            walk(q, out);
        }
    }
    walk(payload, &mut out);
    out
}

/// A header value, encoded if it isn't plain ASCII (RFC 2047).
fn encode_header(v: &str) -> String {
    if v.is_ascii() {
        v.to_string()
    } else {
        format!("=?UTF-8?B?{}?=", STANDARD.encode(v))
    }
}

/// Headers a single line can't break out of.
fn one_line(v: &str) -> String {
    v.replace(['\r', '\n'], " ")
}

struct Mail<'a> {
    to: &'a str,
    cc: Option<&'a str>,
    subject: &'a str,
    body: &'a str,
    in_reply_to: Option<String>,
}

/// An RFC 2822 message, as Gmail's `raw` wants it.
fn raw_message(m: &Mail) -> String {
    let mut text = format!("To: {}\r\n", one_line(m.to));
    if let Some(cc) = m.cc {
        text += &format!("Cc: {}\r\n", one_line(cc));
    }
    text += &format!("Subject: {}\r\n", encode_header(&one_line(m.subject)));
    if let Some(id) = &m.in_reply_to {
        text += &format!("In-Reply-To: {id}\r\nReferences: {id}\r\n");
    }
    text += "MIME-Version: 1.0\r\nContent-Type: text/plain; charset=UTF-8\r\nContent-Transfer-Encoding: base64\r\n\r\n";
    text += &STANDARD.encode(m.body.replace("\r\n", "\n").replace('\n', "\r\n"));
    URL_SAFE.encode(text)
}

impl Gmail {
    async fn search(&self, api: &Api, args: &Value) -> ApiResult<String> {
        let q = arg(args, "query")?;
        let list = api
            .get(
                &format!("{GMAIL}/messages"),
                &[
                    ("q", q.into()),
                    ("maxResults", limit(args, 10, 25).to_string()),
                ],
            )
            .await?;
        let ids: Vec<&str> = list["messages"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|m| m["id"].as_str())
            .collect();
        if ids.is_empty() {
            return Ok(format!("No emails match \"{q}\"."));
        }
        let mut out = Vec::new();
        for id in ids {
            let m = api
                .get(
                    &format!("{GMAIL}/messages/{id}"),
                    &[
                        ("format", "metadata".into()),
                        ("metadataHeaders", "From".into()),
                        ("metadataHeaders", "Subject".into()),
                        ("metadataHeaders", "Date".into()),
                    ],
                )
                .await?;
            out.push(format!(
                "id {id}\nFrom: {}\nDate: {}\nSubject: {}\n{}",
                header(&m, "From"),
                header(&m, "Date"),
                header(&m, "Subject"),
                m["snippet"].as_str().unwrap_or("")
            ));
        }
        Ok(out.join("\n\n"))
    }

    async fn read(&self, api: &Api, args: &Value) -> ApiResult<String> {
        let id = arg(args, "id")?;
        let m = api
            .get(
                &format!("{GMAIL}/messages/{id}"),
                &[("format", "full".into())],
            )
            .await?;
        let files = attachments(&m["payload"]);
        Ok(cut(format!(
            "From: {}\nTo: {}\nDate: {}\nSubject: {}\n{}\n{}",
            header(&m, "From"),
            header(&m, "To"),
            header(&m, "Date"),
            header(&m, "Subject"),
            if files.is_empty() {
                String::new()
            } else {
                format!("Attachments: {}\n", files.join(", "))
            },
            body_text(&m["payload"]).trim()
        )))
    }

    /// The message body for a draft or send, threaded when it's a reply.
    async fn message(&self, api: &Api, args: &Value) -> ApiResult<Value> {
        let mut mail = Mail {
            to: arg(args, "to")?,
            cc: opt(args, "cc"),
            subject: opt(args, "subject").unwrap_or(""),
            body: args["body"].as_str().unwrap_or(""),
            in_reply_to: None,
        };
        let mut thread = None;
        if let Some(id) = opt(args, "reply_to") {
            let m = api
                .get(
                    &format!("{GMAIL}/messages/{id}"),
                    &[
                        ("format", "metadata".into()),
                        ("metadataHeaders", "Message-ID".into()),
                    ],
                )
                .await?;
            mail.in_reply_to = Some(header(&m, "Message-ID").to_string()).filter(|s| !s.is_empty());
            thread = m["threadId"].as_str().map(String::from);
        }
        let mut msg = json!({ "raw": raw_message(&mail) });
        if let Some(t) = thread {
            msg["threadId"] = json!(t);
        }
        Ok(msg)
    }
}

pub(super) fn mail_properties() -> Value {
    json!({
        "to": {"type": "string", "description": "Recipients, comma separated"},
        "cc": {"type": "string"},
        "subject": {"type": "string"},
        "body": {"type": "string", "description": "Plain text"},
        "reply_to": {"type": "string", "description": "Id of the email this replies to, to keep the thread"}
    })
}

pub(super) fn mail_detail(args: &Value) -> String {
    let mut d = format!("To: {}\n", args["to"].as_str().unwrap_or(""));
    if let Some(cc) = opt(args, "cc") {
        d += &format!("Cc: {cc}\n");
    }
    d + &format!(
        "Subject: {}\n\n{}",
        args["subject"].as_str().unwrap_or(""),
        args["body"].as_str().unwrap_or("")
    )
}

impl Connector for Gmail {
    fn id(&self) -> &'static str {
        "gmail"
    }
    fn name(&self) -> &'static str {
        "Gmail"
    }
    fn about(&self) -> &'static str {
        "search and read the user's email in Gmail, write drafts and send email"
    }
    fn oauth(&self) -> Option<OAuthSpec> {
        Some(spec(
            &["https://www.googleapis.com/auth/gmail.readonly"],
            &[
                "https://www.googleapis.com/auth/gmail.compose",
                "https://www.googleapis.com/auth/gmail.send",
            ],
        ))
    }
    fn actions(&self) -> Vec<Action> {
        vec![
            Action::read(
                "search",
                "Search email with Gmail search syntax (from:, subject:, newer_than:7d, is:unread). Returns ids, senders, subjects and snippets.",
                json!({"query": {"type": "string"}, "limit": {"type": "integer"}}),
                &["query"],
            ),
            Action::read("read", "Read one email in full by id.", json!({"id": {"type": "string"}}), &["id"]),
            Action::write("draft", "Save an email as a draft for the user to send.", mail_properties(), &["to"], Rule::Allow, &["to", "cc", "subject", "body"]),
            Action::write("send", "Send an email.", mail_properties(), &["to", "subject", "body"], Rule::Ask, &["to", "cc", "subject", "body"]),
        ]
    }
    fn describe(&self, action: &str, args: &Value) -> (String, String) {
        let to = args["to"].as_str().unwrap_or("someone");
        let summary = match action {
            "send" => format!("Send an email to {to}"),
            "draft" => format!("Save a draft to {to}"),
            other => format!("Gmail {other}"),
        };
        (summary, mail_detail(args))
    }
    fn account<'a>(&'a self, api: &'a Api, _: &'a Tokens) -> BoxFuture<'a, ApiResult<String>> {
        Box::pin(async move {
            let p = api.get(&format!("{GMAIL}/profile"), &[]).await?;
            Ok(p["emailAddress"].as_str().unwrap_or("Gmail").to_string())
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
                "draft" => {
                    async {
                        let msg = self.message(api, args).await?;
                        api.post(&format!("{GMAIL}/drafts"), &json!({ "message": msg }))
                            .await?;
                        Ok("Saved the draft in Gmail.".to_string())
                    }
                    .await
                }
                "send" => {
                    async {
                        let msg = self.message(api, args).await?;
                        api.post(&format!("{GMAIL}/messages/send"), &msg).await?;
                        Ok(format!(
                            "Sent the email to {}.",
                            args["to"].as_str().unwrap_or("")
                        ))
                    }
                    .await
                }
                other => Err(ToolOutcome::Permanent(format!(
                    "Gmail has no action {other}."
                ))),
            })
        })
    }
}

// ---------- Google Calendar ----------

pub struct Calendar;

/// A calendar time: a date alone means all day.
fn event_time(v: &str) -> Value {
    if v.len() == 10 {
        json!({ "date": v })
    } else {
        json!({ "dateTime": v })
    }
}

fn event_line(e: &Value) -> String {
    let when = |t: &Value| {
        t["dateTime"]
            .as_str()
            .or(t["date"].as_str())
            .unwrap_or("")
            .to_string()
    };
    let mut line = format!(
        "{} to {}: {}",
        when(&e["start"]),
        when(&e["end"]),
        e["summary"].as_str().unwrap_or("(no title)")
    );
    if let Some(l) = e["location"].as_str() {
        line += &format!(" at {l}");
    }
    let people: Vec<&str> = e["attendees"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|a| a["email"].as_str())
        .collect();
    if !people.is_empty() {
        line += &format!(" with {}", people.join(", "));
    }
    line
}

impl Connector for Calendar {
    fn id(&self) -> &'static str {
        "calendar"
    }
    fn name(&self) -> &'static str {
        "Google Calendar"
    }
    fn about(&self) -> &'static str {
        "see the user's Google Calendar events and add new ones"
    }
    fn oauth(&self) -> Option<OAuthSpec> {
        Some(spec(
            &["https://www.googleapis.com/auth/calendar.readonly"],
            &["https://www.googleapis.com/auth/calendar.events"],
        ))
    }
    fn actions(&self) -> Vec<Action> {
        vec![
            Action::read(
                "events",
                "List events in the user's main calendar between two times (RFC 3339, e.g. 2026-05-01T00:00:00+02:00), optionally matching words.",
                json!({"from": {"type": "string"}, "to": {"type": "string"}, "query": {"type": "string"}, "limit": {"type": "integer"}}),
                &["from", "to"],
            ),
            Action::write(
                "create_event",
                "Add an event to the user's main calendar. Times are RFC 3339 with the UTC offset, or YYYY-MM-DD for all day.",
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
    fn describe(&self, _: &str, args: &Value) -> (String, String) {
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
        (format!("Add \"{title}\" to Google Calendar"), detail)
    }
    fn account<'a>(&'a self, api: &'a Api, _: &'a Tokens) -> BoxFuture<'a, ApiResult<String>> {
        Box::pin(async move {
            let c = api
                .get(&format!("{CALENDAR}/users/me/calendarList/primary"), &[])
                .await?;
            Ok(c["id"].as_str().unwrap_or("Google Calendar").to_string())
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
                "events" => {
                    async {
                        let mut q = vec![
                            ("timeMin", arg(args, "from")?.to_string()),
                            ("timeMax", arg(args, "to")?.to_string()),
                            ("singleEvents", "true".into()),
                            ("orderBy", "startTime".into()),
                            ("maxResults", limit(args, 25, 100).to_string()),
                        ];
                        if let Some(words) = opt(args, "query") {
                            q.push(("q", words.into()));
                        }
                        let list = api
                            .get(&format!("{CALENDAR}/calendars/primary/events"), &q)
                            .await?;
                        let lines: Vec<String> = list["items"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .map(event_line)
                            .collect();
                        Ok(if lines.is_empty() {
                            "No events then.".into()
                        } else {
                            lines.join("\n")
                        })
                    }
                    .await
                }
                "create_event" => {
                    async {
                        let mut e = json!({
                            "summary": arg(args, "title")?,
                            "start": event_time(arg(args, "start")?),
                            "end": event_time(arg(args, "end")?),
                        });
                        for (from, to) in [("location", "location"), ("description", "description")]
                        {
                            if let Some(v) = opt(args, from) {
                                e[to] = json!(v);
                            }
                        }
                        if let Some(a) = opt(args, "attendees") {
                            e["attendees"] = a
                                .split(',')
                                .map(str::trim)
                                .filter(|x| !x.is_empty())
                                .map(|x| json!({ "email": x }))
                                .collect();
                        }
                        let made = api
                            .post(&format!("{CALENDAR}/calendars/primary/events"), &e)
                            .await?;
                        Ok(format!("Added: {}", event_line(&made)))
                    }
                    .await
                }
                other => Err(ToolOutcome::Permanent(format!(
                    "Google Calendar has no action {other}."
                ))),
            })
        })
    }
}

// ---------- Google Drive ----------

pub struct Drive;

/// What Google's own formats export as, for reading.
fn export_type(mime: &str) -> Option<&'static str> {
    match mime {
        "application/vnd.google-apps.document" | "application/vnd.google-apps.presentation" => {
            Some("text/plain")
        }
        "application/vnd.google-apps.spreadsheet" => Some("text/csv"),
        _ => None,
    }
}

fn readable(mime: &str) -> bool {
    mime.starts_with("text/")
        || ["application/json", "application/xml", "application/csv"].contains(&mime)
}

/// A Drive search query: words match names and contents.
fn drive_query(words: &str) -> String {
    let w = words.replace('\\', "\\\\").replace('\'', "\\'");
    format!("fullText contains '{w}' and trashed = false")
}

impl Drive {
    async fn read(&self, api: &Api, args: &Value) -> ApiResult<String> {
        let id = arg(args, "id")?;
        let meta = api
            .get(
                &format!("{DRIVE}/files/{id}"),
                &[("fields", "name,mimeType".into())],
            )
            .await?;
        let mime = meta["mimeType"].as_str().unwrap_or("");
        let name = meta["name"].as_str().unwrap_or(id);
        let text = if let Some(to) = export_type(mime) {
            api.get_text(
                &format!("{DRIVE}/files/{id}/export"),
                &[("mimeType", to.into())],
            )
            .await?
        } else if readable(mime) {
            api.get_text(&format!("{DRIVE}/files/{id}"), &[("alt", "media".into())])
                .await?
        } else {
            return Err(ToolOutcome::Permanent(format!(
                "\"{name}\" is a {mime} file, which Helpy can't read as text."
            )));
        };
        Ok(cut(format!("{name}\n\n{text}")))
    }

    /// Makes a Google Doc from text, with a multipart upload.
    async fn create(&self, api: &Api, args: &Value) -> ApiResult<String> {
        let name = arg(args, "name")?;
        let meta = json!({ "name": name, "mimeType": "application/vnd.google-apps.document" });
        let form = reqwest::multipart::Form::new()
            .part(
                "metadata",
                reqwest::multipart::Part::text(meta.to_string())
                    .mime_str("application/json")
                    .expect("valid mime"),
            )
            .part(
                "file",
                reqwest::multipart::Part::text(args["text"].as_str().unwrap_or("").to_string())
                    .mime_str("text/plain")
                    .expect("valid mime"),
            );
        let resp = api
            .http
            .post("https://www.googleapis.com/upload/drive/v3/files")
            .query(&[("uploadType", "multipart"), ("fields", "id,webViewLink")])
            .bearer_auth(&api.token)
            .multipart(form)
            .send()
            .await
            .map_err(|_| ToolOutcome::Transient("Google Drive didn't respond".into()))?;
        let status = resp.status();
        let v: Value = resp.json().await.unwrap_or(Value::Null);
        if !status.is_success() {
            return Err(ToolOutcome::Permanent(format!(
                "Google Drive answered {status}: {}",
                v["error"]["message"].as_str().unwrap_or("")
            )));
        }
        Ok(format!(
            "Created \"{name}\": {}",
            v["webViewLink"].as_str().unwrap_or("")
        ))
    }
}

impl Connector for Drive {
    fn id(&self) -> &'static str {
        "drive"
    }
    fn name(&self) -> &'static str {
        "Google Drive"
    }
    fn about(&self) -> &'static str {
        "find and read files in the user's Google Drive, and create Google Docs"
    }
    fn oauth(&self) -> Option<OAuthSpec> {
        // drive.file only reaches files Helpy created, so reading needs its own scope.
        Some(spec(
            &["https://www.googleapis.com/auth/drive.readonly"],
            &["https://www.googleapis.com/auth/drive.file"],
        ))
    }
    fn actions(&self) -> Vec<Action> {
        vec![
            Action::read(
                "search",
                "Find files whose name or text contains the words. Returns ids, names, types and links.",
                json!({"query": {"type": "string"}, "limit": {"type": "integer"}}),
                &["query"],
            ),
            Action::read("read", "Read a file's text by id (Docs, Sheets as CSV, Slides, text files).", json!({"id": {"type": "string"}}), &["id"]),
            Action::write(
                "create_doc",
                "Create a Google Doc with the given text.",
                json!({"name": {"type": "string"}, "text": {"type": "string"}}),
                &["name", "text"],
                Rule::Ask,
                &["name", "text"],
            ),
        ]
    }
    fn describe(&self, _: &str, args: &Value) -> (String, String) {
        let name = args["name"].as_str().unwrap_or("a document");
        (
            format!("Create the Google Doc \"{name}\""),
            args["text"].as_str().unwrap_or("").to_string(),
        )
    }
    fn account<'a>(&'a self, api: &'a Api, _: &'a Tokens) -> BoxFuture<'a, ApiResult<String>> {
        Box::pin(async move {
            let a = api
                .get(&format!("{DRIVE}/about"), &[("fields", "user".into())])
                .await?;
            Ok(a["user"]["emailAddress"]
                .as_str()
                .unwrap_or("Google Drive")
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
                "search" => {
                    async {
                        let list = api
                            .get(
                                &format!("{DRIVE}/files"),
                                &[
                                    ("q", drive_query(arg(args, "query")?)),
                                    ("pageSize", limit(args, 10, 50).to_string()),
                                    (
                                        "fields",
                                        "files(id,name,mimeType,modifiedTime,webViewLink)".into(),
                                    ),
                                ],
                            )
                            .await?;
                        let lines: Vec<String> = list["files"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .map(|f| {
                                format!(
                                    "id {} | {} | {} | changed {} | {}",
                                    f["id"].as_str().unwrap_or(""),
                                    f["name"].as_str().unwrap_or(""),
                                    f["mimeType"].as_str().unwrap_or(""),
                                    f["modifiedTime"].as_str().unwrap_or(""),
                                    f["webViewLink"].as_str().unwrap_or("")
                                )
                            })
                            .collect();
                        Ok(if lines.is_empty() {
                            "No files match.".into()
                        } else {
                            lines.join("\n")
                        })
                    }
                    .await
                }
                "read" => self.read(api, args).await,
                "create_doc" => self.create(api, args).await,
                other => Err(ToolOutcome::Permanent(format!(
                    "Google Drive has no action {other}."
                ))),
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_plain_part_or_the_html() {
        let enc = |s: &str| URL_SAFE.encode(s);
        let plain = json!({"mimeType": "multipart/alternative", "parts": [
            {"mimeType": "text/html", "body": {"data": enc("<p>Hi <b>there</b></p>")}},
            {"mimeType": "text/plain", "body": {"data": enc("Hi there")}},
            {"mimeType": "application/pdf", "filename": "invoice.pdf", "body": {}}
        ]});
        assert_eq!(body_text(&plain), "Hi there");
        assert_eq!(attachments(&plain), vec!["invoice.pdf"]);
        let html =
            json!({"mimeType": "text/html", "body": {"data": enc("<p>Hi <b>there</b></p>")}});
        assert!(body_text(&html).contains("there"));
    }

    #[test]
    fn builds_a_mail_that_cannot_inject_headers() {
        let raw = raw_message(&Mail {
            to: "a@x.com\r\nBcc: evil@x.com",
            cc: None,
            subject: "Café",
            body: "line one\nline two",
            in_reply_to: Some("<m1@x>".into()),
        });
        let text = String::from_utf8(URL_SAFE.decode(raw).unwrap()).unwrap();
        assert!(text.starts_with("To: a@x.com  Bcc: evil@x.com\r\n"));
        assert!(text.contains("Subject: =?UTF-8?B?"));
        assert!(text.contains("In-Reply-To: <m1@x>\r\n"));
        let body = text.split("\r\n\r\n").nth(1).unwrap();
        assert_eq!(
            String::from_utf8(STANDARD.decode(body).unwrap()).unwrap(),
            "line one\r\nline two"
        );
    }

    #[test]
    fn calendar_and_drive_helpers() {
        assert_eq!(event_time("2026-05-01"), json!({"date": "2026-05-01"}));
        assert_eq!(
            event_time("2026-05-01T09:00:00+02:00"),
            json!({"dateTime": "2026-05-01T09:00:00+02:00"})
        );
        let e = json!({"summary": "Standup", "start": {"dateTime": "9"}, "end": {"dateTime": "10"}, "attendees": [{"email": "a@x"}]});
        assert_eq!(event_line(&e), "9 to 10: Standup with a@x");
        assert_eq!(
            drive_query("bob's plan"),
            "fullText contains 'bob\\'s plan' and trashed = false"
        );
        assert_eq!(
            export_type("application/vnd.google-apps.spreadsheet"),
            Some("text/csv")
        );
        assert!(readable("text/markdown") && !readable("image/png"));
    }
}
