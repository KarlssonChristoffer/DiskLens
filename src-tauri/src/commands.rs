//! Commands the UI invokes for scanning, browsing, the treemap and settings.
//! Long operations run off the UI thread and report through Tauri channels.

use std::path::PathBuf;
use std::sync::mpsc::{RecvTimeoutError, channel};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};

use disklens_core::csv::attributes;
use disklens_core::dupes::{self, DupeControl, DupeOptions};
use disklens_core::scan::{self, ScanControl};
use disklens_core::search::{self, SearchOptions};
use disklens_core::tree::{NodeId, ROOT, flags};
use disklens_core::treemap::{self, Rect};
use disklens_core::volumes::{self, Volume};
use disklens_core::{NodeKind, SizeMode, Tree, fileops, top};
use serde::{Deserialize, Serialize};
use tauri::ipc::{Channel, Response};
use tauri::State;

use crate::session::{Session, apply_pseudo};
use crate::settings::Settings;
use crate::shell;

pub struct AppState {
    pub session: RwLock<Option<Arc<Session>>>,
    pub scan: Mutex<Option<Arc<ScanControl>>>,
    pub dupes: Mutex<Option<Arc<DupeControl>>>,
    pub settings: Mutex<Settings>,
    pub settings_path: PathBuf,
}

impl AppState {
    pub fn new(settings_path: PathBuf) -> Self {
        AppState {
            session: RwLock::new(None),
            scan: Mutex::new(None),
            dupes: Mutex::new(None),
            settings: Mutex::new(Settings::load(&settings_path)),
            settings_path,
        }
    }

    pub fn session(&self) -> Result<Arc<Session>, String> {
        self.session.read().unwrap().clone().ok_or_else(|| "No scan loaded".to_string())
    }

    pub fn settings(&self) -> Settings {
        self.settings.lock().unwrap().clone()
    }

    pub fn save_settings(&self) {
        let s = self.settings.lock().unwrap();
        let _ = s.save(&self.settings_path);
    }
}

pub type CmdResult<T> = Result<T, String>;
pub type AppStateRef<'a> = State<'a, Arc<AppState>>;

/// FILETIME (100 ns ticks since 1601) to Unix milliseconds.
fn unix_ms(filetime: i64) -> i64 {
    if filetime <= 0 { 0 } else { (filetime - 116_444_736_000_000_000) / 10_000 }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeInfo {
    id: NodeId,
    name: String,
    kind: &'static str,
    is_dir: bool,
    size: u64,
    logical: u64,
    physical: u64,
    files: u32,
    dirs: u32,
    modified: i64,
    /// Share of the parent's size, 0..1.
    share: f64,
    has_children: bool,
    ext: String,
    color: Option<[u8; 3]>,
    flags: u8,
    attributes: u32,
    attribute_text: String,
}

pub fn node_info(tree: &Tree, id: NodeId, mode: SizeMode, colors: &[[u8; 3]]) -> NodeInfo {
    let n = tree.node(id);
    let size = n.size(mode);
    let parent_size = if id == ROOT { size } else { tree.node(n.parent).size(mode) };
    NodeInfo {
        id,
        name: n.name.to_string(),
        kind: match n.kind {
            NodeKind::Dir => "dir",
            NodeKind::File => "file",
            NodeKind::FreeSpace => "free",
            NodeKind::Unknown => "unknown",
        },
        is_dir: n.is_dir(),
        size,
        logical: n.logical,
        physical: n.physical,
        files: n.files,
        dirs: n.dirs,
        modified: unix_ms(n.modified),
        share: if parent_size > 0 { size as f64 / parent_size as f64 } else { 0.0 },
        has_children: n.is_dir() && tree.children(id).next().is_some(),
        ext: if n.is_file() { tree.extensions[n.ext as usize].to_string() } else { String::new() },
        color: match n.kind {
            NodeKind::File => colors.get(n.ext as usize).copied(),
            NodeKind::FreeSpace => Some([100, 100, 100]),
            NodeKind::Unknown => Some([210, 190, 40]),
            NodeKind::Dir => None,
        },
        flags: n.flags,
        attributes: n.attributes,
        attribute_text: attributes(n.attributes & !0x10),
    }
}

// ---------------------------------------------------------------- scanning

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    root_path: String,
    files: u32,
    dirs: u32,
    logical: u64,
    physical: u64,
    errors: u64,
    elapsed_ms: u64,
    volume_total: Option<u64>,
    volume_free: Option<u64>,
    incomplete: bool,
    from_file: bool,
    my_computer: bool,
    has_drives: bool,
}

