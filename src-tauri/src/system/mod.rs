//! Apps and windows on the user's desktop: open any installed app by name,
//! and move, resize, snap, focus, minimize, full-screen, hide or quit the
//! windows of running ones ("Spotify on the right, Chrome on the left").
//!
//! The matching and layout maths here are the same everywhere; each system
//! lists apps and windows and acts on them in its own module (macOS through
//! the Accessibility API, Windows through Win32, Linux through wmctrl on X11).

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(windows)]
mod windows;

#[cfg(target_os = "linux")]
use linux as os;
#[cfg(target_os = "macos")]
use macos as os;
#[cfg(windows)]
use windows as os;

use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde_json::{json, Value};
use tauri::AppHandle;

use crate::agents::allow::AlwaysAllow;
use crate::agents::model::ActionKind;
use crate::agents::runner::Gate;
use crate::ai::types::ToolDef;
use crate::settings::schema::{Approvals, Rule};

/// The agents' tool group.
pub const GROUP: &str = "system";
pub const OPEN_APP: &str = "open_app";
pub const LIST_APPS: &str = "list_apps";
pub const LIST_WINDOWS: &str = "list_windows";
pub const ARRANGE: &str = "arrange_window";
pub const FOCUS: &str = "focus_window";
pub const QUIT: &str = "quit_app";

const TOOLS: [&str; 6] = [OPEN_APP, LIST_APPS, LIST_WINDOWS, ARRANGE, FOCUS, QUIT];

/// How long the list of installed apps is reused.
const APPS_FRESH: Duration = Duration::from_secs(120);

/// A rectangle in the system's window units: points on macOS, pixels elsewhere.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl Rect {
    fn overlap(&self, o: &Rect) -> f64 {
        let w = (self.x + self.w).min(o.x + o.w) - self.x.max(o.x);
        let h = (self.y + self.h).min(o.y + o.h) - self.y.max(o.y);
        w.max(0.0) * h.max(0.0)
    }
}

/// An installed app.
#[derive(Clone, Debug, PartialEq)]
pub struct App {
    pub name: String,
    /// What opens it: a bundle, a shortcut, a .desktop file or an app id.
    pub path: PathBuf,
}

/// A window of a running app.
#[derive(Clone, Debug, PartialEq)]
pub struct Win {
    /// The system's own handle, as text.
    pub key: String,
    pub pid: u32,
    pub app: String,
    pub title: String,
    pub frame: Rect,
    pub minimized: bool,
    pub fullscreen: bool,
}

