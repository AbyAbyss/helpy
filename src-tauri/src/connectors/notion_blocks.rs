//! Markdown to Notion blocks, so what an agent writes shows up as Notion's
//! own headings, lists, quotes, code, tables and styled text rather than as
//! Markdown characters in plain paragraphs.

use serde_json::{json, Value};

/// Notion's limit on one text run.
const MAX_RUN: usize = 2000;
/// Nesting Notion accepts in one request.
const MAX_DEPTH: usize = 2;

/// Styles on a run of text.
#[derive(Clone, Copy, Default, PartialEq)]
struct Style {
    bold: bool,
    italic: bool,
    strike: bool,
    code: bool,
}

/// Inline Markdown (**bold**, *italic*, _italic_, ~~strike~~, `code` and
/// [links](url)) as Notion rich text.
pub fn rich_text(s: &str) -> Vec<Value> {
    let chars: Vec<char> = s.chars().collect();
    let mut out = Vec::new();
    let mut buf = String::new();
    let mut style = Style::default();
    let at = |i: usize, pat: &str| pat.chars().enumerate().all(|(k, c)| chars.get(i + k) == Some(&c));
    let find = |from: usize, pat: &str| (from..chars.len()).find(|&j| at(j, pat));
    let word = |i: Option<usize>| i.and_then(|i| chars.get(i)).is_some_and(|c| c.is_alphanumeric());
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '`' {
            if let Some(end) = find(i + 1, "`") {
                push(&mut out, &mut buf, style, None);
                let code: String = chars[i + 1..end].iter().collect();
                push(&mut out, &mut code.clone(), Style { code: true, ..style }, None);
                i = end + 1;
                continue;
            }
        }
        if c == '[' {
            if let Some(mid) = find(i + 1, "](") {
                if let Some(end) = find(mid + 2, ")") {
                    let text: String = chars[i + 1..mid].iter().collect();
                    let url: String = chars[mid + 2..end].iter().collect();
                    if !text.is_empty() && url.contains(':') && !url.contains(char::is_whitespace) {
                        push(&mut out, &mut buf, style, None);
                        for mut run in split_rich(&text) {
                            push(&mut out, &mut run.0, merge(style, run.1), Some(&url));
                        }
                        i = end + 1;
                        continue;
                    }
                }
            }
        }
        let toggle = if at(i, "**") || at(i, "__") {
            Some((2, 'b'))
        } else if at(i, "~~") {
            Some((2, 's'))
        } else if c == '*' || (c == '_' && !(word(i.checked_sub(1)) && word(Some(i + 1)))) {
            Some((1, 'i'))
        } else {
            None
        };
        if let Some((len, kind)) = toggle {
            // Only a marker that closes later (or is closing now) is styling;
            // a lone "*" or "2 * 3" stays text.
            let pat: String = chars[i..i + len].iter().collect();
            let on = match kind {
                'b' => style.bold,
                's' => style.strike,
                _ => style.italic,
            };
            if on || find(i + len, &pat).is_some() {
                push(&mut out, &mut buf, style, None);
                match kind {
                    'b' => style.bold = !style.bold,
                    's' => style.strike = !style.strike,
                    _ => style.italic = !style.italic,
                }
                i += len;
                continue;
            }
        }
        buf.push(c);
        i += 1;
    }
    push(&mut out, &mut buf, style, None);
    out
}

/// A link's text may carry its own styling.
fn split_rich(text: &str) -> Vec<(String, Style)> {
    rich_text(text)
        .into_iter()
        .map(|r| {
            let a = &r["annotations"];
            let style = Style {
                bold: a["bold"] == json!(true),
                italic: a["italic"] == json!(true),
                strike: a["strikethrough"] == json!(true),
                code: a["code"] == json!(true),
            };
            (r["text"]["content"].as_str().unwrap_or("").to_string(), style)
        })
        .collect()
}

fn merge(a: Style, b: Style) -> Style {
    Style {
        bold: a.bold || b.bold,
        italic: a.italic || b.italic,
        strike: a.strike || b.strike,
        code: a.code || b.code,
    }
}

