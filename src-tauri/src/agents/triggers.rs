//! Triggers: agents that start by themselves, on a schedule or when new
//! files arrive in a folder. Every trigger is capped at 10 runs an hour and
//! pauses itself after 3 failures in a row. They only run while Helpy is
//! running; runs missed while it was closed aren't made up.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use chrono::{DateTime, Local};
use notify::{EventKind, RecursiveMode, Watcher};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};
use ts_rs::TS;

use super::model::{RunMode, Status};
use super::planner::PlanAgent;
use super::tools::files::expand_home;
use super::{AgentsState, NewAgent};
use crate::settings::schema::{Trigger, TriggerWhen};
use crate::settings::{Settings, SettingsStore};

pub const CHANGED_EVENT: &str = "triggers://changed";
pub const MAX_RUNS_PER_HOUR: usize = 10;
pub const MAX_FAILURES: u32 = 3;
/// A schedule may not run more often than this.
pub const MIN_INTERVAL_MINUTES: i64 = 10;
/// New files are gathered until the folder has been quiet this long.
const SETTLE: Duration = Duration::from_secs(5);
/// Changes this soon after the trigger's own run are its agents' doing.
const OWN_CHANGES_MS: i64 = 10_000;
const TICK: Duration = Duration::from_secs(2);

/// What happened with a trigger, kept across restarts (not a setting).
#[derive(Serialize, Deserialize, TS, Clone, Debug, Default, PartialEq)]
#[serde(default, rename_all = "camelCase")]
#[ts(export)]
pub struct Runtime {
    /// Start times in the last hour, for the cap.
    #[ts(type = "Array<number>")]
    pub runs: Vec<i64>,
    /// Failures in a row.
    pub failures: u32,
    /// Why it paused itself, until the user resumes it.
    pub paused: Option<String>,
    #[ts(type = "number | null")]
    pub last_run: Option<i64>,
    /// "Finished", "Failed: …", "Skipped: …".
    pub last_outcome: Option<String>,
    /// The batch it started that hasn't finished yet.
    pub open_batch: Option<String>,
}

#[derive(Serialize, TS, Clone, Debug)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct TriggerStatus {
    pub id: String,
    pub runtime: Runtime,
    #[ts(type = "number | null")]
    pub next_run: Option<i64>,
    /// Something wrong with it right now (a folder that can't be watched).
    pub problem: Option<String>,
}

#[derive(Default)]
pub struct TriggersState {
    path: Option<PathBuf>,
    runtime: Mutex<HashMap<String, Runtime>>,
    watchers: Mutex<HashMap<String, (String, notify::RecommendedWatcher)>>,
    problems: Mutex<HashMap<String, String>>,
    /// New files per folder trigger, and when the last one arrived.
    pending: Mutex<HashMap<String, (Vec<PathBuf>, Instant)>>,
    /// When each trigger's last run ended, to ignore its own changes.
    ended: Mutex<HashMap<String, i64>>,
}

impl TriggersState {
    pub fn load(data_dir: PathBuf) -> Self {
        let path = data_dir.join("triggers.json");
        let runtime = std::fs::read_to_string(&path)
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default();
        Self {
            path: Some(path),
            runtime: Mutex::new(runtime),
            ..Default::default()
        }
    }

    fn save(&self) {
        let map = self.runtime.lock().unwrap().clone();
        if let (Some(p), Ok(t)) = (&self.path, serde_json::to_string(&map)) {
            let _ = std::fs::write(p, t);
        }
    }

