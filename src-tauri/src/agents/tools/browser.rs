//! A headless browser for agents, for pages that need JavaScript, clicks or
//! forms. It uses a Chrome-type browser already on the computer (or one
//! Helpy downloaded), with Helpy's own profile, so the user's logins and
//! history are never touched. Each agent gets its own tab. Only public
//! pages, as with fetching. Tabs hide the usual signs of automation, which
//! gets past many bot checks; pages that still block it are reported, not
//! worked around. (Another fetcher can plug in at `blocked` later.)

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use chromiumoxide::browser::{Browser, BrowserConfig};
use chromiumoxide::Page;
use futures_util::StreamExt;
use reqwest::Url;
use serde::Deserialize;
use serde_json::Value;
use tokio::sync::Mutex;

use super::web;
use crate::agents::model::FileOp;
use crate::agents::runner::ToolOutcome;

const NAV_TIMEOUT: Duration = Duration::from_secs(40);
/// Time for scripts to fill the page after it loads.
const SETTLE: Duration = Duration::from_millis(1200);
const MAX_TEXT: usize = 15_000;
/// The browser closes after this long unused.
const IDLE: Duration = Duration::from_secs(300);

struct Live {
    browser: Browser,
    handler: tokio::task::JoinHandle<()>,
}

pub struct BrowserPool {
    data: PathBuf,
    live: Mutex<Option<Live>>,
    tabs: Mutex<HashMap<String, (Page, Instant)>>,
    last_used: std::sync::Mutex<Instant>,
}

/// Where Helpy keeps a Chromium it downloaded.
fn own_chromium(data: &Path) -> PathBuf {
    data.join("chromium")
}

/// A Chrome-type browser: Helpy's own download, then CHROME, Chrome,
/// Chromium, Edge and Brave where they're usually installed.
pub fn find_browser(data: &Path) -> Option<PathBuf> {
    if let Ok(p) = std::fs::read_to_string(own_chromium(data).join("path.txt")) {
        let p = PathBuf::from(p.trim());
        if p.is_file() {
            return Some(p);
        }
    }
    if let Ok(p) = chromiumoxide::detection::default_executable(Default::default()) {
        return Some(p);
    }
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default();
    let mut paths: Vec<PathBuf> = Vec::new();
    if cfg!(target_os = "macos") {
        for app in [
            "Microsoft Edge",
            "Chromium",
            "Brave Browser",
            "Google Chrome",
        ] {
            let rel = format!("{app}.app/Contents/MacOS/{app}");
            paths.push(Path::new("/Applications").join(&rel));
            paths.push(home.join("Applications").join(&rel));
        }
    } else if cfg!(windows) {
        for base in ["C:\\Program Files", "C:\\Program Files (x86)"] {
            paths.push(Path::new(base).join("Microsoft\\Edge\\Application\\msedge.exe"));
            paths.push(Path::new(base).join("Google\\Chrome\\Application\\chrome.exe"));
            paths
                .push(Path::new(base).join("BraveSoftware\\Brave-Browser\\Application\\brave.exe"));
        }
    } else {
        let search = crate::mcp::shell_path()
            .or_else(|| std::env::var("PATH").ok())
            .unwrap_or_default();
        for dir in std::env::split_paths(&search) {
            for name in [
                "google-chrome",
                "chromium",
                "chromium-browser",
                "brave-browser",
                "microsoft-edge",
            ] {
                paths.push(dir.join(name));
            }
        }
    }
    paths.into_iter().find(|p| p.is_file())
}

/// Downloads Chromium (about 150 MB) into Helpy's data folder.
pub async fn download(data: &Path) -> Result<PathBuf, String> {
    use chromiumoxide::fetcher::{BrowserFetcher, BrowserFetcherOptions};
    let dir = own_chromium(data);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let opts = BrowserFetcherOptions::builder()
        .with_path(&dir)
        .build()
        .map_err(|e| e.to_string())?;
    let got = BrowserFetcher::new(opts)
        .fetch()
        .await
        .map_err(|e| format!("Couldn't download Chromium: {e}"))?;
    std::fs::write(
        dir.join("path.txt"),
        got.executable_path.to_string_lossy().as_bytes(),
    )
    .map_err(|e| e.to_string())?;
    Ok(got.executable_path)
}

