//! Planned transactions (§10.1, §7.3). Structural operations compute a plan first;
//! a plan is emitted for review (`--dry-run`) or applied (`-y`). The plan token
//! guards against acting on a stale review: it hashes the exact computed change, so
//! anything moved underneath in between forces a fresh look (§7.3).

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use serde_json::json;
use sha2::{Digest, Sha256};

use crate::code::Code;
use crate::{Error, Result};

/// One change in a plan. `Mkdir` mints a node; `Rename`/`Remove` are the node cascade's
/// directory and file moves (§10.1); `RewriteRefs` is the `core:slug` ref rewrite a
/// definition-prefix node rename drags along (§5.4) — a record's `refs` array, edited in
/// place, so the ref cascade rides in the same plan and the same token as the renames.
#[derive(Clone, Debug)]
pub enum Change {
    Mkdir {
        code: Code,
        rel_path: PathBuf,
    },
    Rename {
        from: PathBuf,
        to: PathBuf,
    },
    Remove {
        rel_path: PathBuf,
    },
    /// A directory the plan has already emptied, removed with `remove_dir` (§10.1).
    ///
    /// The twin of `Remove` that refuses to destroy a surprise: `Remove` recurses, which
    /// is right for `rm` (the node is *proven* empty first) and wrong for `merge`, where
    /// the source dir is empty only because this same plan just moved everything out of
    /// it. Anything that appeared underneath in between must stop the removal, not be
    /// swept up by it.
    RemoveEmptyDir {
        rel_path: PathBuf,
    },
    RewriteRefs {
        rel_path: PathBuf,
        is_series: bool,
        /// The retired and replacement refs, as `core:slug` tokens.
        from: String,
        to: String,
    },
    /// A rule's `writes=` grant, re-homed for a recoded branch (§9.2, §10.1).
    ///
    /// The twin of `RewriteRefs` one layer down: a ref names a *record* and a grant names
    /// a *node*, and a recode invalidates both. `from`/`to` are the branch's old and new
    /// codes; which entries in the header they touch is the rewrite's to work out.
    RewriteHeader {
        rel_path: PathBuf,
        from: Code,
        to: Code,
    },
}

impl Change {
    fn to_json(&self) -> serde_json::Value {
        match self {
            Change::Mkdir { code, rel_path } => {
                json!({ "op": "mkdir", "code": code.as_str(), "path": rel_path.to_string_lossy() })
            }
            Change::Rename { from, to } => {
                json!({ "op": "rename", "from": from.to_string_lossy(), "to": to.to_string_lossy() })
            }
            Change::Remove { rel_path } => {
                json!({ "op": "remove", "path": rel_path.to_string_lossy() })
            }
            Change::RemoveEmptyDir { rel_path } => {
                json!({ "op": "remove_empty_dir", "path": rel_path.to_string_lossy() })
            }
            Change::RewriteRefs {
                rel_path, from, to, ..
            } => {
                json!({ "op": "rewrite_refs", "path": rel_path.to_string_lossy(), "from": from, "to": to })
            }
            Change::RewriteHeader { rel_path, from, to } => {
                json!({ "op": "rewrite_header", "path": rel_path.to_string_lossy(), "from": from.as_str(), "to": to.as_str() })
            }
        }
    }
}

/// A computed structural transaction (§10.1). Non-atomic; a crash mid-apply is
/// diagnosed by `pan validate` (§5.4).
#[derive(Clone, Debug)]
pub struct Plan {
    pub verb: &'static str,
    pub changes: Vec<Change>,
}

impl Plan {
    #[must_use]
    pub fn new(verb: &'static str, changes: Vec<Change>) -> Self {
        Self { verb, changes }
    }

    /// A hash of the exact computed change (§7.3). Deterministic: serde_json sorts
    /// object keys, so the same plan always hashes the same, and any reordering or
    /// edit of the change list changes the token.
    #[must_use]
    pub fn token(&self) -> String {
        let mut hasher = Sha256::new();
        hasher.update(self.verb.as_bytes());
        for change in &self.changes {
            hasher.update(b"\n");
            let bytes = serde_json::to_vec(&change.to_json()).unwrap_or_default();
            hasher.update(&bytes);
        }
        let digest = hasher.finalize();
        let mut out = String::with_capacity(digest.len() * 2);
        for byte in digest {
            use std::fmt::Write as _;
            let _ = write!(out, "{byte:02x}");
        }
        out
    }

    /// The `--dry-run` contract JSON (§5.5): the verb, the plan token, and the changes.
    #[must_use]
    pub fn to_json(&self) -> serde_json::Value {
        json!({
            "plan": self.verb,
            "token": self.token(),
            "changes": self.changes.iter().map(Change::to_json).collect::<Vec<_>>(),
        })
    }

