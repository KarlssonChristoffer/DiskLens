//! Parallel scanner.
//!
//! Each directory is listed with one batched `NtQueryDirectoryFile` loop and
//! its subdirectories are scanned in parallel on a rayon pool. The result is
//! built as a nested tree of raw directories (cheap to produce concurrently)
//! and flattened into the arena `Tree` at the end. `refresh` rescans one
//! directory and grafts the result into an existing tree.

#[cfg(not(windows))]
compile_error!("the DiskLens scanner currently supports Windows only");

mod filter;
mod windows;

use std::cmp::Reverse;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Condvar, Mutex};
use std::time::Duration;

use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use windows_sys::Win32::Storage::FileSystem::{
    FILE_ATTRIBUTE_COMPRESSED, FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_HIDDEN,
    FILE_ATTRIBUTE_REPARSE_POINT, FILE_ATTRIBUTE_SPARSE_FILE, FILE_ATTRIBUTE_SYSTEM,
};
use windows_sys::Win32::System::SystemServices::{
    IO_REPARSE_TAG_MOUNT_POINT, IO_REPARSE_TAG_SYMLINK, IO_REPARSE_TAG_WOF,
};

pub use filter::Filters;

use crate::tree::{ExtInterner, NO_EXT, NO_NODE, Node, NodeId, NodeKind, ROOT, Tree, flags};
use crate::volumes;

/// What to scan and what to leave out. Defaults match WinDirStat's.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ScanOptions {
    pub exclude_hidden_dirs: bool,
    pub exclude_hidden_files: bool,
    /// Protected operating-system items: both hidden and system.
    pub exclude_protected_dirs: bool,
    pub exclude_protected_files: bool,
    /// Follow junctions and volume mount points (WinDirStat: off).
    pub follow_junctions: bool,
    /// Follow directory symbolic links (WinDirStat: off).
    pub follow_symlink_dirs: bool,
    /// Count symbolic-link files (WinDirStat: off).
    pub count_symlink_files: bool,
    /// Folder names or paths to skip; wildcards `*` `?`, or regexes.
    pub exclude_dirs: Vec<String>,
    /// File names or paths to skip; wildcards `*` `?`, or regexes.
    pub exclude_files: Vec<String>,
    pub use_regex: bool,
    /// Ignore files smaller than this many bytes.
    pub min_file_size: u64,
    /// Scanning threads; 0 means one per logical CPU.
    pub threads: usize,
}

impl Default for ScanOptions {
    fn default() -> Self {
        ScanOptions {
            exclude_hidden_dirs: false,
            exclude_hidden_files: false,
            exclude_protected_dirs: false,
            exclude_protected_files: false,
            follow_junctions: false,
            follow_symlink_dirs: false,
            count_symlink_files: false,
            exclude_dirs: Vec::new(),
            exclude_files: Vec::new(),
            use_regex: false,
            min_file_size: 0,
            threads: 0,
        }
    }
}

/// Shared between the scanning threads and whoever displays progress.
#[derive(Default)]
pub struct ScanControl {
    cancel: AtomicBool,
    stop: AtomicBool,
    paused: Mutex<bool>,
    resumed: Condvar,
    files: AtomicU64,
    dirs: AtomicU64,
    bytes: AtomicU64,
    errors: AtomicU64,
}

#[derive(Clone, Copy, Debug, Default, Serialize)]
pub struct ScanProgress {
    pub files: u64,
    pub dirs: u64,
    /// Physical bytes found so far.
    pub bytes: u64,
    pub errors: u64,
    pub paused: bool,
}

impl ScanControl {
    /// Abandon the scan and its results.
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
        self.resume();
    }

    /// Finish early but keep what has been scanned (WinDirStat's Stop).
    pub fn stop(&self) {
        self.stop.store(true, Ordering::Relaxed);
        self.resume();
    }

    pub fn pause(&self) {
        *self.paused.lock().unwrap() = true;
    }

    pub fn resume(&self) {
        *self.paused.lock().unwrap() = false;
        self.resumed.notify_all();
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }

    pub fn is_stopped(&self) -> bool {
        self.stop.load(Ordering::Relaxed)
    }

    fn should_stop(&self) -> bool {
        self.is_cancelled() || self.is_stopped()
    }

    fn wait_if_paused(&self) {
        let mut paused = self.paused.lock().unwrap();
        while *paused && !self.should_stop() {
            paused = self.resumed.wait_timeout(paused, Duration::from_millis(250)).unwrap().0;
        }
    }

    pub fn progress(&self) -> ScanProgress {
        ScanProgress {
            files: self.files.load(Ordering::Relaxed),
            dirs: self.dirs.load(Ordering::Relaxed),
            bytes: self.bytes.load(Ordering::Relaxed),
            errors: self.errors.load(Ordering::Relaxed),
            paused: *self.paused.lock().unwrap(),
        }
    }
}