    fn update<T>(&self, id: &str, f: impl FnOnce(&mut Runtime) -> T) -> T {
        let out = f(self
            .runtime
            .lock()
            .unwrap()
            .entry(id.to_string())
            .or_default());
        self.save();
        out
    }
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

// ---------- Rules ----------

/// A standard 5-field cron expression (minute hour day month weekday).
pub fn parse_cron(expr: &str) -> Result<cron::Schedule, String> {
    let fields = expr.split_whitespace().count();
    if fields != 5 {
        return Err("A schedule has 5 parts: minute, hour, day of month, month and weekday".into());
    }
    let mut parts: Vec<String> = expr.split_whitespace().map(String::from).collect();
    parts[4] = weekday_names(&parts[4])?;
    cron::Schedule::from_str(&format!("0 {}", parts.join(" ")))
        .map_err(|e| format!("That schedule isn't valid: {e}"))
}

/// Standard cron counts weekdays from 0 (Sunday; 7 is Sunday too), the cron
/// crate from 1. Names mean the same to both, so numbers become names.
fn weekday_names(field: &str) -> Result<String, String> {
    const DAYS: [&str; 8] = ["SUN", "MON", "TUE", "WED", "THU", "FRI", "SAT", "SUN"];
    let name = |v: &str| -> Result<String, String> {
        match v.parse::<usize>() {
            Ok(n) if n < DAYS.len() => Ok(DAYS[n].to_string()),
            Ok(_) => Err("Weekdays go from 0 (Sunday) to 6 (Saturday)".to_string()),
            Err(_) => Ok(v.to_string()),
        }
    };
    field
        .split(',')
        .map(|part| {
            let (range, step) = match part.split_once('/') {
                Some((r, st)) => (r, Some(st)),
                None => (part, None),
            };
            let range = match range.split_once('-') {
                Some((a, b)) => format!("{}-{}", name(a)?, name(b)?),
                None => name(range)?,
            };
            Ok(match step {
                Some(st) => format!("{range}/{st}"),
                None => range,
            })
        })
        .collect::<Result<Vec<_>, String>>()
        .map(|v| v.join(","))
}

pub fn next_after(expr: &str, from: DateTime<Local>) -> Option<DateTime<Local>> {
    parse_cron(expr).ok()?.after(&from).next()
}

/// A schedule's problem, if it has one: invalid, or more often than
/// every 10 minutes.
pub fn check_schedule(expr: &str) -> Result<(), String> {
    let s = parse_cron(expr)?;
    let times: Vec<_> = s.after(&Local::now()).take(30).collect();
    if times.is_empty() {
        return Err("That schedule never runs".into());
    }
    if times
        .windows(2)
        .any(|w| (w[1] - w[0]).num_minutes() < MIN_INTERVAL_MINUTES)
    {
        return Err(format!(
            "A trigger can run at most every {MIN_INTERVAL_MINUTES} minutes"
        ));
    }
    Ok(())
}

/// Whether the schedule has a run in (after, now].
pub fn due(expr: &str, after: DateTime<Local>, now: DateTime<Local>) -> bool {
    next_after(expr, after).is_some_and(|t| t <= now)
}

/// Whether a trigger may run now; the error says why not.
pub fn admit(r: &mut Runtime, now: i64) -> Result<(), String> {
    if let Some(why) = &r.paused {
        return Err(why.clone());
    }
    if r.open_batch.is_some() {
        return Err("its last run is still going".into());
    }
    r.runs.retain(|&t| now - t < 3_600_000);
    if r.runs.len() >= MAX_RUNS_PER_HOUR {
        return Err(format!(
            "it already ran {MAX_RUNS_PER_HOUR} times in the last hour"
        ));
    }
    Ok(())
}

/// Records how a run ended; the third failure in a row pauses it.
pub fn record(r: &mut Runtime, ok: bool, message: &str) {
    r.open_batch = None;
    if ok {
        r.failures = 0;
        r.last_outcome = Some("Finished".into());
    } else {
        r.failures += 1;
        r.last_outcome = Some(format!("Failed: {message}"));
        if r.failures >= MAX_FAILURES {
            r.paused = Some(format!(
                "Paused after {MAX_FAILURES} failures in a row. Last: {message}"
            ));
        }
    }
}

/// Files a folder trigger reacts to: not hidden, not a download still in
/// progress, and matching the pattern ("*.pdf, *.png"; empty is any).
pub fn wanted(name: &str, pattern: &str) -> bool {
    let lower = name.to_lowercase();
    if lower.starts_with('.') || lower.starts_with("~$") {
        return false;
    }
    if [
        ".crdownload",
        ".part",
        ".partial",
        ".download",
        ".tmp",
        ".opdownload",
    ]
    .iter()
    .any(|e| lower.ends_with(e))
    {
        return false;
    }
    let pats: Vec<&str> = pattern
        .split([',', ' ', ';'])
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .collect();
    pats.is_empty() || pats.iter().any(|p| glob(&p.to_lowercase(), &lower))
}

/// "*" and "?" wildcards.
fn glob(pat: &str, text: &str) -> bool {
    fn go(p: &[char], t: &[char]) -> bool {
        match (p.first(), t.first()) {
            (None, None) => true,
            (Some('*'), _) => go(&p[1..], t) || (!t.is_empty() && go(p, &t[1..])),
            (Some('?'), Some(_)) => go(&p[1..], &t[1..]),
            (Some(a), Some(b)) if a == b => go(&p[1..], &t[1..]),
            _ => false,
        }
    }
    go(
        &pat.chars().collect::<Vec<_>>(),
        &text.chars().collect::<Vec<_>>(),
    )
}

// ---------- Running ----------

/// The agents a trigger starts, with the folders they need.
fn agents_for(s: &Settings, t: &Trigger) -> Result<(RunMode, Vec<PlanAgent>, Vec<String>), String> {
    if t.template.is_empty() {
        if t.goal.trim().is_empty() {
            return Err("it has no task".into());
        }
        let agent = PlanAgent {
            name: t.name.clone(),
            goal: t.goal.clone(),
            tools: t.tools.clone(),
            keep_open: false,
            after: Vec::new(),
        };
        return Ok((RunMode::Single, vec![agent], Vec::new()));
    }
    let template = super::templates::all(s)
        .into_iter()
        .find(|x| x.id == t.template)
        .ok_or("its template was deleted")?;
    let values = t
        .values
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    super::templates::build(&template, &values)
}

fn approved(folder: &str, s: &Settings) -> bool {
    let f = expand_home(folder);
    s.agents
        .approved_folders
        .iter()
        .any(|a| f.starts_with(expand_home(a)))
}

/// Starts a trigger's agents now, if its limits allow. `files` are the new
/// files for a folder trigger.
pub fn fire(app: &AppHandle, id: &str, files: &[PathBuf]) -> Result<(), String> {
    let s = app.state::<SettingsStore>().get();
    let state = app.state::<TriggersState>();
    let t = s
        .agents
        .triggers
        .iter()
        .find(|t| t.id == id)
        .cloned()
        .ok_or("That trigger is gone")?;
    let now = now_ms();
    let skipped = |why: String| {
        state.update(id, |r| r.last_outcome = Some(format!("Skipped: {why}")));
        let _ = app.emit(CHANGED_EVENT, ());
        Err(format!("{} didn't run: {why}", t.name))
    };
    if let Err(why) = state.update(id, |r| admit(r, now)) {
        return skipped(why);
    }
    let (mode, mut agents, folders) = match agents_for(&s, &t) {
        Ok(x) => x,
        Err(e) => return skipped(e),
    };
    if let Some(f) = folders.iter().find(|f| !approved(f, &s)) {
        return skipped(format!("{f} isn't one of the folders agents may use"));
    }
    if !files.is_empty() {
        let list: Vec<String> = files.iter().map(|f| f.display().to_string()).collect();
        for a in agents.iter_mut().filter(|a| a.after.is_empty()) {
            a.goal += &format!("\n\nNew files that started this run: {}", list.join(", "));
        }
    }
    let names: Vec<String> = agents.iter().map(|a| a.name.clone()).collect();
    let agents = agents
        .into_iter()
        .map(|a| NewAgent {
            after: a
                .after
                .iter()
                .filter_map(|n| names.iter().position(|x| x == n))
                .collect(),
            name: a.name,
            goal: a.goal,
            tools: a.tools,
            keep_open: false,
        })
        .collect();
    let batch = super::create_batch(
        app,
        &format!("{} (automatic)", t.name),
        mode,
        agents,
        None,
        Some(t.id.clone()),
    );
    state.update(id, |r| {
        r.runs.push(now);
        r.last_run = Some(now);
        r.open_batch = Some(batch);
        r.last_outcome = Some("Running".into());
    });
    let _ = app.emit(CHANGED_EVENT, ());
    Ok(())
}

/// Called when an agent of a trigger's batch finishes: once all have, the
/// run counts as finished or failed.
pub fn batch_changed(app: &AppHandle, trigger: &str, batch: &str) {
    let agents = app.state::<AgentsState>();
    let list: Vec<(Status, String)> = agents
        .agents
        .lock()
        .unwrap()
        .values()
        .filter(|a| a.batch == batch && a.parent.is_none())
        .map(|a| {
            (
                a.status,
                a.error
                    .clone()
                    .or(a.stop.as_ref().map(|s| s.message.clone()))
                    .unwrap_or_default(),
            )
        })
        .collect();
    if list
        .iter()
        .any(|(s, _)| !s.is_finished() && *s != Status::Ready)
    {
        return;
    }
    let state = app.state::<TriggersState>();
    if state
        .runtime
        .lock()
        .unwrap()
        .get(trigger)
        .and_then(|r| r.open_batch.as_deref())
        != Some(batch)
    {
        return;
    }
    let failed = list
        .iter()
        .find(|(s, _)| !matches!(s, Status::Done | Status::Ready));
    state.update(trigger, |r| match failed {
        None => record(r, true, ""),
        Some((Status::Cancelled, _)) => record(r, false, "cancelled"),
        Some((_, why)) => record(
            r,
            false,
            if why.is_empty() {
                "an agent didn't finish"
            } else {
                why
            },
        ),
    });
    state
        .ended
        .lock()
        .unwrap()
        .insert(trigger.to_string(), now_ms());
    let _ = app.emit(CHANGED_EVENT, ());
}

// ---------- Watching ----------

/// Starts and stops folder watchers to match the settings.
pub fn sync(app: &AppHandle, s: &Settings) {
    let state = app.state::<TriggersState>();
    let mut watchers = state.watchers.lock().unwrap();
    let wanted: HashMap<String, (String, String)> = s
        .agents
        .triggers
        .iter()
        .filter(|t| t.enabled)
        .filter_map(|t| match &t.when {
            TriggerWhen::Folder { path, pattern } => {
                Some((t.id.clone(), (path.clone(), pattern.clone())))
            }
            _ => None,
        })
        .collect();
    watchers.retain(|id, (key, _)| {
        wanted
            .get(id)
            .is_some_and(|(p, pat)| &format!("{p}\u{0}{pat}") == key)
    });
    for (id, (path, pattern)) in wanted {
        if watchers.contains_key(&id) {
            continue;
        }
        let folder = expand_home(&path);
        match watch(app, &id, &folder, &pattern) {
            Ok(w) => {
                watchers.insert(id.clone(), (format!("{path}\u{0}{pattern}"), w));
                state.problems.lock().unwrap().remove(&id);
            }
            Err(e) => {
                state
                    .problems
                    .lock()
                    .unwrap()
                    .insert(id, format!("Can't watch {}: {e}", folder.display()));
            }
        }
    }
    let _ = app.emit(CHANGED_EVENT, ());
}

fn watch(
    app: &AppHandle,
    id: &str,
    folder: &Path,
    pattern: &str,
) -> Result<notify::RecommendedWatcher, String> {
    let app = app.clone();
    let id = id.to_string();
    let top = folder.to_path_buf();
    let pattern = pattern.to_string();
    let mut w = notify::recommended_watcher(move |ev: notify::Result<notify::Event>| {
        let Ok(ev) = ev else { return };
        let arrived = matches!(ev.kind, EventKind::Create(_))
            || matches!(
                ev.kind,
                EventKind::Modify(notify::event::ModifyKind::Name(_))
            );
        if !arrived {
            return;
        }
        let state = app.state::<TriggersState>();
        // The trigger's own agents moving files around aren't new files.
        let busy = state
            .runtime
            .lock()
            .unwrap()
            .get(&id)
            .is_some_and(|r| r.open_batch.is_some());
        let recent = state
            .ended
            .lock()
            .unwrap()
            .get(&id)
            .is_some_and(|&t| now_ms() - t < OWN_CHANGES_MS);
        if busy || recent {
            return;
        }
        let files: Vec<PathBuf> = ev
            .paths
            .into_iter()
            .filter(|p| p.parent() == Some(top.as_path()) && p.is_file())
            .filter(|p| {
                p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| wanted(n, &pattern))
            })
            .collect();
        if files.is_empty() {
            return;
        }
        let mut pending = state.pending.lock().unwrap();
        let entry = pending
            .entry(id.clone())
            .or_insert_with(|| (Vec::new(), Instant::now()));
        for f in files {
            if !entry.0.contains(&f) {
                entry.0.push(f);
            }
        }
        entry.1 = Instant::now();
    })
    .map_err(|e| e.to_string())?;
    w.watch(folder, RecursiveMode::NonRecursive)
        .map_err(|e| e.to_string())?;
    Ok(w)
}