/// Adds `buf` as runs of at most MAX_RUN characters, and empties it.
fn push(out: &mut Vec<Value>, buf: &mut String, style: Style, url: Option<&str>) {
    let chars: Vec<char> = std::mem::take(buf).chars().collect();
    for piece in chars.chunks(MAX_RUN) {
        let mut text = json!({ "content": piece.iter().collect::<String>() });
        if let Some(u) = url {
            text["link"] = json!({ "url": u });
        }
        let mut run = json!({ "type": "text", "text": text });
        if style != Style::default() {
            run["annotations"] = json!({
                "bold": style.bold,
                "italic": style.italic,
                "strikethrough": style.strike,
                "code": style.code,
            });
        }
        out.push(run);
    }
}

fn block(kind: &str, body: Value) -> Value {
    json!({ "object": "block", "type": kind, kind: body })
}

fn text_block(kind: &str, text: &str) -> Value {
    block(kind, json!({ "rich_text": rich_text(text) }))
}

/// Notion's name for a code fence's language. Notion accepts only its own
/// list, so anything else is plain text.
fn language(tag: &str) -> &'static str {
    match tag.trim().to_lowercase().as_str() {
        "rust" | "rs" => "rust",
        "js" | "javascript" | "jsx" => "javascript",
        "ts" | "typescript" | "tsx" => "typescript",
        "py" | "python" => "python",
        "sh" | "bash" | "zsh" | "shell" | "console" => "shell",
        "json" => "json",
        "html" => "html",
        "css" => "css",
        "sql" => "sql",
        "yaml" | "yml" => "yaml",
        "go" => "go",
        "java" => "java",
        "kotlin" | "kt" => "kotlin",
        "swift" => "swift",
        "c" => "c",
        "cpp" | "c++" => "c++",
        "cs" | "csharp" | "c#" => "c#",
        "ruby" | "rb" => "ruby",
        "php" => "php",
        "md" | "markdown" => "markdown",
        "xml" => "xml",
        "diff" => "diff",
        _ => "plain text",
    }
}

/// A line's list marker, if it has one: (kind, text after the marker).
fn list_item(line: &str) -> Option<(&'static str, &str, Option<bool>)> {
    for bullet in ["- ", "* ", "+ "] {
        if let Some(rest) = line.strip_prefix(bullet) {
            for (mark, done) in [("[ ] ", false), ("[x] ", true), ("[X] ", true)] {
                if let Some(task) = rest.strip_prefix(mark) {
                    return Some(("to_do", task, Some(done)));
                }
            }
            return Some(("bulleted_list_item", rest, None));
        }
    }
    let digits = line.chars().take_while(char::is_ascii_digit).count();
    if digits > 0 {
        let rest = &line[digits..];
        if let Some(text) = rest.strip_prefix(". ").or_else(|| rest.strip_prefix(") ")) {
            return Some(("numbered_list_item", text, None));
        }
    }
    None
}

fn is_table_line(line: &str) -> bool {
    let t = line.trim();
    t.starts_with('|') && t.ends_with('|') && t.len() > 1
}

fn cells(line: &str) -> Vec<String> {
    let t = line.trim();
    t[1..t.len() - 1].split('|').map(|c| c.trim().to_string()).collect()
}

fn is_separator(line: &str) -> bool {
    cells(line)
        .iter()
        .all(|c| !c.is_empty() && c.chars().all(|ch| matches!(ch, '-' | ':' | ' ')))
}