    /// Refuse a plan that would destroy something already on disk (§5.4, §10.1).
    ///
    /// `std::fs::rename` **replaces** whatever sits at its destination, so a plan whose
    /// `to` is occupied eats a file silently and still exits `0`. Every collision is
    /// collected and named at once, so one dry-run answers for the whole plan rather
    /// than failing on the first.
    ///
    /// It **simulates** rather than asking the disk directly, and that is the whole of
    /// the work: a plan is a *sequence*, and a recode renames the branch's directory
    /// first — so a later change's `to` names a path that does not exist yet, while the
    /// file it would destroy sits under the old one. Each virtual path is therefore
    /// mapped back through the renames already planned before the (unchanged) tree is
    /// asked, and a path this plan has already vacated is not a collision.
    ///
    /// Also the pre-flight §10.1 wanted for its own sake: an occupied destination is
    /// refused whether it holds a file or a directory, so a plan aborting halfway on
    /// `ENOTEMPTY` — a partial apply, repaired by hand — never starts.
    pub fn preflight(&self, root: &Path) -> Result<()> {
        let mut renames: Vec<(PathBuf, PathBuf)> = Vec::new();
        let mut gone: HashSet<PathBuf> = HashSet::new();
        let mut collisions: Vec<String> = Vec::new();

        for change in &self.changes {
            match change {
                Change::Rename { from, to } => {
                    let real_from = real_path(&renames, from);
                    let real_to = real_path(&renames, to);
                    if !gone.contains(&real_to)
                        && occupied(&root.join(&real_to), &root.join(&real_from))
                    {
                        // Named by where they sit *now*, not by their names in the plan:
                        // a plan path is virtual (it may live under a rename this plan
                        // has not made yet), and what a hand has to move is real.
                        collisions.push(format!(
                            "{} is in the way of {}",
                            real_to.display(),
                            real_from.display()
                        ));
                    }
                    gone.insert(real_from);
                    gone.remove(&real_to);
                    renames.push((from.clone(), to.clone()));
                }
                Change::Remove { rel_path } | Change::RemoveEmptyDir { rel_path } => {
                    gone.insert(real_path(&renames, rel_path));
                }
                // `create_dir_all` is idempotent; a rewrite edits a file in place.
                Change::Mkdir { .. }
                | Change::RewriteRefs { .. }
                | Change::RewriteHeader { .. } => {}
            }
        }
        if collisions.is_empty() {
            return Ok(());
        }
        Err(Error::validation(format!(
            "{} of this plan's renames would overwrite something already there, and \
             nothing was applied — move what is in the way, then run it again (§5.4): {}",
            collisions.len(),
            collisions.join("; ")
        )))
    }

    /// Apply the plan against the tree root. Node mints are `create_dir`; a crash
    /// leaves a partial tree that `pan validate` reports and re-running completes.
    pub fn apply(&self, root: &Path) -> Result<()> {
        // Nothing a plan did not name may be destroyed by it (§5.4).
        self.preflight(root)?;
        for change in &self.changes {
            match change {
                Change::Mkdir { rel_path, .. } => {
                    std::fs::create_dir_all(root.join(rel_path))?;
                }
                Change::Rename { from, to } => {
                    let (source, dest) = (root.join(from), root.join(to));
                    // Asked again, a hair before the call that would replace it. The
                    // pre-flight is a plan-time answer and the plan token does not close
                    // the window it leaves: the token is checked against a freshly
                    // *computed* plan, recomputed from the same tree, so a file created
                    // in between is invisible to both. (An atomic no-clobber rename —
                    // `renameat2(RENAME_NOREPLACE)`, `renamex_np(RENAME_EXCL)` — would
                    // close it outright, at the cost of `libc` and `unsafe` in the spine
                    // and a Windows arm beside them.)
                    if occupied(&dest, &source) {
                        return Err(Error::validation(format!(
                            "{} appeared at the destination since the plan was computed — \
                             the rename would overwrite it, so it was not made (§5.4)",
                            to.display()
                        )));
                    }
                    std::fs::rename(source, dest)?;
                }
                Change::RemoveEmptyDir { rel_path } => {
                    // `remove_dir`, never `remove_dir_all`: anything that arrived under
                    // it since the plan was computed stops the removal (§5.4).
                    std::fs::remove_dir(root.join(rel_path))?;
                }
                Change::Remove { rel_path } => {
                    let target = root.join(rel_path);
                    if target.is_dir() {
                        std::fs::remove_dir_all(&target)?;
                    } else {
                        std::fs::remove_file(&target)?;
                    }
                }
                // Ref rewrites come after the renames (the record's own file has already
                // moved), so a crash leaves refs dangling on the old slug — exactly what
                // `pan validate` names (§5.4, §10.1).
                Change::RewriteRefs {
                    rel_path,
                    is_series,
                    from,
                    to,
                } => {
                    let from = crate::envelope::Ref::parse(from)?;
                    let to = crate::envelope::Ref::parse(to)?;
                    crate::cascade::rewrite_refs_in_file(root, rel_path, *is_series, &from, &to)?;
                }
                // Likewise after the renames: the rule file has already moved, so this
                // path is where it is now (§10.1).
                Change::RewriteHeader { rel_path, from, to } => {
                    let path = root.join(rel_path);
                    let text = std::fs::read_to_string(&path)?;
                    if let Some(rewritten) = crate::rule::rewrite_writes_homes(&text, from, to) {
                        std::fs::write(&path, rewritten)?;
                    }
                }
            }
        }
        Ok(())
    }

