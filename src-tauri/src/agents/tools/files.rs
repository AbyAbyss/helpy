//! Files inside the folders the user approved, and nowhere else. Every
//! change is recorded with a backup of what it replaced, so a whole run can
//! be undone in one go.

use std::fs;
use std::path::{Component, Path, PathBuf};

use crate::agents::model::FileOp;

/// Read at most this much of a file.
const MAX_READ: u64 = 200_000;
const MAX_LIST: usize = 500;

pub struct Files {
    /// Approved folders, resolved (no symlinks, no "..").
    roots: Vec<PathBuf>,
    /// Where backups of changed files go, per agent.
    backups: PathBuf,
}

/// "~/x" and "~" → the home folder.
pub fn expand_home(p: &str) -> PathBuf {
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from);
    match (p.strip_prefix("~"), home) {
        (Some(rest), Some(h)) if rest.is_empty() || rest.starts_with('/') || rest.starts_with('\\') => {
            h.join(rest.trim_start_matches(['/', '\\']))
        }
        _ => PathBuf::from(p),
    }
}

/// Removes "." and ".." without touching the disk.
fn normalize(p: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other),
        }
    }
    out
}

impl Files {
    pub fn new(folders: &[String], backups: PathBuf) -> Self {
        let roots = folders
            .iter()
            .filter_map(|f| fs::canonicalize(expand_home(f)).ok())
            .collect();
        Self { roots, backups }
    }

    pub fn roots(&self) -> &[PathBuf] {
        &self.roots
    }

    /// The real path for `p`, if it's inside an approved folder. Follows
    /// symlinks on the part that exists, so a link can't lead outside.
    pub fn resolve(&self, p: &str) -> Result<PathBuf, String> {
        let raw = expand_home(p.trim());
        if !raw.is_absolute() {
            return Err(format!("Use a full path (like ~/Desktop/notes.txt), not \"{p}\"."));
        }
        let path = normalize(&raw);
        // Resolve the deepest part that exists, then add the rest back.
        let mut existing = path.clone();
        let mut rest = Vec::new();
        while !existing.exists() {
            match (existing.file_name().map(|n| n.to_owned()), existing.parent()) {
                (Some(name), Some(parent)) => {
                    rest.push(name);
                    existing = parent.to_path_buf();
                }
                _ => break,
            }
        }
        let mut real = fs::canonicalize(&existing).map_err(|e| format!("Can't open {}: {e}", existing.display()))?;
        for name in rest.into_iter().rev() {
            real.push(name);
        }
        if self.roots.iter().any(|r| real.starts_with(r)) {
            Ok(real)
        } else if self.roots.is_empty() {
            Err("No folders are approved for agents yet. The user can add some in Settings → Agents.".into())
        } else {
            Err(format!(
                "{} is outside the folders you may use: {}.",
                real.display(),
                self.roots.iter().map(|r| r.display().to_string()).collect::<Vec<_>>().join(", ")
            ))
        }
    }

    pub fn list(&self, p: &str) -> Result<String, String> {
        let dir = self.resolve(p)?;
        let mut entries: Vec<_> = fs::read_dir(&dir)
            .map_err(|e| format!("Can't list {}: {e}", dir.display()))?
            .filter_map(Result::ok)
            .collect();
        entries.sort_by_key(|e| e.file_name());
        let total = entries.len();
        let mut out = format!("{} ({total} items)\n", dir.display());
        for e in entries.into_iter().take(MAX_LIST) {
            let Ok(meta) = e.metadata() else { continue };
            let name = e.file_name().to_string_lossy().to_string();
            if meta.is_dir() {
                out += &format!("[folder] {name}\n");
            } else {
                out += &format!("{name}  {}\n", human_size(meta.len()));
            }
        }
        if total > MAX_LIST {
            out += &format!("… and {} more\n", total - MAX_LIST);
        }
        Ok(out)
    }

