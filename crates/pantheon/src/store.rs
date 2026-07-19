//! The generic verb machinery (§7.1). `pantheon::store` implements the twelve verbs
//! generically over a core's small declaration — validate, then (by the token's
//! shape) rewrite an entity `.json`, add/edit/rm a keyed line in a `.jsonl` series,
//! or rewrite a document, each under the file lock (§6.4).
//!
//! Step 2 (Annales) landed the **Series** paths; step 3 (Album) lands the
//! **Partitioned** ones. The Document path stays deferred until Tabella (step 5):
//! a verb with no core to exercise it cannot be honestly snapshot-frozen.
//!
//! A series file is `[code]__[kind]__[name].jsonl` in its node's meta dir (§5.2),
//! one [`Line`] per keyed record. A write rewrites the whole file under the record
//! lock, leaving untouched lines byte-for-byte as they were — the file stays a thing
//! a hand can read and edit (I6, I8).
//!
//! A partitioned entity is one `.json` object in that same meta dir, its kind and
//! slug in the filename (§6.1). It takes one of two forms — `[code]__[kind]__[slug]`
//! ordinarily, or `[code]__[kind]` for an entity promoted to its own node, whose
//! slug *is* the node's definition (§5.2). [`Store::entity_file_name`] is the single
//! place that chooses, so `add`, `move`, `edit -k`, and `rename` cannot disagree.

use std::marker::PhantomData;
use std::path::{Path, PathBuf};

use crate::classify::{DocExt, FileClass, classify};
use crate::code::{Code, CodeForm, NodeName};
use crate::core::Core;
use crate::document::Document;
use crate::envelope::{Entity, Frontmatter, Key, KeyShape, Line, RawLine, Ref};
use crate::shape::Shape;
use crate::tree::{Node, TreeRoot, build_tree, resolve_code, resolve_node};
use crate::{Error, Result};

/// One of this core's series files, located in the tree (§5.2). The home, kind, and
/// name are the file's location and name, never read from inside it (I3).
#[derive(Clone, Debug)]
pub struct SeriesRef {
    pub home: Code,
    pub kind: String,
    /// The hand-chosen name, or `None` where the core's series is **nameless** —
    /// Pensum's one `task` per node, filed as `[code]__task.jsonl` (§7.1, §7.3).
    /// `None` is exactly [`FileClass::DeterminedSeries`]: a determined series that
    /// still *carries* a name (Rationes' `balance`, named for its holding) splits
    /// into three segments like any other, and only the registry's `named` bit tells
    /// the two apart (§5.4).
    pub name: Option<String>,
    pub path: PathBuf,
}

impl SeriesRef {
    /// What to call this series in a message. A nameless series has only its token
    /// to be called by, and that is what a hand typed to reach it (§7.3).
    #[must_use]
    pub fn label(&self) -> &str {
        self.name.as_deref().unwrap_or(&self.kind)
    }
}

/// Which of the two partitioned filename forms an entity file wears (§5.2). The
/// difference is only ever a filename: both hold the same object, and both are the
/// same `core:slug` to every ref that points at them.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum EntityForm {
    /// `[code]__[kind]__[slug].json` — one of many entities in a node's meta dir.
    Partitioned,
    /// `[code]__[kind].json` — an entity promoted to its own node, its slug the
    /// node's definition. **One per node** (§5.2).
    AsNode,
}

/// One of this core's entity files, located in the tree (§5.2). Home, kind, and slug
/// are the file's location and name, never read from inside it (I3).
#[derive(Clone, Debug)]
pub struct EntityRef {
    pub home: Code,
    pub kind: String,
    pub slug: String,
    pub path: PathBuf,
    pub form: EntityForm,
}

/// Where an entity is to be written — the address a write, a move, a kind change, or
/// a rename aims at. The same three fields an [`EntityRef`] carries, without a path,
/// because the path is derived from them (I3).
#[derive(Clone, Debug)]
pub struct EntityAddr {
    pub home: Code,
    pub kind: String,
    pub slug: String,
}

/// One of this core's documents, located in the tree (§6.1). Home and slug are the
/// file's location and name, never read from inside it (I3).
///
/// There is no `kind`: a Document core declares no tokens, which is what *names* it
/// Document (§7.1), and why these filenames carry a single `_` and no `__` segment.
#[derive(Clone, Debug)]
pub struct DocumentRef {
    pub home: Code,
    pub slug: String,
    pub ext: DocExt,
    pub path: PathBuf,
}

/// Where a document is to be written — the address an `add`, a `move`, or a
/// `rename` aims at, without a path because the path is derived from it (I3).
#[derive(Clone, Debug)]
pub struct DocumentAddr {
    pub home: Code,
    pub slug: String,
    pub ext: DocExt,
}