#[derive(Debug)]
pub enum ScanError {
    /// The path does not exist or is not a directory.
    NotADirectory(String),
    /// An exclusion pattern is not a valid regular expression.
    BadPattern(String),
    NothingToScan,
}

impl std::fmt::Display for ScanError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ScanError::NotADirectory(p) => write!(f, "{p} is not a folder"),
            ScanError::BadPattern(p) => write!(f, "invalid exclusion pattern: {p}"),
            ScanError::NothingToScan => write!(f, "nothing to scan"),
        }
    }
}

impl std::error::Error for ScanError {}

struct RawFile {
    name: Box<str>,
    logical: u64,
    physical: u64,
    modified: i64,
    attributes: u32,
    flags: u8,
    kind: NodeKind,
}

#[derive(Default)]
struct RawDir {
    name: Box<str>,
    modified: i64,
    attributes: u32,
    flags: u8,
    files: Vec<RawFile>,
    dirs: Vec<RawDir>,
    // Subtree totals, filled in once the subdirectories are done.
    logical: u64,
    physical: u64,
    file_count: u32,
    dir_count: u32,
    newest: i64,
}

struct Context<'a> {
    control: &'a ScanControl,
    options: &'a ScanOptions,
    filters: Filters,
    cluster_size: u64,
}

const BACKSLASH: u16 = b'\\' as u16;

fn with_pool<T: Send>(options: &ScanOptions, f: impl FnOnce() -> T + Send) -> T {
    if options.threads == 0 {
        return f();
    }
    match rayon::ThreadPoolBuilder::new().num_threads(options.threads).build() {
        Ok(pool) => pool.install(f),
        Err(_) => f(),
    }
}

/// Scan `root` (a drive like `C:\` or any folder) into a tree.
pub fn scan(root: &str, options: &ScanOptions, control: &ScanControl) -> Result<Tree, ScanError> {
    scan_many(&[root.to_string()], options, control)
}

/// Scan one or more roots. Several roots get a synthetic "This PC" root,
/// like WinDirStat's "All local drives".
pub fn scan_many(roots: &[String], options: &ScanOptions, control: &ScanControl) -> Result<Tree, ScanError> {
    let filters = Filters::new(options).map_err(ScanError::BadPattern)?;
    let mut raws = Vec::new();
    for root in roots {
        let (display, extended) = normalize(root);
        let attributes = windows::attributes(&extended)
            .filter(|a| a & FILE_ATTRIBUTE_DIRECTORY != 0)
            .ok_or_else(|| ScanError::NotADirectory(display.clone()))?;
        raws.push((display, extended, attributes));
    }
    if raws.is_empty() {
        return Err(ScanError::NothingToScan);
    }

    let mut scanned: Vec<RawDir> = with_pool(options, || {
        raws.into_par_iter()
            .map(|(display, extended, attributes)| {
                let ctx = Context {
                    control,
                    options,
                    filters: filters.clone(),
                    cluster_size: windows::cluster_size(&volume_root(&extended)),
                };
                let drive = is_drive_root(&display);
                let mut raw = scan_dir(
                    &ctx,
                    &extended,
                    RawDir {
                        name: display.clone().into_boxed_str(),
                        attributes,
                        flags: if drive { flags::DRIVE } else { 0 },
                        ..RawDir::default()
                    },
                );
                if drive {
                    attach_pseudo(&mut raw, &display, false, false);
                }
                raw
            })
            .collect()
    });

    let errors = control.progress().errors;
    let mut tree = if scanned.len() == 1 {
        flatten(scanned.pop().unwrap())
    } else {
        let mut root = RawDir {
            name: "This PC".into(),
            flags: flags::MY_COMPUTER,
            ..RawDir::default()
        };
        for d in &scanned {
            root.logical += d.logical;
            root.physical += d.physical;
            root.file_count += d.file_count;
            root.dir_count += d.dir_count + 1;
            root.newest = root.newest.max(d.newest);
        }
        root.dirs = scanned;
        flatten(root)
    };
    tree.errors = errors;
    Ok(tree)
}