    pub fn read(&self, p: &str) -> Result<String, String> {
        let path = self.resolve(p)?;
        let meta = fs::metadata(&path).map_err(|e| format!("Can't read {}: {e}", path.display()))?;
        if meta.is_dir() {
            return Err(format!("{} is a folder; list it instead.", path.display()));
        }
        let mut buf = Vec::new();
        use std::io::Read;
        fs::File::open(&path)
            .and_then(|f| f.take(MAX_READ).read_to_end(&mut buf))
            .map_err(|e| format!("Can't read {}: {e}", path.display()))?;
        if buf.iter().take(4096).any(|&b| b == 0) {
            return Ok(format!("{} is a binary file ({}).", path.display(), human_size(meta.len())));
        }
        let mut text = String::from_utf8_lossy(&buf).to_string();
        if meta.len() > MAX_READ {
            text += &format!("\n[… file continues, {} in total]", human_size(meta.len()));
        }
        Ok(text)
    }

    /// Creates missing parent folders, recording each.
    fn ensure_parent(&self, path: &Path, ops: &mut Vec<FileOp>) -> Result<(), String> {
        let Some(parent) = path.parent() else { return Ok(()) };
        let mut missing = Vec::new();
        let mut p = parent.to_path_buf();
        while !p.exists() {
            missing.push(p.clone());
            if !p.pop() {
                break;
            }
        }
        for dir in missing.into_iter().rev() {
            fs::create_dir(&dir).map_err(|e| format!("Can't create {}: {e}", dir.display()))?;
            ops.push(FileOp::Created {
                path: dir.display().to_string(),
            });
        }
        Ok(())
    }

    /// A copy of `path` in the backups folder.
    fn backup(&self, agent: &str, path: &Path) -> Result<String, String> {
        let dir = self.backups.join(agent);
        fs::create_dir_all(&dir).map_err(|e| format!("Can't make a backup folder: {e}"))?;
        let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        let n = fs::read_dir(&dir).map(|d| d.count()).unwrap_or(0);
        let dest = dir.join(format!("{n:04}-{name}"));
        copy_all(path, &dest).map_err(|e| format!("Can't back up {}: {e}", path.display()))?;
        Ok(dest.display().to_string())
    }

    pub fn write(&self, agent: &str, p: &str, content: &str) -> Result<(String, Vec<FileOp>), String> {
        let path = self.resolve(p)?;
        if path.is_dir() {
            return Err(format!("{} is a folder.", path.display()));
        }
        let mut ops = Vec::new();
        self.ensure_parent(&path, &mut ops)?;
        if path.exists() {
            let backup = self.backup(agent, &path)?;
            ops.push(FileOp::Replaced {
                path: path.display().to_string(),
                backup,
            });
        } else {
            ops.push(FileOp::Created {
                path: path.display().to_string(),
            });
        }
        fs::write(&path, content).map_err(|e| format!("Can't write {}: {e}", path.display()))?;
        Ok((format!("Wrote {} ({}).", path.display(), human_size(content.len() as u64)), ops))
    }

    pub fn make_folder(&self, p: &str) -> Result<(String, Vec<FileOp>), String> {
        let path = self.resolve(p)?;
        if path.is_dir() {
            return Ok((format!("{} already exists.", path.display()), Vec::new()));
        }
        let mut ops = Vec::new();
        self.ensure_parent(&path, &mut ops)?;
        fs::create_dir(&path).map_err(|e| format!("Can't create {}: {e}", path.display()))?;
        ops.push(FileOp::Created {
            path: path.display().to_string(),
        });
        Ok((format!("Created {}.", path.display()), ops))
    }