/// A series folded to its present (I1): the line at the latest key, carrying the
/// series it came from — a fold across a subtree loses that otherwise.
#[derive(Clone, Debug)]
pub struct PresentLine<T> {
    pub home: Code,
    pub kind: String,
    /// The series' name, or `None` where the core's series is nameless (§7.1).
    pub name: Option<String>,
    pub line: Line<T>,
}

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

    pub fn root(&self) -> &Path {
        &self.root
    }

    // ── the core's own tokens (§5.0: another core's series is not ours to count) ──

    /// This core's tokens stored as a series (§7.1).
    fn series_kinds() -> impl Iterator<Item = &'static str> {
        C::kinds()
            .iter()
            .filter(|(_, shape)| matches!(shape, Shape::Series { .. }))
            .map(|(token, _)| *token)
    }

    /// Whether `kind` is one of this core's series tokens.
    pub fn owns_series_kind(kind: &str) -> bool {
        Self::series_kinds().any(|k| k == kind)
    }

    /// The core's sole series token. A core declaring more than one must be told
    /// which with `-k` (§7.2); a core keeping no series cannot take a series verb.
    pub fn sole_series_kind() -> Result<&'static str> {
        let mut kinds = Self::series_kinds();
        let first = kinds
            .next()
            .ok_or_else(|| Error::usage(format!("{} keeps no series (§7.1)", C::NAME)))?;
        if kinds.next().is_some() {
            return Err(Error::usage(format!(
                "{} declares more than one series token; name one with -k (§7.2)",
                C::NAME
            )));
        }
        Ok(first)
    }

    // ── locating ────────────────────────────────────────────────────────────────

    /// The single place that chooses between the two series filename forms (§5.2) —
    /// the counterpart of [`Store::entity_file_name`], and held to the same
    /// discipline: every writer goes through here, so `add`, `move`, and `rename`
    /// cannot land a record in one form and look for it in the other.
    fn series_file_name(home: &Code, kind: &str, name: Option<&str>) -> String {
        match name {
            Some(name) => format!("{}__{kind}__{name}.jsonl", home.as_str()),
            None => format!("{}__{kind}.jsonl", home.as_str()),
        }
    }

    /// Where a series lives, whether or not it exists yet (§5.2).
    pub fn series_path(&self, home: &Code, kind: &str, name: Option<&str>) -> Result<PathBuf> {
        let node = resolve_code(&self.root, home)?;
        Ok(node
            .join(format!("{}__", home.as_str()))
            .join(Self::series_file_name(home, kind, name)))
    }

    /// Every one of this core's series in the tree (or under `at`), optionally
    /// filtered by token and name. The walk reads filenames only (I5, §5.0).
    pub fn find_series(
        &self,
        at: Option<&Code>,
        kind: Option<&str>,
        name: Option<&str>,
    ) -> Result<Vec<SeriesRef>> {
        let nodes = match build_tree(&self.root, at)? {
            TreeRoot::Forest(nodes) => nodes,
            TreeRoot::Subtree(node) => vec![node],
        };
        let mut out = Vec::new();
        for node in &nodes {
            Self::collect_series(node, kind, name, &mut out)?;
        }
        out.sort_by(|a, b| {
            a.home
                .as_str()
                .cmp(b.home.as_str())
                .then_with(|| a.name.cmp(&b.name))
        });
        Ok(out)
    }

    fn collect_series(
        node: &Node,
        kind: Option<&str>,
        name: Option<&str>,
        out: &mut Vec<SeriesRef>,
    ) -> Result<()> {
        // Records live in the node's meta dir (§5.2).
        let meta = node.path.join(format!("{}__", node.code.as_str()));
        if meta.is_dir() {
            for entry in std::fs::read_dir(&meta)? {
                let entry = entry?;
                if entry.file_type()?.is_dir() {
                    continue;
                }
                let file_name = entry.file_name().to_string_lossy().into_owned();
                // Both series filename forms (§5.2): three segments carry a name,
                // two are the nameless form a determined series wears (§7.1).
                let (found_kind, found_name) = match classify(&file_name, false, &node.code) {
                    FileClass::NamedSeries {
                        kind, name: series, ..
                    } => (kind, Some(series)),
                    FileClass::DeterminedSeries { kind, .. } => (kind, None),
                    _ => continue,
                };
                // Another core's series at this node is not ours to count (§5.0). A
                // name filter asks for a named series, so it never matches a nameless
                // one — which is why Pensum, having only the one, always filters by
                // home and token instead (§7.3).
                if !Self::owns_series_kind(&found_kind)
                    || kind.is_some_and(|want| want != found_kind)
                    || name.is_some_and(|want| Some(want) != found_name.as_deref())
                {
                    continue;
                }
                out.push(SeriesRef {
                    home: node.code.clone(),
                    kind: found_kind,
                    name: found_name,
                    path: entry.path(),
                });
            }
        }
        for child in &node.children {
            Self::collect_series(child, kind, name, out)?;
        }
        Ok(())
    }

    /// Resolve a series by name to exactly one file (§7.3). Zero is not found (exit
    /// `4`); more than one is reported with its candidate homes rather than guessed
    /// (exit `2`).
    pub fn locate(&self, name: &str, kind: Option<&str>, at: Option<&Code>) -> Result<SeriesRef> {
        let mut found = self.find_series(at, kind, Some(name))?;
        match found.len() {
            0 => Err(Error::not_found(format!(
                "no {} series named {name:?} (§7.3)",
                C::NAME
            ))),
            1 => Ok(found.pop().expect("one candidate")),
            _ => Err(Error::usage(format!(
                "series {name:?} is at more than one node: {} — name one with -H (§7.3)",
                found
                    .iter()
                    .map(|s| s.home.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ))),
        }
    }

    // ── reading ─────────────────────────────────────────────────────────────────

    /// Read a collection whole (§7.2).
    pub fn read_series(&self, sref: &SeriesRef) -> Result<Vec<Line<C::Record>>> {
        let text = match std::fs::read_to_string(&sref.path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Err(Error::not_found(format!(
                    "no series {:?} at {} (§7.3)",
                    sref.name,
                    sref.home.as_str()
                )));
            }
            Err(e) => return Err(e.into()),
        };
        Self::parse_lines(&text, &sref.path)
    }

    fn parse_lines(text: &str, path: &Path) -> Result<Vec<Line<C::Record>>> {
        let mut out = Vec::new();
        for (i, raw) in text.lines().enumerate() {
            if raw.trim().is_empty() {
                continue;
            }
            let line: Line<C::Record> = serde_json::from_str(raw).map_err(|e| {
                Error::validation(format!(
                    "{}: line {} does not parse: {e}",
                    path.display(),
                    i + 1
                ))
            })?;
            out.push(line);
        }
        Ok(out)
    }

    /// The present of a series (I1), and the one rule that covers both kinds of key
    /// (§5.4). A **date-keyed** line is a *sample*, so the series folds to the line at
    /// the latest key — keys sort lexicographically, which for `YYMMDD`(`Thhmm`) is
    /// chronological. A **name-keyed** line is a *record* (a Pensum task), so it is
    /// already its own present and every one survives the fold.
    ///
    /// [`Key::classify`] is the whole discriminant, so this holds for any core
    /// without the spine learning what a token means (I5).
    fn present(lines: Vec<Line<C::Record>>) -> Vec<Line<C::Record>> {
        let (mut named, dated): (Vec<_>, Vec<_>) = lines
            .into_iter()
            .partition(|line| line.key.classify() == KeyShape::Name);
        if let Some(latest) = dated
            .into_iter()
            .max_by(|a, b| a.key.as_str().cmp(b.key.as_str()))
        {
            named.push(latest);
        }
        named
    }

    /// `get` for a Series core (§7.2): the named series folded to its present.
    pub fn get(
        &self,
        name: &str,
        kind: Option<&str>,
        at: Option<&Code>,
    ) -> Result<PresentLine<C::Record>> {
        let sref = self.locate(name, kind, at)?;
        let lines = self.read_series(&sref)?;
        let line = Self::present(lines).pop().ok_or_else(|| {
            Error::not_found(format!(
                "series {:?} at {} holds no readings yet (§7.3)",
                sref.label(),
                sref.home.as_str()
            ))
        })?;
        Ok(PresentLine {
            home: sref.home,
            kind: sref.kind,
            name: sref.name,
            line,
        })
    }

    /// A subtree walk folded to the present (§7.1): every one of this core's series
    /// under `at`, each folded by the rule above — a date-keyed series to its latest
    /// key, a name-keyed one to all of its records.
    pub fn fold(
        &self,
        at: Option<&Code>,
        kind: Option<&str>,
    ) -> Result<Vec<PresentLine<C::Record>>> {
        let mut out = Vec::new();
        for sref in self.find_series(at, kind, None)? {
            let lines = self.read_series(&sref)?;
            for line in Self::present(lines) {
                out.push(PresentLine {
                    home: sref.home.clone(),
                    kind: sref.kind.clone(),
                    name: sref.name.clone(),
                    line,
                });
            }
        }
        Ok(out)
    }

    // ── writing (§6.4: every write under the record lock) ───────────────────────

    /// Mint an empty series — the `-c` path (§7.3). A hand-named series is minted
    /// explicitly so a typo cannot conjure one.
    pub fn create_series(&self, home: &Code, kind: &str, name: &str) -> Result<SeriesRef> {
        let path = self.series_path(home, kind, Some(name))?;
        if path.exists() {
            return Err(Error::validation(format!(
                "series {name:?} already exists at {} (§7.3)",
                home.as_str()
            )));
        }
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        crate::lock::with_record_lock(&path, |_| Ok(Vec::new()))?;
        crate::hook::note_write(C::NAME, home);
        Ok(SeriesRef {
            home: home.clone(),
            kind: kind.to_string(),
            name: Some(name.to_string()),
            path,
        })
    }

    /// Whether this core files `kind` under a hand-chosen name (§7.1). The first read
    /// of the `named` bit anywhere: it is what decides whether a missing series file
    /// is a not-found the hand must answer with `-c`, or one the write mints itself.
    fn series_is_named(kind: &str) -> Option<bool> {
        C::kinds().iter().find_map(|(token, shape)| match shape {
            Shape::Series { named } if *token == kind => Some(*named),
            _ => None,
        })
    }

    /// Add or overwrite one keyed line (§6.1). A correction rewrites the keyed line
    /// in place; it never stacks a second (I1). Lines the write does not touch are
    /// carried through verbatim.
    pub fn write_line(&self, sref: &SeriesRef, line: &Line<C::Record>) -> Result<()> {
        if !sref.path.exists() {
            // A hand-named series is minted explicitly, so a typo cannot conjure one
            // (§7.3, §18). A determined-name series has nothing to mistype and is
            // minted by its determinant — for Pensum's nameless `task`, the node's
            // first task, which is this write.
            //
            // Step 7 note: Rationes' `balance` is determined by a *holding entity*,
            // not by the node, so `rat` must check that determinant exists in its own
            // bin before writing. The store cannot know — it links no core (I5).
            if Self::series_is_named(&sref.kind) != Some(false) {
                return Err(Error::not_found(format!(
                    "no series {:?} at {} — mint it with -c (§7.3)",
                    sref.label(),
                    sref.home.as_str()
                )));
            }
            // The mint rides *inside* the write's own lock rather than preceding it:
            // `open_for_lock` creates, and the inode re-check retries a writer whose
            // file was renamed underneath (§6.4). Minting first would be a second,
            // unsynchronized write — two hands filing a node's first task at once
            // would each truncate the file the other had just filled.
            if let Some(parent) = sref.path.parent() {
                std::fs::create_dir_all(parent)?;
            }
        }
        let encoded = serde_json::to_string(line)?;
        let key = line.key.clone();
        crate::lock::with_record_lock(&sref.path, move |prev| {
            let text = Self::as_text(prev)?;
            let mut out = String::with_capacity(text.len() + encoded.len() + 1);
            let mut replaced = false;
            for raw in text.lines() {
                if raw.trim().is_empty() {
                    continue;
                }
                let existing: RawLine = serde_json::from_str(raw)?;
                if !replaced && existing.key == key {
                    out.push_str(&encoded);
                    replaced = true;
                } else {
                    out.push_str(raw);
                }
                out.push('\n');
            }
            if !replaced {
                out.push_str(&encoded);
                out.push('\n');
            }
            Ok(out.into_bytes())
        })?;
        crate::hook::note_write(C::NAME, &sref.home);
        Ok(())
    }

    /// Drop one keyed line (§7.2). Irreversible — §18 keeps no history.
    pub fn remove_line(&self, sref: &SeriesRef, key: &Key) -> Result<()> {
        if !sref.path.exists() {
            return Err(Error::not_found(format!(
                "no series {:?} at {} (§7.3)",
                sref.label(),
                sref.home.as_str()
            )));
        }
        let key = key.clone();
        let mut found = false;
        crate::lock::with_record_lock(&sref.path, |prev| {
            let text = Self::as_text(prev)?;
            let mut out = String::with_capacity(text.len());
            for raw in text.lines() {
                if raw.trim().is_empty() {
                    continue;
                }
                let existing: RawLine = serde_json::from_str(raw)?;
                if existing.key == key {
                    found = true;
                    continue;
                }
                out.push_str(raw);
                out.push('\n');
            }
            Ok(out.into_bytes())
        })?;
        if found {
            crate::hook::note_write(C::NAME, &sref.home);
            Ok(())
        } else {
            Err(Error::not_found(format!(
                "no line keyed {key} in series {:?} at {} (§7.3)",
                sref.label(),
                sref.home.as_str()
            )))
        }
    }

    /// Move a series file to a new home, a new name, or both — the primitive behind
    /// a hand-named series' `rename` and `move` (§7.2), and the exact counterpart of
    /// [`Store::relocate_entity`]. A file `mv`; the lines are not touched.
    ///
    /// Refuses to land on an occupied path rather than clobber it (exit `3`).
    pub fn relocate_series(
        &self,
        sref: &SeriesRef,
        to_home: &Code,
        to_name: &str,
    ) -> Result<SeriesRef> {
        let path = self.series_path(to_home, &sref.kind, Some(to_name))?;
        if path == sref.path {
            return Ok(sref.clone());
        }
        if path.exists() {
            return Err(Error::validation(format!(
                "{} already holds a {:?} series named {to_name:?} (§7.2)",
                to_home.as_str(),
                sref.kind
            )));
        }
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::rename(&sref.path, &path)?;
        // Both ends: a relocation that crosses homes has written at each, which is
        // what collapses the wake to triggerless (§9.3).
        crate::hook::note_write(C::NAME, &sref.home);
        crate::hook::note_write(C::NAME, to_home);
        Ok(SeriesRef {
            home: to_home.clone(),
            kind: sref.kind.clone(),
            name: Some(to_name.to_string()),
            path,
        })
    }

    // ── reaching a line by its key (§5.4) ───────────────────────────────────────

    /// Every one of this core's lines keyed `key`, with the series each sits in.
    ///
    /// **The one lookup that opens record files** rather than resting on their names
    /// (§5.0): a name-keyed line's identity is inside its series, so finding it walks
    /// this core's series in scope and scans their keys. The naming triple isolates
    /// the walk to the small text, and bulk is never opened (§6.3).
    pub fn find_line(
        &self,
        key: &Key,
        kind: Option<&str>,
        at: Option<&Code>,
    ) -> Result<Vec<(SeriesRef, Line<C::Record>)>> {
        let mut out = Vec::new();
        for sref in self.find_series(at, kind, None)? {
            if let Some(line) = self
                .read_series(&sref)?
                .into_iter()
                .find(|line| line.key == *key)
            {
                out.push((sref, line));
            }
        }
        Ok(out)
    }

    /// Resolve a key to exactly one line (§7.3). Zero is not found (exit `4`); more
    /// than one is listed with its candidate homes rather than guessed (exit `2`),
    /// exactly as an ambiguous slug is — a key is unique **per core**, not per node,
    /// so a lookup with no `-H` is deliberately tree-wide (§5.4).
    pub fn locate_line(
        &self,
        key: &Key,
        kind: Option<&str>,
        at: Option<&Code>,
    ) -> Result<(SeriesRef, Line<C::Record>)> {
        let mut found = self.find_line(key, kind, at)?;
        match found.len() {
            0 => Err(Error::not_found(format!(
                "no {} record keyed {key} (§7.3)",
                C::NAME
            ))),
            1 => Ok(found.pop().expect("one candidate")),
            _ => Err(Error::usage(format!(
                "{key} keys a record at more than one node: {} — name one with -H (§7.3)",
                found
                    .iter()
                    .map(|(s, _)| s.home.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ))),
        }
    }

    /// This core's lines keyed `key` at some node *other* than `home` — the soft half
    /// of §5.4's per-core uniqueness. Finding these costs a walk, which is the cost
    /// the softness exists to avoid paying on every write, so `add` warns and the fix
    /// is made at the source (§18).
    pub fn duplicate_keys_elsewhere(
        &self,
        home: &Code,
        key: &Key,
        kind: Option<&str>,
    ) -> Result<Vec<SeriesRef>> {
        Ok(self
            .find_line(key, kind, None)?
            .into_iter()
            .map(|(sref, _)| sref)
            .filter(|sref| sref.home.as_str() != home.as_str())
            .collect())
    }

    /// Re-key one line in place — the primitive behind a name-keyed record's `rename`
    /// (§7.2), and the counterpart of [`Store::relocate_series`] and
    /// [`Store::relocate_entity`]. Where those are a file `mv`, this is a rewrite
    /// *inside* a file: a task's identity is its key, and its key is a line's first
    /// field rather than a filename (§5.4).
    ///
    /// **Call this before [`Cascade`](crate::Cascade)`::apply`, never after** — the
    /// same ordering as a relocate, for the same reason. Refuses an occupied key
    /// (exit `3`): the within-file half of the check `plan_cascade` makes tree-wide.
    /// Untouched lines are carried through byte-for-byte.
    pub fn rename_line(&self, sref: &SeriesRef, from: &Key, to: &Key) -> Result<()> {
        if from == to {
            return Ok(());
        }
        let (from, to) = (from.clone(), to.clone());
        let mut found = false;
        crate::lock::with_record_lock(&sref.path, |prev| {
            let text = Self::as_text(prev)?;
            let mut out = String::with_capacity(text.len());
            for raw in text.lines() {
                if raw.trim().is_empty() {
                    continue;
                }
                let mut existing: RawLine = serde_json::from_str(raw)?;
                if existing.key == to {
                    return Err(Error::validation(format!(
                        "{} already keys a record at {} — renaming onto it would make \
                         the two indistinguishable (§7.2, §18)",
                        to,
                        sref.home.as_str()
                    )));
                }
                if existing.key == from {
                    found = true;
                    existing.key = to.clone();
                    out.push_str(&serde_json::to_string(&existing)?);
                } else {
                    out.push_str(raw);
                }
                out.push('\n');
            }
            Ok(out.into_bytes())
        })?;
        if found {
            crate::hook::note_write(C::NAME, &sref.home);
            Ok(())
        } else {
            Err(Error::not_found(format!(
                "no line keyed {from} in series {:?} at {} (§7.3)",
                sref.label(),
                sref.home.as_str()
            )))
        }
    }

    /// Re-home one line — the primitive behind a name-keyed record's `move` (§7.2).
    ///
    /// Unlike [`Store::relocate_series`] this is **not a file `mv`**: a task's home is
    /// its node's series file, so re-homing the record moves a *line between two
    /// files*. Two files means two record locks (§6.4) and no atomicity — `rename`'s
    /// one-syscall guarantee is simply not available here. So the order is chosen by
    /// what `pan validate` can say if a crash lands between them:
    ///
    /// - **Destination first** leaves the line at *both* nodes. That is the soft
    ///   `duplicate_slug` finding §5.4 already models, naming both files — precisely
    ///   the two a hand must look at — every ref still resolves, and the fix is
    ///   finishing the move.
    /// - **Source first** leaves it at *neither*, and §18 keeps no copy. Every ref
    ///   becomes a dangling-ref *error* reported at the referring file, which cannot
    ///   say where the record went.
    ///
    /// One order's crash is a state the lint already describes; the other's is loss
    /// diagnosed at the wrong file. Destination first, source second.
    pub fn move_line(&self, from: &SeriesRef, to_home: &Code, key: &Key) -> Result<SeriesRef> {
        if from.home.as_str() == to_home.as_str() {
            return Ok(from.clone());
        }
        let line = self
            .read_series(from)?
            .into_iter()
            .find(|line| line.key == *key)
            .ok_or_else(|| {
                Error::not_found(format!(
                    "no line keyed {key} in series {:?} at {} (§7.3)",
                    from.label(),
                    from.home.as_str()
                ))
            })?;

        let dest = SeriesRef {
            home: to_home.clone(),
            kind: from.kind.clone(),
            name: from.name.clone(),
            path: self.series_path(to_home, &from.kind, from.name.as_deref())?,
        };
        // Refusing here rather than clobbering. The window between this check and the
        // write is the one §6.4 already declines to close — only a tree-wide lock
        // would, and §18 leaves nowhere to keep one.
        if dest.path.exists() && self.read_series(&dest)?.iter().any(|line| line.key == *key) {
            return Err(Error::validation(format!(
                "{} already holds a record keyed {key} (§7.2)",
                to_home.as_str()
            )));
        }
        self.write_line(&dest, &line)?;
        self.remove_line(from, key)?;
        Ok(dest)
    }

    fn as_text(prev: Option<&[u8]>) -> Result<&str> {
        prev.map(std::str::from_utf8)
            .transpose()
            .map_err(|e| Error::runtime(format!("series file is not UTF-8: {e}")))
            .map(Option::unwrap_or_default)
    }

    // ── the partitioned register (§6.1) ─────────────────────────────────────────

    /// This core's tokens stored as one object per entity (§7.1).
    fn entity_kinds() -> impl Iterator<Item = &'static str> {
        C::kinds()
            .iter()
            .filter(|(_, shape)| matches!(shape, Shape::Partitioned))
            .map(|(token, _)| *token)
    }

    /// Whether `kind` is one of this core's partitioned tokens.
    pub fn owns_entity_kind(kind: &str) -> bool {
        Self::entity_kinds().any(|k| k == kind)
    }

    /// The one rule that chooses between the two entity filename forms (§5.2): the
    /// entity-as-node form iff this node is definition-prefix **and** the slug *is*
    /// its definition; otherwise the ordinary partitioned form.
    ///
    /// Every writer goes through here — `add`, `move`, `edit -k`, and `rename`'s file
    /// half — so a record cannot land in one form and be looked for in the other.
    fn entity_file_name(node: &NodeName, kind: &str, slug: &str) -> (String, EntityForm) {
        let code = node.code.as_str();
        if node.form == CodeForm::DefinitionPrefix && node.label == slug {
            (format!("{code}__{kind}.json"), EntityForm::AsNode)
        } else {
            (
                format!("{code}__{kind}__{slug}.json"),
                EntityForm::Partitioned,
            )
        }
    }

    /// Where an entity lives, whether or not it exists yet (§5.2).
    pub fn entity_path(&self, addr: &EntityAddr) -> Result<(PathBuf, EntityForm)> {
        let (node, node_path) = resolve_node(&self.root, &addr.home)?;
        let (file_name, form) = Self::entity_file_name(&node, &addr.kind, &addr.slug);
        let path = node_path
            .join(format!("{}__", addr.home.as_str()))
            .join(file_name);
        Ok((path, form))
    }

    /// Every one of this core's entities in the tree (or under `at`), optionally
    /// filtered by token and slug. The walk reads filenames only (I5, §5.0) — a fold
    /// opens records, but finding them never does.
    pub fn find_entities(
        &self,
        at: Option<&Code>,
        kind: Option<&str>,
        slug: Option<&str>,
    ) -> Result<Vec<EntityRef>> {
        let nodes = match build_tree(&self.root, at)? {
            TreeRoot::Forest(nodes) => nodes,
            TreeRoot::Subtree(node) => vec![node],
        };
        let mut out = Vec::new();
        for node in &nodes {
            Self::collect_entities(node, kind, slug, &mut out)?;
        }
        out.sort_by(|a, b| {
            a.home
                .as_str()
                .cmp(b.home.as_str())
                .then_with(|| a.slug.cmp(&b.slug))
        });
        Ok(out)
    }

    fn collect_entities(
        node: &Node,
        kind: Option<&str>,
        slug: Option<&str>,
        out: &mut Vec<EntityRef>,
    ) -> Result<()> {
        let meta = node.path.join(format!("{}__", node.code.as_str()));
        if meta.is_dir() {
            for entry in std::fs::read_dir(&meta)? {
                let entry = entry?;
                if entry.file_type()?.is_dir() {
                    continue;
                }
                let file_name = entry.file_name().to_string_lossy().into_owned();
                // Both forms are the same entity to a ref; only the filename differs.
                // An entity-as-node's slug is the node's definition (§5.2), which the
                // filename does not carry — the walk supplies it from the node.
                let (found_kind, found_slug, form) = match classify(&file_name, false, &node.code) {
                    FileClass::Partitioned {
                        kind: k, slug: s, ..
                    } => (k, s, EntityForm::Partitioned),
                    FileClass::EntityNode { kind: k, .. } => {
                        (k, node.label.clone(), EntityForm::AsNode)
                    }
                    _ => continue,
                };
                // Another core's entity at this node is not ours to count (§5.0).
                if !Self::owns_entity_kind(&found_kind)
                    || kind.is_some_and(|want| want != found_kind)
                    || slug.is_some_and(|want| want != found_slug)
                {
                    continue;
                }
                out.push(EntityRef {
                    home: node.code.clone(),
                    kind: found_kind,
                    slug: found_slug,
                    path: entry.path(),
                    form,
                });
            }
        }
        for child in &node.children {
            Self::collect_entities(child, kind, slug, out)?;
        }
        Ok(())
    }

    /// Resolve a slug to exactly one entity (§7.3). Zero is not found (exit `4`);
    /// more than one is reported with its candidate homes rather than guessed
    /// (exit `2`) — a cross-node duplicate stays soft, so a resolve meeting two
    /// lists them (§5.4, §18).
    pub fn locate_entity(
        &self,
        slug: &str,
        kind: Option<&str>,
        at: Option<&Code>,
    ) -> Result<EntityRef> {
        let mut found = self.find_entities(at, kind, Some(slug))?;
        match found.len() {
            0 => Err(Error::not_found(format!(
                "no {} entity named {slug:?} (§7.3)",
                C::NAME
            ))),
            1 => Ok(found.pop().expect("one candidate")),
            _ => Err(Error::usage(format!(
                "{slug:?} is at more than one node: {} — name one with -H (§7.3)",
                found
                    .iter()
                    .map(|e| e.home.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ))),
        }
    }

    /// Whether this node already holds `slug` under **any** of this core's kinds.
    ///
    /// Exactly one `read_dir` — the hard, cheap half of §5.4's two-tier uniqueness.
    /// The filesystem refuses a duplicate *filename*, which is not the same
    /// guarantee: `csa__person__book_club.json` and `csa__group__book_club.json` are
    /// two files and one ref. **Do not let this grow past its one `read_dir`** — the
    /// cross-node check is a walk, which is exactly why it stays soft (§18).
    pub fn slug_taken_at(&self, home: &Code, slug: &str) -> Result<Option<EntityRef>> {
        let (node, node_path) = resolve_node(&self.root, home)?;
        let meta = node_path.join(format!("{}__", home.as_str()));
        if !meta.is_dir() {
            return Ok(None);
        }
        for entry in std::fs::read_dir(&meta)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                continue;
            }
            let file_name = entry.file_name().to_string_lossy().into_owned();
            let (found_kind, found_slug, form) = match classify(&file_name, false, home) {
                FileClass::Partitioned {
                    kind: k, slug: s, ..
                } => (k, s, EntityForm::Partitioned),
                FileClass::EntityNode { kind: k, .. } => {
                    (k, node.label.clone(), EntityForm::AsNode)
                }
                _ => continue,
            };
            if Self::owns_entity_kind(&found_kind) && found_slug == slug {
                return Ok(Some(EntityRef {
                    home: home.clone(),
                    kind: found_kind,
                    slug: found_slug,
                    path: entry.path(),
                    form,
                }));
            }
        }
        Ok(None)
    }

    /// This core's entities holding `slug` at some *other* node — the soft half of
    /// §5.4's uniqueness. A walk, so `add` only warns on it and `pan validate`
    /// reports it; you fix it at the source (§18).
    pub fn duplicate_slugs_elsewhere(&self, home: &Code, slug: &str) -> Result<Vec<EntityRef>> {
        Ok(self
            .find_entities(None, None, Some(slug))?
            .into_iter()
            .filter(|e| e.home.as_str() != home.as_str())
            .collect())
    }

    /// Read one entity whole — the envelope and its `data` (§6.1).
    pub fn read_entity(&self, eref: &EntityRef) -> Result<Entity<C::Record>> {
        let text = match std::fs::read_to_string(&eref.path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Err(Error::not_found(format!(
                    "no entity {:?} at {} (§7.3)",
                    eref.slug,
                    eref.home.as_str()
                )));
            }
            Err(e) => return Err(e.into()),
        };
        serde_json::from_str(&text)
            .map_err(|e| Error::validation(format!("{}: does not parse: {e}", eref.path.display())))
    }

    /// `get` for a partitioned core (§7.2): one entity by slug. An entity is not a
    /// sample, so there is no fold to a present here — the object *is* the present.
    pub fn get_entity(
        &self,
        slug: &str,
        kind: Option<&str>,
        at: Option<&Code>,
    ) -> Result<(EntityRef, Entity<C::Record>)> {
        let eref = self.locate_entity(slug, kind, at)?;
        let entity = self.read_entity(&eref)?;
        Ok((eref, entity))
    }

    /// A subtree walk over this core's entities (§6.3). Unlike a series fold there is
    /// nothing to collapse: each file is already one object.
    pub fn fold_entities(
        &self,
        at: Option<&Code>,
        kind: Option<&str>,
    ) -> Result<Vec<(EntityRef, Entity<C::Record>)>> {
        let mut out = Vec::new();
        for eref in self.find_entities(at, kind, None)? {
            let entity = self.read_entity(&eref)?;
            out.push((eref, entity));
        }
        Ok(out)
    }

    /// Write one entity object, creating or overwriting it (§6.1, §6.4).
    ///
    /// Pretty-printed with a trailing newline: a series line must be one line, but an
    /// entity file is a whole object a hand opens and reads (I6, §6.5).
    pub fn write_entity(
        &self,
        addr: &EntityAddr,
        refs: Vec<Ref>,
        record: &C::Record,
    ) -> Result<EntityRef> {
        let (path, form) = self.entity_path(addr)?;
        let entity = Entity { refs, data: record };
        let mut encoded = serde_json::to_vec_pretty(&entity)?;
        encoded.push(b'\n');
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        crate::lock::with_record_lock(&path, move |_| Ok(encoded))?;
        crate::hook::note_write(C::NAME, &addr.home);
        Ok(EntityRef {
            home: addr.home.clone(),
            kind: addr.kind.clone(),
            slug: addr.slug.clone(),
            path,
            form,
        })
    }

    /// Delete an entity file (§7.2). Irreversible — §18 keeps no history.
    pub fn remove_entity(&self, eref: &EntityRef) -> Result<()> {
        match std::fs::remove_file(&eref.path) {
            Ok(()) => {
                crate::hook::note_write(C::NAME, &eref.home);
                Ok(())
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Err(Error::not_found(format!(
                "no entity {:?} at {} (§7.3)",
                eref.slug,
                eref.home.as_str()
            ))),
            Err(e) => Err(e.into()),
        }
    }

    /// Move an entity file to a new address — the one primitive behind three verbs:
    /// `move` (a new home), `edit -k` (a new kind), and `rename`'s file half (a new
    /// slug). Each is a file `mv` and nothing more; the object is not touched (§7.2).
    ///
    /// Refuses to land on an occupied path rather than clobber it (exit `3`).
    pub fn relocate_entity(&self, eref: &EntityRef, to: &EntityAddr) -> Result<EntityRef> {
        let (path, form) = self.entity_path(to)?;
        if path == eref.path {
            return Ok(eref.clone());
        }
        if path.exists() {
            return Err(Error::validation(format!(
                "{} already holds {:?} as a {} (§7.2)",
                to.home.as_str(),
                to.slug,
                to.kind
            )));
        }
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::rename(&eref.path, &path)?;
        // Both ends, as in `relocate_series` — a `move` across homes has no one write
        // to name (§9.3); a `rename` or `edit -k` in place notes the same home twice
        // and still names it.
        crate::hook::note_write(C::NAME, &eref.home);
        crate::hook::note_write(C::NAME, &to.home);
        Ok(EntityRef {
            home: to.home.clone(),
            kind: to.kind.clone(),
            slug: to.slug.clone(),
            path,
            form,
        })
    }

    // ── the document (§6.1) ─────────────────────────────────────────────────────
    //
    // Documents break the pattern the two shapes above share, and they break it
    // structurally rather than by degree: they live **loose in the open node dir**,
    // not the meta dir. Every `collect_*` above walks `<node>/<code>__/` and so can
    // never see one; `collect_documents` walks `<node>/` and is the only walk here
    // that does. Ownership is not by token either — a Document core has none (§7.1) —
    // it is the extension plus the node's own `[code]_` prefix, which `classify`
    // already checks, after rejecting an Auspex `function` rule (§5.2).

    /// Whether this core is the Document core — i.e. declares no tokens (§7.1). The
    /// emptiness *is* the declaration; there is nothing else to ask.
    pub fn is_document_core() -> bool {
        C::kinds().is_empty()
    }

    /// The one rule that spells a document filename (§6.1): `[code]_[slug].[ext]`,
    /// a **single** underscore. Every writer goes through here, so `add`, `move`, and
    /// `rename` cannot disagree about where a document lands.
    fn document_file_name(home: &Code, slug: &str, ext: DocExt) -> String {
        format!("{}_{slug}.{}", home.as_str(), ext.as_str())
    }

    /// Where a document lives, whether or not it exists yet (§6.1). Unlike an entity
    /// there is one filename form, so the node's *label* is never consulted — only
    /// its path.
    pub fn document_path(&self, addr: &DocumentAddr) -> Result<PathBuf> {
        let node_path = resolve_code(&self.root, &addr.home)?;
        Ok(node_path.join(Self::document_file_name(&addr.home, &addr.slug, addr.ext)))
    }

    /// Every document in the tree (or under `at`), optionally filtered by slug. Reads
    /// filenames only (§5.0) — finding a document never opens it.
    pub fn find_documents(
        &self,
        at: Option<&Code>,
        slug: Option<&str>,
    ) -> Result<Vec<DocumentRef>> {
        let nodes = match build_tree(&self.root, at)? {
            TreeRoot::Forest(nodes) => nodes,
            TreeRoot::Subtree(node) => vec![node],
        };
        let mut out = Vec::new();
        for node in &nodes {
            Self::collect_documents(node, slug, &mut out)?;
        }
        out.sort_by(|a, b| {
            a.home
                .as_str()
                .cmp(b.home.as_str())
                .then_with(|| a.slug.cmp(&b.slug))
        });
        Ok(out)
    }

    fn collect_documents(
        node: &Node,
        slug: Option<&str>,
        out: &mut Vec<DocumentRef>,
    ) -> Result<()> {
        for entry in std::fs::read_dir(&node.path)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                continue;
            }
            let file_name = entry.file_name().to_string_lossy().into_owned();
            let FileClass::Document {
                slug: found, ext, ..
            } = classify(&file_name, false, &node.code)
            else {
                continue;
            };
            if slug.is_some_and(|want| want != found) {
                continue;
            }
            out.push(DocumentRef {
                home: node.code.clone(),
                slug: found,
                ext,
                path: entry.path(),
            });
        }
        for child in &node.children {
            Self::collect_documents(child, slug, out)?;
        }
        Ok(())
    }

    /// Resolve a slug to exactly one document (§7.3). Zero is not found (exit `4`);
    /// more than one is listed rather than guessed (exit `2`), since a cross-node
    /// duplicate stays soft (§5.4, §18).
    pub fn locate_document(&self, slug: &str, at: Option<&Code>) -> Result<DocumentRef> {
        let mut found = self.find_documents(at, Some(slug))?;
        match found.len() {
            0 => Err(Error::not_found(format!(
                "no {} document named {slug:?} (§7.3)",
                C::NAME
            ))),
            1 => Ok(found.pop().expect("one candidate")),
            _ => Err(Error::usage(format!(
                "{slug:?} is at more than one node: {} — name one with -H (§7.3)",
                found
                    .iter()
                    .map(|d| d.home.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ))),
        }
    }

    /// Whether this node already holds `slug` under **any** document extension.
    ///
    /// The filesystem refuses a duplicate *filename*, which is not the same guarantee:
    /// `csa_trip_idea.md` and `csa_trip_idea.txt` are two files and one ref — §5.4's
    /// kind trap in the extension dimension. One `read_dir`, like [`Self::slug_taken_at`].
    pub fn document_slug_taken_at(&self, home: &Code, slug: &str) -> Result<Option<DocumentRef>> {
        let node_path = resolve_code(&self.root, home)?;
        for entry in std::fs::read_dir(&node_path)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                continue;
            }
            let file_name = entry.file_name().to_string_lossy().into_owned();
            if let FileClass::Document {
                slug: found, ext, ..
            } = classify(&file_name, false, home)
                && found == slug
            {
                return Ok(Some(DocumentRef {
                    home: home.clone(),
                    slug: found,
                    ext,
                    path: entry.path(),
                }));
            }
        }
        Ok(None)
    }

    /// Documents holding `slug` at some *other* node — the soft half of §5.4's
    /// uniqueness, which `add` warns on and `pan validate` reports (§18).
    pub fn duplicate_document_slugs_elsewhere(
        &self,
        home: &Code,
        slug: &str,
    ) -> Result<Vec<DocumentRef>> {
        Ok(self
            .find_documents(None, Some(slug))?
            .into_iter()
            .filter(|d| d.home.as_str() != home.as_str())
            .collect())
    }

    /// Read one document whole — frontmatter *and* body (§6.1). This is `get`'s path;
    /// a fold must use [`Self::fold_documents`], which never reads a body.
    pub fn read_document(&self, dref: &DocumentRef) -> Result<Document> {
        let text = match std::fs::read_to_string(&dref.path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Err(Error::not_found(format!(
                    "no document {:?} at {} (§7.3)",
                    dref.slug,
                    dref.home.as_str()
                )));
            }
            Err(e) => return Err(e.into()),
        };
        crate::document::parse(&text)
            .map_err(|e| Error::validation(format!("{}: {e}", dref.path.display())))
    }

    /// A subtree walk over this core's documents (§6.3), reading **frontmatter only**
    /// — "a fold never reads bodies" (§7.1, §7.2, §8.7). The prose never enters memory.
    pub fn fold_documents(&self, at: Option<&Code>) -> Result<Vec<(DocumentRef, Frontmatter)>> {
        let mut out = Vec::new();
        for dref in self.find_documents(at, None)? {
            let frontmatter = crate::document::read_frontmatter(&dref.path)
                .map_err(|e| Error::validation(format!("{}: {e}", dref.path.display())))?;
            out.push((dref, frontmatter));
        }
        Ok(out)
    }

    /// Write a document, creating or overwriting it (§6.1, §6.4).
    ///
    /// Takes the whole [`Document`] rather than its parts because `front_raw` must
    /// ride along: that is what carries a hand's comments, key ordering, and unread
    /// keys through the rewrite (§6.6, I6, I8).
    pub fn write_document(&self, addr: &DocumentAddr, document: &Document) -> Result<DocumentRef> {
        let path = self.document_path(addr)?;
        let encoded = document.to_text()?.into_bytes();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        crate::lock::with_record_lock(&path, move |_| Ok(encoded))?;
        crate::hook::note_write(C::NAME, &addr.home);
        Ok(DocumentRef {
            home: addr.home.clone(),
            slug: addr.slug.clone(),
            ext: addr.ext,
            path,
        })
    }

    /// Delete a document (§7.2). Irreversible — §18 keeps no history.
    pub fn remove_document(&self, dref: &DocumentRef) -> Result<()> {
        match std::fs::remove_file(&dref.path) {
            Ok(()) => {
                crate::hook::note_write(C::NAME, &dref.home);
                Ok(())
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Err(Error::not_found(format!(
                "no document {:?} at {} (§7.3)",
                dref.slug,
                dref.home.as_str()
            ))),
            Err(e) => Err(e.into()),
        }
    }

    /// Move a document file to a new address — the primitive behind `move` (a new
    /// home) and `rename`'s file half (a new slug). A `mv` between **node dirs**, not
    /// meta dirs (§7.2); the text is not touched.
    ///
    /// Refuses to land on an occupied path rather than clobber it (exit `3`).
    pub fn relocate_document(&self, dref: &DocumentRef, to: &DocumentAddr) -> Result<DocumentRef> {
        let path = self.document_path(to)?;
        if path == dref.path {
            return Ok(dref.clone());
        }
        if path.exists() {
            return Err(Error::validation(format!(
                "{} already holds a document named {:?} (§7.2)",
                to.home.as_str(),
                to.slug
            )));
        }
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::rename(&dref.path, &path)?;
        // Both ends, as in the other two relocations (§9.3).
        crate::hook::note_write(C::NAME, &dref.home);
        crate::hook::note_write(C::NAME, &to.home);
        Ok(DocumentRef {
            home: to.home.clone(),
            slug: to.slug.clone(),
            ext: to.ext,
            path,
        })
    }
}