/// Runs schedules when they're due and folder triggers once their folder
/// has settled.
pub fn setup(app: &AppHandle) {
    sync(app, &app.state::<SettingsStore>().get());
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let mut last = Local::now();
        loop {
            tokio::time::sleep(TICK).await;
            let now = Local::now();
            let s = app.state::<SettingsStore>().get();
            for t in s.agents.triggers.iter().filter(|t| t.enabled) {
                if let TriggerWhen::Schedule { cron } = &t.when {
                    if due(cron, last, now) {
                        if let Err(e) = fire(&app, &t.id, &[]) {
                            log::info!("trigger: {e}");
                        }
                    }
                }
            }
            last = now;
            let settled: Vec<(String, Vec<PathBuf>)> = {
                let state = app.state::<TriggersState>();
                let mut pending = state.pending.lock().unwrap();
                let ids: Vec<String> = pending
                    .iter()
                    .filter(|(_, (_, at))| at.elapsed() >= SETTLE)
                    .map(|(id, _)| id.clone())
                    .collect();
                ids.into_iter()
                    .filter_map(|id| pending.remove(&id).map(|(files, _)| (id, files)))
                    .collect()
            };
            for (id, files) in settled {
                if let Err(e) = fire(&app, &id, &files) {
                    log::info!("trigger: {e}");
                }
            }
        }
    });
}