/// What to do to a window.
#[derive(Clone, Debug, PartialEq)]
pub enum Op {
    Frame(Rect),
    /// Fill the work area (the system's own maximize where there is one).
    Maximize(Rect),
    FullScreen(bool),
    Minimize,
    Restore,
    Focus,
    Hide,
    Quit,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Layout {
    Left,
    Right,
    Top,
    Bottom,
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
    LeftThird,
    MiddleThird,
    RightThird,
    LeftTwoThirds,
    RightTwoThirds,
    Center,
    Maximize,
    FullScreen,
    ExitFullScreen,
    Minimize,
    Restore,
    Hide,
    /// Same place and size, relative to the screen (for moving to another one).
    Keep,
}

const LAYOUTS: &[(&str, Layout)] = &[
    ("left", Layout::Left),
    ("right", Layout::Right),
    ("top", Layout::Top),
    ("bottom", Layout::Bottom),
    ("top_left", Layout::TopLeft),
    ("top_right", Layout::TopRight),
    ("bottom_left", Layout::BottomLeft),
    ("bottom_right", Layout::BottomRight),
    ("left_third", Layout::LeftThird),
    ("middle_third", Layout::MiddleThird),
    ("right_third", Layout::RightThird),
    ("left_two_thirds", Layout::LeftTwoThirds),
    ("right_two_thirds", Layout::RightTwoThirds),
    ("center", Layout::Center),
    ("maximize", Layout::Maximize),
    ("full_screen", Layout::FullScreen),
    ("exit_full_screen", Layout::ExitFullScreen),
    ("minimize", Layout::Minimize),
    ("restore", Layout::Restore),
    ("hide", Layout::Hide),
    ("keep", Layout::Keep),
];

pub fn parse_layout(s: &str) -> Option<Layout> {
    let k = s.trim().to_lowercase().replace([' ', '-'], "_");
    let k = match k.as_str() {
        "left_half" => "left",
        "right_half" => "right",
        "top_half" => "top",
        "bottom_half" => "bottom",
        "fullscreen" => "full_screen",
        "fill" | "maximise" | "max" => "maximize",
        "centre" => "center",
        "minimise" => "minimize",
        "unminimize" | "show" => "restore",
        "exit_fullscreen" => "exit_full_screen",
        other => other,
    };
    LAYOUTS.iter().find(|(n, _)| *n == k).map(|(_, l)| *l)
}

/// Where a layout puts a window on a screen whose usable area is `area`;
/// None for layouts that aren't a place (minimize, full screen…).
pub fn frame(layout: Layout, area: Rect, current: Rect, from: Rect) -> Option<Rect> {
    let Rect { x, y, w, h } = area;
    let part = |fx: f64, fy: f64, fw: f64, fh: f64| Rect {
        x: (x + w * fx).round(),
        y: (y + h * fy).round(),
        w: (w * fw).round(),
        h: (h * fh).round(),
    };
    Some(match layout {
        Layout::Left => part(0.0, 0.0, 0.5, 1.0),
        Layout::Right => part(0.5, 0.0, 0.5, 1.0),
        Layout::Top => part(0.0, 0.0, 1.0, 0.5),
        Layout::Bottom => part(0.0, 0.5, 1.0, 0.5),
        Layout::TopLeft => part(0.0, 0.0, 0.5, 0.5),
        Layout::TopRight => part(0.5, 0.0, 0.5, 0.5),
        Layout::BottomLeft => part(0.0, 0.5, 0.5, 0.5),
        Layout::BottomRight => part(0.5, 0.5, 0.5, 0.5),
        Layout::LeftThird => part(0.0, 0.0, 1.0 / 3.0, 1.0),
        Layout::MiddleThird => part(1.0 / 3.0, 0.0, 1.0 / 3.0, 1.0),
        Layout::RightThird => part(2.0 / 3.0, 0.0, 1.0 / 3.0, 1.0),
        Layout::LeftTwoThirds => part(0.0, 0.0, 2.0 / 3.0, 1.0),
        Layout::RightTwoThirds => part(1.0 / 3.0, 0.0, 2.0 / 3.0, 1.0),
        Layout::Maximize => area,
        Layout::Center => {
            // Its own size, unless it doesn't fit; then most of the screen.
            let cw = if current.w > 0.0 && current.w <= w { current.w } else { (w * 0.7).round() };
            let ch = if current.h > 0.0 && current.h <= h { current.h } else { (h * 0.75).round() };
            Rect { x: (x + (w - cw) / 2.0).round(), y: (y + (h - ch) / 2.0).round(), w: cw, h: ch }
        }
        Layout::Keep => {
            if from.w <= 0.0 || from.h <= 0.0 {
                return None;
            }
            let fx = ((current.x - from.x) / from.w).clamp(0.0, 1.0);
            let fy = ((current.y - from.y) / from.h).clamp(0.0, 1.0);
            let fw = (current.w / from.w).clamp(0.1, 1.0);
            let fh = (current.h / from.h).clamp(0.1, 1.0);
            part(fx.min(1.0 - fw), fy.min(1.0 - fh), fw, fh)
        }
        _ => return None,
    })
}

fn norm(s: &str) -> String {
    s.to_lowercase().chars().filter(|c| c.is_alphanumeric()).collect()
}

/// Other names people use for apps.
const ALIASES: &[(&str, &[&str])] = &[
    ("vscode", &["visual studio code", "code"]),
    ("vs code", &["visual studio code", "code"]),
    ("code", &["visual studio code"]),
    ("chrome", &["google chrome"]),
    ("settings", &["system settings", "system preferences", "settings", "gnome control center"]),
    ("system preferences", &["system settings"]),
    ("files", &["finder", "file explorer", "files", "nautilus"]),
    ("file explorer", &["finder", "file explorer"]),
    ("explorer", &["file explorer", "finder"]),
    ("terminal", &["terminal", "windows terminal", "gnome terminal", "konsole"]),
    ("edge", &["microsoft edge"]),
    ("teams", &["microsoft teams"]),
    ("word", &["microsoft word"]),
    ("excel", &["microsoft excel"]),
    ("powerpoint", &["microsoft powerpoint"]),
    ("outlook", &["microsoft outlook"]),
    ("whatsapp", &["whatsapp"]),
    ("calculator", &["calculator", "gnome calculator"]),
    ("notes", &["notes", "notepad"]),
    ("app store", &["app store", "microsoft store"]),
];

/// How well `name` fits what the user called it, 0 for not at all.
pub fn score(query: &str, name: &str) -> u32 {
    let (q, n) = (norm(query), norm(name));
    if q.is_empty() || n.is_empty() {
        return 0;
    }
    if q == n {
        return 100;
    }
    let lower = query.trim().to_lowercase();
    if let Some((_, names)) = ALIASES.iter().find(|(a, _)| *a == lower) {
        if names.iter().any(|a| norm(a) == n) {
            return 95;
        }
    }
    let words: Vec<String> = name.to_lowercase().split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).map(String::from).collect();
    if n.starts_with(&q) {
        return 85;
    }
    if words.iter().any(|w| w.starts_with(&q)) && q.len() >= 3 {
        return 75;
    }
    if n.contains(&q) && q.len() >= 3 {
        return 65;
    }
    if q.contains(&n) && n.len() >= 4 {
        return 55;
    }
    if q.len() >= 5 && edits(&q, &n[..n.len().min(q.len() + 1)]) <= 2 {
        return 45;
    }
    0
}