    /// Moves or renames. Never overwrites.
    pub fn move_to(&self, from: &str, to: &str) -> Result<(String, Vec<FileOp>), String> {
        let src = self.resolve(from)?;
        let mut dest = self.resolve(to)?;
        if !src.exists() {
            return Err(format!("{} doesn't exist.", src.display()));
        }
        // Moving into a folder keeps the name.
        if dest.is_dir() {
            if let Some(name) = src.file_name() {
                dest = dest.join(name);
            }
        }
        if dest.exists() {
            return Err(format!("{} already exists; pick another name.", dest.display()));
        }
        if dest.starts_with(&src) {
            return Err("Can't move a folder into itself.".into());
        }
        let mut ops = Vec::new();
        self.ensure_parent(&dest, &mut ops)?;
        fs::rename(&src, &dest).map_err(|e| format!("Can't move {}: {e}", src.display()))?;
        ops.push(FileOp::Moved {
            from: src.display().to_string(),
            to: dest.display().to_string(),
        });
        Ok((format!("Moved {} to {}.", src.display(), dest.display()), ops))
    }

    pub fn delete(&self, agent: &str, p: &str) -> Result<(String, Vec<FileOp>), String> {
        let path = self.resolve(p)?;
        if !path.exists() {
            return Err(format!("{} doesn't exist.", path.display()));
        }
        if self.roots.contains(&path) {
            return Err("An approved folder itself can't be deleted.".into());
        }
        let backup = self.backup(agent, &path)?;
        let result = if path.is_dir() {
            fs::remove_dir_all(&path)
        } else {
            fs::remove_file(&path)
        };
        result.map_err(|e| format!("Can't delete {}: {e}", path.display()))?;
        Ok((
            format!("Deleted {} (a backup was kept).", path.display()),
            vec![FileOp::Deleted {
                path: path.display().to_string(),
                backup,
            }],
        ))
    }
}

/// Reverses recorded changes, newest first. Returns what couldn't be undone.
pub fn undo(journal: &[FileOp]) -> Vec<String> {
    let mut problems = Vec::new();
    for op in journal.iter().rev() {
        let result = match op {
            FileOp::Created { path } => {
                let p = Path::new(path);
                if p.is_dir() {
                    // Only if nothing else was put in it since.
                    fs::remove_dir(p)
                } else if p.exists() {
                    fs::remove_file(p)
                } else {
                    Ok(())
                }
            }
            FileOp::Replaced { path, backup } => fs::copy(backup, path).map(|_| ()),
            FileOp::Moved { from, to } => {
                if Path::new(from).exists() {
                    Err(std::io::Error::other(format!("{from} is in the way")))
                } else {
                    fs::rename(to, from)
                }
            }
            FileOp::Deleted { path, backup } => {
                if Path::new(path).exists() {
                    Err(std::io::Error::other(format!("{path} is in the way")))
                } else {
                    copy_all(Path::new(backup), Path::new(path))
                }
            }
        };
        if let Err(e) = result {
            problems.push(format!("{}: {e}", describe(op)));
        }
    }
    problems
}

fn describe(op: &FileOp) -> String {
    match op {
        FileOp::Created { path } => format!("remove {path}"),
        FileOp::Replaced { path, .. } => format!("restore {path}"),
        FileOp::Moved { from, to } => format!("move {to} back to {from}"),
        FileOp::Deleted { path, .. } => format!("bring back {path}"),
    }
}

fn copy_all(src: &Path, dest: &Path) -> std::io::Result<()> {
    if src.is_dir() {
        fs::create_dir_all(dest)?;
        for e in fs::read_dir(src)? {
            let e = e?;
            copy_all(&e.path(), &dest.join(e.file_name()))?;
        }
        Ok(())
    } else {
        fs::copy(src, dest).map(|_| ())
    }
}

/// Removes backups older than `days`.
pub fn prune_backups(backups: &Path, days: u32) {
    let Ok(dirs) = fs::read_dir(backups) else { return };
    let max = std::time::Duration::from_secs(days as u64 * 86_400);
    for d in dirs.filter_map(Result::ok) {
        let old = d
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.elapsed().ok())
            .is_some_and(|age| age > max);
        if old {
            let _ = fs::remove_dir_all(d.path());
        }
    }
}