// ---------- Commands ----------

#[tauri::command]
pub fn agents_triggers_status(app: AppHandle) -> Vec<TriggerStatus> {
    let s = app.state::<SettingsStore>().get();
    let state = app.state::<TriggersState>();
    let runtime = state.runtime.lock().unwrap();
    let problems = state.problems.lock().unwrap();
    s.agents
        .triggers
        .iter()
        .map(|t| TriggerStatus {
            id: t.id.clone(),
            runtime: runtime.get(&t.id).cloned().unwrap_or_default(),
            next_run: match (&t.when, t.enabled) {
                (TriggerWhen::Schedule { cron }, true) => {
                    next_after(cron, Local::now()).map(|d| d.timestamp_millis())
                }
                _ => None,
            },
            problem: problems.get(&t.id).cloned(),
        })
        .collect()
}

#[tauri::command]
pub fn agents_trigger_run_now(app: AppHandle, id: String) -> Result<(), String> {
    fire(&app, &id, &[])
}

/// Clears a pause after failures.
#[tauri::command]
pub fn agents_trigger_resume(app: AppHandle, id: String) {
    app.state::<TriggersState>().update(&id, |r| {
        r.paused = None;
        r.failures = 0;
    });
    let _ = app.emit(CHANGED_EVENT, ());
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn schedules_parse_and_fire_once_per_time() {
        assert!(parse_cron("0 9 * * 1-5").is_ok());
        assert!(parse_cron("0 9 * *").is_err());
        let at = |h, m| Local.with_ymd_and_hms(2026, 9, 25, h, m, 0).unwrap();
        // Friday 25 September 2026.
        assert!(due("0 9 * * 1-5", at(8, 59), at(9, 0)));
        assert!(!due("0 9 * * 1-5", at(9, 0), at(9, 2)));
        assert!(!due("0 9 * * 0,6", at(8, 59), at(9, 1)));
        // Weekends only: Sunday is 0 (and 7), as in standard cron.
        let sat = Local.with_ymd_and_hms(2026, 9, 26, 8, 59, 0).unwrap();
        let two = chrono::Duration::minutes(2);
        assert!(due("0 9 * * 0,6", sat, sat + two));
        let sun = sat + chrono::Duration::days(1);
        assert!(due("0 9 * * 7", sun, sun + two));
        assert_eq!(weekday_names("1-5,0/2").unwrap(), "MON-FRI,SUN/2");
        assert!(check_schedule("*/5 * * * *")
            .unwrap_err()
            .contains("10 minutes"));
        assert!(check_schedule("*/15 * * * *").is_ok());
    }

    #[test]
    fn at_most_ten_runs_an_hour_and_pauses_after_three_failures() {
        let mut r = Runtime::default();
        for i in 0..10 {
            admit(&mut r, i * 1000).unwrap();
            r.runs.push(i * 1000);
        }
        assert!(admit(&mut r, 20_000).unwrap_err().contains("10 times"));
        // An hour after the first run, there's room again.
        assert!(admit(&mut r, 3_600_000 + 500).is_ok());

        let mut r = Runtime::default();
        record(&mut r, false, "boom");
        record(&mut r, false, "boom");
        record(&mut r, true, "");
        assert_eq!(r.failures, 0);
        for _ in 0..3 {
            record(&mut r, false, "no network");
        }
        assert!(r.paused.as_ref().unwrap().contains("3 failures"));
        assert!(admit(&mut r, 0).is_err());
        r.open_batch = Some("b".into());
        r.paused = None;
        assert!(admit(&mut r, 0).unwrap_err().contains("still going"));
    }

    #[test]
    fn folder_triggers_skip_partial_and_hidden_files() {
        assert!(wanted("report.pdf", ""));
        assert!(wanted("Report.PDF", "*.pdf, *.png"));
        assert!(!wanted("photo.jpg", "*.pdf"));
        assert!(!wanted("movie.mp4.crdownload", ""));
        assert!(!wanted(".DS_Store", ""));
        assert!(!wanted("~$budget.xlsx", ""));
        assert!(wanted("invoice-2026.pdf", "invoice-*"));
    }
}
