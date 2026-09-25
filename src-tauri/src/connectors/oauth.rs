//! OAuth 2 for connectors and MCP servers: the authorization-code flow with
//! PKCE in the system browser and a loopback redirect (RFC 8252), token
//! refresh, and for MCP servers the discovery and dynamic client
//! registration the MCP spec builds on (RFC 9728, RFC 8414, RFC 7591).

use std::time::Duration;

use base64::Engine;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use url::Url;

/// How long the user has to finish signing in.
const SIGN_IN_TIMEOUT: Duration = Duration::from_secs(300);

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ClientAuth {
    /// client_id (and secret) in the form body.
    Body,
    /// HTTP Basic with client_id:secret.
    Basic,
    /// HTTP Basic, with the parameters as a JSON body (Notion).
    BasicJson,
}

/// One authorization request.
#[derive(Clone, Debug)]
pub struct Request {
    pub authorize_url: String,
    pub token_url: String,
    pub client_id: String,
    pub client_secret: Option<String>,
    pub client_auth: ClientAuth,
    pub scopes: Vec<String>,
    /// "scope" for most, "user_scope" for Slack user tokens.
    pub scope_param: &'static str,
    /// Extra query parameters for the authorize URL.
    pub extra: Vec<(String, String)>,
    /// RFC 8707 resource indicator (MCP servers).
    pub resource: Option<String>,
    /// "127.0.0.1" or "localhost", whichever the provider accepts.
    pub host: &'static str,
    /// A fixed loopback port (for dynamically registered clients); 0 picks one.
    pub port: u16,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Tokens {
    pub access_token: String,
    pub refresh_token: Option<String>,
    /// Unix milliseconds.
    pub expires_at: Option<i64>,
    pub scope: Option<String>,
    /// The whole token response, for provider-specific fields (Notion's
    /// workspace name, Slack's team).
    #[serde(default)]
    pub raw: Value,
}

impl Tokens {
    /// Expired, or about to be (a minute's margin).
    pub fn stale(&self, now_ms: i64) -> bool {
        self.expires_at.is_some_and(|t| t - 60_000 <= now_ms)
    }

    pub fn from_response(v: &Value, now_ms: i64) -> Result<Self, String> {
        if let Some(err) = v["error"].as_str() {
            let why = v["error_description"].as_str().unwrap_or(err);
            return Err(format!("Sign-in was refused: {why}"));
        }
        // Slack user tokens come nested under authed_user.
        let src = if v["access_token"].is_string() {
            v
        } else {
            &v["authed_user"]
        };
        let access_token = src["access_token"]
            .as_str()
            .ok_or("The sign-in answer had no access token")?
            .to_string();
        Ok(Self {
            access_token,
            refresh_token: src["refresh_token"].as_str().map(String::from),
            expires_at: src["expires_in"].as_i64().map(|s| now_ms + s * 1000),
            scope: src["scope"].as_str().map(String::from),
            raw: v.clone(),
        })
    }
}

fn b64url(bytes: &[u8]) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

pub fn random_token(bytes: usize) -> String {
    let mut buf = vec![0u8; bytes];
    getrandom::fill(&mut buf).expect("the OS random source");
    b64url(&buf)
}

/// A PKCE verifier and its S256 challenge.
pub fn pkce() -> (String, String) {
    let verifier = random_token(48);
    let challenge = b64url(&Sha256::digest(verifier.as_bytes()));
    (verifier, challenge)
}

pub fn authorize_url(
    req: &Request,
    redirect: &str,
    state: &str,
    challenge: &str,
) -> Result<String, String> {
    let mut u =
        Url::parse(&req.authorize_url).map_err(|_| "The sign-in address is invalid".to_string())?;
    {
        let mut q = u.query_pairs_mut();
        q.append_pair("response_type", "code")
            .append_pair("client_id", &req.client_id)
            .append_pair("redirect_uri", redirect)
            .append_pair("state", state)
            .append_pair("code_challenge", challenge)
            .append_pair("code_challenge_method", "S256");
        if !req.scopes.is_empty() {
            // Slack's user_scope is comma separated; OAuth's scope uses spaces.
            let sep = if req.scope_param == "user_scope" {
                ","
            } else {
                " "
            };
            q.append_pair(req.scope_param, &req.scopes.join(sep));
        }
        if let Some(r) = &req.resource {
            q.append_pair("resource", r);
        }
        for (k, v) in &req.extra {
            q.append_pair(k, v);
        }
    }
    Ok(u.into())
}

/// What the browser brought back to the loopback address.
#[derive(Debug, PartialEq)]
pub enum Callback {
    Code {
        code: String,
        state: String,
    },
    Error(String),
    /// Something else (a favicon): answer and keep waiting.
    Other,
}

pub fn parse_callback(request_line: &str) -> Callback {
    let Some(target) = request_line.split_whitespace().nth(1) else {
        return Callback::Other;
    };
    let Ok(u) = Url::parse(&format!("http://loopback{target}")) else {
        return Callback::Other;
    };
    if u.path() != "/callback" {
        return Callback::Other;
    }
    let get = |k: &str| {
        u.query_pairs()
            .find(|(n, _)| n == k)
            .map(|(_, v)| v.to_string())
    };
    if let Some(e) = get("error") {
        return Callback::Error(get("error_description").unwrap_or(e));
    }
    match (get("code"), get("state")) {
        (Some(code), Some(state)) => Callback::Code { code, state },
        _ => Callback::Other,
    }
}

fn page(title: &str, body: &str) -> String {
    let html = format!(
        "<!doctype html><meta charset=utf-8><title>Helpy</title>\
         <body style=\"font:16px/1.5 system-ui,sans-serif;background:#12151c;color:#eef1f6;display:grid;place-items:center;height:100vh;margin:0\">\
         <div style=\"max-width:420px;text-align:center\"><div style=\"font-size:40px\">{}</div><h1 style=\"font-size:22px\">{title}</h1>\
         <p style=\"color:#aab2c0\">{body}</p></div>",
        if title.starts_with("Connected") { "✓" } else { "!" }
    );
    format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{html}",
        html.len()
    )
}