/// Linux as root (containers): the process's real user id is 0.
fn running_as_root() -> bool {
    cfg!(target_os = "linux")
        && std::fs::read_to_string("/proc/self/status")
            .ok()
            .and_then(|s| {
                s.lines()
                    .find(|l| l.starts_with("Uid:"))
                    .map(|l| l.split_whitespace().nth(1) == Some("0"))
            })
            .unwrap_or(false)
}

/// Markers of pages that are a block or a challenge, not the content.
pub fn looks_blocked(title: &str, text: &str) -> bool {
    let t = title.to_lowercase();
    let head: String = text.chars().take(2000).collect::<String>().to_lowercase();
    [
        "just a moment",
        "attention required",
        "access denied",
        "are you a robot",
        "verify you are human",
        "pardon our interruption",
    ]
    .iter()
    .any(|m| t.contains(m))
        || [
            "enable javascript and cookies to continue",
            "checking your browser",
            "captcha",
            "unusual traffic from your",
        ]
        .iter()
        .any(|m| head.contains(m) && head.len() < 1500)
}

#[derive(Deserialize, Default)]
struct PageRead {
    #[serde(default)]
    text: String,
    #[serde(default)]
    links: Vec<(String, String)>,
    #[serde(default)]
    forms: u32,
}

const READ_JS: &str = r#"(() => {
  const text = document.body ? document.body.innerText : "";
  const links = [...document.querySelectorAll("a[href]")]
    .map(a => [(a.innerText || a.getAttribute("aria-label") || "").trim().replace(/\s+/g, " ").slice(0, 80), a.href])
    .filter(([t, h]) => t && /^https?:/.test(h)).slice(0, 40);
  return { text: text.slice(0, 200000), links, forms: document.forms.length };
})()"#;

const TABLES_JS: &str = r#"(() => [...document.querySelectorAll("table")].slice(0, 5).map(t => {
  const rows = [...t.querySelectorAll("tr")].slice(0, 300).map(r =>
    [...r.querySelectorAll("th,td")].map(c => c.innerText.trim().replace(/\s+/g, " ")));
  return rows.filter(r => r.length);
}).filter(t => t.length))()"#;

/// Finds a clickable element by CSS selector or by its visible text.
fn click_js(target: &str) -> String {
    let t = serde_json::to_string(target).unwrap_or_default();
    format!(
        r#"(() => {{
  const want = {t};
  let el = null;
  try {{ el = document.querySelector(want); }} catch (e) {{}}
  if (!el) {{
    const lower = want.toLowerCase();
    const all = [...document.querySelectorAll("a,button,[role=button],input[type=submit],input[type=button],summary,label")];
    el = all.find(e => (e.innerText || e.value || "").trim().toLowerCase() === lower)
      || all.find(e => (e.innerText || e.value || e.getAttribute("aria-label") || "").toLowerCase().includes(lower));
  }}
  if (!el) return null;
  el.scrollIntoView({{ block: "center" }});
  el.click();
  return (el.innerText || el.value || el.tagName).trim().slice(0, 80);
}})()"#
    )
}

/// Fills a field found by selector, label, placeholder or name; submits its
/// form when asked.
fn type_js(field: &str, text: &str, submit: bool) -> String {
    let f = serde_json::to_string(field).unwrap_or_default();
    let v = serde_json::to_string(text).unwrap_or_default();
    format!(
        r#"(() => {{
  const want = {f}, value = {v};
  let el = null;
  try {{ el = document.querySelector(want); }} catch (e) {{}}
  const lower = want.toLowerCase();
  const fields = [...document.querySelectorAll("input,textarea,select")];
  if (!el) el = fields.find(e => [e.name, e.id, e.placeholder, e.getAttribute("aria-label")].some(x => x && x.toLowerCase() === lower));
  if (!el) {{
    const label = [...document.querySelectorAll("label")].find(l => l.innerText.trim().toLowerCase().includes(lower));
    if (label) el = label.control || label.querySelector("input,textarea,select");
  }}
  if (!el) el = fields.find(e => [e.name, e.placeholder, e.getAttribute("aria-label")].some(x => x && x.toLowerCase().includes(lower)));
  if (!el) return null;
  el.focus();
  const proto = el.tagName === "TEXTAREA" ? HTMLTextAreaElement.prototype : el.tagName === "SELECT" ? HTMLSelectElement.prototype : HTMLInputElement.prototype;
  Object.getOwnPropertyDescriptor(proto, "value").set.call(el, value);
  el.dispatchEvent(new Event("input", {{ bubbles: true }}));
  el.dispatchEvent(new Event("change", {{ bubbles: true }}));
  if ({submit}) {{
    if (el.form) {{ el.form.requestSubmit ? el.form.requestSubmit() : el.form.submit(); }}
    else el.dispatchEvent(new KeyboardEvent("keydown", {{ key: "Enter", bubbles: true }}));
  }}
  return el.name || el.id || el.placeholder || el.tagName;
}})()"#
    )
}