/// Add `<Free Space>` and `<Unknown>` items to a scanned drive root. Shown
/// items count toward the drive's totals; hidden ones wait for their option.
fn attach_pseudo(raw: &mut RawDir, root: &str, show_free: bool, show_unknown: bool) {
    let Some((total, free)) = volumes::space(root) else { return };
    let used = total.saturating_sub(free);
    let unknown = used.saturating_sub(raw.physical);
    for (name, bytes, kind, show) in [
        ("<Free Space>", free, NodeKind::FreeSpace, show_free),
        ("<Unknown>", unknown, NodeKind::Unknown, show_unknown),
    ] {
        if show {
            raw.logical += bytes;
            raw.physical += bytes;
        }
        raw.files.push(RawFile {
            name: name.into(),
            logical: bytes,
            physical: bytes,
            modified: 0,
            attributes: 0,
            flags: if show { 0 } else { flags::HIDDEN },
            kind,
        });
    }
}

/// Recompute `<Free Space>` and `<Unknown>` after files changed on disk.
pub fn update_pseudo(tree: &mut Tree) {
    for drive in tree.drives() {
        let Some((total, free)) = volumes::space(&tree.path(drive)) else { continue };
        let free_id = tree.pseudo_child(drive, NodeKind::FreeSpace);
        let unknown_id = tree.pseudo_child(drive, NodeKind::Unknown);
        if let Some(id) = free_id {
            tree.set_pseudo_size(id, free);
        }
        if let Some(id) = unknown_id {
            // Unknown = used space not accounted for by scanned items.
            let mut scanned = tree.node(drive).physical;
            for pid in [free_id, unknown_id].into_iter().flatten() {
                if !tree.node(pid).is_gone() {
                    scanned -= tree.node(pid).physical;
                }
            }
            tree.set_pseudo_size(id, total.saturating_sub(free).saturating_sub(scanned));
        }
    }
}

/// Rescan the directory `id` (or the folder holding a file) and replace its
/// subtree. Returns the node that was refreshed.
pub fn refresh(tree: &mut Tree, id: NodeId, options: &ScanOptions, control: &ScanControl) -> Result<NodeId, ScanError> {
    let mut target = id;
    while target != ROOT && !tree.node(target).is_dir() {
        target = tree.node(target).parent;
    }
    if tree.node(target).flags & flags::MY_COMPUTER != 0 {
        for drive in tree.drives() {
            refresh(tree, drive, options, control)?;
        }
        return Ok(target);
    }

    let filters = Filters::new(options).map_err(ScanError::BadPattern)?;
    let display = tree.path(target);
    let (_, extended) = normalize(&display);
    let Some(attributes) = windows::attributes(&extended).filter(|a| a & FILE_ATTRIBUTE_DIRECTORY != 0) else {
        if target == ROOT {
            return Err(ScanError::NotADirectory(display));
        }
        tree.remove(target);
        return Ok(target);
    };

    let errors_before = control.progress().errors;
    let node = tree.node(target);
    let keep_flags = node.flags & (flags::DRIVE | flags::MY_COMPUTER | flags::NAME_LOSSY);
    let ctx = Context {
        control,
        options,
        filters,
        cluster_size: windows::cluster_size(&volume_root(&extended)),
    };
    let mut raw = with_pool(options, || {
        scan_dir(
            &ctx,
            &extended,
            RawDir {
                name: node.name.clone(),
                attributes,
                flags: keep_flags,
                modified: node.modified,
                ..RawDir::default()
            },
        )
    });
    if keep_flags & flags::DRIVE != 0 {
        let shown = |kind| {
            tree.pseudo_child(target, kind)
                .is_some_and(|c| !tree.node(c).is_gone())
        };
        attach_pseudo(&mut raw, &display, shown(NodeKind::FreeSpace), shown(NodeKind::Unknown));
    }
    graft(tree, target, raw);
    tree.errors += control.progress().errors - errors_before;
    Ok(target)
}

