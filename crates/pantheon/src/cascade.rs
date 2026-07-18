//! The rename cascade (§5.4, §7.2). A record's name *is* its slug, so correcting a
//! name re-slugs the record — and every `core:slug` ref pointing at it must follow,
//! or the fixed typo orphans what used to reach it.
//!
//! This lives in the spine and not in a core, because the rewrite touches only the
//! envelope's raw `refs` array, which Pantheon owns. A core would otherwise be
//! reaching into another core's records, which I5 forbids outright — the cascade is
//! the one operation that spans every core's files, and it does so without knowing
//! what any of them mean.
//!
//! There is no reverse index and there will not be one (§18): the refs are found by
//! **one walk**, affordable at personal scale (§5.0). The walk that finds them is
//! also the walk that finds a record already holding the new name — the two questions
//! have the same answer, so they are asked once.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use crate::classify::{FileClass, classify};
use crate::envelope::{RawEntity, RawLine, Ref};
use crate::tree::{Node, TreeRoot, build_tree};
use crate::{Error, Result};

/// One file the cascade would rewrite, and how many of its refs point at the old
/// slug. The path is relative to the tree root — stable across machines, and what
/// the emitted JSON shows.
#[derive(Clone, Debug)]
pub struct RefRewrite {
    pub rel_path: PathBuf,
    pub refs: usize,
    pub is_series: bool,
}

/// A computed rename: the ref it retires, the ref that replaces it, and every file
/// that has to change. Non-atomic by design — see [`Cascade::apply`].
#[derive(Clone, Debug)]
pub struct Cascade {
    pub from: Ref,
    pub to: Ref,
    pub rewrites: Vec<RefRewrite>,
}

/// Plan a rename in one walk (§5.4).
///
/// `own_kinds` are the calling core's own tokens — passed in rather than discovered,
/// so the cascade needs no `CoreRegistry` and no PATH probe: a core already knows
/// what it owns, and asking would be a core learning about cores (I5).
///
/// The walk does two things at once. It **refuses an occupied slug** (exit `3`): if
/// any of this core's records already answers to the new name, landing on it would
/// rewrite every `album:johnn` into an `album:john` indistinguishable from the refs
/// that always meant the other John, and §18 keeps no history to recover the
/// difference. This check is tree-wide and hard, unlike `add`'s cross-node duplicate
/// warning — §7.2 draws that line deliberately: a duplicate born of `add` can still
/// be fixed at the source, while a cascade onto an occupied slug spends the very
/// token that told the two apart.
///
/// And it **collects the refs**. Documents are skipped entirely: a document's
/// frontmatter carries `type` and `tags` and no refs (§6.1), so there is nothing in
/// one to rewrite.
pub fn plan_cascade(root: &Path, own_kinds: &[&str], from: &Ref, to: &Ref) -> Result<Cascade> {
    if from.core != to.core {
        return Err(Error::usage(
            "a rename changes a slug, never a core (§5.4)".to_string(),
        ));
    }
    let nodes = match build_tree(root, None)? {
        TreeRoot::Forest(nodes) => nodes,
        TreeRoot::Subtree(node) => vec![node],
    };
    // Sorted by path so a plan — and so its token — is stable across two runs on the
    // same tree, whatever order the filesystem hands entries back in (§7.3).
    let mut found: BTreeMap<PathBuf, RefRewrite> = BTreeMap::new();
    for node in &nodes {
        walk(root, node, own_kinds, from, to, &mut found)?;
    }
    Ok(Cascade {
        from: from.clone(),
        to: to.clone(),
        rewrites: found.into_values().collect(),
    })
}

fn walk(
    root: &Path,
    node: &Node,
    own_kinds: &[&str],
    from: &Ref,
    to: &Ref,
    found: &mut BTreeMap<PathBuf, RefRewrite>,
) -> Result<()> {
    let meta = node.path.join(format!("{}__", node.code.as_str()));
    if meta.is_dir() {
        for entry in std::fs::read_dir(&meta)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                continue;
            }
            let file_name = entry.file_name().to_string_lossy().into_owned();
            let class = classify(&file_name, false, &node.code);

            // Does this file's own identity already answer to the new name?
            let identity = match &class {
                FileClass::Partitioned { kind, slug, .. } => Some((kind, slug.clone())),
                FileClass::EntityNode { kind, .. } => Some((kind, node.label.clone())),
                FileClass::NamedSeries { kind, name, .. } => Some((kind, name.clone())),
                _ => None,
            };
            if let Some((kind, ident)) = identity
                && own_kinds.contains(&kind.as_str())
                && ident == to.slug
            {
                return Err(Error::validation(format!(
                    "{} already names a record at {} — renaming onto it would make the \
                     two indistinguishable, and there is no history to tell them apart \
                     again (§7.2, §18)",
                    to.to_token(),
                    node.code.as_str()
                )));
            }

            // Whose refs might point at the old name? Any record's — the cascade
            // spans cores, which is exactly why it is the spine's and not a core's.
            let is_series = match &class {
                FileClass::NamedSeries { .. } | FileClass::DeterminedSeries { .. } => true,
                FileClass::Partitioned { .. } | FileClass::EntityNode { .. } => false,
                _ => continue,
            };
            let path = entry.path();
            let refs = crate::validate::record_refs(&path, is_series)
                .map_err(|e| Error::validation(format!("{}: {e}", path.display())))?;
            let count = refs.iter().filter(|r| *r == from).count();
            if count > 0 {
                let rel_path = path.strip_prefix(root).unwrap_or(&path).to_path_buf();
                found.insert(
                    path.clone(),
                    RefRewrite {
                        rel_path,
                        refs: count,
                        is_series,
                    },
                );
            }
        }
    }
    for child in &node.children {
        walk(root, child, own_kinds, from, to, found)?;
    }
    Ok(())
}

