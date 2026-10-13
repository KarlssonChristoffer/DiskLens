//! Arena tree produced by a scan.
//!
//! Children of a directory occupy a contiguous range of `nodes`, stored by
//! descending physical size at scan time. Node ids stay stable for the
//! lifetime of a scan: deletions only mark nodes and adjust ancestor totals,
//! a refreshed directory gets a fresh child range appended to the arena, and
//! consumers that need an order (listing, treemap layout) sort the live
//! children of one directory on demand.

use std::collections::HashMap;
use std::ops::Range;

pub type NodeId = u32;
pub const NO_NODE: NodeId = u32::MAX;
pub const NO_EXT: u32 = u32::MAX;
pub const ROOT: NodeId = 0;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum NodeKind {
    Dir,
    File,
    /// `<Free Space>` of a drive (WinDirStat's IT_FREESPACE).
    FreeSpace,
    /// `<Unknown>`: space in use on a drive that the scan could not attribute
    /// to any file, e.g. inside unreadable folders (IT_UNKNOWN).
    Unknown,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum SizeMode {
    /// Bytes the files occupy on disk (allocation size). WinDirStat's default.
    #[default]
    Physical,
    /// Bytes of file content (end of file).
    Logical,
}

pub mod flags {
    /// Moved to the Recycle Bin or otherwise removed after the scan.
    pub const DELETED: u8 = 1;
    /// The directory could not be listed (usually access denied).
    pub const INACCESSIBLE: u8 = 2;
    /// A junction or directory symlink that was not followed.
    pub const REPARSE_SKIPPED: u8 = 4;
    /// The on-disk name was not valid UTF-16, so `name` is lossy and the path
    /// cannot be reconstructed for file actions.
    pub const NAME_LOSSY: u8 = 8;
    /// The scan was stopped before this directory was listed completely.
    pub const INCOMPLETE: u8 = 16;
    /// Present but not shown (pseudo items while their option is off).
    pub const HIDDEN: u8 = 32;
    /// The synthetic "This PC" root of a multi-drive scan; it has no path.
    pub const MY_COMPUTER: u8 = 64;
    /// The root directory of a drive (`C:\`).
    pub const DRIVE: u8 = 128;
}

#[derive(Debug)]
pub struct Node {
    pub name: Box<str>,
    /// End-of-file bytes; summed over the subtree for directories.
    pub logical: u64,
    /// Allocated bytes; summed over the subtree for directories.
    pub physical: u64,
    /// Last write time as a Windows FILETIME; newest in subtree for directories.
    pub modified: i64,
    pub parent: NodeId,
    pub first_child: NodeId,
    pub child_count: u32,
    /// Files in the subtree (1 for a file).
    pub files: u32,
    /// Directories in the subtree, excluding the node itself.
    pub dirs: u32,
    pub attributes: u32,
    /// Index into `Tree::extensions`, or `NO_EXT` for directories.
    pub ext: u32,
    pub kind: NodeKind,
    pub flags: u8,
}

impl Node {
    pub fn size(&self, mode: SizeMode) -> u64 {
        match mode {
            SizeMode::Physical => self.physical,
            SizeMode::Logical => self.logical,
        }
    }

    pub fn is_deleted(&self) -> bool {
        self.flags & flags::DELETED != 0
    }

    /// Deleted or hidden: not part of the visible tree or its totals.
    pub fn is_gone(&self) -> bool {
        self.flags & (flags::DELETED | flags::HIDDEN) != 0
    }

    pub fn is_dir(&self) -> bool {
        self.kind == NodeKind::Dir
    }

    pub fn is_file(&self) -> bool {
        self.kind == NodeKind::File
    }

    pub fn is_pseudo(&self) -> bool {
        matches!(self.kind, NodeKind::FreeSpace | NodeKind::Unknown)
    }
}

#[derive(Debug, Default)]
pub struct Tree {
    pub nodes: Vec<Node>,
    /// Lower-case extensions including the leading dot; "" for none.
    pub extensions: Vec<Box<str>>,
    /// Directories that could not be listed.
    pub errors: u64,
}

impl Tree {
    pub fn root(&self) -> &Node {
        &self.nodes[ROOT as usize]
    }

    pub fn node(&self, id: NodeId) -> &Node {
        &self.nodes[id as usize]
    }

    pub fn children_range(&self, id: NodeId) -> Range<NodeId> {
        let n = self.node(id);
        n.first_child..n.first_child + n.child_count
    }

    /// Visible children of a directory, in storage order.
    pub fn children(&self, id: NodeId) -> impl Iterator<Item = NodeId> + '_ {
        self.children_range(id)
            .filter(|&c| !self.nodes[c as usize].is_gone())
    }

    /// Visible children with a non-zero size, largest first. This is the order
    /// both treemap layouts expect.
    pub fn children_by_size(&self, id: NodeId, mode: SizeMode) -> Vec<NodeId> {
        let mut kids: Vec<NodeId> = self
            .children(id)
            .filter(|&c| self.node(c).size(mode) > 0)
            .collect();
        kids.sort_by(|&a, &b| self.node(b).size(mode).cmp(&self.node(a).size(mode)));
        kids
    }

    /// Full path of a node. The root node's name is the scanned path itself,
    /// except for a multi-drive scan whose "This PC" root has no path.
    pub fn path(&self, id: NodeId) -> String {
        let mut chain = Vec::new();
        let mut cur = id;
        while cur != ROOT {
            chain.push(cur);
            cur = self.node(cur).parent;
        }
        let mut path = if self.root().flags & flags::MY_COMPUTER != 0 {
            String::new()
        } else {
            String::from(&*self.root().name)
        };
        for &c in chain.iter().rev() {
            if !path.is_empty() && !path.ends_with('\\') {
                path.push('\\');
            }
            path.push_str(&self.node(c).name);
        }
        path
    }

    /// Whether this node or any ancestor has `flag` set.
    pub fn has_flag_in_ancestry(&self, id: NodeId, flag: u8) -> bool {
        let mut cur = id;
        loop {
            if self.node(cur).flags & flag != 0 {
                return true;
            }
            if cur == ROOT {
                return false;
            }
            cur = self.node(cur).parent;
        }
    }

    /// Ancestors from the root down to (and including) `id`.
    pub fn ancestry(&self, id: NodeId) -> Vec<NodeId> {
        let mut chain = vec![id];
        let mut cur = id;
        while cur != ROOT {
            cur = self.node(cur).parent;
            chain.push(cur);
        }
        chain.reverse();
        chain
    }

    /// Visit every visible real file in the subtree under `from`. Deleting a
    /// directory only marks the directory, so walking visible children is
    /// what skips its contents.
    pub fn for_each_live_file(&self, from: NodeId, mut f: impl FnMut(NodeId, &Node)) {
        self.for_each_live(from, |id, n| {
            if n.is_file() {
                f(id, n);
            }
        });
    }

    /// Visit every visible node (directories, files, pseudo items) under `from`.
    pub fn for_each_live(&self, from: NodeId, mut f: impl FnMut(NodeId, &Node)) {
        let mut stack = vec![from];
        while let Some(id) = stack.pop() {
            let n = self.node(id);
            if n.is_gone() {
                continue;
            }
            f(id, n);
            if n.is_dir() {
                stack.extend(self.children(id));
            }
        }
    }

    /// Add signed deltas to every ancestor of `id` (not `id` itself).
    pub fn adjust_ancestors(&mut self, id: NodeId, logical: i64, physical: i64, files: i64, dirs: i64) {
        if id == ROOT {
            return;
        }
        let mut cur = self.node(id).parent;
        loop {
            let n = &mut self.nodes[cur as usize];
            n.logical = n.logical.saturating_add_signed(logical);
            n.physical = n.physical.saturating_add_signed(physical);
            n.files = (n.files as i64 + files).max(0) as u32;
            n.dirs = (n.dirs as i64 + dirs).max(0) as u32;
            if cur == ROOT {
                break;
            }
            cur = n.parent;
        }
    }

    fn totals(&self, id: NodeId) -> (i64, i64, i64, i64) {
        let n = self.node(id);
        (n.logical as i64, n.physical as i64, n.files as i64, (n.dirs + n.is_dir() as u32) as i64)
    }

    /// Mark a node deleted and remove its totals from every ancestor.
    pub fn remove(&mut self, id: NodeId) {
        if id == ROOT || self.node(id).is_deleted() {
            return;
        }
        let visible = !self.node(id).is_gone();
        let (l, p, f, d) = self.totals(id);
        self.nodes[id as usize].flags |= flags::DELETED;
        if visible {
            self.adjust_ancestors(id, -l, -p, -f, -d);
        }
    }

    /// Show or hide a node, keeping ancestor totals consistent.
    pub fn set_hidden(&mut self, id: NodeId, hidden: bool) {
        let n = self.node(id);
        if n.is_deleted() || (n.flags & flags::HIDDEN != 0) == hidden {
            return;
        }
        let (l, p, f, d) = self.totals(id);
        if hidden {
            self.nodes[id as usize].flags |= flags::HIDDEN;
            self.adjust_ancestors(id, -l, -p, -f, -d);
        } else {
            self.nodes[id as usize].flags &= !flags::HIDDEN;
            self.adjust_ancestors(id, l, p, f, d);
        }
    }

    /// Set the size of a pseudo item, updating ancestors if it is shown.
    pub fn set_pseudo_size(&mut self, id: NodeId, bytes: u64) {
        let n = self.node(id);
        let delta = bytes as i64 - n.physical as i64;
        let visible = !n.is_gone();
        let n = &mut self.nodes[id as usize];
        n.logical = bytes;
        n.physical = bytes;
        if visible {
            self.adjust_ancestors(id, delta, delta, 0, 0);
        }
    }

    /// Drive root nodes: the root itself, or the drives under "This PC".
    pub fn drives(&self) -> Vec<NodeId> {
        if self.root().flags & flags::MY_COMPUTER != 0 {
            self.children_range(ROOT)
                .filter(|&c| self.node(c).flags & flags::DRIVE != 0)
                .collect()
        } else if self.root().flags & flags::DRIVE != 0 {
            vec![ROOT]
        } else {
            Vec::new()
        }
    }

    /// The pseudo item of `kind` directly under a drive node, if any.
    pub fn pseudo_child(&self, drive: NodeId, kind: NodeKind) -> Option<NodeId> {
        self.children_range(drive).find(|&c| self.node(c).kind == kind && !self.node(c).is_deleted())
    }

    /// Lower-case extension of a file name including the dot, "" when none.
    /// Matches WinDirStat, where ".gitignore" has the extension ".gitignore".
    pub fn extension_of(name: &str) -> String {
        match name.rfind('.') {
            Some(i) => name[i..].to_lowercase(),
            None => String::new(),
        }
    }
}

/// Interns extensions while a tree is being built.
#[derive(Default)]
pub(crate) struct ExtInterner {
    ids: HashMap<String, u32>,
    pub(crate) names: Vec<Box<str>>,
}

impl ExtInterner {
    /// Continue interning into an existing tree's extension table.
    pub(crate) fn from_existing(names: Vec<Box<str>>) -> Self {
        let ids = names.iter().enumerate().map(|(i, n)| (n.to_string(), i as u32)).collect();
        ExtInterner { ids, names }
    }

    pub(crate) fn intern(&mut self, file_name: &str) -> u32 {
        let ext = Tree::extension_of(file_name);
        if let Some(&id) = self.ids.get(&ext) {
            return id;
        }
        let id = self.names.len() as u32;
        self.names.push(ext.clone().into_boxed_str());
        self.ids.insert(ext, id);
        id
    }
}