/// Levenshtein distance, for small typos and mishearings ("spotfy").
fn edits(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.chars().enumerate() {
        let mut cur = vec![i + 1];
        for (j, cb) in b.iter().enumerate() {
            cur.push((prev[j] + (ca != *cb) as usize).min(prev[j + 1] + 1).min(cur[j] + 1));
        }
        prev = cur;
    }
    prev[b.len()]
}

/// The installed app the user means, if one fits well enough.
pub fn best_app<'a>(query: &str, apps: &'a [App]) -> Option<&'a App> {
    apps.iter()
        .map(|a| (score(query, &a.name), a))
        .filter(|(s, _)| *s >= 45)
        // Best score, then the shortest name ("Spotify" over "Spotify Helper").
        .max_by(|(s1, a1), (s2, a2)| s1.cmp(s2).then(a2.name.len().cmp(&a1.name.len())))
        .map(|(_, a)| a)
}

/// Names close to `query`, for "did you mean".
fn near(query: &str, names: impl Iterator<Item = String>) -> Vec<String> {
    let q = norm(query);
    let mut out: Vec<(usize, String)> = names
        .filter_map(|n| {
            let m = norm(&n);
            let d = edits(&q, &m[..m.len().min(q.len() + 2)]);
            (d <= q.len() / 2 + 1 || m.contains(&q[..q.len().min(3)])).then_some((d, n))
        })
        .collect();
    out.sort();
    out.dedup_by(|a, b| a.1 == b.1);
    out.into_iter().take(5).map(|(_, n)| n).collect()
}

/// The window the user means: of the app they named, with the title words
/// if given, the frontmost first (the list is in front-to-back order).
pub fn pick_window<'a>(wins: &'a [Win], app: &str, title: &str) -> Option<&'a Win> {
    let title = title.trim().to_lowercase();
    let best = wins.iter().map(|w| score(app, &w.app)).max().unwrap_or(0);
    let mut fits: Vec<&Win> = if best >= 45 {
        wins.iter().filter(|w| score(app, &w.app) == best).collect()
    } else {
        // Some systems name the app only in the title ("… - Google Chrome").
        let a = app.trim().to_lowercase();
        wins.iter().filter(|w| !a.is_empty() && w.title.to_lowercase().contains(&a)).collect()
    };
    if !title.is_empty() {
        fits.retain(|w| w.title.to_lowercase().contains(&title));
    }
    fits.iter().find(|w| !w.minimized).or(fits.first()).copied()
}

/// The screen holding most of `r`, by index into `areas`.
fn screen_of(r: Rect, areas: &[Screen]) -> usize {
    areas
        .iter()
        .enumerate()
        .max_by(|(_, a), (_, b)| r.overlap(&a.full).total_cmp(&r.overlap(&b.full)))
        .map(|(i, _)| i)
        .unwrap_or(0)
}

/// A display: its whole frame and the part windows may use (without the
/// menu bar, Dock or taskbar), in window units.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Screen {
    pub full: Rect,
    pub work: Rect,
}

static APP: OnceLock<AppHandle> = OnceLock::new();
static APPS: Mutex<Option<(Instant, Vec<App>)>> = Mutex::new(None);

pub fn init(app: &AppHandle) {
    let _ = APP.set(app.clone());
}

