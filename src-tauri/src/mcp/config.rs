//! MCP server configs as JSON, in the `mcpServers` shape most MCP clients
//! use (Claude Desktop, Cursor), and VS Code's `servers` shape. Secret
//! values are split off for the keychain on import and hidden on export.

use std::collections::BTreeMap;

use serde_json::{json, Map, Value};

use crate::settings::schema::{KeyValue, McpServer, McpTransport, Permission};

/// A secret from an imported config, bound for the keychain.
#[derive(Debug, PartialEq)]
pub struct Secret {
    pub server: String,
    pub kind: &'static str,
    pub key: String,
    pub value: String,
}

#[derive(Debug, Default, PartialEq)]
pub struct Imported {
    pub servers: Vec<McpServer>,
    pub secrets: Vec<Secret>,
    /// Entries that couldn't be used, and why.
    pub skipped: Vec<String>,
}

/// Names that almost always hold something private.
pub fn looks_secret(key: &str) -> bool {
    let k = key.to_ascii_lowercase();
    ["key", "secret", "token", "password", "passwd", "authorization", "credential", "cookie"]
        .iter()
        .any(|w| k.contains(w))
        // "PAT" only as a whole word, so PATH isn't a secret.
        || k.split(|c: char| !c.is_ascii_alphanumeric()).any(|w| w == "pat")
}

/// A readable, unique id from a name: "Jira Cloud" → "jira-cloud".
pub fn slug(name: &str, taken: &[String]) -> String {
    let base: String = name
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    let base = if base.is_empty() {
        "server".to_string()
    } else {
        base.chars().take(24).collect()
    };
    let mut id = base.clone();
    let mut n = 2;
    while taken.contains(&id) {
        id = format!("{base}-{n}");
        n += 1;
    }
    id
}

fn pairs(v: &Value, server: &str, kind: &'static str, secrets: &mut Vec<Secret>) -> Vec<KeyValue> {
    let Some(map) = v.as_object() else {
        return Vec::new();
    };
    map.iter()
        .map(|(k, v)| {
            let value = match v {
                Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            let secret = looks_secret(k);
            if secret && !value.is_empty() {
                secrets.push(Secret {
                    server: server.to_string(),
                    kind,
                    key: k.clone(),
                    value: value.clone(),
                });
            }
            KeyValue {
                key: k.clone(),
                value: if secret { String::new() } else { value },
                secret,
            }
        })
        .collect()
}

/// Reads a pasted config. `taken` are ids already in use.
pub fn import(text: &str, taken: &[String]) -> Result<Imported, String> {
    let v: Value =
        serde_json::from_str(text.trim()).map_err(|e| format!("That isn't valid JSON: {e}"))?;
    let map: &Map<String, Value> = v["mcpServers"]
        .as_object()
        .or_else(|| v["servers"].as_object())
        .or_else(|| v.as_object().filter(|m| m.values().all(|x| x.is_object())))
        .ok_or("Expected an \"mcpServers\" object")?;
    let mut out = Imported::default();
    let mut ids: Vec<String> = taken.to_vec();
    for (name, cfg) in map {
        let kind = cfg["type"]
            .as_str()
            .or(cfg["transport"].as_str())
            .unwrap_or("");
        let id = slug(name, &ids);
        let mut server = McpServer {
            id: id.clone(),
            name: name.clone(),
            enabled: cfg["disabled"] != Value::Bool(true),
            permission: Permission::ReadOnly,
            ..Default::default()
        };
        if kind.eq_ignore_ascii_case("sse") {
            out.skipped.push(format!(
                "{name}: it uses the older SSE transport, which Helpy doesn't support. Many servers also offer a Streamable HTTP address, often ending in /mcp."
            ));
            continue;
        }
        if let Some(cmd) = cfg["command"].as_str() {
            server.transport = McpTransport::Stdio;
            server.command = cmd.to_string();
            server.args = cfg["args"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .map(|x| {
                            x.as_str()
                                .map(String::from)
                                .unwrap_or_else(|| x.to_string())
                        })
                        .collect()
                })
                .unwrap_or_default();
            server.cwd = cfg["cwd"].as_str().unwrap_or("").to_string();
            server.env = pairs(&cfg["env"], &id, "env", &mut out.secrets);
        } else if let Some(url) = cfg["url"].as_str().or(cfg["serverUrl"].as_str()) {
            server.transport = McpTransport::Http;
            server.url = url.to_string();
            server.headers = pairs(&cfg["headers"], &id, "header", &mut out.secrets);
            // No key given: the server most likely signs in with OAuth.
            server.oauth = server.headers.is_empty() || cfg["oauth"] == Value::Bool(true);
        } else {
            out.skipped
                .push(format!("{name}: it has neither a command nor a url."));
            continue;
        }
        ids.push(id);
        out.servers.push(server);
    }
    Ok(out)
}

