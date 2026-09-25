//! Reminders and calendar events in the OS where it has them (R1):
//! - macOS: the Reminders and Calendar apps, through AppleScript.
//! - Windows: calendar events in Outlook desktop when it's installed.
//! - Everywhere else, reminders are kept by Helpy and shown as a desktop
//!   notification at the time. Calendar events aren't offered there.
//!
//! Values reach the scripts as arguments or environment variables, never
//! spliced into the script text, so a title can't inject script code.

use chrono::{Local, NaiveDateTime, TimeZone};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::agents::runner::ToolOutcome;

/// A reminder Helpy keeps itself, where the OS has no reminders app.
#[derive(Serialize, Deserialize, TS, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct HelpyReminder {
    pub id: String,
    /// Unix milliseconds.
    #[ts(type = "number")]
    pub at: i64,
    pub title: String,
    pub notes: String,
}

/// Whether reminders go to the OS app (true) or Helpy keeps them.
pub fn os_reminders() -> bool {
    cfg!(target_os = "macos")
}

/// Whether calendar events can be created on this computer.
pub fn calendar_supported() -> bool {
    #[cfg(windows)]
    {
        crate::platform::outlook_installed()
    }
    #[cfg(not(windows))]
    {
        cfg!(target_os = "macos")
    }
}

/// "2026-09-26T15:00" (or with a space, or seconds) in local time.
pub fn parse_when(s: &str, now: NaiveDateTime) -> Result<NaiveDateTime, String> {
    let s = s.trim().replace(' ', "T");
    let t = NaiveDateTime::parse_from_str(&s, "%Y-%m-%dT%H:%M")
        .or_else(|_| NaiveDateTime::parse_from_str(&s, "%Y-%m-%dT%H:%M:%S"))
        .map_err(|_| format!("\"{s}\" isn't a date and time like 2026-09-26T15:00."))?;
    if t < now {
        return Err(format!(
            "{t} is in the past; it's {} now.",
            now.format("%Y-%m-%d %H:%M")
        ));
    }
    Ok(t)
}

pub fn local_ms(t: NaiveDateTime) -> i64 {
    Local
        .from_local_datetime(&t)
        .earliest()
        .map(|d| d.timestamp_millis())
        .unwrap_or_else(|| t.and_utc().timestamp_millis())
}

/// Date parts as strings, for AppleScript's `on run argv`.
fn parts(t: NaiveDateTime) -> Vec<String> {
    t.format("%Y %-m %-d %-H %-M")
        .to_string()
        .split(' ')
        .map(String::from)
        .collect()
}

const APPLESCRIPT_DATE: &str = r#"
on mkdate(y, m, d, h, mi)
  set t to current date
  set day of t to 1
  set year of t to y as integer
  set month of t to m as integer
  set day of t to d as integer
  set hours of t to h as integer
  set minutes of t to mi as integer
  set seconds of t to 0
  return t
end mkdate
"#;

async fn osascript(script: &str, args: Vec<String>) -> Result<(), String> {
    let out = tokio::process::Command::new("osascript")
        .arg("-e")
        .arg(script)
        .args(args)
        .output()
        .await
        .map_err(|e| format!("Couldn't run AppleScript: {e}"))?;
    if out.status.success() {
        Ok(())
    } else {
        let err = String::from_utf8_lossy(&out.stderr);
        Err(if err.contains("-1743") || err.contains("not allowed") {
            "macOS didn't let Helpy control that app. Allow it under System Settings → Privacy & Security → Automation.".into()
        } else {
            format!("AppleScript failed: {}", err.trim())
        })
    }
}

/// Creates a reminder in the Reminders app (macOS).
pub async fn os_reminder(title: &str, notes: &str, at: NaiveDateTime) -> ToolOutcome {
    let script = format!(
        "{APPLESCRIPT_DATE}
on run argv
  set d to mkdate(item 3 of argv, item 4 of argv, item 5 of argv, item 6 of argv, item 7 of argv)
  tell application \"Reminders\" to make new reminder with properties {{name:item 1 of argv, body:item 2 of argv, remind me date:d}}
end run"
    );
    let mut args = vec![title.to_string(), notes.to_string()];
    args.extend(parts(at));
    match osascript(&script, args).await {
        Ok(()) => ToolOutcome::Ok {
            text: format!(
                "Added \"{title}\" to Reminders for {}.",
                at.format("%A %-d %B at %H:%M")
            ),
            ops: Vec::new(),
        },
        Err(e) => ToolOutcome::Permanent(e),
    }
}