fn scan_dir(ctx: &Context<'_>, path: &[u16], mut dir: RawDir) -> RawDir {
    ctx.control.wait_if_paused();
    if ctx.control.should_stop() {
        dir.flags |= flags::INCOMPLETE;
        return dir;
    }

    let options = ctx.options;
    let mut pending: Vec<(Vec<u16>, RawDir)> = Vec::new();
    let listed = windows::read_dir(path, |e| {
        let (name, lossy) = match String::from_utf16(e.name) {
            Ok(s) => (s, false),
            Err(_) => (String::from_utf16_lossy(e.name), true),
        };
        let reparse_tag = if e.attributes & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            e.reparse_tag
        } else {
            0
        };
        let hidden = e.attributes & FILE_ATTRIBUTE_HIDDEN != 0;
        let protected = hidden && e.attributes & FILE_ATTRIBUTE_SYSTEM != 0;

        if e.attributes & FILE_ATTRIBUTE_DIRECTORY != 0 {
            if (options.exclude_hidden_dirs && hidden)
                || (options.exclude_protected_dirs && protected)
                || ctx.filters.excludes_dir(&name, || display_path(path, e.name))
            {
                return;
            }
            let mut child = RawDir {
                name: name.into_boxed_str(),
                modified: e.last_write,
                attributes: e.attributes,
                flags: if lossy { flags::NAME_LOSSY } else { 0 },
                ..RawDir::default()
            };
            // Junctions and directory symlinks point elsewhere; following them
            // can count data twice or loop. Other reparse directories (such
            // as OneDrive folders) hold real content and are scanned.
            let skip = (reparse_tag == IO_REPARSE_TAG_MOUNT_POINT && !options.follow_junctions)
                || (reparse_tag == IO_REPARSE_TAG_SYMLINK && !options.follow_symlink_dirs);
            if skip {
                child.flags |= flags::REPARSE_SKIPPED;
                child.newest = e.last_write;
                dir.dirs.push(child);
            } else {
                pending.push((join(path, e.name), child));
            }
            return;
        }

        if (reparse_tag == IO_REPARSE_TAG_SYMLINK && !options.count_symlink_files)
            || (options.exclude_hidden_files && hidden)
            || (options.exclude_protected_files && protected)
            || e.end_of_file < options.min_file_size
            || ctx.filters.excludes_file(&name, || display_path(path, e.name))
        {
            return;
        }

        let mut attributes = e.attributes;
        if reparse_tag == IO_REPARSE_TAG_WOF {
            attributes |= FILE_ATTRIBUTE_COMPRESSED;
        }
        let mut physical = e.allocation_size;
        // Same correction as WinDirStat: compressed, sparse and cloud files can
        // report no allocation in the directory entry.
        if physical == 0
            && (e.end_of_file > ctx.cluster_size
                || attributes & (FILE_ATTRIBUTE_SPARSE_FILE | FILE_ATTRIBUTE_COMPRESSED) != 0)
        {
            if let Some(size) = windows::compressed_size(&join(path, e.name)) {
                physical = size;
            }
        }
        dir.files.push(RawFile {
            name: name.into_boxed_str(),
            logical: e.end_of_file,
            physical,
            modified: e.last_write,
            attributes,
            flags: if lossy { flags::NAME_LOSSY } else { 0 },
            kind: NodeKind::File,
        });
    });

    if listed.is_err() {
        dir.flags |= flags::INACCESSIBLE;
        ctx.control.errors.fetch_add(1, Ordering::Relaxed);
    }
    ctx.control.dirs.fetch_add(1, Ordering::Relaxed);
    ctx.control.files.fetch_add(dir.files.len() as u64, Ordering::Relaxed);
    ctx.control.bytes.fetch_add(dir.files.iter().map(|f| f.physical).sum(), Ordering::Relaxed);

    let scanned: Vec<RawDir> = pending
        .into_par_iter()
        .map(|(child_path, child)| scan_dir(ctx, &child_path, child))
        .collect();
    dir.dirs.extend(scanned);

    dir.newest = dir.modified;
    for f in &dir.files {
        dir.logical += f.logical;
        dir.physical += f.physical;
        dir.newest = dir.newest.max(f.modified);
    }
    dir.file_count = dir.files.len() as u32;
    for d in &dir.dirs {
        dir.logical += d.logical;
        dir.physical += d.physical;
        dir.file_count += d.file_count;
        dir.dir_count += d.dir_count + 1;
        dir.newest = dir.newest.max(d.newest);
        if d.flags & flags::INCOMPLETE != 0 {
            dir.flags |= flags::INCOMPLETE;
        }
    }
    dir
}

