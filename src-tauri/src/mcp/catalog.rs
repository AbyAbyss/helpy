//! Well-known MCP servers, ready to add with the right transport, sign-in
//! and keys filled in. Adding one is a list entry here.

use serde::Serialize;
use ts_rs::TS;

use crate::settings::schema::{KeyValue, McpServer, McpTransport, Permission};

#[derive(Serialize, TS, Clone, Debug)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Preset {
    pub id: String,
    pub name: String,
    pub about: String,
    /// What the user still has to do, if anything.
    pub setup: String,
    pub docs: String,
    /// The server as it will be added; secret values are left empty.
    pub server: McpServer,
}

fn remote(url: &str, oauth: bool, headers: Vec<KeyValue>) -> McpServer {
    McpServer {
        transport: McpTransport::Http,
        url: url.into(),
        oauth,
        headers,
        ..Default::default()
    }
}

fn local(command: &str, args: &[&str], env: Vec<KeyValue>) -> McpServer {
    McpServer {
        transport: McpTransport::Stdio,
        command: command.into(),
        args: args.iter().map(|a| a.to_string()).collect(),
        env,
        ..Default::default()
    }
}

fn secret(key: &str) -> KeyValue {
    KeyValue {
        key: key.into(),
        value: String::new(),
        secret: true,
    }
}

fn plain(key: &str, value: &str) -> KeyValue {
    KeyValue {
        key: key.into(),
        value: value.into(),
        secret: false,
    }
}

fn preset(
    id: &str,
    name: &str,
    about: &str,
    setup: &str,
    docs: &str,
    mut server: McpServer,
) -> Preset {
    server.id = id.into();
    server.name = name.into();
    server.enabled = true;
    server.preset = id.into();
    server.permission = Permission::ReadOnly;
    Preset {
        id: id.into(),
        name: name.into(),
        about: about.into(),
        setup: setup.into(),
        docs: docs.into(),
        server,
    }
}

pub fn presets() -> Vec<Preset> {
    vec![
        preset(
            "atlassian",
            "Atlassian (Jira, Confluence)",
            "Jira issues and Confluence pages.",
            "Sign in with your Atlassian account.",
            "https://github.com/atlassian/atlassian-mcp-server",
            remote("https://mcp.atlassian.com/v2/mcp", true, vec![]),
        ),
        preset(
            "linear",
            "Linear",
            "Linear issues, projects and comments.",
            "Sign in with your Linear account.",
            "https://linear.app/docs/mcp",
            remote("https://mcp.linear.app/mcp", true, vec![]),
        ),
        preset(
            "notion-mcp",
            "Notion (hosted MCP)",
            "Notion's own MCP server, as an alternative to the built-in Notion connector.",
            "Sign in with your Notion account.",
            "https://developers.notion.com/guides/mcp/build-mcp-client",
            remote("https://mcp.notion.com/mcp", true, vec![]),
        ),
        preset(
            "github-mcp",
            "GitHub (hosted MCP)",
            "GitHub's own MCP server: repositories, issues, pull requests, Actions.",
            "Make a personal access token and paste it as the Authorization value in the form \"Bearer <token>\".",
            "https://github.com/github/github-mcp-server",
            remote("https://api.githubcopilot.com/mcp/", false, vec![secret("Authorization")]),
        ),
        preset(
            "sentry",
            "Sentry",
            "Sentry issues, errors and releases.",
            "Sign in with your Sentry account.",
            "https://docs.sentry.io/product/sentry-mcp/",
            remote("https://mcp.sentry.dev/mcp", true, vec![]),
        ),
        preset(
            "stripe",
            "Stripe",
            "Stripe customers, payments and the Stripe docs.",
            "Sign in with your Stripe account, or turn sign-in off and add an Authorization header \"Bearer <restricted key>\".",
            "https://docs.stripe.com/mcp",
            remote("https://mcp.stripe.com", true, vec![]),
        ),
        preset(
            "context7",
            "Context7",
            "Up-to-date documentation for libraries and frameworks.",
            "Works without a key; add an Authorization header \"Bearer <key>\" for higher limits.",
            "https://github.com/upstash/context7",
            remote("https://mcp.context7.com/mcp", false, vec![]),
        ),
        preset(
            "aws",
            "AWS",
            "Runs AWS CLI commands in your account.",
            "Needs uv installed. Paste an access key and secret, or remove them and set AWS_API_MCP_PROFILE_NAME to use a profile from ~/.aws.",
            "https://github.com/awslabs/mcp/tree/main/src/aws-api-mcp-server",
            local(
                "uvx",
                &["awslabs.aws-api-mcp-server@latest"],
                vec![plain("AWS_REGION", "us-east-1"), secret("AWS_ACCESS_KEY_ID"), secret("AWS_SECRET_ACCESS_KEY")],
            ),
        ),
        preset(
            "filesystem",
            "Files (MCP)",
            "Reads and writes files in the folders you list.",
            "Needs Node.js. Each argument after the package is a folder it may use.",
            "https://github.com/modelcontextprotocol/servers/tree/main/src/filesystem",
            local("npx", &["-y", "@modelcontextprotocol/server-filesystem", "~/Documents"], vec![]),
        ),
        preset(
            "playwright",
            "Playwright browser",
            "Drives a web browser: opens pages, clicks, fills forms.",
            "Needs Node.js.",
            "https://github.com/microsoft/playwright-mcp",
            local("npx", &["@playwright/mcp@latest"], vec![]),
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presets_are_valid_servers() {
        let all = presets();
        let mut s = crate::settings::Settings::default();
        s.connectors.mcp = all.iter().map(|p| p.server.clone()).collect();
        let errors = crate::settings::validate(&s);
        assert!(errors.is_empty(), "{errors:?}");
        for p in &all {
            assert_eq!(p.server.id, p.id);
            assert!(p.docs.starts_with("https://"));
        }
    }
}
