//! File-type statistics and the extension-to-colour assignment.

use serde::Serialize;

use crate::palette::{Rgb, equalized};
use crate::tree::{ROOT, SizeMode, Tree};

#[derive(Clone, Debug, Serialize)]
pub struct ExtStat {
    pub id: u32,
    /// Lower-case extension including the dot; "" for files without one.
    pub ext: String,
    pub bytes: u64,
    pub files: u64,
    /// Equalised treemap colour.
    pub color: Rgb,
}

pub struct ExtStats {
    /// Sorted by bytes, largest first.
    pub stats: Vec<ExtStat>,
    /// Treemap colour per extension id.
    pub colors: Vec<Rgb>,
}

/// Totals per extension over the whole live tree, coloured like WinDirStat:
/// the palette's colours go to the extensions with the most bytes, in order,
/// and every remaining extension shares the palette's last colour.
pub fn compute(tree: &Tree, mode: SizeMode, palette: &[Rgb]) -> ExtStats {
    let mut bytes = vec![0u64; tree.extensions.len()];
    let mut files = vec![0u64; tree.extensions.len()];
    tree.for_each_live_file(ROOT, |_, n| {
        bytes[n.ext as usize] += n.size(mode);
        files[n.ext as usize] += 1;
    });

    let mut order: Vec<u32> = (0..tree.extensions.len() as u32)
        .filter(|&i| files[i as usize] > 0)
        .collect();
    order.sort_by(|&a, &b| bytes[b as usize].cmp(&bytes[a as usize]));

    let palette = equalized(palette);
    let fallback = *palette.last().expect("palette is not empty");
    let mut colors = vec![fallback; tree.extensions.len()];
    let stats = order
        .iter()
        .enumerate()
        .map(|(rank, &id)| {
            let color = palette.get(rank).copied().unwrap_or(fallback);
            colors[id as usize] = color;
            ExtStat {
                id,
                ext: tree.extensions[id as usize].to_string(),
                bytes: bytes[id as usize],
                files: files[id as usize],
                color,
            }
        })
        .collect();
    ExtStats { stats, colors }
}
