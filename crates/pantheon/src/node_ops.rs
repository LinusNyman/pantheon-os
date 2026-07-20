//! Node-level structural operations (§10.1): `rm`, `rename`, `mv`, `mv-file`, and the
//! bulk `rename-prefix` / `rename-pattern`. Each builds a [`Plan`](crate::plan::Plan) of
//! directory and file renames — a node's code *is* its path (§5.2), so changing a code
//! rewrites every descendant directory name and `[code]` filename prefix under the
//! branch. The plans are dry-run-first and non-atomic (§10.1): a crash is diagnosed from
//! the tree by `pan validate`, never from an in-flight log (§18).
//!
//! `pan new` (minting) lives in [`mint`](crate::mint); these are the operations over an
//! existing tree. The record-level *ref* cascade a definition-prefix node rename triggers
//! is [`cascade`](crate::cascade)'s, reused here.

use std::path::Path;

use serde_json::{Value, json};

use crate::code::Code;
use crate::plan::{Change, Plan};
use crate::tree::resolve_node;
use crate::{Error, Result};

/// Plan the removal of a node (§10.1). **Refused** if the node holds anything but its
/// own meta scaffold — a child node, a record, a series, a document, a rule, or homed
/// bulk — because a node's contents are the last thing that should drop without a word
/// (§10.1). An empty node (only its `[code]__` meta dir, itself holding at most the
/// annotation `[code]__.toml`) is removed whole.
pub fn plan_rm(root: &Path, code: &Code) -> Result<(Plan, Value)> {
    let (_nn, path) = resolve_node(root, code)?;

    let occupants = node_occupants(&path, code)?;
    if !occupants.is_empty() {
        return Err(Error::validation(format!(
            "node {} is not empty — it holds {}; `rm` refuses a node with children, records, \
             documents, rules, or bulk (§10.1). Empty or move them first.",
            code.as_str(),
            occupants.join(", ")
        )));
    }

    let rel = rel_path(root, &path);
    let plan = Plan::new(
        "rm",
        vec![Change::Remove {
            rel_path: rel.clone(),
        }],
    );
    let removed = json!({ "code": code.as_str(), "path": rel.to_string_lossy() });
    Ok((plan, removed))
}

/// What a node holds beyond its meta scaffold — the names that make `rm` refuse. Empty
/// means removable. The meta dir itself and its annotation `[code]__.toml` are scaffold,
/// not contents, so they never appear here.
fn node_occupants(path: &Path, code: &Code) -> Result<Vec<String>> {
    let meta_name = format!("{}__", code.as_str());
    let annotation = format!("{}__.toml", code.as_str());
    let mut found = Vec::new();

    // Anything in the node dir other than its meta dir: a child node, a loose document,
    // or homed bulk.
    for entry in std::fs::read_dir(path)? {
        let name = entry?.file_name().to_string_lossy().into_owned();
        if name != meta_name {
            found.push(name);
        }
    }
    // Anything in the meta dir other than the annotation: a record, series, or rule.
    let meta = path.join(&meta_name);
    if meta.is_dir() {
        for entry in std::fs::read_dir(&meta)? {
            let name = entry?.file_name().to_string_lossy().into_owned();
            if name != annotation {
                found.push(name);
            }
        }
    }
    found.sort();
    Ok(found)
}

/// A path relative to the tree root, for a [`Change`].
fn rel_path(root: &Path, path: &Path) -> std::path::PathBuf {
    path.strip_prefix(root).unwrap_or(path).to_path_buf()
}
