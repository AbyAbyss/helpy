//! Moving Helpy into the Applications folder when it was started from the
//! disk image or the Downloads folder (macOS). The welcome tour offers it.

use std::path::{Path, PathBuf};

use serde::Serialize;
use ts_rs::TS;

#[derive(Serialize, TS, Clone, Debug)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct InstallInfo {
    /// The app isn't in an Applications folder yet, and could be moved there.
    pub can_move: bool,
    /// The folder it runs from now.
    pub from: String,
}

/// The .app bundle this process runs from: …/Helpy.app/Contents/MacOS/helpy.
fn bundle_of(exe: &Path) -> Option<PathBuf> {
    let app = exe.parent()?.parent()?.parent()?;
    app.extension()
        .is_some_and(|e| e == "app")
        .then(|| app.to_path_buf())
}

fn in_applications(app: &Path) -> bool {
    app.parent()
        .and_then(Path::file_name)
        .is_some_and(|n| n == "Applications")
}

/// The disk image volume an app runs from, if any. A quarantined app opened
/// from a disk image runs from a translocated copy, so the volume is found
/// by name in that case.
fn volume_of(app: &Path) -> Option<PathBuf> {
    if app.starts_with("/Volumes") {
        return Some(app.components().take(3).collect());
    }
    if !app.to_string_lossy().contains("/AppTranslocation/") {
        return None;
    }
    let name = app.file_stem()?.to_string_lossy().to_string();
    std::fs::read_dir("/Volumes")
        .ok()?
        .flatten()
        .map(|e| e.path())
        .find(|p| {
            p.file_name()
                .is_some_and(|n| n.to_string_lossy().starts_with(&name))
        })
}

#[tauri::command]
pub fn install_info() -> InstallInfo {
    if cfg!(target_os = "macos") {
        if let Some(app) = std::env::current_exe().ok().and_then(|e| bundle_of(&e)) {
            return InstallInfo {
                can_move: !in_applications(&app),
                from: app
                    .parent()
                    .map(|p| p.display().to_string())
                    .unwrap_or_default(),
            };
        }
    }
    InstallInfo {
        can_move: false,
        from: String::new(),
    }
}

/// Copies the bundle into /Applications (or ~/Applications when that isn't
/// allowed), starts the copy, ejects the disk image it came from, and quits.
#[tauri::command]
pub fn install_move(app: tauri::AppHandle) -> Result<(), String> {
    use std::process::Command;
    if !cfg!(target_os = "macos") {
        return Err("Only on macOS.".into());
    }
    let src = std::env::current_exe()
        .ok()
        .and_then(|e| bundle_of(&e))
        .ok_or("Helpy isn't running from an app bundle.")?;
    let name = src.file_name().ok_or("The bundle has no name.")?.to_owned();
    let mut dest = None;
    for dir in [
        PathBuf::from("/Applications"),
        crate::agents::tools::files::expand_home("~/Applications"),
    ] {
        let target = dir.join(&name);
        if target == src {
            continue;
        }
        if target.exists() {
            // An older copy: quit it if it's running, and put it in the Trash.
            let _ = Command::new("pkill")
                .args(["-f", &format!("{}/Contents/MacOS/", target.display())])
                .status();
            let trash = crate::agents::tools::files::expand_home("~/.Trash");
            let mut bin = trash.join(&name);
            if bin.exists() {
                bin = trash.join(format!(
                    "{} {}.app",
                    src.file_stem().unwrap_or_default().to_string_lossy(),
                    chrono::Local::now().format("%H.%M.%S")
                ));
            }
            if std::fs::rename(&target, &bin).is_err() {
                continue;
            }
        }
        let _ = std::fs::create_dir_all(&dir);
        // ditto keeps the signature, permissions and attributes intact.
        let copied = Command::new("ditto")
            .arg(&src)
            .arg(&target)
            .status()
            .map_err(|e| format!("Couldn't run ditto: {e}"))?
            .success();
        if copied {
            dest = Some(target);
            break;
        }
    }
    let dest = dest.ok_or("Couldn't copy Helpy into /Applications or ~/Applications.")?;
    // Start the copy once this one has quit, then eject the disk image.
    let mut script = format!("sleep 1.5; open -n \"{}\"", dest.display());
    if let Some(volume) = volume_of(&src) {
        script += &format!("; sleep 4; hdiutil detach \"{}\" -quiet", volume.display());
    }
    Command::new("sh")
        .args(["-c", &script])
        .spawn()
        .map_err(|e| format!("Couldn't start the moved copy: {e}"))?;
    app.exit(0);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_bundle_and_where_it_lives() {
        let exe = Path::new("/Volumes/Helpy/Helpy.app/Contents/MacOS/helpy");
        let app = bundle_of(exe).unwrap();
        assert_eq!(app, Path::new("/Volumes/Helpy/Helpy.app"));
        assert!(!in_applications(&app));
        assert_eq!(volume_of(&app).unwrap(), Path::new("/Volumes/Helpy"));
        assert!(in_applications(Path::new("/Applications/Helpy.app")));
        assert!(in_applications(Path::new("/Users/x/Applications/Helpy.app")));
        assert_eq!(volume_of(Path::new("/Users/x/Downloads/Helpy.app")), None);
        assert_eq!(bundle_of(Path::new("/usr/local/bin/helpy")), None);
    }
}