/// Waits for the browser's redirect and returns the code.
async fn wait_for_code(listener: TcpListener, state: &str) -> Result<String, String> {
    loop {
        let (mut sock, _) = listener
            .accept()
            .await
            .map_err(|e| format!("The sign-in listener failed: {e}"))?;
        let mut buf = vec![0u8; 8192];
        let n = tokio::time::timeout(Duration::from_secs(10), sock.read(&mut buf))
            .await
            .ok()
            .and_then(Result::ok)
            .unwrap_or(0);
        let head = String::from_utf8_lossy(&buf[..n]).to_string();
        let first = head.lines().next().unwrap_or("");
        let (reply, result) = match parse_callback(first) {
            Callback::Other => (
                "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                    .to_string(),
                None,
            ),
            Callback::Error(e) => (
                page(
                    "Not connected",
                    &format!("{e}. You can close this tab and try again from Helpy."),
                ),
                Some(Err(format!("Sign-in didn't finish: {e}"))),
            ),
            Callback::Code { state: s, .. } if s != state => (
                page(
                    "Not connected",
                    "This sign-in didn't come from Helpy. Close this tab and try again.",
                ),
                Some(Err("The sign-in answer didn't match; try again".into())),
            ),
            Callback::Code { code, .. } => (
                page(
                    "Connected to Helpy",
                    "You can close this tab and go back to Helpy.",
                ),
                Some(Ok(code)),
            ),
        };
        let _ = sock.write_all(reply.as_bytes()).await;
        let _ = sock.shutdown().await;
        if let Some(r) = result {
            return r;
        }
    }
}

