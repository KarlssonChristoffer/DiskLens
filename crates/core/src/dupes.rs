//! Duplicate files.
//!
//! Same strategy as WinDirStat (Controls/FileDupeControl.cpp): only files
//! sharing a size can be duplicates, and candidates are hashed in widening
//! stages (first 4 KiB, first 1 MiB, then the whole file), so most of them
//! are ruled out after reading very little. WinDirStat hashes with SHA-512
//! through Windows CNG; DiskLens uses BLAKE3, which is also cryptographic and
//! considerably faster.

use std::collections::HashMap;
use std::ffi::OsString;
use std::fs::OpenOptions;
use std::io::Read;
use std::os::windows::ffi::OsStringExt;
use std::os::windows::fs::OpenOptionsExt;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use rayon::prelude::*;
use serde::Serialize;
use windows_sys::Win32::Storage::FileSystem::{
    FILE_ATTRIBUTE_COMPRESSED, FILE_ATTRIBUTE_OFFLINE, FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS,
    FILE_ATTRIBUTE_RECALL_ON_OPEN, FILE_ATTRIBUTE_REPARSE_POINT, FILE_FLAG_SEQUENTIAL_SCAN,
    FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE,
};

use crate::scan::normalize;
use crate::tree::{NodeId, Tree, flags};

/// Hash limits per stage; the last stage reads whole files.
const STAGES: [u64; 3] = [4 * 1024, 1024 * 1024, u64::MAX];

/// Files whose content lives in the cloud would be downloaded by hashing.
const CLOUD_ATTRIBUTES: u32 =
    FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS | FILE_ATTRIBUTE_RECALL_ON_OPEN | FILE_ATTRIBUTE_OFFLINE;

#[derive(Default)]
pub struct DupeControl {
    cancel: AtomicBool,
    candidates: AtomicU64,
    files_hashed: AtomicU64,
    bytes_hashed: AtomicU64,
}

#[derive(Clone, Copy, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DupeProgress {
    pub candidates: u64,
    pub files_hashed: u64,
    pub bytes_hashed: u64,
}