/// The displays, left to right.
fn screens() -> Vec<Screen> {
    let Some(app) = APP.get() else { return Vec::new() };
    let Ok(monitors) = app.available_monitors() else { return Vec::new() };
    let mut out: Vec<Screen> = monitors
        .iter()
        .map(|m| {
            // macOS moves windows in points; elsewhere in pixels.
            let k = if os::POINTS { m.scale_factor() } else { 1.0 };
            let (p, s, wa) = (m.position(), m.size(), m.work_area());
            Screen {
                full: Rect { x: p.x as f64 / k, y: p.y as f64 / k, w: s.width as f64 / k, h: s.height as f64 / k },
                work: Rect {
                    x: wa.position.x as f64 / k,
                    y: wa.position.y as f64 / k,
                    w: wa.size.width as f64 / k,
                    h: wa.size.height as f64 / k,
                },
            }
        })
        .collect();
    out.sort_by(|a, b| a.full.x.total_cmp(&b.full.x).then(a.full.y.total_cmp(&b.full.y)));
    out
}

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> Result<T, String> {
    tokio::task::spawn_blocking(f).await.map_err(|e| e.to_string())
}

/// Installed apps, from the cache when it's fresh.
async fn apps() -> Vec<App> {
    if let Some((at, list)) = APPS.lock().unwrap().as_ref() {
        if at.elapsed() < APPS_FRESH {
            return list.clone();
        }
    }
    let mut list = blocking(os::apps).await.unwrap_or_default();
    list.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    list.dedup_by(|a, b| a.name.eq_ignore_ascii_case(&b.name));
    *APPS.lock().unwrap() = Some((Instant::now(), list.clone()));
    list
}

async fn windows() -> Result<Vec<Win>, String> {
    blocking(os::windows).await?
}

// ---------- Tools ----------

pub fn is_tool(name: &str) -> bool {
    TOOLS.contains(&name)
}

fn def(name: &str, description: &str, props: Value, required: &[&str]) -> ToolDef {
    ToolDef {
        name: name.into(),
        description: description.into(),
        schema: json!({ "type": "object", "properties": props, "required": required }),
    }
}

pub fn defs() -> Vec<ToolDef> {
    let app = json!({ "type": "string", "description": "The app's name as the user said it, e.g. Spotify, Chrome, VS Code." });
    let title = json!({ "type": "string", "description": "Words from the window's title, to pick one of several windows. Optional." });
    let layouts: Vec<&str> = LAYOUTS.iter().map(|(n, _)| *n).collect();
    vec![
        def(
            OPEN_APP,
            "Open (or bring forward) an app installed on this computer, by name. Says so if no such app is installed.",
            json!({ "name": app }),
            &["name"],
        ),
        def(
            LIST_APPS,
            "List the apps installed on this computer, optionally only those whose names contain some text.",
            json!({ "filter": { "type": "string" } }),
            &[],
        ),
        def(
            LIST_WINDOWS,
            "List the open windows (app, title, size, which screen, minimized or full screen), front to back, and the screens.",
            json!({}),
            &[],
        ),
        def(
            ARRANGE,
            "Move, resize or change a window of a running app: snap it to half, a quarter or a third of the screen, \
             center or maximize it, enter or leave full screen, minimize, restore or hide it, and optionally move it to \
             another screen. Call once per window; for \"Spotify right, Chrome left\" make two calls.",
            json!({
                "app": app,
                "layout": { "type": "string", "enum": layouts, "description": "Where or how. keep: same place, for moving to another screen." },
                "screen": { "type": "string", "description": "Optional: next, previous, or a screen number from 1 (left to right)." },
                "title": title,
            }),
            &["app", "layout"],
        ),
        def(
            FOCUS,
            "Bring a window of a running app to the front and focus it (restoring it if minimized). Opens the app if it isn't running.",
            json!({ "app": app, "title": title }),
            &["app"],
        ),
        def(
            QUIT,
            "Quit a running app, as with its Quit menu item (it may ask to save work).",
            json!({ "app": app }),
            &["app"],
        ),
    ]
}

fn s<'a>(args: &'a Value, key: &str) -> &'a str {
    args[key].as_str().unwrap_or("").trim()
}

