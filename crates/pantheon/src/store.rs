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

use crate::classify::{FileClass, classify};
use crate::code::{Code, CodeForm, NodeName};
use crate::core::Core;
use crate::envelope::{Entity, Key, Line, RawLine, Ref};
use crate::shape::Shape;
use crate::tree::{Node, TreeRoot, build_tree, resolve_code, resolve_node};
use crate::{Error, Result};

/// One of this core's series files, located in the tree (§5.2). The home, kind, and
/// name are the file's location and name, never read from inside it (I3).
#[derive(Clone, Debug)]
pub struct SeriesRef {
    pub home: Code,
    pub kind: String,
    pub name: String,
    pub path: PathBuf,
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

/// A series folded to its present (I1): the line at the latest key, carrying the
/// series it came from — a fold across a subtree loses that otherwise.
#[derive(Clone, Debug)]
pub struct PresentLine<T> {
    pub home: Code,
    pub kind: String,
    pub name: String,
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

    fn series_file_name(home: &Code, kind: &str, name: &str) -> String {
        format!("{}__{kind}__{name}.jsonl", home.as_str())
    }

    /// Where a series lives, whether or not it exists yet (§5.2).
    pub fn series_path(&self, home: &Code, kind: &str, name: &str) -> Result<PathBuf> {
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
                let FileClass::NamedSeries {
                    kind: found_kind,
                    name: found_name,
                    ..
                } = classify(&file_name, false, &node.code)
                else {
                    continue;
                };
                // Another core's series at this node is not ours to count (§5.0).
                if !Self::owns_series_kind(&found_kind)
                    || kind.is_some_and(|want| want != found_kind)
                    || name.is_some_and(|want| want != found_name)
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

    /// The present of a series (I1): the line at the latest key. Keys sort
    /// lexicographically, which for `YYMMDD`(`Thhmm`) is chronological (§5.4).
    fn present(lines: Vec<Line<C::Record>>) -> Option<Line<C::Record>> {
        lines
            .into_iter()
            .max_by(|a, b| a.key.as_str().cmp(b.key.as_str()))
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
        let line = Self::present(lines).ok_or_else(|| {
            Error::not_found(format!(
                "series {:?} at {} holds no readings yet (§7.3)",
                sref.name,
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
    /// under `at`, each at its latest key.
    pub fn fold(
        &self,
        at: Option<&Code>,
        kind: Option<&str>,
    ) -> Result<Vec<PresentLine<C::Record>>> {
        let mut out = Vec::new();
        for sref in self.find_series(at, kind, None)? {
            let lines = self.read_series(&sref)?;
            if let Some(line) = Self::present(lines) {
                out.push(PresentLine {
                    home: sref.home,
                    kind: sref.kind,
                    name: sref.name,
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
        let path = self.series_path(home, kind, name)?;
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
        Ok(SeriesRef {
            home: home.clone(),
            kind: kind.to_string(),
            name: name.to_string(),
            path,
        })
    }

    /// Add or overwrite one keyed line (§6.1). A correction rewrites the keyed line
    /// in place; it never stacks a second (I1). Lines the write does not touch are
    /// carried through verbatim.
    pub fn write_line(&self, sref: &SeriesRef, line: &Line<C::Record>) -> Result<()> {
        if !sref.path.exists() {
            return Err(Error::not_found(format!(
                "no series {:?} at {} — mint it with -c (§7.3)",
                sref.name,
                sref.home.as_str()
            )));
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
        })
    }

    /// Drop one keyed line (§7.2). Irreversible — §18 keeps no history.
    pub fn remove_line(&self, sref: &SeriesRef, key: &Key) -> Result<()> {
        if !sref.path.exists() {
            return Err(Error::not_found(format!(
                "no series {:?} at {} (§7.3)",
                sref.name,
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
            Ok(())
        } else {
            Err(Error::not_found(format!(
                "no line keyed {key} in series {:?} at {} (§7.3)",
                sref.name,
                sref.home.as_str()
            )))
        }
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
            Ok(()) => Ok(()),
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
        Ok(EntityRef {
            home: to.home.clone(),
            kind: to.kind.clone(),
            slug: to.slug.clone(),
            path,
            form,
        })
    }
}