pub fn human_size(bytes: u64) -> String {
    match bytes {
        b if b < 1024 => format!("{b} B"),
        b if b < 1024 * 1024 => format!("{:.1} KB", b as f64 / 1024.0),
        b => format!("{:.1} MB", b as f64 / 1_048_576.0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> (tempfile::TempDir, Files) {
        let tmp = tempfile::tempdir().unwrap();
        let desk = tmp.path().join("Desktop");
        fs::create_dir(&desk).unwrap();
        fs::create_dir(tmp.path().join("Secret")).unwrap();
        fs::write(desk.join("a.pdf"), "pdf").unwrap();
        fs::write(desk.join("b.png"), "png").unwrap();
        fs::write(tmp.path().join("Secret/key.txt"), "shh").unwrap();
        let files = Files::new(&[desk.display().to_string()], tmp.path().join("backups"));
        (tmp, files)
    }

    #[test]
    fn stays_inside_approved_folders() {
        let (tmp, f) = setup();
        let desk = tmp.path().join("Desktop");
        assert!(f.resolve(&desk.join("a.pdf").display().to_string()).is_ok());
        // New files inside are fine, even in folders that don't exist yet.
        assert!(f.resolve(&desk.join("Docs/new.txt").display().to_string()).is_ok());
        for bad in [
            tmp.path().join("Secret/key.txt"),
            desk.join("../Secret/key.txt"),
            PathBuf::from("relative.txt"),
        ] {
            assert!(f.resolve(&bad.display().to_string()).is_err(), "{}", bad.display());
        }
    }

    #[cfg(unix)]
    #[test]
    fn a_symlink_cant_lead_outside() {
        let (tmp, f) = setup();
        let link = tmp.path().join("Desktop/escape");
        std::os::unix::fs::symlink(tmp.path().join("Secret"), &link).unwrap();
        assert!(f.read(&link.join("key.txt").display().to_string()).is_err());
        assert!(f
            .write("a1", &link.join("new.txt").display().to_string(), "x")
            .is_err());
    }

    #[test]
    fn a_desktop_cleanup_can_be_undone_in_one_go() {
        let (tmp, f) = setup();
        let d = |p: &str| tmp.path().join("Desktop").join(p).display().to_string();
        let mut journal = Vec::new();
        journal.extend(f.make_folder(&d("Documents")).unwrap().1);
        journal.extend(f.move_to(&d("a.pdf"), &d("Documents")).unwrap().1);
        journal.extend(f.move_to(&d("b.png"), &d("Images/b.png")).unwrap().1);
        journal.extend(f.write("a1", &d("Documents/a.pdf"), "changed").unwrap().1);
        journal.extend(f.delete("a1", &d("Images")).unwrap().1);
        assert!(!Path::new(&d("a.pdf")).exists());
        assert_eq!(fs::read_to_string(d("Documents/a.pdf")).unwrap(), "changed");

        assert_eq!(undo(&journal), Vec::<String>::new());
        assert_eq!(fs::read_to_string(d("a.pdf")).unwrap(), "pdf");
        assert_eq!(fs::read_to_string(d("b.png")).unwrap(), "png");
        assert!(!Path::new(&d("Documents")).exists() && !Path::new(&d("Images")).exists());
    }

    #[test]
    fn never_overwrites_or_deletes_an_approved_folder() {
        let (tmp, f) = setup();
        let d = |p: &str| tmp.path().join("Desktop").join(p).display().to_string();
        assert!(f.move_to(&d("a.pdf"), &d("b.png")).unwrap_err().contains("already exists"));
        assert!(f.delete("a1", &d("")).is_err());
        assert!(f.list(&d("")).unwrap().contains("a.pdf"));
        assert_eq!(f.read(&d("a.pdf")).unwrap(), "pdf");
    }

    #[test]
    fn nothing_is_approved_until_the_user_says_so() {
        let f = Files::new(&[], PathBuf::from("/tmp/x"));
        assert!(f.resolve("/tmp").unwrap_err().contains("Settings → Agents"));
    }
}
