//! Notion pages and databases, through a public integration (OAuth) or an
//! internal integration's token.

use futures_util::future::BoxFuture;
use reqwest::Method;
use serde_json::{json, Value};

use super::api::{arg, done, limit, Api, ApiResult};
use super::notion_blocks::blocks;
use super::google::cut;
use super::oauth::{ClientAuth, Tokens};
use super::{Action, Connector, OAuthSpec, TokenHelp};
use crate::agents::runner::ToolOutcome;
use crate::settings::schema::Rule;

const API: &str = "https://api.notion.com/v1";
/// The API version the calls below are written for (data sources).
const VERSION: &str = "2025-09-03";
/// Blocks read from one page, nested ones included.
const MAX_BLOCKS: usize = 300;
/// Blocks Notion takes in one request.
const BATCH: usize = 100;
/// How agents should write text for Notion.
macro_rules! markdown {
    () => {
        "Markdown: # headings, - bullets, 1. numbered, - [ ] to-dos, > quotes, ``` code, | tables |, **bold**, \
         *italic*, `code`, [links](url). It becomes Notion's own blocks and styles."
    };
}

pub struct Notion;

fn plain(rich: &Value) -> String {
    rich.as_array()
        .into_iter()
        .flatten()
        .filter_map(|r| r["plain_text"].as_str())
        .collect()
}

/// A page's or data source's title.
fn title(obj: &Value) -> String {
    if obj["title"].is_array() {
        return plain(&obj["title"]);
    }
    obj["properties"]
        .as_object()
        .into_iter()
        .flatten()
        .find(|(_, p)| p["type"] == "title")
        .map(|(_, p)| plain(&p["title"]))
        .unwrap_or_default()
}

/// One block as a line of text, in a Markdown-like form.
fn block_line(b: &Value) -> Option<String> {
    let kind = b["type"].as_str()?;
    let data = &b[kind];
    let text = plain(&data["rich_text"]);
    Some(match kind {
        "heading_1" => format!("# {text}"),
        "heading_2" => format!("## {text}"),
        "heading_3" => format!("### {text}"),
        "bulleted_list_item" => format!("- {text}"),
        "numbered_list_item" => format!("1. {text}"),
        "to_do" => format!(
            "[{}] {text}",
            if data["checked"] == json!(true) {
                "x"
            } else {
                " "
            }
        ),
        "quote" | "callout" => format!("> {text}"),
        "code" => format!("```\n{text}\n```"),
        "child_page" => format!(
            "[page: {}] id {}",
            data["title"].as_str().unwrap_or(""),
            b["id"].as_str().unwrap_or("")
        ),
        "child_database" => format!(
            "[database: {}] id {}",
            data["title"].as_str().unwrap_or(""),
            b["id"].as_str().unwrap_or("")
        ),
        "divider" => "---".into(),
        "image" | "file" | "pdf" | "video" => format!("[{kind}]"),
        "bookmark" | "embed" | "link_preview" => data["url"].as_str().unwrap_or("").to_string(),
        _ if !text.is_empty() => text,
        _ => return None,
    })
}

/// A property's value as text, for database rows.
fn property_text(p: &Value) -> String {
    let kind = p["type"].as_str().unwrap_or("");
    let v = &p[kind];
    match kind {
        "title" | "rich_text" => plain(v),
        "number" => v.as_f64().map(|n| n.to_string()).unwrap_or_default(),
        "select" | "status" => v["name"].as_str().unwrap_or("").to_string(),
        "multi_select" => v
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|o| o["name"].as_str())
            .collect::<Vec<_>>()
            .join(", "),
        "date" => match (v["start"].as_str(), v["end"].as_str()) {
            (Some(s), Some(e)) => format!("{s} to {e}"),
            (Some(s), None) => s.to_string(),
            _ => String::new(),
        },
        "checkbox" => if v == &json!(true) { "yes" } else { "no" }.into(),
        "url" | "email" | "phone_number" => v.as_str().unwrap_or("").to_string(),
        "people" => v
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|u| u["name"].as_str())
            .collect::<Vec<_>>()
            .join(", "),
        _ => String::new(),
    }
}

