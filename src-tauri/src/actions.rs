//! File and system actions: WinDirStat's Clean Up menu and the item
//! context menu. Changes on disk are reflected in the scan afterwards by
//! removing what vanished and rescanning what changed.

use std::ffi::OsString;
use std::os::windows::ffi::OsStringExt;
use std::path::Path;
use std::sync::Arc;

use disklens_core::fileops::{self, Compression, Hashes};
use disklens_core::scan::{self, ScanControl, normalize};
use disklens_core::tree::{NodeId, ROOT, flags};
use disklens_core::{Tree, csv};
use serde::Serialize;
use tauri::{AppHandle, Manager};
use tauri_plugin_opener::OpenerExt;

use crate::commands::{AppStateRef, CmdResult, Summary, install_session, summary};
use crate::session::Session;
use crate::shell::{self, FileOp, Hwnd};

fn hwnd(app: &AppHandle) -> Hwnd {
    app.get_webview_window("main")
        .and_then(|w| w.hwnd().ok())
        .map(|h| h.0 as Hwnd)
        .unwrap_or(0)
}

fn exists(path: &str) -> bool {
    Path::new(&OsString::from_wide(&normalize(path).1)).exists()
}

/// The on-disk path of an item, or why actions can't touch it.
fn actionable(session: &Session, tree: &Tree, id: NodeId) -> CmdResult<String> {
    if session.from_file {
        return Err("These results were loaded from a file; scan the folder to work with its files".into());
    }
    if id as usize >= tree.nodes.len() || tree.has_flag_in_ancestry(id, flags::DELETED) {
        return Err("This item no longer exists".into());
    }
    let n = tree.node(id);
    if n.is_pseudo() || (id == ROOT && n.flags & flags::MY_COMPUTER != 0) {
        return Err(format!("{} isn't a file or folder", n.name));
    }
    if tree.has_flag_in_ancestry(id, flags::NAME_LOSSY) {
        return Err("This item's name contains characters DiskLens cannot pass back to Windows".into());
    }
    Ok(tree.path(id))
}

fn path_of(state: &AppStateRef<'_>, id: NodeId) -> CmdResult<String> {
    let session = state.session()?;
    let tree = session.tree.read().unwrap();
    actionable(&session, &tree, id)
}

/// Folder for "open console here": the item itself, or a file's folder.
fn folder_of(state: &AppStateRef<'_>, id: NodeId) -> CmdResult<String> {
    let session = state.session()?;
    let tree = session.tree.read().unwrap();
    let mut target = id;
    while target != ROOT && !tree.node(target).is_dir() {
        target = tree.node(target).parent;
    }
    actionable(&session, &tree, target)
}

/// After changes on disk: drop items that vanished, rescan the rest, then
/// refresh free space and file-type statistics.
fn reconcile(session: &Session, state: &AppStateRef<'_>, touched: &[(NodeId, String)], rescan: &[NodeId]) -> (Vec<NodeId>, Vec<String>, u64) {
    let options = state.settings().scan;
    let mut removed = Vec::new();
    let mut remaining = Vec::new();
    let mut freed = 0;
    {
        let mut tree = session.tree.write().unwrap();
        let control = ScanControl::default();
        for (id, path) in touched {
            if tree.has_flag_in_ancestry(*id, flags::DELETED) {
                continue;
            }
            if exists(path) {
                remaining.push(path.clone());
                // Part of a folder may be gone; rescan it to show what's left.
                if tree.node(*id).is_dir() {
                    let before = tree.node(*id).physical;
                    if scan::refresh(&mut tree, *id, &options, &control).is_ok() {
                        freed += before.saturating_sub(tree.node(*id).physical);
                    }
                }
            } else {
                freed += tree.node(*id).physical;
                tree.remove(*id);
                removed.push(*id);
            }
        }
        for &id in rescan {
            if !tree.has_flag_in_ancestry(id, flags::DELETED) {
                let _ = scan::refresh(&mut tree, id, &options, &control);
            }
        }
        scan::update_pseudo(&mut tree);
    }
    session.refresh_ext();
    (removed, remaining, freed)
}

