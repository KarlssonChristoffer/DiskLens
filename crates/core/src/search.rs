//! Find files and folders by name (WinDirStat's Edit › Search).

use regex::RegexBuilder;
use serde::{Deserialize, Serialize};

use crate::tree::{NodeId, Tree};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SearchOptions {
    pub case_sensitive: bool,
    /// The whole name must match, not just part of it.
    pub whole_name: bool,
    pub regex: bool,
    pub max_results: usize,
}

impl Default for SearchOptions {
    fn default() -> Self {
        SearchOptions { case_sensitive: false, whole_name: false, regex: false, max_results: 10_000 }
    }
}

/// Visible files and folders under `from` whose names match `query`, in tree
/// order. Returns an error for an invalid regular expression.
pub fn search(tree: &Tree, from: NodeId, query: &str, options: &SearchOptions) -> Result<Vec<NodeId>, String> {
    let body = if options.regex { query.to_string() } else { regex::escape(query) };
    let source = if options.whole_name { format!("^(?:{body})$") } else { body };
    let regex = RegexBuilder::new(&source)
        .case_insensitive(!options.case_sensitive)
        .build()
        .map_err(|e| e.to_string())?;

    let mut found = Vec::new();
    let limit = options.max_results.max(1);
    tree.for_each_live(from, |id, n| {
        if found.len() < limit && id != from && !n.is_pseudo() && regex.is_match(&n.name) {
            found.push(id);
        }
    });
    Ok(found)
}