/// Adds blocks to the end of a page or block, a batch at a time.
async fn append_blocks(api: &Api, id: &str, mut list: Vec<Value>) -> ApiResult<()> {
    while !list.is_empty() {
        let rest = list.split_off(list.len().min(BATCH));
        api.patch(&format!("{API}/blocks/{id}/children"), &json!({ "children": list }))
            .await?;
        list = rest;
    }
    Ok(())
}

/// Deletes a page's content, keeping its sub-pages and databases.
async fn clear(api: &Api, id: &str) -> ApiResult<usize> {
    let mut ids = Vec::new();
    let mut cursor: Option<String> = None;
    loop {
        let mut q = vec![("page_size", "100".to_string())];
        if let Some(c) = &cursor {
            q.push(("start_cursor", c.clone()));
        }
        let list = api.get(&format!("{API}/blocks/{id}/children"), &q).await?;
        for b in list["results"].as_array().into_iter().flatten() {
            if !matches!(b["type"].as_str(), Some("child_page" | "child_database")) {
                ids.extend(b["id"].as_str().map(String::from));
            }
        }
        cursor = list["next_cursor"].as_str().map(String::from);
        if cursor.is_none() {
            break;
        }
    }
    for b in &ids {
        api.send(Method::DELETE, &format!("{API}/blocks/{b}"), &[], None)
            .await?;
    }
    Ok(ids.len())
}

impl Notion {
    async fn search(&self, api: &Api, args: &Value) -> ApiResult<String> {
        let found = api
            .post(
                &format!("{API}/search"),
                &json!({ "query": arg(args, "query")?, "page_size": limit(args, 10, 50) }),
            )
            .await?;
        let lines: Vec<String> = found["results"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|r| {
                let kind = if r["object"] == "data_source" {
                    "database"
                } else {
                    "page"
                };
                format!(
                    "{kind} id {} | {} | edited {} | {}",
                    r["id"].as_str().unwrap_or(""),
                    title(r),
                    r["last_edited_time"].as_str().unwrap_or(""),
                    r["url"].as_str().unwrap_or("")
                )
            })
            .collect();
        Ok(if lines.is_empty() {
            "Nothing matches. The page may not be shared with Helpy's integration.".into()
        } else {
            lines.join("\n")
        })
    }

    async fn read(&self, api: &Api, args: &Value) -> ApiResult<String> {
        let id = arg(args, "id")?;
        let page = api.get(&format!("{API}/pages/{id}"), &[]).await?;
        let mut lines = vec![format!("# {}", title(&page))];
        // Walk the blocks depth-first, indenting nested ones.
        let mut stack = vec![(id.to_string(), 0usize)];
        let mut seen = 0;
        while let Some((parent, depth)) = stack.pop() {
            let mut cursor: Option<String> = None;
            let mut children = Vec::new();
            loop {
                let mut q = vec![("page_size", "100".to_string())];
                if let Some(c) = &cursor {
                    q.push(("start_cursor", c.clone()));
                }
                let list = api
                    .get(&format!("{API}/blocks/{parent}/children"), &q)
                    .await?;
                children.extend(list["results"].as_array().cloned().unwrap_or_default());
                cursor = list["next_cursor"].as_str().map(String::from);
                if cursor.is_none() || seen + children.len() >= MAX_BLOCKS {
                    break;
                }
            }
            for b in &children {
                seen += 1;
                if let Some(l) = block_line(b) {
                    lines.push(format!("{}{l}", "  ".repeat(depth)));
                }
                // Sub-pages are read on their own; other nesting is inline.
                let kind = b["type"].as_str().unwrap_or("");
                if b["has_children"] == json!(true)
                    && kind != "child_page"
                    && kind != "child_database"
                    && depth < 3
                {
                    stack.push((b["id"].as_str().unwrap_or("").to_string(), depth + 1));
                }
            }
            if seen >= MAX_BLOCKS {
                lines.push("[the rest of the page was cut]".into());
                break;
            }
        }
        Ok(cut(lines.join("\n")))
    }