/// What one call would do, for the approval.
fn describe(tool: &str, args: &Value) -> String {
    match tool {
        OPEN_APP => format!("Open {}", s(args, "name")),
        FOCUS => format!("Bring {} to the front", s(args, "app")),
        QUIT => format!("Quit {}", s(args, "app")),
        ARRANGE => {
            let screen = s(args, "screen");
            format!(
                "Put {} {}{}",
                s(args, "app"),
                s(args, "layout").replace('_', " "),
                if screen.is_empty() { String::new() } else { format!(" on screen {screen}") }
            )
        }
        _ => tool.replace('_', " "),
    }
}

/// Whether a call may go ahead under the user's rules.
pub fn gate(a: &Approvals, tool: &str, args: &Value) -> Gate {
    let (rule, key) = match tool {
        LIST_APPS | LIST_WINDOWS => return Gate::Allow,
        QUIT => (a.quit_apps, "quitApps"),
        _ => (a.apps_and_windows, "appsAndWindows"),
    };
    match rule {
        Rule::Allow => Gate::Allow,
        Rule::Never => Gate::Never("the user turned this off in Settings → Permissions.".into()),
        Rule::Ask => Gate::Ask {
            kind: ActionKind::System,
            source: "Apps and windows".into(),
            summary: describe(tool, args),
            detail: String::new(),
            editable: Vec::new(),
            always: AlwaysAllow::approval(key),
        },
    }
}

fn fmt_rect(r: Rect) -> String {
    format!("{}×{} at {},{}", r.w.round(), r.h.round(), r.x.round(), r.y.round())
}

/// Runs a tool the gate let through. Ok is what happened, Err why not.
pub async fn run(tool: &str, args: &Value) -> Result<String, String> {
    match tool {
        OPEN_APP => open_app(s(args, "name")).await,
        LIST_APPS => {
            let filter = norm(s(args, "filter"));
            let names: Vec<String> = apps()
                .await
                .into_iter()
                .map(|a| a.name)
                .filter(|n| filter.is_empty() || norm(n).contains(&filter))
                .collect();
            Ok(if names.is_empty() {
                "No installed apps match.".into()
            } else {
                format!("{} apps: {}", names.len(), names.join(", "))
            })
        }
        LIST_WINDOWS => {
            let wins = windows().await?;
            let screens = screens();
            let mut out = String::from("Screens (left to right):\n");
            for (i, sc) in screens.iter().enumerate() {
                out += &format!("{}. {}\n", i + 1, fmt_rect(sc.full));
            }
            out += "Windows (front to back):\n";
            for w in &wins {
                let state = if w.minimized { " [minimized]" } else if w.fullscreen { " [full screen]" } else { "" };
                out += &format!(
                    "- {}: \"{}\" {}, screen {}{state}\n",
                    w.app,
                    w.title,
                    fmt_rect(w.frame),
                    screen_of(w.frame, &screens) + 1
                );
            }
            if wins.is_empty() {
                out += "(none)\n";
            }
            Ok(out)
        }
        ARRANGE => arrange(s(args, "app"), s(args, "title"), s(args, "layout"), s(args, "screen")).await,
        FOCUS => {
            let (app, title) = (s(args, "app"), s(args, "title"));
            match find(app, title).await {
                Ok(w) => act(&w, Op::Focus).await.map(|_| format!("{} is in front.", w.app)),
                // Not running: opening it brings it forward.
                Err(_) if title.is_empty() => open_app(app).await,
                Err(e) => Err(e),
            }
        }
        QUIT => {
            let w = find(s(args, "app"), "").await?;
            act(&w, Op::Quit).await.map(|_| format!("Asked {} to quit.", w.app))
        }
        _ => Err(format!("There's no tool called {tool}.")),
    }
}

async fn open_app(name: &str) -> Result<String, String> {
    if name.is_empty() {
        return Err("Say which app to open.".into());
    }
    let list = apps().await;
    let Some(app) = best_app(name, &list).cloned() else {
        let close = near(name, list.iter().map(|a| a.name.clone()));
        return Err(if close.is_empty() {
            format!("There's no app called \"{name}\" on this computer.")
        } else {
            format!("There's no app called \"{name}\" on this computer. Close names: {}.", close.join(", "))
        });
    };
    let name = app.name.clone();
    blocking(move || os::launch(&app)).await??;
    Ok(format!("Opened {name}."))
}