fn dir_node(d: &RawDir, parent: u32) -> Node {
    Node {
        name: d.name.clone(),
        logical: d.logical,
        physical: d.physical,
        modified: d.newest,
        parent,
        first_child: NO_NODE,
        child_count: 0,
        files: d.file_count,
        dirs: d.dir_count,
        attributes: d.attributes,
        ext: NO_EXT,
        kind: NodeKind::Dir,
        flags: d.flags,
    }
}

/// Appends raw directories to an arena breadth-first, so every directory's
/// children end up contiguous and stored largest first.
struct Builder {
    nodes: Vec<Node>,
    exts: ExtInterner,
}

impl Builder {
    fn expand(&mut self, raw: RawDir, id: NodeId) {
        enum Child {
            File(RawFile),
            Dir(RawDir),
        }
        impl Child {
            fn physical(&self) -> u64 {
                match self {
                    Child::File(f) => f.physical,
                    Child::Dir(d) => d.physical,
                }
            }
        }

        let mut queue = VecDeque::from([(raw, id)]);
        while let Some((mut raw, id)) = queue.pop_front() {
            let mut children: Vec<Child> = std::mem::take(&mut raw.files)
                .into_iter()
                .map(Child::File)
                .chain(std::mem::take(&mut raw.dirs).into_iter().map(Child::Dir))
                .collect();
            children.sort_by_key(|c| Reverse(c.physical()));

            let first = self.nodes.len() as u32;
            self.nodes[id as usize].first_child = first;
            self.nodes[id as usize].child_count = children.len() as u32;
            for child in children {
                let child_id = self.nodes.len() as u32;
                match child {
                    Child::File(f) => {
                        let ext = if f.kind == NodeKind::File { self.exts.intern(&f.name) } else { NO_EXT };
                        self.nodes.push(Node {
                            name: f.name,
                            logical: f.logical,
                            physical: f.physical,
                            modified: f.modified,
                            parent: id,
                            first_child: NO_NODE,
                            child_count: 0,
                            files: (f.kind == NodeKind::File) as u32,
                            dirs: 0,
                            attributes: f.attributes,
                            ext,
                            kind: f.kind,
                            flags: f.flags,
                        });
                    }
                    Child::Dir(d) => {
                        self.nodes.push(dir_node(&d, id));
                        queue.push_back((d, child_id));
                    }
                }
            }
        }
    }
}

fn flatten(root: RawDir) -> Tree {
    let total = 1 + root.file_count as usize + root.dir_count as usize;
    let mut builder = Builder {
        nodes: Vec::with_capacity(total),
        exts: ExtInterner::default(),
    };
    builder.nodes.push(dir_node(&root, NO_NODE));
    builder.expand(root, ROOT);
    Tree {
        nodes: builder.nodes,
        extensions: builder.exts.names,
        errors: 0,
    }
}

