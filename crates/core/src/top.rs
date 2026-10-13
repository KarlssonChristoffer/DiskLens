//! The largest files in a subtree.

use std::cmp::Reverse;
use std::collections::BinaryHeap;

use crate::tree::{NodeId, SizeMode, Tree};

/// Up to `count` largest live files under `from`, largest first.
pub fn largest_files(tree: &Tree, from: NodeId, mode: SizeMode, count: usize) -> Vec<NodeId> {
    if count == 0 {
        return Vec::new();
    }
    let mut heap: BinaryHeap<Reverse<(u64, NodeId)>> = BinaryHeap::with_capacity(count + 1);
    tree.for_each_live_file(from, |id, n| {
        let size = n.size(mode);
        if heap.len() < count {
            heap.push(Reverse((size, id)));
        } else if heap.peek().is_some_and(|Reverse((min, _))| size > *min) {
            heap.pop();
            heap.push(Reverse((size, id)));
        }
    });
    let mut out: Vec<(u64, NodeId)> = heap.into_iter().map(|Reverse(e)| e).collect();
    out.sort_by(|a, b| b.cmp(a));
    out.into_iter().map(|(_, id)| id).collect()
}