/// The window the user means, or why none fits.
async fn find(app: &str, title: &str) -> Result<Win, String> {
    if app.is_empty() {
        return Err("Say which app's window.".into());
    }
    let wins = windows().await?;
    if let Some(w) = pick_window(&wins, app, title) {
        return Ok(w.clone());
    }
    let mut running: Vec<String> = wins.iter().map(|w| w.app.clone()).collect();
    running.dedup();
    Err(if title.is_empty() {
        format!(
            "{app} has no open window. Open windows belong to: {}. (Use open_app to start it.)",
            if running.is_empty() { "nothing".into() } else { running.join(", ") }
        )
    } else {
        format!("{app} has no window with \"{title}\" in its title.")
    })
}

async fn act(w: &Win, op: Op) -> Result<(), String> {
    let w = w.clone();
    blocking(move || os::act(&w, &op)).await?
}

async fn arrange(app: &str, title: &str, layout: &str, screen: &str) -> Result<String, String> {
    let Some(layout) = parse_layout(if layout.is_empty() && !screen.is_empty() { "keep" } else { layout }) else {
        return Err(format!(
            "Unknown layout \"{layout}\". Use one of: {}.",
            LAYOUTS.iter().map(|(n, _)| *n).collect::<Vec<_>>().join(", ")
        ));
    };
    let w = find(app, title).await?;
    let op = match layout {
        Layout::FullScreen => Some(Op::FullScreen(true)),
        Layout::ExitFullScreen => Some(Op::FullScreen(false)),
        Layout::Minimize => Some(Op::Minimize),
        Layout::Restore => Some(Op::Restore),
        Layout::Hide => Some(Op::Hide),
        _ => None,
    };
    let what = |done: &str| format!("{} {done}.", w.app);
    if let Some(op) = op {
        act(&w, op).await?;
        return Ok(what(match layout {
            Layout::FullScreen => "is full screen",
            Layout::ExitFullScreen => "left full screen",
            Layout::Minimize => "is minimized",
            Layout::Restore => "is restored",
            _ => "is hidden",
        }));
    }
    let screens = screens();
    if screens.is_empty() {
        return Err("Couldn't find the screens.".into());
    }
    let here = screen_of(w.frame, &screens);
    let n = screens.len();
    let to = match screen.to_lowercase().as_str() {
        "" | "same" | "current" | "this" => here,
        "next" | "other" => (here + 1) % n,
        "previous" | "prev" => (here + n - 1) % n,
        "main" | "primary" => 0,
        other => match other.trim_start_matches("screen ").parse::<usize>() {
            Ok(i) if (1..=n).contains(&i) => i - 1,
            _ => return Err(format!("There are {n} screens; say next, previous, or a number from 1 to {n}.")),
        },
    };
    let target = screens[to].work;
    let Some(rect) = frame(layout, target, w.frame, screens[here].work) else {
        return Err("That layout isn't a place on the screen.".into());
    };
    let op = if layout == Layout::Maximize { Op::Maximize(rect) } else { Op::Frame(rect) };
    act(&w, op).await?;
    let on = if n > 1 { format!(" on screen {}", to + 1) } else { String::new() };
    Ok(format!("{} is now {}{on}.", w.app, layout_name(layout)))
}

fn layout_name(l: Layout) -> String {
    LAYOUTS.iter().find(|(_, x)| *x == l).map(|(n, _)| n.replace('_', " ")).unwrap_or_default()
}

