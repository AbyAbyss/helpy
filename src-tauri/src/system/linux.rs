//! Apps from .desktop files, opened with gtk-launch (or their Exec line);
//! windows through wmctrl (and xdotool to minimize), which work on X11.
//! Wayland doesn't let one app move another's windows.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use super::{App, Op, Rect, Win};

/// Windows move in pixels.
pub const POINTS: bool = false;

const NEED_WMCTRL: &str = "Arranging windows needs wmctrl (install it, e.g. `sudo apt install wmctrl`) and an X11 \
                           session; Wayland doesn't let apps move other apps' windows.";

fn dirs() -> Vec<PathBuf> {
    let home = std::env::var("HOME").map(PathBuf::from).unwrap_or_default();
    let mut out = vec![home.join(".local/share/applications"), home.join(".local/share/flatpak/exports/share/applications")];
    let data = std::env::var("XDG_DATA_DIRS").unwrap_or_else(|_| "/usr/local/share:/usr/share".into());
    out.extend(data.split(':').filter(|d| !d.is_empty()).map(|d| Path::new(d).join("applications")));
    out.push("/var/lib/flatpak/exports/share/applications".into());
    out.push("/var/lib/snapd/desktop/applications".into());
    out
}

/// The name of an app a .desktop file shows in menus, if it's shown.
fn entry_name(text: &str) -> Option<String> {
    let mut in_entry = false;
    let mut name = None;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_entry = line == "[Desktop Entry]";
            continue;
        }
        if !in_entry {
            continue;
        }
        match line.split_once('=') {
            Some(("Name", v)) if name.is_none() => name = Some(v.trim().to_string()),
            Some(("NoDisplay", v)) | Some(("Hidden", v)) if v.trim() == "true" => return None,
            Some(("Type", v)) if v.trim() != "Application" => return None,
            _ => {}
        }
    }
    name
}

pub fn apps() -> Vec<App> {
    let mut out: Vec<App> = Vec::new();
    for dir in dirs() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for e in entries.flatten() {
            let path = e.path();
            if path.extension().is_none_or(|x| x != "desktop") {
                continue;
            }
            // The first folder with an id wins, as menus do.
            if out.iter().any(|a| a.path.file_name() == path.file_name()) {
                continue;
            }
            if let Some(name) = std::fs::read_to_string(&path).ok().as_deref().and_then(entry_name) {
                out.push(App { name, path });
            }
        }
    }
    out
}

fn has(program: &str) -> bool {
    Command::new("sh")
        .args(["-c", &format!("command -v {program}")])
        .stdout(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

/// The Exec line without its field codes (%f, %U…).
fn exec_line(text: &str) -> Option<String> {
    let line = text.lines().find_map(|l| l.trim().strip_prefix("Exec="))?;
    let words: Vec<&str> = line.split_whitespace().filter(|w| !(w.len() == 2 && w.starts_with('%'))).collect();
    (!words.is_empty()).then(|| words.join(" "))
}

pub fn launch(app: &App) -> Result<(), String> {
    let id = app.path.file_stem().and_then(|s| s.to_str()).unwrap_or_default();
    let spawned = if has("gtk-launch") {
        Command::new("gtk-launch").arg(id).stdout(Stdio::null()).stderr(Stdio::null()).spawn()
    } else {
        let text = std::fs::read_to_string(&app.path).map_err(|e| e.to_string())?;
        let exec = exec_line(&text).ok_or_else(|| format!("{} has no command to start it.", app.name))?;
        Command::new("sh").args(["-c", &exec]).stdout(Stdio::null()).stderr(Stdio::null()).spawn()
    };
    spawned.map(|_| ()).map_err(|e| format!("Couldn't open {}: {e}", app.name))
}

fn run(program: &str, args: &[&str]) -> Result<String, String> {
    let out = Command::new(program).args(args).output().map_err(|_| NEED_WMCTRL.to_string())?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

/// The app's name from WM_CLASS ("spotify.Spotify" → "Spotify").
fn class_name(class: &str) -> String {
    class.rsplit('.').next().unwrap_or(class).to_string()
}

pub fn windows() -> Result<Vec<Win>, String> {
    if std::env::var("WAYLAND_DISPLAY").is_ok() && std::env::var("DISPLAY").is_err() {
        return Err(NEED_WMCTRL.into());
    }
    let own = std::process::id();
    let text = run("wmctrl", &["-lpGx"])?;
    // Stacking order comes back to front; wmctrl lists oldest first.
    let mut out: Vec<Win> = text
        .lines()
        .filter_map(|l| {
            let mut f = l.split_whitespace();
            let key = f.next()?.to_string();
            let desktop: i64 = f.next()?.parse().ok()?;
            let pid: u32 = f.next()?.parse().ok()?;
            let nums: Vec<f64> = (0..4).filter_map(|_| f.next()?.parse().ok()).collect();
            let class = f.next()?;
            let _host = f.next()?;
            let title: Vec<&str> = f.collect();
            (desktop >= 0 && pid != own && nums.len() == 4).then(|| Win {
                key,
                pid,
                app: class_name(class),
                title: title.join(" "),
                frame: Rect { x: nums[0], y: nums[1], w: nums[2], h: nums[3] },
                minimized: false,
                fullscreen: false,
            })
        })
        .collect();
    out.reverse();
    Ok(out)
}

fn wm(key: &str, args: &[&str]) -> Result<(), String> {
    let mut all = vec!["-i", "-r", key];
    all.extend_from_slice(args);
    run("wmctrl", &all).map(|_| ())
}

pub fn act(w: &Win, op: &Op) -> Result<(), String> {
    let k = w.key.as_str();
    match op {
        Op::Frame(r) => {
            wm(k, &["-b", "remove,maximized_vert,maximized_horz,fullscreen"])?;
            let geometry = format!("0,{},{},{},{}", r.x as i64, r.y as i64, r.w as i64, r.h as i64);
            wm(k, &["-e", &geometry])?;
            run("wmctrl", &["-i", "-a", k]).map(|_| ())
        }
        Op::Maximize(r) => {
            let geometry = format!("0,{},{},{},{}", r.x as i64, r.y as i64, r.w as i64, r.h as i64);
            wm(k, &["-e", &geometry])?;
            wm(k, &["-b", "add,maximized_vert,maximized_horz"])?;
            run("wmctrl", &["-i", "-a", k]).map(|_| ())
        }
        Op::FullScreen(on) => wm(k, &["-b", if *on { "add,fullscreen" } else { "remove,fullscreen" }]),
        Op::Restore | Op::Focus => run("wmctrl", &["-i", "-a", k]).map(|_| ()),
        Op::Minimize | Op::Hide => {
            let id = i64::from_str_radix(k.trim_start_matches("0x"), 16).map_err(|e| e.to_string())?;
            run("xdotool", &["windowminimize", &id.to_string()])
                .map(|_| ())
                .map_err(|_| "Minimizing needs xdotool (e.g. `sudo apt install xdotool`).".into())
        }
        Op::Quit => run("wmctrl", &["-i", "-c", k]).map(|_| ()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn desktop_files_are_read_like_menus_read_them() {
        let shown = "[Desktop Entry]\nType=Application\nName=Spotify\nExec=spotify %U\n[Desktop Action New]\nName=Other\n";
        assert_eq!(entry_name(shown).as_deref(), Some("Spotify"));
        assert_eq!(exec_line(shown).as_deref(), Some("spotify"));
        assert_eq!(entry_name("[Desktop Entry]\nName=X\nNoDisplay=true\n"), None);
        assert_eq!(class_name("spotify.Spotify"), "Spotify");
    }
}