pub fn summary(session: &Session) -> Summary {
    let tree = session.tree.read().unwrap();
    let root = tree.root();
    let my_computer = root.flags & flags::MY_COMPUTER != 0;
    let space = if my_computer || session.from_file { None } else { volumes::space(&root.name) };
    Summary {
        root_path: root.name.to_string(),
        files: root.files,
        dirs: root.dirs,
        logical: root.logical,
        physical: root.physical,
        errors: tree.errors,
        elapsed_ms: session.elapsed.as_millis() as u64,
        volume_total: space.map(|s| s.0),
        volume_free: space.map(|s| s.1),
        incomplete: session.incomplete,
        from_file: session.from_file,
        my_computer,
        has_drives: !tree.drives().is_empty(),
    }
}

#[derive(Clone, Serialize)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum ScanEvent {
    Progress { files: u64, dirs: u64, bytes: u64, errors: u64, elapsed_ms: u64, paused: bool },
    Done { summary: Summary },
    Cancelled,
    Failed { message: String },
}

#[tauri::command]
pub fn list_volumes() -> Vec<Volume> {
    volumes::list()
}

/// Install a freshly built tree as the current session.
pub fn install_session(state: &AppState, tree: Tree, elapsed: Duration, incomplete: bool, from_file: bool) -> Summary {
    let settings = state.settings();
    let mut session = Session::new(tree, settings.view, elapsed, settings.show_free_space, settings.show_unknown);
    session.incomplete = incomplete;
    session.from_file = from_file;
    let session = Arc::new(session);
    let s = summary(&session);
    *state.session.write().unwrap() = Some(session);
    s
}

#[tauri::command]
pub fn start_scan(paths: Vec<String>, events: Channel<ScanEvent>, state: AppStateRef<'_>) -> CmdResult<()> {
    let control = Arc::new(ScanControl::default());
    {
        let mut current = state.scan.lock().unwrap();
        if current.is_some() {
            return Err("A scan is already running".into());
        }
        *current = Some(control.clone());
    }
    if let Some(c) = state.dupes.lock().unwrap().take() {
        c.cancel();
    }
    {
        let mut settings = state.settings.lock().unwrap();
        for p in &paths {
            settings.remember_folder(p);
        }
    }
    state.save_settings();
    let options = state.settings().scan;

    let state = Arc::clone(&state);
    std::thread::spawn(move || {
        let started = Instant::now();
        let (tx, rx) = channel();
        let scanner = control.clone();
        std::thread::spawn(move || {
            let _ = tx.send(scan::scan_many(&paths, &options, &scanner));
        });

        let result = loop {
            match rx.recv_timeout(Duration::from_millis(100)) {
                Ok(result) => break Some(result),
                Err(RecvTimeoutError::Timeout) => {
                    let p = control.progress();
                    let _ = events.send(ScanEvent::Progress {
                        files: p.files,
                        dirs: p.dirs,
                        bytes: p.bytes,
                        errors: p.errors,
                        elapsed_ms: started.elapsed().as_millis() as u64,
                        paused: p.paused,
                    });
                }
                Err(RecvTimeoutError::Disconnected) => break None,
            }
        };

        let event = match result {
            _ if control.is_cancelled() => ScanEvent::Cancelled,
            Some(Ok(tree)) => ScanEvent::Done {
                summary: install_session(&state, tree, started.elapsed(), control.is_stopped(), false),
            },
            Some(Err(e)) => ScanEvent::Failed { message: e.to_string() },
            None => ScanEvent::Failed { message: "The scanner stopped unexpectedly".into() },
        };
        *state.scan.lock().unwrap() = None;
        let _ = events.send(event);
    });
    Ok(())
}

fn with_scan(state: &AppState, f: impl FnOnce(&ScanControl)) {
    if let Some(c) = state.scan.lock().unwrap().as_ref() {
        f(c);
    }
}

#[tauri::command]
pub fn cancel_scan(state: AppStateRef<'_>) {
    with_scan(&state, |c| c.cancel());
}

#[tauri::command]
pub fn stop_scan(state: AppStateRef<'_>) {
    with_scan(&state, |c| c.stop());
}

