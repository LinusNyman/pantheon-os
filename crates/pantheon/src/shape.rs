//! The record [`Shape`] (§7.1): the three storage shapes, told apart by a core's
//! declared token → shape mapping. `named` is structure, not semantics, so the
//! spine reads it — to decide whether a series name is a uniqueness-checked ref
//! target — without knowing a token's meaning (I5).

use serde::{Deserialize, Serialize};

/// One of the three storage shapes a token can name (§6.1, §7.1).
///
/// The serde form is contract: it rides in each core's `schema` JSON and the spine
/// reads it over PATH discovery (§5.0). Internally tagged, so:
/// `{"shape":"partitioned"}` · `{"shape":"series","named":true}` ·
/// `{"shape":"document"}`.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(tag = "shape", rename_all = "snake_case")]
pub enum Shape {
    /// One `.json` object per entity; kind (and, when partitioned, slug) in the
    /// filename. A thing that endures (§6.1).
    Partitioned,
    /// One `.jsonl` collection, many keyed lines. A thing sampled over time (§6.1).
    /// `named`: a hand-named series (`true`) is a ref target checked for
    /// uniqueness; a determined-name series (`false`) is reached only through its
    /// entity and is never checked as a name (§5.4).
    Series { named: bool },
    /// One text file per document, TOML frontmatter over opaque prose. A thing
    /// written (§6.1).
    Document,
}