/// Markdown as Notion blocks. List items indented under another item become
/// its children, two levels deep; deeper ones stay at the second level.
pub fn blocks(markdown: &str) -> Vec<Value> {
    let lines: Vec<&str> = markdown.lines().collect();
    let mut out: Vec<Value> = Vec::new();
    // Index path of the list items that can take children, by depth.
    let mut parents: Vec<Vec<usize>> = Vec::new();
    let mut para: Vec<&str> = Vec::new();
    let flush = |out: &mut Vec<Value>, para: &mut Vec<&str>| {
        if !para.is_empty() {
            out.push(text_block("paragraph", &para.join("\n")));
            para.clear();
        }
    };
    let mut i = 0;
    while i < lines.len() {
        let raw = lines[i];
        let line = raw.trim_end();
        let trimmed = line.trim_start();
        let indent = line.len() - trimmed.len();

        if let Some(tag) = trimmed.strip_prefix("```") {
            flush(&mut out, &mut para);
            let mut code = Vec::new();
            i += 1;
            while i < lines.len() && !lines[i].trim_start().starts_with("```") {
                code.push(lines[i]);
                i += 1;
            }
            let body = code.join("\n");
            out.push(block(
                "code",
                json!({ "rich_text": plain_runs(&body), "language": language(tag) }),
            ));
            parents.clear();
            i += 1;
            continue;
        }
        if is_table_line(trimmed) && lines.get(i + 1).is_some_and(|l| is_table_line(l) && is_separator(l)) {
            flush(&mut out, &mut para);
            let header = cells(trimmed);
            let width = header.len();
            let row = |c: Vec<String>| {
                let mut c = c;
                c.resize(width, String::new());
                block("table_row", json!({ "cells": c.iter().map(|t| rich_text(t)).collect::<Vec<_>>() }))
            };
            let mut rows = vec![row(header)];
            i += 2;
            while i < lines.len() && is_table_line(lines[i]) {
                rows.push(row(cells(lines[i])));
                i += 1;
            }
            out.push(block(
                "table",
                json!({ "table_width": width, "has_column_header": true, "has_row_header": false, "children": rows }),
            ));
            parents.clear();
            continue;
        }
        if trimmed.is_empty() {
            flush(&mut out, &mut para);
            i += 1;
            continue;
        }
        if matches!(trimmed, "---" | "***" | "___") {
            flush(&mut out, &mut para);
            out.push(block("divider", json!({})));
            parents.clear();
            i += 1;
            continue;
        }
        let heading = trimmed.chars().take_while(|c| *c == '#').count();
        if (1..=6).contains(&heading) && trimmed[heading..].starts_with(' ') {
            flush(&mut out, &mut para);
            let kind = ["heading_1", "heading_2", "heading_3"][heading.min(3) - 1];
            out.push(text_block(kind, trimmed[heading..].trim()));
            parents.clear();
            i += 1;
            continue;
        }
        if let Some(quote) = trimmed.strip_prefix('>') {
            flush(&mut out, &mut para);
            out.push(text_block("quote", quote.trim_start()));
            parents.clear();
            i += 1;
            continue;
        }
        if let Some((kind, text, checked)) = list_item(trimmed) {
            flush(&mut out, &mut para);
            let mut body = json!({ "rich_text": rich_text(text) });
            if let Some(done) = checked {
                body["checked"] = json!(done);
            }
            let item = block(kind, body);
            // Two spaces or a tab per level, capped at what Notion takes.
            let depth = (raw[..raw.len() - raw.trim_start().len()].replace('\t', "  ").len() / 2)
                .min(parents.len())
                .min(MAX_DEPTH);
            parents.truncate(depth);
            let path = match parents.last() {
                Some(parent) => {
                    let at = children_of(&mut out, parent);
                    at.push(item);
                    let mut p = parent.clone();
                    p.push(at.len() - 1);
                    p
                }
                None => {
                    out.push(item);
                    vec![out.len() - 1]
                }
            };
            parents.push(path);
            i += 1;
            continue;
        }
        // Plain text: consecutive lines make one paragraph. A line indented
        // under a list item continues it.
        if indent > 0 && para.is_empty() && !parents.is_empty() {
            let path = parents.last().unwrap().clone();
            let item = at_path(&mut out, &path);
            let kind = item["type"].as_str().unwrap_or("").to_string();
            let runs = item[&kind]["rich_text"].as_array_mut().unwrap();
            runs.extend(rich_text(&format!("\n{trimmed}")));
            i += 1;
            continue;
        }
        parents.clear();
        para.push(trimmed);
        i += 1;
    }
    flush(&mut out, &mut para);
    out
}