    /// Verify a caller-supplied plan token still matches (§7.3). A mismatch means the
    /// tree moved under the review — a validation failure (exit `3`).
    pub fn check_token(&self, supplied: &str) -> Result<()> {
        if self.token() == supplied {
            Ok(())
        } else {
            Err(Error::validation(
                "plan token is stale: the tree changed since the dry-run — review again (§7.3)",
            ))
        }
    }
}

/// Whether something **other than `src`** already sits at `dest`.
///
/// Identity, not existence — and the difference is not academic. APFS and HFS+ compare
/// names case- and normalization-insensitively, so `träning` in NFD and `träning` in NFC
/// are byte-different paths naming **one file**, as are `Ars` and `ars`. Asked whether the
/// destination merely *exists*, a rename between two spellings of one name collides with
/// itself: the message printed one path twice and there was nothing to move out of the
/// way. That is exactly the rename `pan validate` asks for — name normalization is
/// lowercase and NFC (§5.1) — so the guard was refusing the repair the tool itself
/// prescribes, and `pan rename <code> --label <normalized>` could not be run at all.
///
/// `symlink_metadata`, not [`Path::exists`], at both ends: a **dangling symlink** is a
/// name a rename would replace just the same, and `exists` follows the link and answers
/// `false`. Comparing the link as itself rather than as its target is also what keeps a
/// symlink out of the walk it points into.
fn occupied(dest: &Path, src: &Path) -> bool {
    let Ok(there) = std::fs::symlink_metadata(dest) else {
        return false;
    };
    match std::fs::symlink_metadata(src) {
        Ok(here) => !same_file(&there, &here),
        // Something is at the destination and the source is not there to be the same
        // thing as: a real obstruction.
        Err(_) => true,
    }
}

/// Whether two metadata readings describe one file.
///
/// `dev` + `ino` is the question itself rather than a proxy for it — it answers across
/// hard links and every spelling a case- or normalization-insensitive filesystem accepts.
#[cfg(unix)]
fn same_file(a: &std::fs::Metadata, b: &std::fs::Metadata) -> bool {
    use std::os::unix::fs::MetadataExt;
    a.dev() == b.dev() && a.ino() == b.ino()
}

/// Off unix there is no `dev`/`ino` in `std`, and the portable answers each cost
/// something this does not need: `canonicalize` follows symlinks, which would compare a
/// link against its target, and a `same-file` dependency buys one predicate (§13).
///
/// So the answer is **no**, which resolves to *occupied* and refuses. This loosens a
/// guard against data loss, and the conservative direction for a platform the suite does
/// not test on is to keep refusing — a case-only rename on Windows is a rename through a
/// temporary name, where a wrong `true` here is a file destroyed.
#[cfg(not(unix))]
fn same_file(_a: &std::fs::Metadata, _b: &std::fs::Metadata) -> bool {
    false
}

/// Map a path in the plan's *virtual* tree back to where it lives on the unchanged
/// tree, by undoing the renames planned before it — newest first, since a later rename
/// may sit under an earlier one's target.
fn real_path(renames: &[(PathBuf, PathBuf)], virt: &Path) -> PathBuf {
    let mut out = virt.to_path_buf();
    for (from, to) in renames.iter().rev() {
        if out == *to {
            out.clone_from(from);
        } else if let Ok(rest) = out.strip_prefix(to) {
            out = from.join(rest);
        }
    }
    out
}

/// A command's result: emitted data, or a plan awaiting confirmation / dry-run.
#[derive(Clone, Debug)]
pub enum Outcome {
    Emitted(serde_json::Value),
    Plan(Plan),
}