fn cut(s: &str, max: usize) -> String {
    match s.char_indices().nth(max) {
        Some((i, _)) => format!("{}\n[… the rest of the page was cut]", &s[..i]),
        None => s.to_string(),
    }
}

/// The page as the agent sees it: title, address, text and links.
fn summary(title: &str, url: &str, read: &PageRead, max: usize) -> String {
    let mut out = format!("{title}\n{url}\n\n{}", cut(read.text.trim(), max));
    if !read.links.is_empty() {
        out += "\n\nLinks:\n";
        for (t, h) in &read.links {
            out += &format!("- {t}: {h}\n");
        }
    }
    if read.forms > 0 {
        out += &format!(
            "\n(The page has {} form{}.)",
            read.forms,
            if read.forms == 1 { "" } else { "s" }
        );
    }
    out
}

impl BrowserPool {
    pub fn new(data: PathBuf) -> Arc<Self> {
        let pool = Arc::new(Self {
            data,
            live: Mutex::new(None),
            tabs: Mutex::new(HashMap::new()),
            last_used: std::sync::Mutex::new(Instant::now()),
        });
        // Closes the browser when agents haven't used it for a while.
        let weak = Arc::downgrade(&pool);
        tauri::async_runtime::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(60)).await;
                let Some(pool) = weak.upgrade() else { break };
                if pool.last_used.lock().unwrap().elapsed() > IDLE {
                    pool.shut().await;
                }
            }
        });
        pool
    }

    pub fn data(&self) -> &Path {
        &self.data
    }

    async fn shut(&self) {
        self.tabs.lock().await.clear();
        if let Some(mut live) = self.live.lock().await.take() {
            let _ = live.browser.close().await;
            live.handler.abort();
        }
    }

    /// The agent's tab, starting the browser if needed.
    async fn tab(&self, agent: &str) -> Result<Page, String> {
        *self.last_used.lock().unwrap() = Instant::now();
        if let Some((page, at)) = self.tabs.lock().await.get_mut(agent) {
            *at = Instant::now();
            return Ok(page.clone());
        }
        let mut live = self.live.lock().await;
        if live.is_none() {
            let exe = find_browser(&self.data).ok_or(
                "There's no Chrome-type browser on this computer (Chrome, Edge, Chromium or Brave). The user can download \
                 one in Settings → Agents → Web browser.",
            )?;
            let mut cfg = BrowserConfig::builder()
                .chrome_executable(exe)
                .user_data_dir(self.data.join("browser-profile"))
                .window_size(1366, 900)
                .new_headless_mode()
                .launch_timeout(Duration::from_secs(30));
            // Chrome refuses its sandbox when running as root (containers).
            if running_as_root() {
                cfg = cfg.no_sandbox();
            }
            let (browser, mut handler) = Browser::launch(cfg.build()?)
                .await
                .map_err(|e| format!("The browser didn't start: {e}"))?;
            let handler = tokio::spawn(async move { while handler.next().await.is_some() {} });
            *live = Some(Live { browser, handler });
        }
        let browser = &live.as_ref().expect("started above").browser;
        let page = browser
            .new_page("about:blank")
            .await
            .map_err(|e| format!("Couldn't open a tab: {e}"))?;
        // Hide the signs of automation, with the browser's own version in
        // the user agent (not "HeadlessChrome", and not an outdated one).
        let ua = browser
            .version()
            .await
            .map(|v| v.user_agent.replace("HeadlessChrome", "Chrome"))
            .unwrap_or_default();
        if let Err(e) = page.enable_stealth_mode_with_agent(&ua).await {
            log::warn!("browser stealth mode: {e}");
        }
        self.tabs
            .lock()
            .await
            .insert(agent.to_string(), (page.clone(), Instant::now()));
        Ok(page)
    }

    /// Closes an agent's tab (when it finishes).
    pub async fn close_tab(&self, agent: &str) {
        if let Some((page, _)) = self.tabs.lock().await.remove(agent) {
            let _ = page.close().await;
        }
    }

    async fn read(page: &Page) -> Result<(String, String, PageRead), String> {
        tokio::time::sleep(SETTLE).await;
        let title = page.get_title().await.ok().flatten().unwrap_or_default();
        let url = page.url().await.ok().flatten().unwrap_or_default();
        let read: PageRead = page
            .evaluate(READ_JS)
            .await
            .map_err(|e| format!("Couldn't read the page: {e}"))?
            .into_value()
            .unwrap_or_default();
        Ok((title, url, read))
    }

    /// Where the page ended up must be public too (redirects, scripts).
    async fn landed(url: &str) -> Result<(), String> {
        match Url::parse(url) {
            Ok(u) if u.scheme() == "about" => Ok(()),
            Ok(u) => web::check(&u).await,
            Err(_) => Ok(()),
        }
    }

    pub async fn open(&self, agent: &str, url: &str) -> ToolOutcome {
        let parsed = match Url::parse(url.trim()) {
            Ok(u) => u,
            Err(_) => return ToolOutcome::Permanent(format!("\"{url}\" isn't a web address.")),
        };
        if let Err(e) = web::check(&parsed).await {
            return ToolOutcome::Permanent(e);
        }
        let page = match self.tab(agent).await {
            Ok(p) => p,
            Err(e) => return ToolOutcome::Permanent(e),
        };
        match tokio::time::timeout(NAV_TIMEOUT, page.goto(parsed.as_str())).await {
            Err(_) => {
                return ToolOutcome::Transient(format!(
                    "{} took too long to load",
                    parsed.host_str().unwrap_or("")
                ))
            }
            Ok(Err(e)) => return ToolOutcome::Transient(format!("Couldn't load {parsed}: {e}")),
            Ok(Ok(_)) => {}
        }
        let (title, now, read) = match Self::read(&page).await {
            Ok(r) => r,
            Err(e) => return ToolOutcome::Transient(e),
        };
        if let Err(e) = Self::landed(&now).await {
            let _ = page.goto("about:blank").await;
            return ToolOutcome::Permanent(e);
        }
        if looks_blocked(&title, &read.text) {
            return self.blocked(parsed.as_str()).await;
        }
        ok(summary(&title, &now, &read, MAX_TEXT))
    }

    /// A page that blocks automated browsing even with the usual signs
    /// hidden. The agent tells the user instead of trying to get around it.
    async fn blocked(&self, url: &str) -> ToolOutcome {
        ToolOutcome::Permanent(format!(
            "{url} blocked the browser (it looks like bot protection). Don't try to get around it: tell the user, who \
             can open the page in their own browser."
        ))
    }

    async fn current(&self, agent: &str) -> Result<Page, ToolOutcome> {
        match self.tabs.lock().await.get(agent) {
            Some((p, _)) => Ok(p.clone()),
            None => Err(ToolOutcome::Permanent(
                "No page is open; use browser_open first.".into(),
            )),
        }
    }

    async fn after_action(page: &Page, did: String) -> ToolOutcome {
        let (title, url, read) = match Self::read(page).await {
            Ok(r) => r,
            Err(e) => return ToolOutcome::Transient(e),
        };
        if let Err(e) = Self::landed(&url).await {
            let _ = page.goto("about:blank").await;
            return ToolOutcome::Permanent(e);
        }
        ok(format!("{did}\n\n{}", summary(&title, &url, &read, 6000)))
    }

    pub async fn click(&self, agent: &str, target: &str) -> ToolOutcome {
        let page = match self.current(agent).await {
            Ok(p) => p,
            Err(e) => return e,
        };
        let clicked: Option<String> = match page.evaluate(click_js(target)).await {
            Ok(v) => v.into_value().unwrap_or(None),
            Err(e) => return ToolOutcome::Transient(format!("Couldn't click: {e}")),
        };
        match clicked {
            None => ToolOutcome::Permanent(format!(
                "Nothing to click matches \"{target}\" on this page."
            )),
            Some(what) => Self::after_action(&page, format!("Clicked \"{what}\".")).await,
        }
    }

    pub async fn fill(&self, agent: &str, field: &str, text: &str, submit: bool) -> ToolOutcome {
        let page = match self.current(agent).await {
            Ok(p) => p,
            Err(e) => return e,
        };
        let filled: Option<String> = match page.evaluate(type_js(field, text, submit)).await {
            Ok(v) => v.into_value().unwrap_or(None),
            Err(e) => return ToolOutcome::Transient(format!("Couldn't type: {e}")),
        };
        match filled {
            None => ToolOutcome::Permanent(format!("No field matches \"{field}\" on this page.")),
            Some(name) => {
                let did = if submit {
                    format!("Filled in {name} and sent the form.")
                } else {
                    format!("Filled in {name}.")
                };
                Self::after_action(&page, did).await
            }
        }
    }

    pub async fn tables(&self, agent: &str) -> ToolOutcome {
        let page = match self.current(agent).await {
            Ok(p) => p,
            Err(e) => return e,
        };
        let tables: Vec<Vec<Vec<String>>> = match page.evaluate(TABLES_JS).await {
            Ok(v) => v.into_value().unwrap_or_default(),
            Err(e) => return ToolOutcome::Transient(format!("Couldn't read the tables: {e}")),
        };
        if tables.is_empty() {
            return ok("The page has no tables. Use the page text instead.".into());
        }
        ok(cut(
            &serde_json::to_string(&tables).unwrap_or_default(),
            MAX_TEXT,
        ))
    }
}