/// Replace the subtree under `target` with a fresh scan of it.
fn graft(tree: &mut Tree, target: NodeId, raw: RawDir) {
    let old = tree.node(target);
    let (old_l, old_p, old_f, old_d) = (old.logical as i64, old.physical as i64, old.files as i64, old.dirs as i64);
    let (new_l, new_p, new_f, new_d) = (raw.logical as i64, raw.physical as i64, raw.file_count as i64, raw.dir_count as i64);

    // Retire the old subtree so lingering references (selection, duplicate
    // lists) see it as deleted. Totals are replaced wholesale below.
    let mut stack: Vec<NodeId> = tree.children_range(target).collect();
    while let Some(id) = stack.pop() {
        if tree.node(id).is_dir() {
            stack.extend(tree.children_range(id));
        }
        tree.nodes[id as usize].flags |= flags::DELETED;
    }

    let mut builder = Builder {
        nodes: std::mem::take(&mut tree.nodes),
        exts: ExtInterner::from_existing(std::mem::take(&mut tree.extensions)),
    };
    {
        let n = &mut builder.nodes[target as usize];
        n.logical = raw.logical;
        n.physical = raw.physical;
        n.files = raw.file_count;
        n.dirs = raw.dir_count;
        n.modified = raw.newest;
        n.attributes = raw.attributes;
        n.flags = (n.flags & flags::HIDDEN) | raw.flags;
    }
    builder.expand(raw, target);
    tree.nodes = builder.nodes;
    tree.extensions = builder.exts.names;
    if !tree.node(target).is_gone() {
        tree.adjust_ancestors(target, new_l - old_l, new_p - old_p, new_f - old_f, new_d - old_d);
    }
}

/// One row of saved results, as read by `csv::import`.
pub struct ImportRow {
    /// Full path without a trailing backslash ("C:" for a drive root), or
    /// "This PC" for a multi-drive root.
    pub path: String,
    /// `None` when the file doesn't say; rows containing other rows are folders.
    pub is_dir: Option<bool>,
    pub logical: u64,
    pub physical: u64,
    pub modified: i64,
    pub attributes: u32,
}

/// Rebuild a tree from saved rows. The first row is the root.
pub fn build_from_rows(rows: Vec<ImportRow>) -> Result<Tree, String> {
    use std::collections::{HashMap, HashSet};

    let first = rows.first().ok_or("The file has no rows")?;
    let my_computer = first.path == "This PC";
    let key = |p: &str| p.to_lowercase();
    let parent_key = |p: &str| p.rfind('\\').map(|i| key(&p[..i]));
    let containers: HashSet<String> = rows.iter().filter_map(|r| parent_key(&r.path)).collect();
    let display = |p: &str| if p.len() == 2 && p.ends_with(':') { format!("{p}\\") } else { p.to_string() };

    // Folders by index; each knows its subfolders by index until assembled.
    let mut dirs: Vec<Option<RawDir>> = Vec::new();
    let mut subdirs: Vec<Vec<usize>> = Vec::new();
    let mut index: HashMap<String, usize> = HashMap::new();
    dirs.push(Some(RawDir {
        name: display(&first.path).into_boxed_str(),
        modified: first.modified,
        attributes: first.attributes,
        flags: if my_computer {
            flags::MY_COMPUTER
        } else if is_drive_root(&display(&first.path)) {
            flags::DRIVE
        } else {
            0
        },
        ..RawDir::default()
    }));
    subdirs.push(Vec::new());
    index.insert(key(&first.path), 0);

    for row in rows.iter().skip(1) {
        let parent = match parent_key(&row.path) {
            Some(p) => index.get(&p).copied(),
            None if my_computer => Some(0),
            None => None,
        };
        let Some(parent) = parent else { continue };
        let name = match row.path.rfind('\\') {
            Some(i) => row.path[i + 1..].to_string(),
            None => display(&row.path),
        };
        let is_dir = row.is_dir.unwrap_or_else(|| containers.contains(&key(&row.path)));
        if is_dir {
            let drive = my_computer && parent == 0 && is_drive_root(&name);
            dirs.push(Some(RawDir {
                name: name.into_boxed_str(),
                modified: row.modified,
                attributes: row.attributes,
                flags: if drive { flags::DRIVE } else { 0 },
                ..RawDir::default()
            }));
            subdirs.push(Vec::new());
            let i = dirs.len() - 1;
            subdirs[parent].push(i);
            index.insert(key(&row.path), i);
        } else if let Some(Some(dir)) = dirs.get_mut(parent) {
            dir.files.push(RawFile {
                name: name.into_boxed_str(),
                logical: row.logical,
                physical: row.physical,
                modified: row.modified,
                attributes: row.attributes,
                flags: 0,
                kind: NodeKind::File,
            });
        }
    }

    fn assemble(i: usize, dirs: &mut [Option<RawDir>], subdirs: &[Vec<usize>]) -> RawDir {
        let mut dir = dirs[i].take().expect("each folder is assembled once");
        for &c in &subdirs[i] {
            dir.dirs.push(assemble(c, dirs, subdirs));
        }
        dir.newest = dir.modified;
        for f in &dir.files {
            dir.logical += f.logical;
            dir.physical += f.physical;
            dir.newest = dir.newest.max(f.modified);
        }
        dir.file_count = dir.files.len() as u32;
        for d in &dir.dirs {
            dir.logical += d.logical;
            dir.physical += d.physical;
            dir.file_count += d.file_count;
            dir.dir_count += d.dir_count + 1;
            dir.newest = dir.newest.max(d.newest);
        }
        dir
    }
    Ok(flatten(assemble(0, &mut dirs, &subdirs)))
}