#[tauri::command]
pub fn pause_scan(state: AppStateRef<'_>) {
    with_scan(&state, |c| c.pause());
}

#[tauri::command]
pub fn resume_scan(state: AppStateRef<'_>) {
    with_scan(&state, |c| c.resume());
}

#[tauri::command]
pub fn get_summary(state: AppStateRef<'_>) -> CmdResult<Summary> {
    let session = state.session()?;
    Ok(summary(&session))
}

/// Rescan items in place (WinDirStat's Refresh Selected, F5).
#[tauri::command(async)]
pub fn refresh_items(ids: Vec<NodeId>, state: AppStateRef<'_>) -> CmdResult<Summary> {
    let session = state.session()?;
    if session.from_file {
        return Err("Results loaded from a file can't be refreshed; scan the folder instead".into());
    }
    let options = state.settings().scan;
    {
        let mut tree = session.tree.write().unwrap();
        let control = ScanControl::default();
        for id in ids {
            if (id as usize) < tree.nodes.len() && !tree.has_flag_in_ancestry(id, flags::DELETED) {
                scan::refresh(&mut tree, id, &options, &control).map_err(|e| e.to_string())?;
            }
        }
        scan::update_pseudo(&mut tree);
    }
    session.refresh_ext();
    Ok(summary(&session))
}

// ---------------------------------------------------------------- browsing

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SortKey {
    Size,
    Name,
    Files,
    Folders,
    Items,
    Modified,
    Logical,
    Physical,
    Attributes,
}

#[derive(Serialize)]
pub struct Page {
    total: usize,
    items: Vec<NodeInfo>,
}