// Linux has no calendar to put these in.
#[cfg_attr(not(any(target_os = "macos", windows)), allow(dead_code))]
pub struct Event<'a> {
    pub title: &'a str,
    pub start: NaiveDateTime,
    pub end: NaiveDateTime,
    pub location: &'a str,
    pub notes: &'a str,
}

pub async fn create_event(e: Event<'_>) -> ToolOutcome {
    let when = e.start.format("%A %-d %B at %H:%M").to_string();
    #[cfg(target_os = "macos")]
    let result = {
        let script = format!(
            "{APPLESCRIPT_DATE}
on run argv
  set s to mkdate(item 4 of argv, item 5 of argv, item 6 of argv, item 7 of argv, item 8 of argv)
  set f to mkdate(item 9 of argv, item 10 of argv, item 11 of argv, item 12 of argv, item 13 of argv)
  tell application \"Calendar\"
    tell (first calendar whose writable is true)
      make new event with properties {{summary:item 1 of argv, location:item 2 of argv, description:item 3 of argv, start date:s, end date:f}}
    end tell
  end tell
end run"
        );
        let mut args = vec![
            e.title.to_string(),
            e.location.to_string(),
            e.notes.to_string(),
        ];
        args.extend(parts(e.start));
        args.extend(parts(e.end));
        osascript(&script, args).await
    };
    #[cfg(windows)]
    let result = outlook_event(&e).await;
    #[cfg(not(any(target_os = "macos", windows)))]
    let result: Result<(), String> =
        Err("Calendar events aren't available on this computer.".into());
    match result {
        Ok(()) => ToolOutcome::Ok {
            text: format!("Added \"{}\" to the calendar for {when}.", e.title),
            ops: Vec::new(),
        },
        Err(m) => ToolOutcome::Permanent(m),
    }
}

#[cfg(windows)]
async fn outlook_event(e: &Event<'_>) -> Result<(), String> {
    const SCRIPT: &str = "$ErrorActionPreference = 'Stop'; \
        $o = New-Object -ComObject Outlook.Application; $a = $o.CreateItem(1); \
        $f = 'yyyy-MM-dd HH:mm'; $c = [Globalization.CultureInfo]::InvariantCulture; \
        $a.Subject = $env:HELPY_TITLE; $a.Location = $env:HELPY_LOCATION; $a.Body = $env:HELPY_NOTES; \
        $a.Start = [datetime]::ParseExact($env:HELPY_START, $f, $c); \
        $a.End = [datetime]::ParseExact($env:HELPY_END, $f, $c); \
        $a.ReminderSet = $true; $a.ReminderMinutesBeforeStart = 15; $a.Save()";
    let f = "%Y-%m-%d %H:%M";
    let out = tokio::process::Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", SCRIPT])
        .env("HELPY_TITLE", e.title)
        .env("HELPY_LOCATION", e.location)
        .env("HELPY_NOTES", e.notes)
        .env("HELPY_START", e.start.format(f).to_string())
        .env("HELPY_END", e.end.format(f).to_string())
        .output()
        .await
        .map_err(|err| format!("Couldn't start PowerShell: {err}"))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(format!(
            "Outlook couldn't add the event: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn now() -> NaiveDateTime {
        NaiveDateTime::parse_from_str("2026-09-25T10:00", "%Y-%m-%dT%H:%M").unwrap()
    }

    #[test]
    fn reads_times_and_refuses_the_past() {
        let t = parse_when("2026-09-26 15:00", now()).unwrap();
        assert_eq!(parts(t), ["2026", "9", "26", "15", "0"]);
        assert!(parse_when("2026-09-26T15:00:30", now()).is_ok());
        assert!(parse_when("2026-09-24T15:00", now())
            .unwrap_err()
            .contains("past"));
        assert!(parse_when("tomorrow at 3", now()).is_err());
    }
}