fn join(parent: &[u16], name: &[u16]) -> Vec<u16> {
    let mut p = Vec::with_capacity(parent.len() + name.len() + 1);
    p.extend_from_slice(parent);
    if p.last() != Some(&BACKSLASH) {
        p.push(BACKSLASH);
    }
    p.extend_from_slice(name);
    p
}

/// Display path of `name` inside the extended-length directory `parent`.
fn display_path(parent: &[u16], name: &[u16]) -> String {
    let full = String::from_utf16_lossy(&join(parent, name));
    match full.strip_prefix(r"\\?\UNC\") {
        Some(unc) => format!(r"\\{unc}"),
        None => full.strip_prefix(r"\\?\").unwrap_or(&full).to_string(),
    }
}

fn is_drive_root(display: &str) -> bool {
    let b = display.as_bytes();
    b.len() == 3 && b[0].is_ascii_alphabetic() && b[1] == b':' && b[2] == b'\\'
}

/// Display path plus the extended-length form used for Win32 calls.
/// `C:` and `C:\` both mean the drive root; other trailing separators go.
pub fn normalize(root: &str) -> (String, Vec<u16>) {
    let mut display = root.trim().replace('/', "\\");
    if let Ok(abs) = std::path::absolute(&display) {
        display = abs.to_string_lossy().into_owned();
    }
    while display.len() > 3 && display.ends_with('\\') {
        display.pop();
    }
    if display.len() == 2 && display.ends_with(':') {
        display.push('\\');
    }

    let extended = if display.starts_with(r"\\?\") {
        display.clone()
    } else if let Some(unc) = display.strip_prefix(r"\\") {
        format!(r"\\?\UNC\{unc}")
    } else {
        format!(r"\\?\{display}")
    };
    (display, extended.encode_utf16().collect())
}

/// `\\?\C:\` for `\\?\C:\Users\x`; `\\?\UNC\srv\share\` for UNC paths.
fn volume_root(extended: &[u16]) -> Vec<u16> {
    let s = String::from_utf16_lossy(extended);
    let rest = &s[4..];
    let root = if let Some(unc) = rest.strip_prefix(r"UNC\") {
        let parts: Vec<&str> = unc.splitn(3, '\\').collect();
        format!(r"\\?\UNC\{}\{}\", parts[0], parts.get(1).copied().unwrap_or(""))
    } else {
        format!(r"\\?\{}\", &rest[..2.min(rest.len())])
    };
    root.encode_utf16().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_tree(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("disklens-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("a/b")).unwrap();
        std::fs::write(dir.join("one.txt"), vec![0u8; 10]).unwrap();
        std::fs::write(dir.join("a/two.bin"), vec![0u8; 5000]).unwrap();
        std::fs::write(dir.join("a/b/three.TXT"), vec![0u8; 3]).unwrap();
        dir
    }

    #[test]
    fn normalizes_drive_roots_and_trailing_separators() {
        assert_eq!(normalize("C:").0, r"C:\");
        assert_eq!(normalize(r"C:\").0, r"C:\");
        assert_eq!(normalize(r"C:\Windows\\").0, r"C:\Windows");
        let ext = String::from_utf16(&normalize(r"C:\Windows").1).unwrap();
        assert_eq!(ext, r"\\?\C:\Windows");
        let unc = String::from_utf16(&normalize(r"\\srv\share\dir").1).unwrap();
        assert_eq!(unc, r"\\?\UNC\srv\share\dir");
        assert!(is_drive_root(r"D:\"));
        assert!(!is_drive_root(r"D:\x"));
    }

    #[test]
    fn volume_roots() {
        let v = |p: &str| String::from_utf16(&volume_root(&normalize(p).1)).unwrap();
        assert_eq!(v(r"C:\Windows\System32"), r"\\?\C:\");
        assert_eq!(v(r"\\srv\share\dir"), r"\\?\UNC\srv\share\");
    }

    #[test]
    fn scans_a_directory_tree() {
        let dir = temp_tree("scan");
        let control = ScanControl::default();
        let tree = scan(dir.to_str().unwrap(), &ScanOptions::default(), &control).unwrap();
        let root = tree.root();
        assert_eq!(root.files, 3);
        assert_eq!(root.dirs, 2);
        assert_eq!(root.logical, 5013);
        // Largest child first: directory "a" (5003 bytes) before one.txt.
        let kids: Vec<_> = tree.children(0).map(|c| &*tree.node(c).name).collect();
        assert_eq!(kids, ["a", "one.txt"]);
        let txt = tree.extensions.iter().filter(|e| &***e == ".txt").count();
        assert_eq!(txt, 1);
        let three = tree.nodes.iter().position(|n| &*n.name == "three.TXT").unwrap() as u32;
        assert!(tree.path(three).ends_with(r"a\b\three.TXT"));
        assert_eq!(control.progress().files, 3);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn exclusion_filters_and_minimum_size() {
        let dir = temp_tree("filter");
        let options = ScanOptions {
            exclude_dirs: vec!["b".into()],
            exclude_files: vec!["*.txt".into()],
            ..ScanOptions::default()
        };
        let tree = scan(dir.to_str().unwrap(), &options, &ScanControl::default()).unwrap();
        let names: Vec<_> = tree.nodes.iter().map(|n| n.name.to_string()).collect();
        assert!(names.contains(&"two.bin".to_string()));
        assert!(!names.iter().any(|n| n == "b" || n.ends_with(".txt") || n.ends_with(".TXT")));

        let options = ScanOptions { min_file_size: 100, ..ScanOptions::default() };
        let tree = scan(dir.to_str().unwrap(), &options, &ScanControl::default()).unwrap();
        assert_eq!(tree.root().files, 1);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn refresh_grafts_a_rescanned_subtree() {
        let dir = temp_tree("refresh");
        let options = ScanOptions::default();
        let control = ScanControl::default();
        let mut tree = scan(dir.to_str().unwrap(), &options, &control).unwrap();
        let a = tree.nodes.iter().position(|n| &*n.name == "a").unwrap() as u32;

        std::fs::write(dir.join("a/new.dat"), vec![0u8; 1000]).unwrap();
        std::fs::remove_file(dir.join("a/two.bin")).unwrap();
        refresh(&mut tree, a, &options, &control).unwrap();

        assert_eq!(tree.node(a).logical, 1003);
        assert_eq!(tree.root().logical, 1013);
        assert_eq!(tree.root().files, 3);
        let kids: Vec<_> = tree.children(a).map(|c| tree.node(c).name.to_string()).collect();
        assert!(kids.contains(&"new.dat".to_string()) && !kids.contains(&"two.bin".to_string()));

        // Refreshing a folder that vanished removes it.
        std::fs::remove_dir_all(dir.join("a")).unwrap();
        refresh(&mut tree, a, &options, &control).unwrap();
        assert!(tree.node(a).is_deleted());
        assert_eq!(tree.root().logical, 10);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn scans_several_roots_under_this_pc() {
        let a = temp_tree("multi-a");
        let b = temp_tree("multi-b");
        let roots = [a.to_string_lossy().into_owned(), b.to_string_lossy().into_owned()];
        let tree = scan_many(&roots, &ScanOptions::default(), &ScanControl::default()).unwrap();
        assert!(tree.root().flags & flags::MY_COMPUTER != 0);
        assert_eq!(tree.root().files, 6);
        let first = tree.children_range(ROOT).start;
        assert_eq!(tree.path(first), tree.node(first).name.to_string());
        std::fs::remove_dir_all(&a).unwrap();
        std::fs::remove_dir_all(&b).unwrap();
    }

    #[test]
    fn rejects_missing_paths() {
        let control = ScanControl::default();
        assert!(scan(r"C:\definitely\not\here\disklens", &ScanOptions::default(), &control).is_err());
    }
}