async fn token_request(
    http: &reqwest::Client,
    req: &Request,
    form: Vec<(&str, String)>,
) -> Result<Tokens, String> {
    let mut form = form;
    let mut rb = http
        .post(&req.token_url)
        .header("Accept", "application/json");
    match (req.client_auth, &req.client_secret) {
        (ClientAuth::Basic | ClientAuth::BasicJson, Some(secret)) => {
            rb = rb.basic_auth(&req.client_id, Some(secret))
        }
        (_, secret) => {
            form.push(("client_id", req.client_id.clone()));
            if let Some(s) = secret {
                form.push(("client_secret", s.clone()));
            }
        }
    }
    if let Some(r) = &req.resource {
        form.push(("resource", r.clone()));
    }
    rb = if req.client_auth == ClientAuth::BasicJson {
        rb.json(
            &form
                .iter()
                .cloned()
                .collect::<std::collections::HashMap<_, _>>(),
        )
    } else {
        rb.form(&form)
    };
    let resp = rb
        .send()
        .await
        .map_err(|e| format!("Couldn't reach the sign-in service: {e}"))?;
    let status = resp.status();
    let text = resp.text().await.unwrap_or_default();
    let v: Value = serde_json::from_str(&text).unwrap_or_else(|_| {
        // A few providers still answer with a form-encoded body.
        Value::Object(
            url::form_urlencoded::parse(text.as_bytes())
                .map(|(k, v)| (k.to_string(), Value::String(v.to_string())))
                .collect(),
        )
    });
    if !status.is_success() && v["error"].is_null() {
        return Err(format!("The sign-in service answered {status}"));
    }
    Tokens::from_response(&v, now_ms())
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

/// Runs the whole flow: opens the browser with `open`, waits for the
/// redirect, and exchanges the code for tokens.
pub async fn authorize(
    http: &reqwest::Client,
    req: &Request,
    open: impl FnOnce(&str) -> Result<(), String>,
) -> Result<Tokens, String> {
    let listener = TcpListener::bind((req.host, req.port))
        .await
        .map_err(|e| format!("Couldn't open a local port for sign-in: {e}"))?;
    let port = listener.local_addr().map_err(|e| e.to_string())?.port();
    let redirect = format!("http://{}:{port}/callback", req.host);
    let state = random_token(24);
    let (verifier, challenge) = pkce();
    open(&authorize_url(req, &redirect, &state, &challenge)?)?;
    let code = tokio::time::timeout(SIGN_IN_TIMEOUT, wait_for_code(listener, &state))
        .await
        .map_err(|_| "Sign-in timed out after 5 minutes".to_string())??;
    token_request(
        http,
        req,
        vec![
            ("grant_type", "authorization_code".into()),
            ("code", code),
            ("redirect_uri", redirect),
            ("code_verifier", verifier),
        ],
    )
    .await
}

/// New tokens from a refresh token. Keeps the old refresh token when the
/// provider doesn't send a new one.
pub async fn refresh(
    http: &reqwest::Client,
    req: &Request,
    refresh_token: &str,
) -> Result<Tokens, String> {
    let mut t = token_request(
        http,
        req,
        vec![
            ("grant_type", "refresh_token".into()),
            ("refresh_token", refresh_token.to_string()),
        ],
    )
    .await?;
    if t.refresh_token.is_none() {
        t.refresh_token = Some(refresh_token.to_string());
    }
    Ok(t)
}

// ---------- MCP authorization discovery ----------

/// Where an MCP server's authorization server lives, and how to register.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct AuthServer {
    pub authorize_url: String,
    pub token_url: String,
    pub registration_url: Option<String>,
    pub scopes: Vec<String>,
}

/// The `resource_metadata` URL from a 401's WWW-Authenticate header.
pub fn resource_metadata_hint(www_authenticate: &str) -> Option<String> {
    let i = www_authenticate.find("resource_metadata=")?;
    let rest = &www_authenticate[i + "resource_metadata=".len()..];
    let v = if let Some(r) = rest.strip_prefix('"') {
        r.split('"').next()?
    } else {
        rest.split([',', ' ']).next()?
    };
    Some(v.to_string())
}

/// RFC 8414 / RFC 9728 well-known locations for a URL: the path is put
/// after the well-known segment, and the bare origin is tried last.
pub fn well_known(url: &Url, name: &str) -> Vec<String> {
    let origin = url.origin().ascii_serialization();
    let path = url.path().trim_end_matches('/');
    let mut out = Vec::new();
    if !path.is_empty() {
        out.push(format!("{origin}/.well-known/{name}{path}"));
    }
    out.push(format!("{origin}/.well-known/{name}"));
    out
}

async fn get_json(http: &reqwest::Client, url: &str) -> Option<Value> {
    let r = http
        .get(url)
        .header("Accept", "application/json")
        .send()
        .await
        .ok()?;
    if !r.status().is_success() {
        return None;
    }
    r.json().await.ok()
}