    async fn query(&self, api: &Api, args: &Value) -> ApiResult<String> {
        let id = arg(args, "id")?;
        // A database id works too: use its first data source.
        let source = match api.get(&format!("{API}/databases/{id}"), &[]).await {
            Ok(db) => db["data_sources"][0]["id"]
                .as_str()
                .unwrap_or(id)
                .to_string(),
            Err(_) => id.to_string(),
        };
        let rows = api
            .post(
                &format!("{API}/data_sources/{source}/query"),
                &json!({ "page_size": limit(args, 25, 100) }),
            )
            .await?;
        let lines: Vec<String> = rows["results"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|r| {
                let props: Vec<String> = r["properties"]
                    .as_object()
                    .into_iter()
                    .flatten()
                    .map(|(k, p)| (k, property_text(p)))
                    .filter(|(_, v)| !v.is_empty())
                    .map(|(k, v)| format!("{k}: {v}"))
                    .collect();
                format!(
                    "id {} | {}",
                    r["id"].as_str().unwrap_or(""),
                    props.join(" | ")
                )
            })
            .collect();
        Ok(if lines.is_empty() {
            "The database is empty.".into()
        } else {
            cut(lines.join("\n"))
        })
    }
}

impl Connector for Notion {
    fn id(&self) -> &'static str {
        "notion"
    }
    fn name(&self) -> &'static str {
        "Notion"
    }
    fn about(&self) -> &'static str {
        "search and read the user's Notion pages and databases, create pages and add to them"
    }
    fn oauth(&self) -> Option<OAuthSpec> {
        Some(OAuthSpec {
            provider: "notion",
            authorize_url: "https://api.notion.com/v1/oauth/authorize",
            token_url: "https://api.notion.com/v1/oauth/token",
            // Notion has no scopes; the user picks pages when signing in.
            read_scopes: &[],
            write_scopes: &[],
            needs_secret: true,
            client_auth: ClientAuth::BasicJson,
            scope_param: "scope",
            extra: &[("owner", "user")],
            // Notion matches the redirect exactly, port included.
            host: "localhost",
            port: 53171,
        })
    }
    fn token(&self) -> Option<TokenHelp> {
        Some(TokenHelp {
            label: "Internal integration secret".into(),
            help: "Make an internal integration, copy its secret, then share the pages Helpy may use with it (••• → Connections).".into(),
            url: "https://www.notion.so/profile/integrations".into(),
        })
    }
    fn headers(&self) -> Vec<(&'static str, String)> {
        vec![("Notion-Version", VERSION.into())]
    }
    fn actions(&self) -> Vec<Action> {
        vec![
            Action::read(
                "search",
                "Find pages and databases by title. Returns ids, titles and links.",
                json!({"query": {"type": "string"}, "limit": {"type": "integer"}}),
                &["query"],
            ),
            Action::read(
                "read",
                "Read a page's text by id.",
                json!({"id": {"type": "string"}}),
                &["id"],
            ),
            Action::read(
                "query_database",
                "List a database's rows with their properties, by database id.",
                json!({"id": {"type": "string"}, "limit": {"type": "integer"}}),
                &["id"],
            ),
            Action::write(
                "create_page",
                concat!("Create a page inside another page. The text is ", markdown!()),
                json!({"parent_id": {"type": "string", "description": "Id of the page to put it in"}, "title": {"type": "string"}, "text": {"type": "string"}}),
                &["parent_id", "title"],
                Rule::Ask,
                &["title", "text"],
            ),
            Action::write(
                "append",
                concat!("Add text to the end of a page, keeping what's there. The text is ", markdown!()),
                json!({"id": {"type": "string"}, "text": {"type": "string"}}),
                &["id", "text"],
                Rule::Ask,
                &["text"],
            ),
            Action::write(
                "replace",
                concat!(
                    "Replace a page's content with new text, to rewrite, reformat or restructure it. Its title, \
                     sub-pages and databases stay. Read the page first. The text is ",
                    markdown!()
                ),
                json!({"id": {"type": "string"}, "text": {"type": "string"}}),
                &["id", "text"],
                Rule::Ask,
                &["text"],
            ),
        ]
    }
    fn describe(&self, action: &str, args: &Value) -> (String, String) {
        let text = args["text"].as_str().unwrap_or("").to_string();
        match action {
            "create_page" => (
                format!(
                    "Create the Notion page \"{}\"",
                    args["title"].as_str().unwrap_or("")
                ),
                text,
            ),
            "replace" => ("Replace a Notion page's content".into(), text),
            _ => ("Add text to a Notion page".into(), text),
        }
    }
    fn account<'a>(&'a self, api: &'a Api, tokens: &'a Tokens) -> BoxFuture<'a, ApiResult<String>> {
        Box::pin(async move {
            if let Some(w) = tokens.raw["workspace_name"].as_str() {
                return Ok(w.to_string());
            }
            let me = api.get(&format!("{API}/users/me"), &[]).await?;
            Ok(me["bot"]["workspace_name"]
                .as_str()
                .or(me["name"].as_str())
                .unwrap_or("Notion")
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
                "query_database" => self.query(api, args).await,
                "create_page" => async {
                    let mut content = blocks(args["text"].as_str().unwrap_or(""));
                    let rest = content.split_off(content.len().min(BATCH));
                    let page = api
                        .post(
                            &format!("{API}/pages"),
                            &json!({
                                "parent": { "page_id": arg(args, "parent_id")? },
                                "properties": { "title": { "title": [{ "text": { "content": arg(args, "title")? } }] } },
                                "children": content,
                            }),
                        )
                        .await?;
                    if let Some(id) = page["id"].as_str() {
                        append_blocks(api, id, rest).await?;
                    }
                    Ok(format!(
                        "Created the page (id {}): {}",
                        page["id"].as_str().unwrap_or(""),
                        page["url"].as_str().unwrap_or("")
                    ))
                }
                .await,
                "append" => async {
                    let list = blocks(arg(args, "text")?);
                    let n = list.len();
                    append_blocks(api, arg(args, "id")?, list).await?;
                    Ok(format!("Added {n} blocks to the end of the page."))
                }
                .await,
                "replace" => async {
                    let id = arg(args, "id")?;
                    let list = blocks(arg(args, "text")?);
                    let n = list.len();
                    let removed = clear(api, id).await?;
                    append_blocks(api, id, list).await?;
                    Ok(format!("Replaced the page's content: removed {removed} blocks, wrote {n}."))
                }
                .await,
                other => Err(ToolOutcome::Permanent(format!("Notion has no action {other}."))),
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_blocks_titles_and_properties() {
        let rt = |t: &str| json!([{"plain_text": t}]);
        assert_eq!(
            block_line(&json!({"type": "heading_2", "heading_2": {"rich_text": rt("Plan")}}))
                .unwrap(),
            "## Plan"
        );
        assert_eq!(
            block_line(
                &json!({"type": "to_do", "to_do": {"rich_text": rt("Ship"), "checked": true}})
            )
            .unwrap(),
            "[x] Ship"
        );
        assert!(
            block_line(&json!({"type": "paragraph", "paragraph": {"rich_text": []}})).is_none()
        );
        let page = json!({"properties": {"Name": {"type": "title", "title": rt("Roadmap")}}});
        assert_eq!(title(&page), "Roadmap");
        assert_eq!(title(&json!({"title": rt("Tasks")})), "Tasks");
        assert_eq!(
            property_text(
                &json!({"type": "multi_select", "multi_select": [{"name": "a"}, {"name": "b"}]})
            ),
            "a, b"
        );
        assert_eq!(
            property_text(&json!({"type": "date", "date": {"start": "2026-05-01", "end": null}})),
            "2026-05-01"
        );
    }

}
