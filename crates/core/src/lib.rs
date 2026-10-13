//! DiskLens engine: scanning, the scan tree, treemap rendering, file-type
//! statistics, search, largest-file and duplicate detection, saved results
//! and file maintenance operations. Free of any UI so it can be tested and
//! benchmarked on its own.

pub mod csv;
pub mod dupes;
pub mod ext;
pub mod fileops;
pub mod palette;
pub mod scan;
pub mod search;
pub mod top;
pub mod tree;
pub mod treemap;
pub mod volumes;

pub use tree::{NodeId, NodeKind, SizeMode, Tree};