/// Code keeps its characters as they are.
fn plain_runs(text: &str) -> Vec<Value> {
    let mut out = Vec::new();
    push(&mut out, &mut text.to_string(), Style::default(), None);
    out
}

fn at_path<'a>(out: &'a mut [Value], path: &[usize]) -> &'a mut Value {
    let mut v = &mut out[path[0]];
    for &i in &path[1..] {
        let kind = v["type"].as_str().unwrap_or("").to_string();
        v = &mut v[&kind]["children"][i];
    }
    v
}

fn children_of<'a>(out: &'a mut [Value], path: &[usize]) -> &'a mut Vec<Value> {
    let v = at_path(out, path);
    let kind = v["type"].as_str().unwrap_or("").to_string();
    let body = &mut v[&kind];
    if !body["children"].is_array() {
        body["children"] = json!([]);
    }
    body["children"].as_array_mut().unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(b: &[Value]) -> Vec<&str> {
        b.iter().map(|b| b["type"].as_str().unwrap()).collect()
    }

    fn text(b: &Value) -> String {
        let kind = b["type"].as_str().unwrap();
        b[kind]["rich_text"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["text"]["content"].as_str().unwrap())
            .collect()
    }

    #[test]
    fn turns_markdown_into_notion_blocks() {
        let md = "# HELPY\n\nHelpy lives next to\nyour cursor.\n\n## Stack\n\n- Tauri v2\n- React\n  - TypeScript\n\n1. Install\n2. Run\n\n- [ ] Ship\n- [x] Test\n\n> Note\n\n---\n\n```rust\nfn main() {}\n```";
        let b = blocks(md);
        assert_eq!(
            kinds(&b),
            [
                "heading_1", "paragraph", "heading_2", "bulleted_list_item", "bulleted_list_item",
                "numbered_list_item", "numbered_list_item", "to_do", "to_do", "quote", "divider", "code"
            ]
        );
        assert_eq!(text(&b[0]), "HELPY");
        assert_eq!(text(&b[1]), "Helpy lives next to\nyour cursor.");
        let nested = &b[4]["bulleted_list_item"]["children"][0];
        assert_eq!(text(nested), "TypeScript");
        assert_eq!(b[8]["to_do"]["checked"], json!(true));
        assert_eq!(b[11]["code"]["language"], "rust");
        assert_eq!(b[11]["code"]["rich_text"][0]["text"]["content"], "fn main() {}");
    }

    #[test]
    fn styles_inline_text() {
        let r = rich_text("Use **Tauri** and *React*, run `npm i`, see [docs](https://tauri.app) or ~~old~~.");
        let run = |t: &str| r.iter().find(|x| x["text"]["content"] == t).unwrap().clone();
        assert_eq!(run("Tauri")["annotations"]["bold"], true);
        assert_eq!(run("React")["annotations"]["italic"], true);
        assert_eq!(run("npm i")["annotations"]["code"], true);
        assert_eq!(run("docs")["text"]["link"]["url"], "https://tauri.app");
        assert_eq!(run("old")["annotations"]["strikethrough"], true);
        // Plain text around them carries no styling.
        assert!(run("Use ").get("annotations").is_none());
    }

    #[test]
    fn leaves_lone_markers_and_snake_case_alone() {
        let r = rich_text("2 * 3 = 6 in my_var_name");
        assert_eq!(r.len(), 1);
        assert_eq!(r[0]["text"]["content"], "2 * 3 = 6 in my_var_name");
    }

    #[test]
    fn builds_tables_and_splits_long_runs() {
        let b = blocks("| Name | Size |\n|---|---:|\n| Tauri | small |\n| Electron |");
        assert_eq!(kinds(&b), ["table"]);
        assert_eq!(b[0]["table"]["table_width"], 2);
        let rows = b[0]["table"]["children"].as_array().unwrap();
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[2]["table_row"]["cells"][1], json!([]));
        let long = "x".repeat(4500);
        assert_eq!(blocks(&long)[0]["paragraph"]["rich_text"].as_array().unwrap().len(), 3);
    }
}
