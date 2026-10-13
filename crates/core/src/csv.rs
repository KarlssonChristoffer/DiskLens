//! Save and load scan results as CSV (WinDirStat's File › Save/Load Results).
//!
//! One row per folder or file, parents before children, with the full path in
//! the first column. The columns follow WinDirStat's export; DiskLens adds a
//! `Type` column so files and empty folders round-trip exactly.

use std::fmt::Write as _;
use std::io::{self, BufRead, Write};

use crate::scan::{ImportRow, build_from_rows};
use crate::tree::{NodeId, Tree, flags};

const HEADER: &str = "Name,Files,Folders,Size (Logical),Size (Physical),Attributes,Last Change,Type";

pub fn export(tree: &Tree, out: &mut impl Write) -> io::Result<()> {
    write!(out, "{HEADER}\r\n")?;
    let mut stack: Vec<NodeId> = vec![0];
    while let Some(id) = stack.pop() {
        let n = tree.node(id);
        if n.is_gone() || n.is_pseudo() {
            continue;
        }
        let path = if id == 0 && n.flags & flags::MY_COMPUTER != 0 { n.name.to_string() } else { tree.path(id) };
        write!(
            out,
            "{},{},{},{},{},{},{},{}\r\n",
            quote(&path),
            n.files,
            n.dirs,
            n.logical,
            n.physical,
            attributes(n.attributes),
            iso_time(n.modified),
            if n.is_dir() { "Folder" } else { "File" }
        )?;
        if n.is_dir() {
            // Push in reverse so children come out in storage (size) order.
            let mut kids: Vec<NodeId> = tree.children(id).collect();
            kids.reverse();
            stack.extend(kids);
        }
    }
    Ok(())
}

pub fn export_duplicates(tree: &Tree, groups: &[crate::dupes::DupeGroup], out: &mut impl Write) -> io::Result<()> {
    write!(out, "Group,Hash,Size (Logical),Name,Last Change\r\n")?;
    for (i, g) in groups.iter().enumerate() {
        for &f in &g.files {
            write!(out, "{},{},{},{},{}\r\n", i + 1, g.hash, g.size, quote(&tree.path(f)), iso_time(tree.node(f).modified))?;
        }
    }
    Ok(())
}

/// Load a tree saved by `export` (or a compatible WinDirStat CSV).
pub fn import(input: impl BufRead) -> Result<Tree, String> {
    let mut lines = input.lines();
    let header = lines.next().ok_or("The file is empty")?.map_err(|e| e.to_string())?;
    let columns: Vec<String> = parse_line(header.trim_start_matches('\u{feff}'));
    let find = |names: &[&str]| columns.iter().position(|c| names.iter().any(|n| c.eq_ignore_ascii_case(n)));
    let name = find(&["Name"]).ok_or("Missing the Name column")?;
    let logical = find(&["Size (Logical)", "Logical Size"]);
    let physical = find(&["Size (Physical)", "Physical Size", "Size"]);
    let attrs = find(&["Attributes"]);
    let changed = find(&["Last Change", "Last Changed"]);
    let kind = find(&["Type"]);

    let mut rows = Vec::new();
    for line in lines {
        let line = line.map_err(|e| e.to_string())?;
        if line.trim().is_empty() {
            continue;
        }
        let f = parse_line(&line);
        let get = |i: Option<usize>| i.and_then(|i| f.get(i)).map(String::as_str).unwrap_or("");
        let attr_text = get(attrs);
        rows.push(ImportRow {
            path: get(Some(name)).trim_end_matches('\\').to_string(),
            is_dir: match get(kind) {
                "Folder" => Some(true),
                "File" => Some(false),
                _ => attr_text.contains('D').then_some(true),
            },
            logical: get(logical).parse().unwrap_or(0),
            physical: get(physical).parse().or_else(|_| get(logical).parse()).unwrap_or(0),
            modified: parse_iso_time(get(changed)),
            attributes: parse_attributes(attr_text),
        });
    }
    build_from_rows(rows)
}

fn quote(s: &str) -> String {
    format!("\"{}\"", s.replace('"', "\"\""))
}