/// For the agents' planner and the tool group.
pub fn about() -> &'static str {
    "open apps installed on this computer, and move, resize, snap (left, right, quarters, thirds), center, maximize, \
     full-screen, minimize, hide, focus or quit the windows of running apps"
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app(name: &str) -> App {
        App { name: name.into(), path: PathBuf::from(name) }
    }

    fn win(app: &str, title: &str, minimized: bool) -> Win {
        Win {
            key: String::new(),
            pid: 1,
            app: app.into(),
            title: title.into(),
            frame: Rect { x: 0.0, y: 0.0, w: 800.0, h: 600.0 },
            minimized,
            fullscreen: false,
        }
    }

    #[test]
    fn apps_are_found_by_what_people_call_them() {
        let apps = [
            app("Google Chrome"),
            app("Spotify"),
            app("Visual Studio Code"),
            app("System Settings"),
            app("Microsoft Word"),
            app("Notes"),
        ];
        let find = |q| best_app(q, &apps).map(|a| a.name.as_str());
        assert_eq!(find("spotify"), Some("Spotify"));
        assert_eq!(find("Spotfy"), Some("Spotify"));
        assert_eq!(find("chrome"), Some("Google Chrome"));
        assert_eq!(find("vs code"), Some("Visual Studio Code"));
        assert_eq!(find("settings"), Some("System Settings"));
        assert_eq!(find("word"), Some("Microsoft Word"));
        assert_eq!(find("photoshop"), None);
        assert_eq!(find(""), None);
    }

    #[test]
    fn did_you_mean_lists_close_names() {
        let names = ["Spotify", "Safari", "Slack"].map(String::from);
        assert!(near("spotifi", names.into_iter()).contains(&"Spotify".to_string()));
    }

    #[test]
    fn windows_are_picked_by_app_then_title_front_first() {
        let wins = [
            win("Google Chrome", "Docs", true),
            win("Google Chrome", "Mail", false),
            win("Spotify", "Spotify Premium", false),
        ];
        assert_eq!(pick_window(&wins, "chrome", "").unwrap().title, "Mail");
        assert_eq!(pick_window(&wins, "chrome", "docs").unwrap().title, "Docs");
        assert_eq!(pick_window(&wins, "spotify", "").unwrap().app, "Spotify");
        assert!(pick_window(&wins, "slack", "").is_none());
    }

    #[test]
    fn layouts_split_the_usable_area() {
        let area = Rect { x: 0.0, y: 25.0, w: 1440.0, h: 875.0 };
        let cur = Rect { x: 100.0, y: 100.0, w: 600.0, h: 400.0 };
        let f = |l| frame(l, area, cur, area).unwrap();
        assert_eq!(f(Layout::Left), Rect { x: 0.0, y: 25.0, w: 720.0, h: 875.0 });
        assert_eq!(f(Layout::Right), Rect { x: 720.0, y: 25.0, w: 720.0, h: 875.0 });
        assert_eq!(f(Layout::BottomRight), Rect { x: 720.0, y: 463.0, w: 720.0, h: 438.0 });
        assert_eq!(f(Layout::RightThird).x, 960.0);
        assert_eq!(f(Layout::Maximize), area);
        assert_eq!(f(Layout::Center), Rect { x: 420.0, y: 263.0, w: 600.0, h: 400.0 });
        assert!(frame(Layout::Minimize, area, cur, area).is_none());
        // Moving to a screen twice as wide keeps the same share of it.
        let wide = Rect { x: 1440.0, y: 0.0, w: 2880.0, h: 1800.0 };
        let moved = frame(Layout::Keep, wide, Rect { x: 0.0, y: 25.0, w: 720.0, h: 875.0 }, area).unwrap();
        assert_eq!(moved, Rect { x: 1440.0, y: 0.0, w: 1440.0, h: 1800.0 });
    }

    #[test]
    fn layout_names_are_forgiving() {
        assert_eq!(parse_layout("Left half"), Some(Layout::Left));
        assert_eq!(parse_layout("fullscreen"), Some(Layout::FullScreen));
        assert_eq!(parse_layout("top-right"), Some(Layout::TopRight));
        assert_eq!(parse_layout("maximise"), Some(Layout::Maximize));
        assert_eq!(parse_layout("sideways"), None);
    }

    #[test]
    fn quitting_asks_and_arranging_follows_its_rule() {
        let a = Approvals::default();
        assert!(matches!(gate(&a, ARRANGE, &json!({"app": "Spotify", "layout": "right"})), Gate::Allow));
        assert!(matches!(gate(&a, QUIT, &json!({"app": "Spotify"})),
            Gate::Ask { ref summary, always: Some(_), .. } if summary == "Quit Spotify"));
        let never = Approvals { apps_and_windows: Rule::Never, ..a };
        assert!(matches!(gate(&never, OPEN_APP, &json!({"name": "x"})), Gate::Never(_)));
        assert!(matches!(gate(&never, LIST_WINDOWS, &json!({})), Gate::Allow));
    }

    /// Run by hand: lists this computer's apps and windows.
    #[tokio::test]
    #[ignore]
    async fn this_computers_apps_and_windows() {
        let list = apps().await;
        println!("{} apps; spotify → {:?}; chrome → {:?}", list.len(),
            best_app("spotify", &list).map(|a| &a.path), best_app("chrome", &list).map(|a| &a.path));
        assert!(list.len() > 5);
        match windows().await {
            Ok(w) => w.iter().take(10).for_each(|w| println!("{} | {} | {:?}", w.app, w.title, w.frame)),
            Err(e) => println!("windows: {e}"),
        }
    }
}
