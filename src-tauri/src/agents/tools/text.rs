//! Reading part of a text file, searching text files, and changing part of
//! one in place. Shared by the file tools (the user's approved folders) and
//! the project tools (a builder agent's own folder).

use std::path::{Path, PathBuf};

use regex::{Regex, RegexBuilder};

/// Most matching lines a search returns.
const MAX_MATCHES: usize = 200;
/// Most files a search or find looks at.
const MAX_FILES: usize = 20_000;
/// Files bigger than this aren't searched.
const MAX_FILE: u64 = 2_000_000;
/// Most paths a find returns.
const MAX_FOUND: usize = 300;
/// Folders never looked into: dependencies and builds. Hidden folders are
/// skipped too.
const SKIP: &[&str] = &[
    "node_modules",
    "target",
    "dist",
    "build",
    ".venv",
    "__pycache__",
    "vendor",
];

/// Lines `from` to `to` of `text`, 1-based and inclusive, with a header
/// saying where they sit in the file.
pub fn lines(text: &str, from: usize, to: usize) -> String {
    let total = text.lines().count();
    let from = from.max(1);
    let to = to.min(total);
    if from > to {
        return format!("The file has {total} lines; there's nothing from line {from}.");
    }
    let body: Vec<&str> = text.lines().skip(from - 1).take(to - from + 1).collect();
    format!("Lines {from}-{to} of {total}:\n{}", body.join("\n"))
}

/// `text` with `find` replaced by `with`: once, or everywhere with `all`.
/// The text must be there, and only once unless `all`.
pub fn replace(text: &str, find: &str, with: &str, all: bool) -> Result<(String, usize), String> {
    if find.is_empty() {
        return Err("Give the text to find.".into());
    }
    let n = text.matches(find).count();
    match n {
        0 => Err("That text isn't in the file. Read the file again and copy the text exactly, with its \
                  spaces and line breaks."
            .into()),
        1 => Ok((text.replacen(find, with, 1), 1)),
        _ if all => Ok((text.replace(find, with), n)),
        _ => Err(format!(
            "That text appears {n} times. Include more of the lines around it so it matches once, or set all \
             to true to change every one."
        )),
    }
}

/// Lines matching `pattern` (a regular expression, case-insensitive) in the
/// text files under `root`, or in `root` when it's a file. `glob` limits
/// which files. Paths are full with `full`, else relative to `root`.
pub fn grep(root: &Path, pattern: &str, glob: Option<&str>, full: bool) -> Result<String, String> {
    let re = RegexBuilder::new(pattern)
        .case_insensitive(true)
        .build()
        .map_err(|e| format!("\"{pattern}\" isn't a valid pattern: {e}"))?;
    let only = glob
        .map(str::trim)
        .filter(|g| !g.is_empty())
        .map(glob_regex)
        .transpose()?;
    let mut out = String::new();
    let (mut matches, mut files) = (0, 0);
    for p in walk(root).take(MAX_FILES) {
        if matches >= MAX_MATCHES {
            break;
        }
        let rel = relative(root, &p);
        if only.as_ref().is_some_and(|g| !g.is_match(&rel)) {
            continue;
        }
        if p.metadata().map(|m| m.len() > MAX_FILE).unwrap_or(true) {
            continue;
        }
        let Ok(bytes) = std::fs::read(&p) else { continue };
        if bytes.iter().take(4096).any(|&b| b == 0) {
            continue;
        }
        let text = String::from_utf8_lossy(&bytes);
        let shown = if full { p.display().to_string() } else { rel };
        let mut hit = false;
        for (i, line) in text.lines().enumerate() {
            if !re.is_match(line) {
                continue;
            }
            hit = true;
            matches += 1;
            out += &format!("{shown}:{}: {}\n", i + 1, cut(line.trim(), 200));
            if matches >= MAX_MATCHES {
                break;
            }
        }
        files += hit as usize;
    }
    if matches == 0 {
        return Ok(format!("No lines match {pattern} under {}.", root.display()));
    }
    let more = if matches >= MAX_MATCHES {
        " (stopped there; narrow the search)"
    } else {
        ""
    };
    Ok(format!("{matches} matching lines in {files} files{more}:\n{out}"))
}

/// Files under `root` whose path matches `glob`, like `*.pdf` or
/// `src/**/*.ts`. Paths are full with `full`, else relative to `root`.
pub fn find(root: &Path, glob: &str, full: bool) -> Result<String, String> {
    let re = glob_regex(glob.trim())?;
    let mut out = Vec::new();
    for p in walk(root).take(MAX_FILES) {
        let rel = relative(root, &p);
        if re.is_match(&rel) {
            out.push(if full { p.display().to_string() } else { rel });
            if out.len() >= MAX_FOUND {
                break;
            }
        }
    }
    if out.is_empty() {
        return Ok(format!("No files match {glob} under {}.", root.display()));
    }
    let more = if out.len() >= MAX_FOUND {
        " (the first ones; narrow the pattern)"
    } else {
        ""
    };
    Ok(format!(
        "{} files match {glob}{more}:\n{}",
        out.len(),
        out.join("\n")
    ))
}