#[tauri::command]
pub fn list_children(
    id: NodeId,
    sort: SortKey,
    descending: bool,
    offset: usize,
    limit: usize,
    state: AppStateRef<'_>,
) -> CmdResult<Page> {
    let session = state.session()?;
    let tree = session.tree.read().unwrap();
    let view = session.view.lock().unwrap();
    let mode = view.mode();
    if id as usize >= tree.nodes.len() {
        return Err("Unknown item".into());
    }
    let mut kids: Vec<NodeId> = tree.children(id).collect();
    let node = |c: &NodeId| tree.node(*c);
    match sort {
        SortKey::Size => kids.sort_by_key(|c| node(c).size(mode)),
        SortKey::Name => kids.sort_by_cached_key(|c| node(c).name.to_lowercase()),
        SortKey::Files => kids.sort_by_key(|c| node(c).files),
        SortKey::Folders => kids.sort_by_key(|c| node(c).dirs),
        SortKey::Items => kids.sort_by_key(|c| node(c).files + node(c).dirs),
        SortKey::Modified => kids.sort_by_key(|c| node(c).modified),
        SortKey::Logical => kids.sort_by_key(|c| node(c).logical),
        SortKey::Physical => kids.sort_by_key(|c| node(c).physical),
        SortKey::Attributes => kids.sort_by_cached_key(|c| attributes(node(c).attributes)),
    }
    if descending {
        kids.reverse();
    }
    let total = kids.len();
    let items = kids
        .into_iter()
        .skip(offset)
        .take(limit)
        .map(|c| node_info(&tree, c, mode, &view.ext.colors))
        .collect();
    Ok(Page { total, items })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeDetail {
    info: NodeInfo,
    path: String,
    /// Ids from the root down to this node.
    ancestry: Vec<NodeId>,
    /// False once the item or a folder containing it has been removed.
    alive: bool,
}

#[tauri::command]
pub fn node_detail(id: NodeId, state: AppStateRef<'_>) -> CmdResult<NodeDetail> {
    let session = state.session()?;
    let tree = session.tree.read().unwrap();
    let view = session.view.lock().unwrap();
    if id as usize >= tree.nodes.len() {
        return Err("Unknown item".into());
    }
    let n = tree.node(id);
    Ok(NodeDetail {
        info: node_info(&tree, id, view.mode(), &view.ext.colors),
        path: if n.is_pseudo() || (id == ROOT && n.flags & flags::MY_COMPUTER != 0) { n.name.to_string() } else { tree.path(id) },
        ancestry: tree.ancestry(id),
        alive: !tree.has_flag_in_ancestry(id, flags::DELETED),
    })
}

/// Owners for the Owner column, looked up only for visible rows.
#[tauri::command(async)]
pub fn owners(ids: Vec<NodeId>, state: AppStateRef<'_>) -> CmdResult<Vec<Option<String>>> {
    let session = state.session()?;
    if session.from_file {
        return Ok(vec![None; ids.len()]);
    }
    let paths: Vec<Option<String>> = {
        let tree = session.tree.read().unwrap();
        ids.iter()
            .map(|&id| {
                let valid = (id as usize) < tree.nodes.len();
                let has_path = valid
                    && !tree.node(id).is_pseudo()
                    && !(id == ROOT && tree.root().flags & flags::MY_COMPUTER != 0);
                has_path.then(|| tree.path(id))
            })
            .collect()
    };
    Ok(paths.into_iter().map(|p| p.and_then(|p| fileops::owner(&p))).collect())
}

#[tauri::command]
pub fn ext_stats(state: AppStateRef<'_>) -> CmdResult<Vec<disklens_core::ext::ExtStat>> {
    let session = state.session()?;
    let view = session.view.lock().unwrap();
    Ok(view.ext.stats.clone())
}

#[derive(Serialize)]
pub struct FileEntry {
    pub info: NodeInfo,
    pub path: String,
}

#[tauri::command]
pub fn largest_files(from: NodeId, count: usize, state: AppStateRef<'_>) -> CmdResult<Vec<FileEntry>> {
    let session = state.session()?;
    let tree = session.tree.read().unwrap();
    let view = session.view.lock().unwrap();
    Ok(top::largest_files(&tree, from, view.mode(), count.min(10_000))
        .into_iter()
        .map(|id| FileEntry { info: node_info(&tree, id, view.mode(), &view.ext.colors), path: tree.path(id) })
        .collect())
}

#[tauri::command]
pub fn search_items(query: String, from: NodeId, options: SearchOptions, state: AppStateRef<'_>) -> CmdResult<Vec<FileEntry>> {
    {
        let mut settings = state.settings.lock().unwrap();
        settings.search = options.clone();
    }
    state.save_settings();
    let session = state.session()?;
    let tree = session.tree.read().unwrap();
    let view = session.view.lock().unwrap();
    let from = if (from as usize) < tree.nodes.len() { from } else { ROOT };
    let found = search::search(&tree, from, &query, &options)?;
    Ok(found
        .into_iter()
        .map(|id| FileEntry { info: node_info(&tree, id, view.mode(), &view.ext.colors), path: tree.path(id) })
        .collect())
}

// ---------------------------------------------------------------- treemap

#[tauri::command]
pub fn render_treemap(
    root: NodeId,
    width: u32,
    height: u32,
    highlight: Option<u32>,
    state: AppStateRef<'_>,
) -> CmdResult<Response> {
    let session = state.session()?;
    let tree = session.tree.read().unwrap();
    let mut view = session.view.lock().unwrap();
    let (width, height) = (width.min(8192), height.min(8192));
    let root = if (root as usize) < tree.nodes.len() && !tree.has_flag_in_ancestry(root, flags::DELETED) {
        root
    } else {
        ROOT
    };
    let mode = view.mode();
    let options = view.options.treemap;
    let view = &mut *view;
    let pixels = treemap::render(&tree, root, width, height, mode, &options, &view.ext.colors, highlight, &mut view.layout);
    Ok(Response::new(pixels))
}

#[tauri::command]
pub fn treemap_hit(x: i32, y: i32, state: AppStateRef<'_>) -> CmdResult<Option<NodeId>> {
    let session = state.session()?;
    let tree = session.tree.read().unwrap();
    let view = session.view.lock().unwrap();
    Ok(view.layout.hit(&tree, x, y))
}

#[tauri::command]
pub fn treemap_rect(id: NodeId, state: AppStateRef<'_>) -> CmdResult<Option<Rect>> {
    let session = state.session()?;
    let view = session.view.lock().unwrap();
    Ok(view.layout.rect(id))
}

// ---------------------------------------------------------------- settings

#[tauri::command]
pub fn get_settings(state: AppStateRef<'_>) -> Settings {
    state.settings()
}

/// Store new settings and apply what affects the loaded scan.
#[tauri::command]
pub fn set_settings(settings: Settings, state: AppStateRef<'_>) -> CmdResult<()> {
    let old = std::mem::replace(&mut *state.settings.lock().unwrap(), settings.clone());
    settings.save(&state.settings_path)?;
    if let Ok(session) = state.session() {
        let pseudo_changed = old.show_free_space != settings.show_free_space || old.show_unknown != settings.show_unknown;
        if pseudo_changed {
            apply_pseudo(&mut session.tree.write().unwrap(), settings.show_free_space, settings.show_unknown);
        }
        session.view.lock().unwrap().options = settings.view;
        session.refresh_ext();
    }
    Ok(())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    elevated: bool,
    version: &'static str,
    accent: Option<String>,
    hibernation_file: bool,
}

#[tauri::command]
pub fn app_info() -> AppInfo {
    AppInfo {
        elevated: fileops::is_elevated(),
        version: env!("CARGO_PKG_VERSION"),
        accent: shell::accent_color(),
        hibernation_file: fileops::hibernation_file_exists(),
    }
}

// ---------------------------------------------------------------- duplicates

#[derive(Clone, Serialize)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum DupeEvent {
    Progress { candidates: u64, files_hashed: u64, bytes_hashed: u64, elapsed_ms: u64 },
    Done { groups: usize, wasted: u64, elapsed_ms: u64 },
    Cancelled,
}