fn ok(text: String) -> ToolOutcome {
    ToolOutcome::Ok {
        text,
        ops: Vec::new(),
    }
}

// ---------- CSV ----------

/// One CSV field, quoted when it has to be.
fn field(s: &str) -> String {
    if s.contains([',', '"', '\n', '\r']) || s.starts_with(' ') || s.ends_with(' ') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

pub fn to_csv(columns: &[String], rows: &[Vec<String>]) -> String {
    let mut out = String::new();
    for row in std::iter::once(columns).chain(rows.iter().map(|r| r.as_slice())) {
        out += &row.iter().map(|c| field(c)).collect::<Vec<_>>().join(",");
        out += "\r\n";
    }
    out
}

/// A cell of any JSON type, as text.
fn cell(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

/// Saves rows as a CSV file in the projects folder's Data folder.
pub fn save_csv(projects: &Path, name: &str, args: &Value) -> ToolOutcome {
    let columns: Vec<String> = args["columns"]
        .as_array()
        .map(|a| a.iter().map(cell).collect())
        .unwrap_or_default();
    let rows: Vec<Vec<String>> = args["rows"]
        .as_array()
        .map(|a| {
            a.iter()
                .map(|r| {
                    r.as_array()
                        .map(|c| c.iter().map(cell).collect())
                        .unwrap_or_default()
                })
                .collect()
        })
        .unwrap_or_default();
    if columns.is_empty() || rows.is_empty() {
        return ToolOutcome::Permanent("Give the column names and at least one row.".into());
    }
    let slug: String = name
        .trim()
        .trim_end_matches(".csv")
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == ' ' {
                c
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim()
        .to_string();
    let slug = if slug.is_empty() {
        "data".to_string()
    } else {
        slug
    };
    let dir = projects.join("Data");
    if let Err(e) = std::fs::create_dir_all(&dir) {
        return ToolOutcome::Permanent(format!("Couldn't make {}: {e}", dir.display()));
    }
    let mut path = dir.join(format!("{slug}.csv"));
    let mut n = 2;
    while path.exists() {
        path = dir.join(format!("{slug} {n}.csv"));
        n += 1;
    }
    // A byte-order mark so spreadsheet apps read accented letters right.
    let body = format!("\u{feff}{}", to_csv(&columns, &rows));
    if let Err(e) = std::fs::write(&path, body) {
        return ToolOutcome::Permanent(format!("Couldn't save {}: {e}", path.display()));
    }
    ToolOutcome::Ok {
        text: format!("Saved {} rows to {}", rows.len(), path.display()),
        ops: vec![FileOp::Created {
            path: path.display().to_string(),
        }],
    }
}

/// The first rows of a CSV file, for previews.
pub fn preview_csv(text: &str, max_rows: usize) -> Vec<Vec<String>> {
    let mut rows = Vec::new();
    let (mut row, mut cur, mut quoted) = (Vec::new(), String::new(), false);
    let mut chars = text.trim_start_matches('\u{feff}').chars().peekable();
    while let Some(c) = chars.next() {
        match (c, quoted) {
            ('"', true) if chars.peek() == Some(&'"') => {
                cur.push('"');
                chars.next();
            }
            ('"', _) => quoted = !quoted,
            (',', false) => row.push(std::mem::take(&mut cur)),
            ('\r', false) => {}
            ('\n', false) => {
                row.push(std::mem::take(&mut cur));
                rows.push(std::mem::take(&mut row));
                if rows.len() >= max_rows {
                    return rows;
                }
            }
            (c, _) => cur.push(c),
        }
    }
    if !cur.is_empty() || !row.is_empty() {
        row.push(cur);
        rows.push(row);
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csv_quotes_what_it_must_and_reads_back() {
        let cols = vec!["Name".to_string(), "Price".to_string()];
        let rows = vec![
            vec!["Desk, \"pro\"".to_string(), "£399".to_string()],
            vec!["Chair\nV2".to_string(), "£99".to_string()],
        ];
        let text = to_csv(&cols, &rows);
        assert_eq!(
            text,
            "Name,Price\r\n\"Desk, \"\"pro\"\"\",£399\r\n\"Chair\nV2\",£99\r\n"
        );
        let back = preview_csv(&text, 10);
        assert_eq!(back[1], rows[0]);
        assert_eq!(back[2], rows[1]);
        assert_eq!(preview_csv(&text, 2).len(), 2);
    }

    #[test]
    fn saves_into_the_data_folder_without_overwriting() {
        let dir = tempfile::tempdir().unwrap();
        let args = serde_json::json!({"columns": ["Name", "Price"], "rows": [["Desk", 399], ["Chair", null]]});
        let ToolOutcome::Ok { ops, .. } = save_csv(dir.path(), "desk prices", &args) else {
            panic!()
        };
        let ToolOutcome::Ok { ops: again, .. } = save_csv(dir.path(), "desk prices.csv", &args)
        else {
            panic!()
        };
        let FileOp::Created { path } = &ops[0] else {
            panic!()
        };
        let FileOp::Created { path: second } = &again[0] else {
            panic!()
        };
        assert!(
            path.ends_with("Data/desk prices.csv") && second.ends_with("Data/desk prices 2.csv")
        );
        let text = std::fs::read_to_string(path).unwrap();
        assert_eq!(preview_csv(&text, 5)[2], vec!["Chair", ""]);
        assert!(matches!(
            save_csv(
                dir.path(),
                "x",
                &serde_json::json!({"columns": [], "rows": []})
            ),
            ToolOutcome::Permanent(_)
        ));
    }

    #[test]
    fn spots_block_pages() {
        assert!(looks_blocked("Just a moment...", ""));
        assert!(looks_blocked(
            "",
            "Please enable JavaScript and cookies to continue"
        ));
        assert!(!looks_blocked(
            "Standing desks | Shop",
            &"Lots of products. ".repeat(200)
        ));
    }

    /// Needs a Chrome-type browser (CHROME=…) and a page served at
    /// http://127.0.0.1:8765 with a price table and a "Next page" link,
    /// with HELPY_ALLOW_LOCAL_PAGES=1.
    #[tokio::test]
    #[ignore]
    async fn drives_a_real_browser() {
        let dir = tempfile::tempdir().unwrap();
        let pool = BrowserPool::new(dir.path().to_path_buf());
        let text = match pool.open("a1", "http://127.0.0.1:8765/").await {
            ToolOutcome::Ok { text, .. } => text,
            other => panic!("{other:?}"),
        };
        assert!(
            text.starts_with("Prices\n") && text.contains("FlexiSpot E7"),
            "{text}"
        );
        let ToolOutcome::Ok { text, .. } = pool.tables("a1").await else {
            panic!()
        };
        assert!(text.contains(r#"["Uplift V2","£599"]"#), "{text}");
        let ToolOutcome::Ok { text, .. } = pool.click("a1", "Next page").await else {
            panic!()
        };
        assert!(
            text.starts_with("Clicked \"Next page\"") && text.contains("/p2.html"),
            "{text}"
        );
        assert!(matches!(
            pool.click("a1", "No such button").await,
            ToolOutcome::Permanent(_)
        ));
        // What sites see: no webdriver flag, no "HeadlessChrome".
        let page = pool.tab("a1").await.unwrap();
        let seen: (Option<bool>, String) = page
            .evaluate("[navigator.webdriver || null, navigator.userAgent]")
            .await
            .unwrap()
            .into_value()
            .unwrap();
        assert!(!seen.0.unwrap_or(false), "webdriver shows");
        assert!(
            !seen.1.contains("Headless") && seen.1.contains("Chrome/"),
            "{}",
            seen.1
        );
        pool.close_tab("a1").await;
    }
}
