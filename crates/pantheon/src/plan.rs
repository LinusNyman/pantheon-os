//! Planned transactions (§10.1, §7.3). Structural operations compute a plan first;
//! a plan is emitted for review (`--dry-run`) or applied (`-y`). The plan token
//! guards against acting on a stale review: it hashes the exact computed change, so
//! anything moved underneath in between forces a fresh look (§7.3).

use std::path::{Path, PathBuf};

use serde_json::json;
use sha2::{Digest, Sha256};

use crate::code::Code;
use crate::{Error, Result};

/// One change in a plan. Step 1 mints nodes; `Rename`/`Remove` are here for the
/// structural verbs that land later.
#[derive(Clone, Debug)]
pub enum Change {
    Mkdir { code: Code, rel_path: PathBuf },
    Rename { from: PathBuf, to: PathBuf },
    Remove { rel_path: PathBuf },
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

    /// Apply the plan against the tree root. Node mints are `create_dir`; a crash
    /// leaves a partial tree that `pan validate` reports and re-running completes.
    pub fn apply(&self, root: &Path) -> Result<()> {
        for change in &self.changes {
            match change {
                Change::Mkdir { rel_path, .. } => {
                    std::fs::create_dir_all(root.join(rel_path))?;
                }
                Change::Rename { from, to } => {
                    std::fs::rename(root.join(from), root.join(to))?;
                }
                Change::Remove { rel_path } => {
                    let target = root.join(rel_path);
                    if target.is_dir() {
                        std::fs::remove_dir_all(&target)?;
                    } else {
                        std::fs::remove_file(&target)?;
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

/// A command's result: emitted data, or a plan awaiting confirmation / dry-run.
#[derive(Clone, Debug)]
pub enum Outcome {
    Emitted(serde_json::Value),
    Plan(Plan),
}
