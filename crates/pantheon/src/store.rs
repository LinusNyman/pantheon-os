//! The generic verb machinery (§7.1). `pantheon::store` implements the twelve verbs
//! generically over a core's small declaration — validate, then (by the token's
//! shape) rewrite an entity `.json`, add/edit/rm a keyed line in a `.jsonl` series,
//! or rewrite a document, each under the file lock (§6.4).
//!
//! Step 1 lands the type and its signatures — the substrate every later core
//! compiles against. The verb bodies firm up with the first core (Annales, step 2):
//! a verb with no core to exercise it cannot be honestly snapshot-frozen.

use std::marker::PhantomData;
use std::path::PathBuf;

use crate::code::Code;
use crate::core::Core;
use crate::envelope::{Line, Ref};
use crate::{Error, Result};

/// The subtree-scoped store over one core's records (§7.1).
pub struct Store<C: Core> {
    root: PathBuf,
    _core: PhantomData<C>,
}

impl<C: Core> Store<C> {
    #[must_use]
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            _core: PhantomData,
        }
    }

    /// Validate, then write by the token's shape, under the record lock (§6.4, §7.1).
    pub fn write(
        &self,
        home: &Code,
        kind: &str,
        key_or_slug: &str,
        refs: Vec<Ref>,
        record: &C::Record,
    ) -> Result<()> {
        let _ = (&self.root, home, kind, key_or_slug, refs, record);
        Err(deferred())
    }

    /// A subtree walk folded to the present (§7.1).
    pub fn fold(&self, at: Option<&Code>, kind: Option<&str>) -> Result<Vec<C::Record>> {
        let _ = (&self.root, at, kind);
        Err(deferred())
    }

    /// Read one entity by slug (§7.1).
    pub fn get(&self, slug: &str) -> Result<C::Record> {
        let _ = (&self.root, slug);
        Err(deferred())
    }

    /// Read a series collection (§7.1).
    pub fn series(&self, name: &str) -> Result<Vec<Line<C::Record>>> {
        let _ = (&self.root, name);
        Err(deferred())
    }
}

fn deferred() -> Error {
    Error::runtime("Store verbs land with the first core (step 2, §7.1)")
}
