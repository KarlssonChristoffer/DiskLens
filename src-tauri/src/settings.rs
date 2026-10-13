//! User settings, persisted as JSON in the app's config folder. Covers
//! WinDirStat's Options dialog: view, scanning filters, columns, panes,
//! user-defined cleanups and search defaults.

use std::path::PathBuf;

use disklens_core::scan::ScanOptions;
use disklens_core::search::SearchOptions;
use serde::{Deserialize, Serialize};

use crate::session::ViewOptions;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct UserCleanup {
    pub title: String,
    /// Run through `cmd /C`. Placeholders: %p path, %n name of the selected
    /// item; %sp, %sn the current subfolder when recursing.
    pub command: String,
    pub enabled: bool,
    pub works_for_drives: bool,
    pub works_for_folders: bool,
    pub works_for_files: bool,
    pub works_for_unc: bool,
    /// Run once for the item and once for each subfolder below it.
    pub recurse: bool,
    pub ask_first: bool,
    pub show_console: bool,
    pub wait: bool,
    /// "none", "item" or "parent": what to rescan afterwards.
    pub refresh: String,
}

impl Default for UserCleanup {
    fn default() -> Self {
        UserCleanup {
            title: String::new(),
            command: String::new(),
            enabled: true,
            works_for_drives: false,
            works_for_folders: true,
            works_for_files: false,
            works_for_unc: false,
            recurse: false,
            ask_first: true,
            show_console: true,
            wait: true,
            refresh: "item".into(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub view: ViewOptions,
    pub scan: ScanOptions,
    pub search: SearchOptions,
    pub show_free_space: bool,
    pub show_unknown: bool,
    pub show_file_types: bool,
    pub show_treemap: bool,
    pub show_toolbar: bool,
    pub show_status_bar: bool,
    pub delete_warning: bool,
    pub list_stripes: bool,
    pub large_file_count: usize,
    pub folder_history: Vec<String>,
    /// Optional folder-list columns in display order.
    pub columns: Vec<String>,
    pub user_cleanups: Vec<UserCleanup>,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            view: ViewOptions::default(),
            scan: ScanOptions::default(),
            search: SearchOptions::default(),
            show_free_space: false,
            show_unknown: false,
            show_file_types: true,
            show_treemap: true,
            show_toolbar: true,
            show_status_bar: true,
            delete_warning: true,
            list_stripes: false,
            large_file_count: 100,
            folder_history: Vec::new(),
            columns: ["physical", "items", "modified"].map(String::from).to_vec(),
            user_cleanups: Vec::new(),
        }
    }
}

const HISTORY_LIMIT: usize = 10;

impl Settings {
    pub fn load(path: &PathBuf) -> Settings {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, path: &PathBuf) -> Result<(), String> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let json = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, json).map_err(|e| e.to_string())?;
        std::fs::rename(&tmp, path).map_err(|e| e.to_string())
    }

    /// Remember a scanned folder (drive roots aren't history-worthy).
    pub fn remember_folder(&mut self, path: &str) {
        if path.len() <= 3 {
            return;
        }
        self.folder_history.retain(|p| !p.eq_ignore_ascii_case(path));
        self.folder_history.insert(0, path.to_string());
        self.folder_history.truncate(HISTORY_LIMIT);
    }
}
