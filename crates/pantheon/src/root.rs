//! Root resolution (§6.2): `--root` wins, else `$PANTHEON_ROOT`, else a usage error.
//! There is no default — an unset, unflagged root is a usage error (exit `2`), never
//! a guess and never a walk-up.

use std::path::{Path, PathBuf};

use crate::{Error, Result};

/// Resolve the tree root from the optional `--root` flag and the environment (§6.2).
/// Resolves the *pointer* only; whether the tree exists there is a later concern.
pub fn resolve_root(flag: Option<&Path>) -> Result<PathBuf> {
    if let Some(p) = flag {
        return Ok(p.to_path_buf());
    }
    match std::env::var_os("PANTHEON_ROOT") {
        Some(v) if !v.is_empty() => Ok(PathBuf::from(v)),
        _ => Err(Error::usage(
            "no root: pass --root or set $PANTHEON_ROOT — there is no default (§6.2)",
        )),
    }
}
