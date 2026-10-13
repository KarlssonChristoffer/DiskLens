//! Exclusion patterns for folders and files.
//!
//! A pattern without a backslash matches the item name; one with a backslash
//! matches the full path. Patterns are wildcards (`*`, `?`) unless the scan
//! options ask for regular expressions. Matching ignores case, like Windows.

use regex::{Regex, RegexBuilder};

use super::ScanOptions;

#[derive(Clone, Default)]
pub struct Filters {
    dirs: Vec<Matcher>,
    files: Vec<Matcher>,
}

#[derive(Clone)]
struct Matcher {
    regex: Regex,
    on_path: bool,
}

impl Filters {
    pub fn new(options: &ScanOptions) -> Result<Self, String> {
        let build = |patterns: &[String]| -> Result<Vec<Matcher>, String> {
            patterns
                .iter()
                .map(|p| p.trim())
                .filter(|p| !p.is_empty())
                .map(|p| {
                    let source = if options.use_regex { p.to_string() } else { wildcard(p) };
                    let regex = RegexBuilder::new(&source)
                        .case_insensitive(true)
                        .build()
                        .map_err(|_| p.to_string())?;
                    // A literal backslash (`\\` in a regex) means a path pattern.
                    let on_path = if options.use_regex { p.contains(r"\\") } else { p.contains('\\') };
                    Ok(Matcher { regex, on_path })
                })
                .collect()
        };
        Ok(Filters {
            dirs: build(&options.exclude_dirs)?,
            files: build(&options.exclude_files)?,
        })
    }

    pub fn excludes_dir(&self, name: &str, path: impl FnOnce() -> String) -> bool {
        matches(&self.dirs, name, path)
    }

    pub fn excludes_file(&self, name: &str, path: impl FnOnce() -> String) -> bool {
        matches(&self.files, name, path)
    }
}

fn matches(list: &[Matcher], name: &str, path: impl FnOnce() -> String) -> bool {
    if list.is_empty() {
        return false;
    }
    let mut full: Option<String> = None;
    let mut path = Some(path);
    list.iter().any(|m| {
        if m.on_path {
            let p = full.get_or_insert_with(|| (path.take().unwrap())());
            m.regex.is_match(p)
        } else {
            m.regex.is_match(name)
        }
    })
}

/// `*.tmp` → `^.*\.tmp$`
fn wildcard(pattern: &str) -> String {
    let mut out = String::from("^");
    for c in pattern.chars() {
        match c {
            '*' => out.push_str(".*"),
            '?' => out.push('.'),
            c => out.push_str(&regex::escape(&c.to_string())),
        }
    }
    out.push('$');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn filters(dirs: &[&str], regex: bool) -> Filters {
        Filters::new(&ScanOptions {
            exclude_dirs: dirs.iter().map(|s| s.to_string()).collect(),
            use_regex: regex,
            ..ScanOptions::default()
        })
        .unwrap()
    }

    #[test]
    fn wildcards_match_names_case_insensitively() {
        let f = filters(&["node_modules", "*.cache", "tmp?"], false);
        assert!(f.excludes_dir("node_modules", String::new));
        assert!(f.excludes_dir("Node_Modules", String::new));
        assert!(f.excludes_dir("pip.cache", String::new));
        assert!(f.excludes_dir("tmp1", String::new));
        assert!(!f.excludes_dir("tmp12", String::new));
        assert!(!f.excludes_dir("src", String::new));
    }

    #[test]
    fn patterns_with_backslashes_match_paths() {
        let f = filters(&[r"C:\Windows\WinSxS"], false);
        assert!(f.excludes_dir("WinSxS", || r"C:\Windows\WinSxS".into()));
        assert!(!f.excludes_dir("WinSxS", || r"D:\Backup\WinSxS".into()));
    }

    #[test]
    fn regex_mode_and_bad_patterns() {
        let f = filters(&["^\\.git$"], true);
        assert!(f.excludes_dir(".git", String::new));
        assert!(!f.excludes_dir(".github", String::new));
        let bad = Filters::new(&ScanOptions { exclude_dirs: vec!["(".into()], use_regex: true, ..ScanOptions::default() });
        assert!(bad.is_err());
    }
}
