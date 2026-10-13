//! The state of one completed scan and how it is being viewed.

use std::sync::{Mutex, RwLock};
use std::time::Duration;

use disklens_core::dupes::DupeGroup;
use disklens_core::ext::{self, ExtStats};
use disklens_core::palette::{self, Rgb};
use disklens_core::tree::flags;
use disklens_core::treemap::{self, Layout};
use disklens_core::{NodeKind, SizeMode, Tree};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PaletteKind {
    #[default]
    Fluent,
    Classic,
}

impl PaletteKind {
    pub fn colors(self) -> &'static [Rgb] {
        match self {
            PaletteKind::Fluent => &palette::FLUENT,
            PaletteKind::Classic => &palette::CLASSIC,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SizeModeDto {
    #[default]
    Physical,
    Logical,
}

impl From<SizeModeDto> for SizeMode {
    fn from(m: SizeModeDto) -> Self {
        match m {
            SizeModeDto::Physical => SizeMode::Physical,
            SizeModeDto::Logical => SizeMode::Logical,
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ViewOptions {
    pub size_mode: SizeModeDto,
    pub palette: PaletteKind,
    pub treemap: treemap::Options,
}

impl Default for ViewOptions {
    fn default() -> Self {
        ViewOptions {
            size_mode: SizeModeDto::Physical,
            palette: PaletteKind::default(),
            treemap: treemap::Options::default(),
        }
    }
}

pub struct View {
    pub options: ViewOptions,
    pub ext: ExtStats,
    pub layout: Layout,
}

impl View {
    pub fn mode(&self) -> SizeMode {
        self.options.size_mode.into()
    }
}

pub struct Session {
    pub tree: RwLock<Tree>,
    pub view: Mutex<View>,
    pub dupes: Mutex<Option<Vec<DupeGroup>>>,
    pub elapsed: Duration,
    /// The scan was stopped early; some folders are incomplete.
    pub incomplete: bool,
    /// Loaded from a CSV file rather than scanned; file actions are refused.
    pub from_file: bool,
}

impl Session {
    pub fn new(mut tree: Tree, options: ViewOptions, elapsed: Duration, show_free: bool, show_unknown: bool) -> Self {
        apply_pseudo(&mut tree, show_free, show_unknown);
        let ext = ext::compute(&tree, options.size_mode.into(), options.palette.colors());
        Session {
            tree: RwLock::new(tree),
            view: Mutex::new(View { options, ext, layout: Layout::default() }),
            dupes: Mutex::new(None),
            elapsed,
            incomplete: false,
            from_file: false,
        }
    }

    /// Recompute file-type statistics after the tree or the options changed,
    /// and drop duplicate entries that no longer exist.
    pub fn refresh_ext(&self) {
        let tree = self.tree.read().unwrap();
        {
            let mut view = self.view.lock().unwrap();
            view.ext = ext::compute(&tree, view.mode(), view.options.palette.colors());
        }
        if let Some(groups) = self.dupes.lock().unwrap().as_mut() {
            for g in groups.iter_mut() {
                g.files.retain(|&f| !tree.has_flag_in_ancestry(f, flags::DELETED));
            }
            groups.retain(|g| g.files.len() > 1);
        }
    }
}

/// Show or hide every drive's `<Free Space>` and `<Unknown>` items.
pub fn apply_pseudo(tree: &mut Tree, show_free: bool, show_unknown: bool) {
    for drive in tree.drives() {
        if let Some(id) = tree.pseudo_child(drive, NodeKind::FreeSpace) {
            tree.set_hidden(id, !show_free);
        }
        if let Some(id) = tree.pseudo_child(drive, NodeKind::Unknown) {
            tree.set_hidden(id, !show_unknown);
        }
    }
}