fn collect(state: &AppStateRef<'_>, ids: &[NodeId]) -> CmdResult<(Arc<Session>, Vec<(NodeId, String)>)> {
    if state.dupes.lock().unwrap().is_some() {
        return Err("Wait for the duplicate search to finish first".into());
    }
    let session = state.session()?;
    let items = {
        let tree = session.tree.read().unwrap();
        let mut items = Vec::new();
        for &id in ids {
            if id == ROOT {
                return Err("The scanned folder itself can't be removed from here".into());
            }
            items.push((id, actionable(&session, &tree, id)?));
        }
        // Skip items inside other selected items; the parent covers them.
        let selected: std::collections::HashSet<NodeId> = ids.iter().copied().collect();
        items.retain(|(id, _)| !tree.ancestry(*id).iter().rev().skip(1).any(|a| selected.contains(a)));
        items
    };
    Ok((session, items))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Outcome {
    removed: Vec<NodeId>,
    /// Paths that still exist afterwards (skipped, in use, or cancelled).
    remaining: Vec<String>,
    freed: u64,
    summary: Summary,
}

/// Delete items to the Recycle Bin or permanently, through Explorer's
/// engine so Windows can ask about items too big for the bin or in use.
#[tauri::command(async)]
pub fn delete_items(ids: Vec<NodeId>, permanent: bool, app: AppHandle, state: AppStateRef<'_>) -> CmdResult<Outcome> {
    let (session, items) = collect(&state, &ids)?;
    let paths: Vec<String> = items.iter().map(|(_, p)| p.clone()).collect();
    shell::file_operation(hwnd(&app), if permanent { FileOp::Delete } else { FileOp::Recycle }, &paths, None)?;
    let (removed, remaining, freed) = reconcile(&session, &state, &items, &[]);
    Ok(Outcome { removed, remaining, freed, summary: summary(&session) })
}

/// Delete everything inside folders but keep the folders (permanent, as in
/// WinDirStat).
#[tauri::command(async)]
pub fn empty_folders(ids: Vec<NodeId>, app: AppHandle, state: AppStateRef<'_>) -> CmdResult<Outcome> {
    let (session, items) = collect(&state, &ids)?;
    let mut contents = Vec::new();
    for (_, folder) in &items {
        let entries = std::fs::read_dir(OsString::from_wide(&normalize(folder).1)).map_err(|e| format!("{folder}: {e}"))?;
        for e in entries.flatten() {
            contents.push(format!("{}\\{}", folder.trim_end_matches('\\'), e.file_name().to_string_lossy()));
        }
    }
    shell::file_operation(hwnd(&app), FileOp::Delete, &contents, None)?;
    let folders: Vec<NodeId> = items.iter().map(|(id, _)| *id).collect();
    let before: u64 = {
        let tree = session.tree.read().unwrap();
        folders.iter().map(|&f| tree.node(f).physical).sum()
    };
    reconcile(&session, &state, &[], &folders);
    let after: u64 = {
        let tree = session.tree.read().unwrap();
        folders.iter().map(|&f| tree.node(f).physical).sum()
    };
    Ok(Outcome { removed: Vec::new(), remaining: Vec::new(), freed: before.saturating_sub(after), summary: summary(&session) })
}

/// Find the deepest scanned folder containing `path`.
fn node_for_path(tree: &Tree, path: &str) -> Option<NodeId> {
    let lower = path.to_lowercase();
    let mut node = ROOT;
    let root_path = tree.path(ROOT).to_lowercase();
    if tree.root().flags & flags::MY_COMPUTER == 0 && !lower.starts_with(&root_path) {
        return None;
    }
    'down: loop {
        for c in tree.children(node) {
            if !tree.node(c).is_dir() {
                continue;
            }
            let p = tree.path(c).to_lowercase();
            if lower == p || lower.starts_with(&format!("{}\\", p.trim_end_matches('\\'))) {
                node = c;
                continue 'down;
            }
        }
        return (node != ROOT || tree.root().flags & flags::MY_COMPUTER == 0).then_some(node);
    }
}

#[tauri::command(async)]
pub fn move_items(ids: Vec<NodeId>, destination: String, app: AppHandle, state: AppStateRef<'_>) -> CmdResult<Outcome> {
    let (session, items) = collect(&state, &ids)?;
    let paths: Vec<String> = items.iter().map(|(_, p)| p.clone()).collect();
    shell::file_operation(hwnd(&app), FileOp::Move, &paths, Some(&destination))?;
    // If the destination is inside the scan, rescan it to show the arrivals.
    let dest_node = node_for_path(&session.tree.read().unwrap(), &destination);
    let (removed, remaining, _) = reconcile(&session, &state, &items, &dest_node.into_iter().collect::<Vec<_>>());
    Ok(Outcome { removed, remaining, freed: 0, summary: summary(&session) })
}