#[tauri::command]
pub fn find_duplicates(min_size: u64, events: Channel<DupeEvent>, state: AppStateRef<'_>) -> CmdResult<()> {
    let session = state.session()?;
    if session.from_file {
        return Err("Duplicates need the files themselves; scan the folder instead of loading results".into());
    }
    let control = Arc::new(DupeControl::default());
    {
        let mut current = state.dupes.lock().unwrap();
        if current.is_some() {
            return Err("A duplicate search is already running".into());
        }
        *current = Some(control.clone());
    }

    let state = Arc::clone(&state);
    std::thread::spawn(move || {
        let started = Instant::now();
        let (tx, rx) = channel();
        let worker = control.clone();
        let searched = session.clone();
        std::thread::spawn(move || {
            let tree = searched.tree.read().unwrap();
            let _ = tx.send(dupes::find(&tree, ROOT, &DupeOptions { min_size }, &worker));
        });
        let groups = loop {
            match rx.recv_timeout(Duration::from_millis(150)) {
                Ok(groups) => break Some(groups),
                Err(RecvTimeoutError::Timeout) => {
                    let p = control.progress();
                    let _ = events.send(DupeEvent::Progress {
                        candidates: p.candidates,
                        files_hashed: p.files_hashed,
                        bytes_hashed: p.bytes_hashed,
                        elapsed_ms: started.elapsed().as_millis() as u64,
                    });
                }
                Err(RecvTimeoutError::Disconnected) => break None,
            }
        };
        let event = match groups {
            Some(groups) if !control.is_cancelled() => {
                let wasted = groups.iter().map(|g| g.wasted()).sum();
                let count = groups.len();
                *session.dupes.lock().unwrap() = Some(groups);
                DupeEvent::Done { groups: count, wasted, elapsed_ms: started.elapsed().as_millis() as u64 }
            }
            _ => DupeEvent::Cancelled,
        };
        *state.dupes.lock().unwrap() = None;
        let _ = events.send(event);
    });
    Ok(())
}

#[tauri::command]
pub fn cancel_duplicates(state: AppStateRef<'_>) {
    if let Some(c) = state.dupes.lock().unwrap().as_ref() {
        c.cancel();
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DupeGroupDto {
    size: u64,
    wasted: u64,
    hash: String,
    files: Vec<FileEntry>,
}

#[tauri::command]
pub fn get_duplicates(state: AppStateRef<'_>) -> CmdResult<Option<Vec<DupeGroupDto>>> {
    let session = state.session()?;
    let tree = session.tree.read().unwrap();
    let view = session.view.lock().unwrap();
    let dupes = session.dupes.lock().unwrap();
    Ok(dupes.as_ref().map(|groups| {
        groups
            .iter()
            .map(|g| DupeGroupDto {
                size: g.size,
                wasted: g.wasted(),
                hash: g.hash.clone(),
                files: g
                    .files
                    .iter()
                    .map(|&id| FileEntry { info: node_info(&tree, id, view.mode(), &view.ext.colors), path: tree.path(id) })
                    .collect(),
            })
            .collect()
    }))
}