impl DupeControl {
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }

    pub fn progress(&self) -> DupeProgress {
        DupeProgress {
            candidates: self.candidates.load(Ordering::Relaxed),
            files_hashed: self.files_hashed.load(Ordering::Relaxed),
            bytes_hashed: self.bytes_hashed.load(Ordering::Relaxed),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct DupeGroup {
    /// Logical size of each file in the group.
    pub size: u64,
    pub files: Vec<NodeId>,
    /// Hex prefix of the content hash, for display.
    pub hash: String,
}

impl DupeGroup {
    /// Bytes that deleting all but one copy would free (logical).
    pub fn wasted(&self) -> u64 {
        self.size * (self.files.len() as u64 - 1)
    }
}

pub struct DupeOptions {
    /// Ignore files smaller than this many bytes (never less than 1).
    pub min_size: u64,
}

impl Default for DupeOptions {
    fn default() -> Self {
        DupeOptions { min_size: 1 }
    }
}

/// Find groups of identical files under `from`, most wasted space first.
/// Returns what was found so far when cancelled.
pub fn find(tree: &Tree, from: NodeId, options: &DupeOptions, control: &DupeControl) -> Vec<DupeGroup> {
    let min_size = options.min_size.max(1);
    let mut by_size: HashMap<u64, Vec<NodeId>> = HashMap::new();
    tree.for_each_live_file(from, |id, n| {
        let reparse = n.attributes & FILE_ATTRIBUTE_REPARSE_POINT != 0
            && n.attributes & FILE_ATTRIBUTE_COMPRESSED == 0;
        if n.logical >= min_size
            && n.attributes & CLOUD_ATTRIBUTES == 0
            && !reparse
            && n.flags & flags::NAME_LOSSY == 0
        {
            by_size.entry(n.logical).or_default().push(id);
        }
    });

    let mut groups: Vec<(u64, Vec<NodeId>)> = by_size.into_iter().filter(|(_, v)| v.len() > 1).collect();
    control
        .candidates
        .store(groups.iter().map(|(_, v)| v.len() as u64).sum(), Ordering::Relaxed);

    let mut found = Vec::new();
    for limit in STAGES {
        if groups.is_empty() || control.is_cancelled() {
            break;
        }
        let work: Vec<(usize, NodeId)> = groups
            .iter()
            .enumerate()
            .flat_map(|(g, (_, ids))| ids.iter().map(move |&id| (g, id)))
            .collect();
        let hashes: Vec<Option<blake3::Hash>> = work
            .par_iter()
            .map(|&(_, id)| hash_prefix(tree, id, limit, control))
            .collect();

        let mut regrouped: Vec<HashMap<blake3::Hash, Vec<NodeId>>> = vec![HashMap::new(); groups.len()];
        for (&(g, id), hash) in work.iter().zip(hashes) {
            if let Some(hash) = hash {
                regrouped[g].entry(hash).or_default().push(id);
            }
        }

        let mut next = Vec::new();
        for ((size, _), by_hash) in groups.iter().zip(regrouped) {
            for (hash, ids) in by_hash {
                if ids.len() < 2 {
                    continue;
                }
                if *size <= limit {
                    // This stage already read every byte of these files.
                    found.push(DupeGroup {
                        size: *size,
                        files: ids,
                        hash: hash.to_hex()[..16].to_string(),
                    });
                } else {
                    next.push((*size, ids));
                }
            }
        }
        groups = next;
    }

    found.sort_by(|a, b| b.wasted().cmp(&a.wasted()));
    found
}

fn hash_prefix(tree: &Tree, id: NodeId, limit: u64, control: &DupeControl) -> Option<blake3::Hash> {
    if control.is_cancelled() {
        return None;
    }
    let path = OsString::from_wide(&normalize(&tree.path(id)).1);
    let mut file = OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
        .custom_flags(FILE_FLAG_SEQUENTIAL_SCAN)
        .open(path)
        .ok()?;

    thread_local! {
        static BUFFER: std::cell::RefCell<Vec<u8>> = std::cell::RefCell::new(vec![0; 1024 * 1024]);
    }
    BUFFER.with_borrow_mut(|buffer| {
        let mut hasher = blake3::Hasher::new();
        let mut done = 0u64;
        while done < limit {
            if control.is_cancelled() {
                return None;
            }
            let want = (limit - done).min(buffer.len() as u64) as usize;
            let read = file.read(&mut buffer[..want]).ok()?;
            if read == 0 {
                break;
            }
            hasher.update(&buffer[..read]);
            done += read as u64;
            control.bytes_hashed.fetch_add(read as u64, Ordering::Relaxed);
        }
        control.files_hashed.fetch_add(1, Ordering::Relaxed);
        Some(hasher.finalize())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scan::{ScanControl, ScanOptions, scan};

    #[test]
    fn finds_identical_files_across_stages() {
        let dir = std::env::temp_dir().join(format!("disklens-dupes-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        let big: Vec<u8> = (0..3_000_000u32).map(|i| (i % 251) as u8).collect();
        let mut big_changed = big.clone();
        *big_changed.last_mut().unwrap() ^= 1; // differs only in the last byte
        std::fs::write(dir.join("big1.bin"), &big).unwrap();
        std::fs::write(dir.join("sub/big2.bin"), &big).unwrap();
        std::fs::write(dir.join("big3.bin"), &big_changed).unwrap();
        std::fs::write(dir.join("small1.txt"), b"hello").unwrap();
        std::fs::write(dir.join("sub/small2.txt"), b"hello").unwrap();
        std::fs::write(dir.join("other.txt"), b"world").unwrap();

        let tree = scan(dir.to_str().unwrap(), &ScanOptions::default(), &ScanControl::default()).unwrap();
        let groups = find(&tree, 0, &DupeOptions::default(), &DupeControl::default());
        let names = |g: &DupeGroup| {
            let mut v: Vec<String> = g.files.iter().map(|&f| tree.node(f).name.to_string()).collect();
            v.sort();
            v
        };
        assert_eq!(groups.len(), 2);
        assert_eq!(names(&groups[0]), ["big1.bin", "big2.bin"]);
        assert_eq!(names(&groups[1]), ["small1.txt", "small2.txt"]);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