/// The files under `root` (or `root` itself when it's a file), skipping
/// hidden folders, dependencies and builds.
fn walk(root: &Path) -> impl Iterator<Item = PathBuf> {
    walkdir::WalkDir::new(root)
        .sort_by_file_name()
        .into_iter()
        .filter_entry(|e| {
            let name = e.file_name().to_string_lossy();
            e.depth() == 0
                || !e.file_type().is_dir()
                || !(name.starts_with('.') || SKIP.contains(&name.as_ref()))
        })
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file())
        .map(|e| e.into_path())
}

/// `p` relative to `root` with `/` separators; the file name when `p` is
/// `root` itself.
fn relative(root: &Path, p: &Path) -> String {
    let rel = p.strip_prefix(root).unwrap_or(p);
    if rel.as_os_str().is_empty() {
        p.file_name().unwrap_or_default().to_string_lossy().to_string()
    } else {
        rel.to_string_lossy().replace('\\', "/")
    }
}

/// A glob as a regular expression over a `/`-separated relative path:
/// `*` is anything but `/`, `?` one such character, `**` anything. A
/// pattern without `/` matches file names anywhere.
fn glob_regex(glob: &str) -> Result<Regex, String> {
    let mut re = String::from("^");
    if !glob.contains('/') {
        re += "(?:.*/)?";
    }
    let chars: Vec<char> = glob.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        match chars[i] {
            '*' if chars.get(i + 1) == Some(&'*') => {
                if chars.get(i + 2) == Some(&'/') {
                    re += "(?:.*/)?";
                    i += 3;
                } else {
                    re += ".*";
                    i += 2;
                }
                continue;
            }
            '*' => re += "[^/]*",
            '?' => re += "[^/]",
            c => re += &regex::escape(&c.to_string()),
        }
        i += 1;
    }
    re += "$";
    RegexBuilder::new(&re)
        .case_insensitive(true)
        .build()
        .map_err(|e| format!("\"{glob}\" isn't a valid pattern: {e}"))
}

fn cut(s: &str, max: usize) -> String {
    match s.char_indices().nth(max) {
        Some((i, _)) => format!("{}…", &s[..i]),
        None => s.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_range_of_lines() {
        let t = "a\nb\nc\nd";
        assert_eq!(lines(t, 2, 3), "Lines 2-3 of 4:\nb\nc");
        assert_eq!(lines(t, 0, usize::MAX), "Lines 1-4 of 4:\na\nb\nc\nd");
        assert!(lines(t, 9, 10).contains("nothing from line 9"));
    }

    #[test]
    fn replaces_once_unless_told_all() {
        assert_eq!(replace("a b a", "b", "c", false).unwrap(), ("a c a".into(), 1));
        assert!(replace("a b a", "a", "c", false).unwrap_err().contains("2 times"));
        assert_eq!(replace("a b a", "a", "c", true).unwrap(), ("c b c".into(), 2));
        assert!(replace("a", "z", "c", false).unwrap_err().contains("isn't in the file"));
        assert!(replace("a", "", "c", false).is_err());
    }

    #[test]
    fn globs_match_names_anywhere_and_paths_exactly() {
        let m = |g: &str, p: &str| glob_regex(g).unwrap().is_match(p);
        assert!(m("*.md", "docs/a/README.md") && m("*.MD", "x.md"));
        assert!(!m("*.md", "a.mdx"));
        assert!(m("src/**/*.ts", "src/a/b/c.ts") && m("src/**/*.ts", "src/c.ts"));
        assert!(!m("src/**/*.ts", "lib/c.ts"));
        assert!(m("**/test_?.py", "a/test_1.py") && !m("**/test_?.py", "a/test_12.py"));
        assert!(m("a.(b)", "a.(b)") && !m("a.(b)", "axxb"));
    }

    #[test]
    fn searches_files_and_skips_junk() {
        let tmp = tempfile::tempdir().unwrap();
        let d = tmp.path();
        std::fs::create_dir_all(d.join("src/deep")).unwrap();
        std::fs::create_dir_all(d.join("node_modules/x")).unwrap();
        std::fs::create_dir_all(d.join(".git")).unwrap();
        std::fs::write(d.join("src/a.ts"), "let Hello = 1;\n// hello again\n").unwrap();
        std::fs::write(d.join("src/deep/b.md"), "nothing\n").unwrap();
        std::fs::write(d.join("node_modules/x/c.ts"), "hello\n").unwrap();
        std::fs::write(d.join(".git/HEAD"), "hello\n").unwrap();
        std::fs::write(d.join("bin.dat"), b"hello\0\x01").unwrap();

        let g = grep(d, "hello", None, false).unwrap();
        assert!(g.starts_with("2 matching lines in 1 files:\n"), "{g}");
        assert!(g.contains("src/a.ts:1: let Hello = 1;") && g.contains("src/a.ts:2: // hello again"));
        assert!(!g.contains("node_modules") && !g.contains(".git") && !g.contains("bin.dat"));
        assert!(grep(d, "hello", Some("*.md"), false).unwrap().starts_with("No lines match"));
        assert!(grep(d, "(", None, false).is_err());
        // One file, and full paths.
        let one = grep(&d.join("src/a.ts"), "again", None, true).unwrap();
        assert!(one.contains(&format!("{}:2:", d.join("src/a.ts").display())));

        let f = find(d, "*.ts", false).unwrap();
        assert_eq!(f, "1 files match *.ts:\nsrc/a.ts");
        assert!(find(d, "**/*.md", true).unwrap().contains(&d.join("src/deep/b.md").display().to_string()));
        assert!(find(d, "*.rs", false).unwrap().starts_with("No files match"));
    }
}