fn parse_line(line: &str) -> Vec<String> {
    let mut fields = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' if quoted && chars.peek() == Some(&'"') => {
                cur.push('"');
                chars.next();
            }
            '"' => quoted = !quoted,
            ',' if !quoted => fields.push(std::mem::take(&mut cur)),
            c => cur.push(c),
        }
    }
    fields.push(cur);
    fields
}

const ATTRIBUTE_LETTERS: [(u32, char); 7] = [
    (0x1, 'R'),    // read-only
    (0x2, 'H'),    // hidden
    (0x4, 'S'),    // system
    (0x20, 'A'),   // archive
    (0x800, 'C'),  // compressed
    (0x4000, 'E'), // encrypted
    (0x10, 'D'),   // directory
];

/// Attribute letters as in Explorer's Attributes column.
pub fn attributes(a: u32) -> String {
    ATTRIBUTE_LETTERS.iter().filter(|(bit, _)| a & bit != 0).map(|&(_, c)| c).collect()
}

fn parse_attributes(s: &str) -> u32 {
    ATTRIBUTE_LETTERS.iter().filter(|&&(_, c)| s.contains(c)).map(|&(bit, _)| bit).sum()
}

const TICKS_PER_SECOND: i64 = 10_000_000;
const UNIX_EPOCH_TICKS: i64 = 116_444_736_000_000_000;

/// FILETIME as `YYYY-MM-DDTHH:MM:SSZ` (UTC).
pub fn iso_time(filetime: i64) -> String {
    if filetime <= 0 {
        return String::new();
    }
    let secs = (filetime - UNIX_EPOCH_TICKS).div_euclid(TICKS_PER_SECOND);
    let (days, rem) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    let (y, m, d) = civil_from_days(days);
    let mut s = String::new();
    let _ = write!(s, "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z", rem / 3600, rem / 60 % 60, rem % 60);
    s
}

fn parse_iso_time(s: &str) -> i64 {
    let digits: Vec<i64> = s
        .split(|c: char| !c.is_ascii_digit())
        .filter(|p| !p.is_empty())
        .filter_map(|p| p.parse().ok())
        .collect();
    if digits.len() < 6 {
        return 0;
    }
    let days = days_from_civil(digits[0], digits[1], digits[2]);
    let secs = days * 86_400 + digits[3] * 3600 + digits[4] * 60 + digits[5];
    secs * TICKS_PER_SECOND + UNIX_EPOCH_TICKS
}

// Howard Hinnant's civil-date algorithms.
fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}

fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = if m > 2 { m - 3 } else { m + 9 };
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scan::{ScanControl, ScanOptions, scan};

    #[test]
    fn times_round_trip() {
        let ft = UNIX_EPOCH_TICKS + 1_760_000_000 * TICKS_PER_SECOND;
        let s = iso_time(ft);
        assert_eq!(s, "2025-10-09T08:53:20Z");
        assert_eq!(parse_iso_time(&s), ft);
    }

    #[test]
    fn quoting_round_trips() {
        let line = format!("{},2,\"x\"", quote(r#"C:\a "b", c"#));
        assert_eq!(parse_line(&line), vec![r#"C:\a "b", c"#.to_string(), "2".into(), "x".into()]);
    }

    #[test]
    fn export_then_import_restores_the_tree() {
        let dir = std::env::temp_dir().join(format!("disklens-csv-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("sub/empty")).unwrap();
        std::fs::write(dir.join("a.txt"), vec![1u8; 300]).unwrap();
        // Windows forbids `"` in names; commas and apostrophes still need quoting care.
        std::fs::write(dir.join("sub/b, 'q'.bin"), vec![1u8; 7000]).unwrap();

        let tree = scan(dir.to_str().unwrap(), &ScanOptions::default(), &ScanControl::default()).unwrap();
        let mut buf = Vec::new();
        export(&tree, &mut buf).unwrap();
        let back = import(io::Cursor::new(buf)).unwrap();

        assert_eq!(back.root().logical, tree.root().logical);
        assert_eq!(back.root().physical, tree.root().physical);
        assert_eq!(back.root().files, 2);
        assert_eq!(back.root().dirs, 2);
        let q = back.nodes.iter().position(|n| n.name.contains("'q'")).unwrap() as u32;
        assert_eq!(back.path(q), tree.path(tree.nodes.iter().position(|n| n.name.contains("'q'")).unwrap() as u32));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
