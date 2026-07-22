//! Small display helpers shared by every provider (I3, P-II).
//!
//! A stored identifier and the word a reader sees are not the same string: a slug
//! spells a word gap `_` (§5.1) and a node is addressed by a terse code (§5.1). These
//! two helpers turn one into the other, so the de-underscoring and the code→label
//! lookup are **one implementation for twelve** rather than one per app — and the row's
//! real key is left untouched, so a relay still writes the slug it always did (P§7).

use std::collections::HashMap;
use std::path::Path;

use pantheon::build_tree;
use pantheon::tree::{Node, TreeRoot};

/// A stored slug as a reader wants it: underscores are word gaps on disk (§5.1), so
/// they read as spaces. Identity is untouched — this is the label a row *shows*, never
/// the key a relay *writes* (P§7).
#[must_use]
pub fn prettify(slug: &str) -> String {
    slug.replace('_', " ")
}

/// A map from every node's code to its label, walked from the tree once (§5.0, I1).
///
/// A row keyed by a node code (`ac`) can then read as its human label (`cura`) while
/// the caller keeps targeting the code — so this only touches what is drawn. A walk
/// failure yields an empty map, which is the calm fallback: the caller shows the code,
/// legible on its own (I7). Derived fresh, never cached (§5.0, §18).
#[must_use]
pub fn node_labels(root: &Path) -> HashMap<String, String> {
    let mut out = HashMap::new();
    if let Ok(tree) = build_tree(root, None) {
        match &tree {
            TreeRoot::Forest(nodes) => nodes.iter().for_each(|node| collect(node, &mut out)),
            TreeRoot::Subtree(node) => collect(node, &mut out),
        }
    }
    out
}

fn collect(node: &Node, out: &mut HashMap<String, String>) {
    out.insert(node.code.as_str().to_owned(), node.label.clone());
    for child in &node.children {
        collect(child, out);
    }
}