/// The servers as `mcpServers` JSON, secrets replaced by a placeholder.
pub fn export(servers: &[McpServer]) -> String {
    let hide = |kv: &[KeyValue]| -> BTreeMap<String, String> {
        kv.iter()
            .map(|p| {
                (
                    p.key.clone(),
                    if p.secret {
                        "<secret>".into()
                    } else {
                        p.value.clone()
                    },
                )
            })
            .collect()
    };
    let mut map = Map::new();
    for s in servers {
        let v = match s.transport {
            McpTransport::Stdio => {
                let mut o = json!({ "command": s.command, "args": s.args });
                if !s.env.is_empty() {
                    o["env"] = json!(hide(&s.env));
                }
                if !s.cwd.is_empty() {
                    o["cwd"] = json!(s.cwd);
                }
                o
            }
            McpTransport::Http => {
                let mut o = json!({ "type": "http", "url": s.url });
                if !s.headers.is_empty() {
                    o["headers"] = json!(hide(&s.headers));
                }
                o
            }
        };
        map.insert(s.name.clone(), v);
    }
    serde_json::to_string_pretty(&json!({ "mcpServers": map })).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn imports_claude_desktop_style_configs_and_keeps_secrets_out() {
        let text = r#"{"mcpServers": {
            "AWS": {"command": "uvx", "args": ["some-aws-server@latest"], "env": {"AWS_REGION": "eu-west-2", "AWS_SECRET_ACCESS_KEY": "shh", "AWS_ACCESS_KEY_ID": "AKIA"}},
            "Jira": {"url": "https://example.com/v1/mcp"},
            "Internal": {"type": "http", "url": "https://tools.example.com/mcp", "headers": {"Authorization": "Bearer abc", "X-Team": "ops"}},
            "Old": {"type": "sse", "url": "https://example.com/sse"},
            "Broken": {"args": []}
        }}"#;
        let r = import(text, &["aws".into()]).unwrap();
        let ids: Vec<_> = r.servers.iter().map(|s| s.id.as_str()).collect();
        // JSON objects come back sorted by name.
        assert_eq!(ids, ["aws-2", "internal", "jira"]);
        let by = |id: &str| r.servers.iter().find(|s| s.id == id).unwrap();
        let aws = by("aws-2");
        assert_eq!(aws.transport, McpTransport::Stdio);
        assert_eq!(
            aws.env
                .iter()
                .find(|e| e.key == "AWS_REGION")
                .unwrap()
                .value,
            "eu-west-2"
        );
        let secret = aws
            .env
            .iter()
            .find(|e| e.key == "AWS_SECRET_ACCESS_KEY")
            .unwrap();
        assert!(secret.secret && secret.value.is_empty());
        assert!(by("jira").oauth, "a bare URL signs in with OAuth");
        assert!(!by("internal").oauth);
        assert_eq!(
            by("internal")
                .headers
                .iter()
                .find(|h| h.key == "X-Team")
                .unwrap()
                .value,
            "ops"
        );
        let keys: Vec<_> = r
            .secrets
            .iter()
            .map(|s| (s.server.as_str(), s.key.as_str(), s.value.as_str()))
            .collect();
        assert!(keys.contains(&("aws-2", "AWS_SECRET_ACCESS_KEY", "shh")));
        assert!(keys.contains(&("internal", "Authorization", "Bearer abc")));
        assert_eq!(r.skipped.len(), 2);
        assert!(r
            .skipped
            .iter()
            .any(|m| m.contains("Old") && m.contains("SSE")));
    }

    #[test]
    fn exports_without_secrets_and_round_trips() {
        let r = import(r#"{"servers": {"fs": {"command": "npx", "args": ["-y", "server", "/tmp"], "env": {"API_TOKEN": "x"}}}}"#, &[]).unwrap();
        let out = export(&r.servers);
        assert!(out.contains("<secret>") && !out.contains("\"x\""));
        let again = import(&out, &[]).unwrap();
        assert_eq!(again.servers[0].args, ["-y", "server", "/tmp"]);
    }

    #[test]
    fn slugs() {
        assert_eq!(slug("Jira Cloud!", &[]), "jira-cloud");
        assert_eq!(slug("jira cloud", &["jira-cloud".into()]), "jira-cloud-2");
        assert_eq!(slug("***", &[]), "server");
        assert!(looks_secret("GITHUB_PERSONAL_ACCESS_TOKEN") && looks_secret("GITLAB_PAT"));
        assert!(!looks_secret("AWS_REGION") && !looks_secret("PATH"));
    }
}