/// Finds the authorization server for an MCP server URL.
pub async fn discover(
    http: &reqwest::Client,
    server: &str,
    hint: Option<String>,
) -> Result<AuthServer, String> {
    let server_url =
        Url::parse(server).map_err(|_| format!("\"{server}\" isn't a valid address"))?;
    // 1. Protected resource metadata names the authorization server.
    let mut candidates: Vec<String> = hint.into_iter().collect();
    candidates.extend(well_known(&server_url, "oauth-protected-resource"));
    let mut issuer = None;
    let mut scopes = Vec::new();
    for c in candidates {
        if let Some(v) = get_json(http, &c).await {
            issuer = v["authorization_servers"][0].as_str().map(String::from);
            scopes = v["scopes_supported"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|s| s.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default();
            if issuer.is_some() {
                break;
            }
        }
    }
    // Older servers are their own authorization server.
    let issuer = issuer.unwrap_or_else(|| server_url.origin().ascii_serialization());
    let issuer_url = Url::parse(&issuer)
        .map_err(|_| "The server named an invalid sign-in address".to_string())?;
    // 2. Authorization server metadata.
    let mut meta = None;
    for c in well_known(&issuer_url, "oauth-authorization-server")
        .into_iter()
        .chain(well_known(&issuer_url, "openid-configuration"))
    {
        if let Some(v) = get_json(http, &c).await {
            if v["authorization_endpoint"].is_string() {
                meta = Some(v);
                break;
            }
        }
    }
    let base = issuer_url.origin().ascii_serialization();
    let m = meta.unwrap_or(Value::Null);
    let pick = |k: &str, default: &str| {
        m[k].as_str()
            .map(String::from)
            .unwrap_or_else(|| format!("{base}{default}"))
    };
    if scopes.is_empty() {
        scopes = m["scopes_supported"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|s| s.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();
    }
    Ok(AuthServer {
        authorize_url: pick("authorization_endpoint", "/authorize"),
        token_url: pick("token_endpoint", "/token"),
        registration_url: m["registration_endpoint"].as_str().map(String::from),
        scopes,
    })
}

/// Registers Helpy as a public client (RFC 7591).
pub async fn register(
    http: &reqwest::Client,
    registration_url: &str,
    redirect: &str,
) -> Result<(String, Option<String>), String> {
    let body = serde_json::json!({
        "client_name": "Helpy",
        "redirect_uris": [redirect],
        "grant_types": ["authorization_code", "refresh_token"],
        "response_types": ["code"],
        "token_endpoint_auth_method": "none",
        "application_type": "native",
    });
    let r = http
        .post(registration_url)
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("Couldn't register with the sign-in service: {e}"))?;
    let status = r.status();
    let v: Value = r.json().await.unwrap_or(Value::Null);
    match v["client_id"].as_str() {
        Some(id) if status.is_success() => Ok((id.to_string(), v["client_secret"].as_str().map(String::from))),
        _ => Err(format!(
            "The server didn't let Helpy register ({status}). Add a client ID for it in its settings."
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req(token_url: String) -> Request {
        Request {
            authorize_url: "https://accounts.example.com/o/auth?prompt=consent".into(),
            token_url,
            client_id: "helpy-client".into(),
            client_secret: None,
            client_auth: ClientAuth::Body,
            scopes: vec!["read".into(), "write".into()],
            scope_param: "scope",
            extra: vec![("access_type".into(), "offline".into())],
            resource: Some("https://mcp.example.com/mcp".into()),
            host: "127.0.0.1",
            port: 0,
        }
    }

    #[test]
    fn pkce_challenge_is_the_s256_of_the_verifier() {
        let (v, c) = pkce();
        assert!(v.len() >= 43 && v.len() <= 128);
        assert_eq!(c, b64url(&Sha256::digest(v.as_bytes())));
        assert_ne!(pkce().0, v);
    }

    #[test]
    fn builds_the_authorize_url() {
        let u = authorize_url(
            &req("x".into()),
            "http://127.0.0.1:5555/callback",
            "st",
            "ch",
        )
        .unwrap();
        let u = Url::parse(&u).unwrap();
        let q: std::collections::HashMap<_, _> = u.query_pairs().into_owned().collect();
        assert_eq!(q["prompt"], "consent");
        assert_eq!(q["scope"], "read write");
        assert_eq!(q["code_challenge_method"], "S256");
        assert_eq!(q["resource"], "https://mcp.example.com/mcp");
        assert_eq!(q["access_type"], "offline");
    }

    #[test]
    fn reads_callbacks() {
        assert_eq!(
            parse_callback("GET /callback?code=abc&state=xyz HTTP/1.1"),
            Callback::Code {
                code: "abc".into(),
                state: "xyz".into()
            }
        );
        assert_eq!(
            parse_callback(
                "GET /callback?error=access_denied&error_description=You%20said%20no HTTP/1.1"
            ),
            Callback::Error("You said no".into())
        );
        assert_eq!(parse_callback("GET /favicon.ico HTTP/1.1"), Callback::Other);
    }

    #[test]
    fn reads_token_answers_including_slack_style() {
        let t = Tokens::from_response(
            &serde_json::json!({"access_token": "a", "refresh_token": "r", "expires_in": 3600}),
            1000,
        )
        .unwrap();
        assert_eq!(
            (t.access_token.as_str(), t.expires_at),
            ("a", Some(3_601_000))
        );
        assert!(!t.stale(1000) && t.stale(3_560_000));
        let slack = Tokens::from_response(&serde_json::json!({"ok": true, "authed_user": {"access_token": "xoxp-1", "scope": "search:read"}}), 0).unwrap();
        assert_eq!(slack.access_token, "xoxp-1");
        assert!(
            Tokens::from_response(&serde_json::json!({"error": "invalid_grant"}), 0)
                .unwrap_err()
                .contains("invalid_grant")
        );
    }

    #[test]
    fn finds_resource_metadata_and_well_known_urls() {
        assert_eq!(
            resource_metadata_hint(r#"Bearer error="invalid_token", resource_metadata="https://mcp.example.com/.well-known/oauth-protected-resource/mcp""#).as_deref(),
            Some("https://mcp.example.com/.well-known/oauth-protected-resource/mcp")
        );
        let u = Url::parse("https://mcp.example.com/v1/mcp").unwrap();
        assert_eq!(
            well_known(&u, "oauth-protected-resource"),
            [
                "https://mcp.example.com/.well-known/oauth-protected-resource/v1/mcp",
                "https://mcp.example.com/.well-known/oauth-protected-resource"
            ]
        );
    }

    /// A token endpoint that checks the PKCE verifier against the challenge
    /// the browser was sent, then hands out tokens.
    #[tokio::test]
    async fn the_whole_flow_against_a_mock_provider() {
        let token_server = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let token_url = format!("http://{}/token", token_server.local_addr().unwrap());
        let challenge = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
        let seen = challenge.clone();
        tokio::spawn(async move {
            let (mut s, _) = token_server.accept().await.unwrap();
            let mut buf = vec![0u8; 8192];
            let n = s.read(&mut buf).await.unwrap();
            let body = String::from_utf8_lossy(&buf[..n]).to_string();
            let form = body.split("\r\n\r\n").nth(1).unwrap_or("").to_string();
            let q: std::collections::HashMap<String, String> =
                url::form_urlencoded::parse(form.as_bytes())
                    .into_owned()
                    .collect();
            let ok = q["code"] == "the-code"
                && b64url(&Sha256::digest(q["code_verifier"].as_bytes())) == *seen.lock().unwrap()
                && q["client_id"] == "helpy-client"
                && q["resource"] == "https://mcp.example.com/mcp";
            let json = if ok {
                r#"{"access_token":"tok","refresh_token":"ref","expires_in":60}"#
            } else {
                r#"{"error":"bad"}"#
            };
            let reply = format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{json}", json.len());
            s.write_all(reply.as_bytes()).await.unwrap();
        });

        let http = reqwest::Client::new();
        let r = req(token_url);
        let tokens = authorize(&http, &r, |url| {
            // Play the browser: note the challenge, then come back with a code.
            let u = Url::parse(url).unwrap();
            let q: std::collections::HashMap<_, _> = u.query_pairs().into_owned().collect();
            *challenge.lock().unwrap() = q["code_challenge"].clone();
            let back = format!("{}?code=the-code&state={}", q["redirect_uri"], q["state"]);
            tokio::spawn(async move {
                // A stray request first, as browsers do.
                let base = back.split("/callback").next().unwrap().to_string();
                let _ = reqwest::get(format!("{base}/favicon.ico")).await;
                reqwest::get(back).await.unwrap().text().await.unwrap()
            });
            Ok(())
        })
        .await
        .unwrap();
        assert_eq!(tokens.access_token, "tok");
        assert_eq!(tokens.refresh_token.as_deref(), Some("ref"));
    }
}