/// Files under the given items (recursively), for compression and VHDs.
fn files_under(session: &Session, ids: &[NodeId], filter: impl Fn(&str) -> bool) -> CmdResult<Vec<String>> {
    let tree = session.tree.read().unwrap();
    let mut out = Vec::new();
    for &id in ids {
        actionable(session, &tree, id)?;
        tree.for_each_live(id, |fid, n| {
            if n.is_file() && filter(&n.name) && !tree.has_flag_in_ancestry(fid, flags::NAME_LOSSY) {
                out.push(tree.path(fid));
            }
        });
    }
    Ok(out)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchOutcome {
    done: usize,
    failed: Vec<String>,
    summary: Summary,
}

#[tauri::command(async)]
pub fn compress_items(ids: Vec<NodeId>, compression: Compression, state: AppStateRef<'_>) -> CmdResult<BatchOutcome> {
    let session = state.session()?;
    let files = files_under(&session, &ids, |_| true)?;
    let mut failed = Vec::new();
    for f in &files {
        if let Err(e) = fileops::compress_file(f, compression) {
            failed.push(format!("{f}: {e}"));
        }
    }
    reconcile(&session, &state, &[], &ids);
    Ok(BatchOutcome { done: files.len() - failed.len(), failed, summary: summary(&session) })
}

#[tauri::command(async)]
pub fn optimize_vhds(ids: Vec<NodeId>, state: AppStateRef<'_>) -> CmdResult<BatchOutcome> {
    let session = state.session()?;
    let files = files_under(&session, &ids, |name| {
        let n = name.to_lowercase();
        n.ends_with(".vhdx") || n.ends_with(".vhd")
    })?;
    if files.is_empty() {
        return Err("No .vhd or .vhdx files in the selection".into());
    }
    let mut failed = Vec::new();
    for f in &files {
        if let Err(e) = fileops::optimize_vhd(f) {
            failed.push(format!("{f}: {e}"));
        }
    }
    reconcile(&session, &state, &[], &ids);
    Ok(BatchOutcome { done: files.len() - failed.len(), failed, summary: summary(&session) })
}

#[tauri::command(async)]
pub fn compute_hashes(id: NodeId, state: AppStateRef<'_>) -> CmdResult<Hashes> {
    let path = path_of(&state, id)?;
    fileops::compute_hashes(&path)
}

/// Remove items that no longer exist (after Explorer's own menu ran).
#[tauri::command(async)]
pub fn prune_missing(ids: Vec<NodeId>, state: AppStateRef<'_>) -> CmdResult<Summary> {
    let session = state.session()?;
    let items: Vec<(NodeId, String)> = {
        let tree = session.tree.read().unwrap();
        ids.iter().filter_map(|&id| actionable(&session, &tree, id).ok().map(|p| (id, p))).collect()
    };
    let gone: Vec<(NodeId, String)> = items.into_iter().filter(|(_, p)| !exists(p)).collect();
    if !gone.is_empty() {
        reconcile(&session, &state, &gone, &[]);
    }
    Ok(summary(&session))
}

#[tauri::command]
pub fn reveal_item(id: NodeId, state: AppStateRef<'_>) -> CmdResult<()> {
    let path = path_of(&state, id)?;
    tauri_plugin_opener::reveal_item_in_dir(path).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn open_item(id: NodeId, app: AppHandle, state: AppStateRef<'_>) -> CmdResult<()> {
    let path = path_of(&state, id)?;
    app.opener().open_path(path, None::<&str>).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn show_properties(id: NodeId, state: AppStateRef<'_>) -> CmdResult<()> {
    shell::show_properties(&path_of(&state, id)?)
}

#[tauri::command]
pub fn open_console(id: NodeId, powershell: bool, state: AppStateRef<'_>) -> CmdResult<()> {
    shell::open_console(&folder_of(&state, id)?, powershell)
}

/// Explorer's own context menu at a point in the window (CSS pixels).
/// Runs on the UI thread, which owns the window.
#[tauri::command]
pub fn explorer_menu(ids: Vec<NodeId>, x: f64, y: f64, app: AppHandle, state: AppStateRef<'_>) -> CmdResult<bool> {
    use windows::Win32::Foundation::{HWND, POINT};
    use windows::Win32::Graphics::Gdi::ClientToScreen;

    let session = state.session()?;
    let paths: Vec<String> = {
        let tree = session.tree.read().unwrap();
        ids.iter().map(|&id| actionable(&session, &tree, id)).collect::<CmdResult<_>>()?
    };
    let window = app.get_webview_window("main").ok_or("No window")?;
    let scale = window.scale_factor().unwrap_or(1.0);
    let hwnd = hwnd(&app);
    let mut pt = POINT { x: (x * scale) as i32, y: (y * scale) as i32 };
    unsafe {
        let _ = ClientToScreen(HWND(hwnd as *mut _), &mut pt);
    }
    shell::explorer_menu(hwnd, &paths, pt.x, pt.y)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanupCommand {
    /// Commands to be confirmed by the user before running.
    commands: Vec<String>,
}

/// Expand a user-defined cleanup for the selected items (and subfolders when
/// recursing). WinDirStat placeholders: %p %n for the item, %sp %sn for the
/// current subfolder.
fn expand_cleanup(state: &AppStateRef<'_>, index: usize, ids: &[NodeId]) -> CmdResult<Vec<(String, String, NodeId)>> {
    let settings = state.settings();
    let cleanup = settings.user_cleanups.get(index).ok_or("Unknown cleanup")?.clone();
    if !cleanup.enabled {
        return Err(format!("\"{}\" is disabled", cleanup.title));
    }
    let session = state.session()?;
    let tree = session.tree.read().unwrap();
    let name_of = |p: &str| p.trim_end_matches('\\').rsplit('\\').next().unwrap_or(p).to_string();
    let mut runs = Vec::new();
    for &id in ids {
        let path = actionable(&session, &tree, id)?;
        let n = tree.node(id);
        let drive = n.flags & flags::DRIVE != 0;
        let unc = path.starts_with(r"\\");
        let allowed = (drive && cleanup.works_for_drives)
            || (n.is_dir() && !drive && cleanup.works_for_folders)
            || (n.is_file() && cleanup.works_for_files);
        if !allowed || (unc && !cleanup.works_for_unc) {
            return Err(format!("\"{}\" doesn't apply to {}", cleanup.title, n.name));
        }
        let mut targets = vec![(path.clone(), id)];
        if cleanup.recurse && n.is_dir() {
            tree.for_each_live(id, |cid, c| {
                if cid != id && c.is_dir() {
                    targets.push((tree.path(cid), cid));
                }
            });
        }
        for (current, cid) in targets {
            let cmd = cleanup
                .command
                .replace("%sp", "\u{1}sp")
                .replace("%sn", "\u{1}sn")
                .replace("%p", "\u{1}p")
                .replace("%n", "\u{1}n")
                .replace("\u{1}sp", &current)
                .replace("\u{1}sn", &name_of(&current))
                .replace("\u{1}p", &path)
                .replace("\u{1}n", &name_of(&path));
            let dir = if tree.node(cid).is_dir() { current.clone() } else { current.rsplit_once('\\').map(|(d, _)| d.to_string()).unwrap_or(current.clone()) };
            runs.push((cmd, dir, cid));
        }
    }
    Ok(runs)
}

#[tauri::command]
pub fn preview_user_cleanup(index: usize, ids: Vec<NodeId>, state: AppStateRef<'_>) -> CmdResult<CleanupCommand> {
    Ok(CleanupCommand { commands: expand_cleanup(&state, index, &ids)?.into_iter().map(|(c, _, _)| c).collect() })
}

#[tauri::command(async)]
pub fn run_user_cleanup(index: usize, ids: Vec<NodeId>, state: AppStateRef<'_>) -> CmdResult<Summary> {
    let settings = state.settings();
    let cleanup = settings.user_cleanups.get(index).ok_or("Unknown cleanup")?.clone();
    let runs = expand_cleanup(&state, index, &ids)?;
    for (cmd, dir, _) in &runs {
        shell::run_cmd(cmd, dir, cleanup.show_console, cleanup.wait)?;
    }
    let session = state.session()?;
    let rescan: Vec<NodeId> = match cleanup.refresh.as_str() {
        "item" => ids.clone(),
        "parent" => {
            let tree = session.tree.read().unwrap();
            ids.iter().map(|&id| if id == ROOT { ROOT } else { tree.node(id).parent }).collect()
        }
        _ => Vec::new(),
    };
    if !rescan.is_empty() && cleanup.wait {
        let touched: Vec<(NodeId, String)> = {
            let tree = session.tree.read().unwrap();
            ids.iter().filter_map(|&id| actionable(&session, &tree, id).ok().map(|p| (id, p))).filter(|(_, p)| !exists(p)).collect()
        };
        reconcile(&session, &state, &touched, &rescan);
    }
    Ok(summary(&session))
}

/// System-wide cleanups from WinDirStat's Clean Up menu.
#[tauri::command(async)]
pub fn system_action(action: String, app: AppHandle, state: AppStateRef<'_>) -> CmdResult<Option<Summary>> {
    match action.as_str() {
        "emptyRecycleBin" => {
            shell::empty_recycle_bin(hwnd(&app))?;
            // Rescan each drive's $RECYCLE.BIN to show the space returned.
            if let Ok(session) = state.session() {
                let bins: Vec<NodeId> = {
                    let tree = session.tree.read().unwrap();
                    tree.drives()
                        .into_iter()
                        .flat_map(|d| tree.children(d).filter(|&c| tree.node(c).name.eq_ignore_ascii_case("$RECYCLE.BIN")).collect::<Vec<_>>())
                        .collect()
                };
                reconcile(&session, &state, &[], &bins);
                return Ok(Some(summary(&session)));
            }
        }
        "diskCleanup" => shell::shell_execute("cleanmgr.exe", None, None, None, true)?,
        "programs" => shell::shell_execute("control.exe", Some("appwiz.cpl"), None, None, true)?,
        "disableHibernation" => {
            fileops::disable_hibernation()?;
            if let Ok(session) = state.session() {
                let found: Vec<(NodeId, String)> = {
                    let tree = session.tree.read().unwrap();
                    tree.drives()
                        .into_iter()
                        .flat_map(|d| tree.children(d).filter(|&c| tree.node(c).name.eq_ignore_ascii_case("hiberfil.sys")).collect::<Vec<_>>())
                        .map(|id| (id, tree.path(id)))
                        .collect()
                };
                reconcile(&session, &state, &found, &[]);
                return Ok(Some(summary(&session)));
            }
        }
        "removeShadowCopies" => shell::run_elevated_console("DiskLens - Shadow copies", "vssadmin delete shadows /all")?,
        "removeRoamingProfiles" => shell::run_elevated_console(
            "DiskLens - Roaming profiles",
            "powershell -NoProfile -Command \"Get-CimInstance Win32_UserProfile -Filter 'RoamingConfigured = TRUE' | Remove-CimInstance -Confirm\"",
        )?,
        "removeLocalProfiles" => shell::run_elevated_console(
            "DiskLens - Unused local profiles",
            "powershell -NoProfile -Command \"Get-CimInstance Win32_UserProfile -Filter 'RoamingConfigured = FALSE AND Loaded = FALSE AND Special = FALSE' | Remove-CimInstance -Confirm\"",
        )?,
        "dismAnalyze" => shell::run_elevated_console("DiskLens - DISM", "DISM.EXE /Online /Cleanup-Image /AnalyzeComponentStore")?,
        "dismCleanup" => shell::run_elevated_console("DiskLens - DISM", "DISM.EXE /Online /Cleanup-Image /StartComponentCleanup")?,
        "dismReset" => shell::run_elevated_console("DiskLens - DISM", "DISM.EXE /Online /Cleanup-Image /StartComponentCleanup /ResetBase")?,
        "runElevated" => {
            shell::relaunch_elevated()?;
            app.exit(0);
        }
        other => return Err(format!("Unknown action {other}")),
    }
    Ok(None)
}

// ---------------------------------------------------------------- saved results

#[tauri::command(async)]
pub fn save_results(path: String, state: AppStateRef<'_>) -> CmdResult<()> {
    let session = state.session()?;
    let tree = session.tree.read().unwrap();
    let mut out = std::io::BufWriter::new(std::fs::File::create(&path).map_err(|e| e.to_string())?);
    csv::export(&tree, &mut out).map_err(|e| e.to_string())
}

#[tauri::command(async)]
pub fn save_duplicates(path: String, state: AppStateRef<'_>) -> CmdResult<()> {
    let session = state.session()?;
    let tree = session.tree.read().unwrap();
    let dupes = session.dupes.lock().unwrap();
    let groups = dupes.as_ref().ok_or("Find duplicates first")?;
    let mut out = std::io::BufWriter::new(std::fs::File::create(&path).map_err(|e| e.to_string())?);
    csv::export_duplicates(&tree, groups, &mut out).map_err(|e| e.to_string())
}

#[tauri::command(async)]
pub fn load_results(path: String, state: AppStateRef<'_>) -> CmdResult<Summary> {
    let started = std::time::Instant::now();
    let file = std::fs::File::open(&path).map_err(|e| e.to_string())?;
    let tree = csv::import(std::io::BufReader::new(file))?;
    Ok(install_session(&state, tree, started.elapsed(), false, true))
}