impl Cascade {
    /// What the cascade would do, for the review a mutation always gets (§7.3). This
    /// rides inside the [`RecordChange`](crate::contract::RecordChange), so it hashes
    /// into the plan token: a review that showed three refs must not be applied
    /// against a tree that has since grown a fourth.
    #[must_use]
    pub fn to_json(&self) -> Value {
        Value::Array(
            self.rewrites
                .iter()
                .map(|r| {
                    json!({
                        "path": r.rel_path.to_string_lossy(),
                        "refs": r.refs,
                    })
                })
                .collect(),
        )
    }

    /// How many refs, in how many files.
    #[must_use]
    pub fn totals(&self) -> (usize, usize) {
        (
            self.rewrites.iter().map(|r| r.refs).sum(),
            self.rewrites.len(),
        )
    }

    /// Rewrite every ref, each under the record lock (§6.4).
    ///
    /// **Call this after the record's own file has been renamed, never before.** The
    /// transaction is not atomic and cannot be — §18 leaves nowhere to keep an
    /// in-flight plan, and §6.4's lock is one record file's. So a crash mid-cascade
    /// is diagnosed from the tree by `pan validate` (§10.1), and the *order* decides
    /// how good that diagnosis is: with the record already at its new slug, the refs
    /// still on the old one dangle, and `pan validate` names exactly the files that
    /// need fixing. Refs-first would leave a tree that is equally broken but points
    /// the diagnosis at the wrong file.
    ///
    /// Only the envelope's `refs` array is touched. A record's `data` is carried
    /// through as raw bytes and never parsed (I5), and a series' untouched lines are
    /// copied verbatim — the same discipline `write_line` keeps.
    pub fn apply(&self, root: &Path) -> Result<()> {
        for rewrite in &self.rewrites {
            let path = root.join(&rewrite.rel_path);
            let (from, to) = (self.from.clone(), self.to.clone());
            let is_series = rewrite.is_series;
            crate::lock::with_record_lock(&path, move |prev| {
                let bytes = prev.unwrap_or_default();
                if is_series {
                    rewrite_series(bytes, &from, &to)
                } else {
                    rewrite_entity(bytes, &from, &to)
                }
            })?;
        }
        // One batch, one completion. This is where step 8's single, *triggerless*
        // `aus run` fires — triggerless because a batch touching several cores and
        // homes at once has no one write to name as the trigger (§5.4, §9.3–§9.4).
        Ok(())
    }
}

fn swap(refs: &mut [Ref], from: &Ref, to: &Ref) {
    for r in refs.iter_mut() {
        if r == from {
            *r = to.clone();
        }
    }
}

fn rewrite_entity(bytes: &[u8], from: &Ref, to: &Ref) -> Result<Vec<u8>> {
    let mut entity: RawEntity = serde_json::from_slice(bytes)?;
    swap(&mut entity.refs, from, to);
    let mut out = serde_json::to_vec_pretty(&entity)?;
    out.push(b'\n');
    Ok(out)
}

fn rewrite_series(bytes: &[u8], from: &Ref, to: &Ref) -> Result<Vec<u8>> {
    let text = std::str::from_utf8(bytes)
        .map_err(|e| Error::runtime(format!("series file is not UTF-8: {e}")))?;
    let mut out = String::with_capacity(text.len());
    for raw in text.lines() {
        if raw.trim().is_empty() {
            continue;
        }
        let mut line: RawLine = serde_json::from_str(raw)?;
        if line.refs.iter().any(|r| r == from) {
            swap(&mut line.refs, from, to);
            out.push_str(&serde_json::to_string(&line)?);
        } else {
            // Untouched lines are carried through byte-for-byte (§6.1).
            out.push_str(raw);
        }
        out.push('\n');
    }
    Ok(out.into_bytes())
}
