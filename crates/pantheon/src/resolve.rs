//! Reference resolution (§5.0, §5.4): `core:slug` → the record's home and path. A
//! command resolving many refs walks the tree once into a transient map, then does
//! in-memory lookups (§5.0). Resolution rests on filenames — the owning core of a
//! kind names the file, so a slug is matched without opening the record (I5).

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use serde_json::json;

use crate::Result;
use crate::classify::{FileClass, classify};
use crate::code::Code;
use crate::core::CoreRegistry;
use crate::envelope::Ref;
use crate::shape::Shape;
use crate::tree::{Node, TreeRoot, build_tree};

/// A resolved reference: what it points at, and where that record lives now.
#[derive(Clone, Debug)]
pub struct Resolution {
    pub reference: Ref,
    pub kind: String,
    pub shape: Shape,
    pub home: Code,
    /// The record's path, relative to the tree root (stable across machines).
    pub rel_path: PathBuf,
}

impl Resolution {
    #[must_use]
    pub fn to_json(&self) -> serde_json::Value {
        json!({
            "ref": self.reference.to_token(),
            "core": self.reference.core,
            "slug": self.reference.slug,
            "kind": self.kind,
            "shape": self.shape,
            "home": self.home.as_str(),
            "path": self.rel_path.to_string_lossy(),
        })
    }
}

/// The outcome of resolving one reference (§5.4). Ambiguous lists the candidates
/// rather than guessing.
#[derive(Clone, Debug)]
pub enum RefOutcome {
    Resolved(Resolution),
    Ambiguous(Vec<Resolution>),
    Unresolved(Ref),
}

/// Resolve every reference against one tree walk (§5.0). Order of the result matches
/// the input.
pub fn resolve_all(root: &Path, reg: &CoreRegistry, refs: &[Ref]) -> Result<Vec<RefOutcome>> {
    let index = build_index(root, reg)?;
    Ok(refs
        .iter()
        .map(|r| {
            let key = (r.core.clone(), r.slug.clone());
            match index.get(&key) {
                None => RefOutcome::Unresolved(r.clone()),
                Some(v) if v.len() == 1 => RefOutcome::Resolved(v[0].clone()),
                Some(v) => RefOutcome::Ambiguous(v.clone()),
            }
        })
        .collect())
}

/// One identifier that more than one of a core's records answers to (§5.4).
#[derive(Clone, Debug)]
pub struct DuplicateIdentifier {
    pub reference: Ref,
    /// Every record holding the name, by path — at least two.
    pub at: Vec<Resolution>,
}

/// The tree's resolvable identifiers, and those more than one record answers to.
///
/// One walk answers both questions, because they are the same index read two ways: a
/// ref resolving to *nothing* dangles, and a ref resolving to *two things* is a
/// cross-node duplicate (§5.4). Building this twice would pay the walk twice, which
/// is the cost the softness of the duplicate rule exists to avoid (§18).
pub struct Identifiers {
    /// What a dangling-ref check tests against.
    pub known: HashSet<(String, String)>,
    /// What a duplicate-slug check reports — soft, always (§5.4, §18).
    pub duplicates: Vec<DuplicateIdentifier>,
}

/// Walk the tree's records once and index them by identifier (§5.0, §5.4).
pub fn identifiers(root: &Path, reg: &CoreRegistry) -> Result<Identifiers> {
    let mut known = HashSet::new();
    let mut duplicates = Vec::new();
    for (key, mut at) in build_index(root, reg)? {
        known.insert(key);
        if at.len() > 1 {
            // Sorted so a finding list is stable across runs whatever order the
            // filesystem handed the entries back in.
            at.sort_by(|a, b| a.rel_path.cmp(&b.rel_path));
            duplicates.push(DuplicateIdentifier {
                reference: at[0].reference.clone(),
                at,
            });
        }
    }
    duplicates.sort_by(|a, b| a.reference.to_token().cmp(&b.reference.to_token()));
    Ok(Identifiers { known, duplicates })
}

/// Assemble the `pan resolve` contract JSON (§5.5): three arrays, always present, so
/// a machine reads the whole story from the JSON rather than the exit code alone.
#[must_use]
pub fn outcomes_json(outcomes: &[RefOutcome]) -> serde_json::Value {
    let mut resolved = Vec::new();
    let mut ambiguous = Vec::new();
    let mut unresolved = Vec::new();
    for o in outcomes {
        match o {
            RefOutcome::Resolved(r) => resolved.push(r.to_json()),
            RefOutcome::Ambiguous(v) => {
                ambiguous.push(json!({
                    "ref": v.first().map(|r| r.reference.to_token()),
                    "candidates": v.iter().map(Resolution::to_json).collect::<Vec<_>>(),
                }));
            }
            RefOutcome::Unresolved(r) => unresolved.push(serde_json::Value::String(r.to_token())),
        }
    }
    json!({ "resolved": resolved, "ambiguous": ambiguous, "unresolved": unresolved })
}

type Index = HashMap<(String, String), Vec<Resolution>>;

fn build_index(root: &Path, reg: &CoreRegistry) -> Result<Index> {
    let nodes = match build_tree(root, None)? {
        TreeRoot::Forest(nodes) => nodes,
        TreeRoot::Subtree(_) => unreachable!("build_tree(None) is always a forest"),
    };
    // A Document core declares no kinds (§7.1); documents resolve to it.
    let doc_core = reg
        .cores()
        .iter()
        .find(|c| c.kinds.is_empty())
        .map(|c| c.name.clone());
    let mut index = Index::new();
    for node in &nodes {
        index_node(root, node, reg, doc_core.as_deref(), &mut index)?;
    }
    Ok(index)
}

fn index_node(
    root: &Path,
    node: &Node,
    reg: &CoreRegistry,
    doc_core: Option<&str>,
    index: &mut Index,
) -> Result<()> {
    // Records live in the node's meta dir.
    let meta = node.path.join(format!("{}__", node.code.as_str()));
    if meta.is_dir() {
        for entry in std::fs::read_dir(&meta)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().into_owned();
            register_record(
                root,
                node,
                reg,
                &classify(&name, false, &node.code),
                &entry.path(),
                index,
            );
        }
    }
    // Documents are loose in the open node dir.
    for entry in std::fs::read_dir(&node.path)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        if let FileClass::Document { slug, .. } = classify(&name, false, &node.code) {
            if let Some(core) = doc_core {
                push(
                    index,
                    root,
                    node,
                    core.to_string(),
                    String::new(),
                    Shape::Document,
                    slug,
                    &entry.path(),
                );
            }
        }
    }
    for child in &node.children {
        index_node(root, child, reg, doc_core, index)?;
    }
    Ok(())
}

fn register_record(
    root: &Path,
    node: &Node,
    reg: &CoreRegistry,
    class: &FileClass,
    path: &Path,
    index: &mut Index,
) {
    // (kind, identifier) for the shapes that are ref targets. An entity-as-node's
    // slug is the node's definition (its label); a determined-name series is never a
    // ref target on its own (§5.4), so it registers nothing.
    let (kind, ident) = match class {
        FileClass::Partitioned { kind, slug, .. } => (kind.clone(), slug.clone()),
        FileClass::EntityNode { kind, .. } => (kind.clone(), node.label.clone()),
        FileClass::NamedSeries { kind, name, .. } => (kind.clone(), name.clone()),
        _ => return,
    };
    let Some(core) = reg.core_of_kind(&kind) else {
        return; // a kind owned by no installed core — a `pan validate` finding, not a ref
    };
    let shape = reg.shape_of_kind(&kind).unwrap_or(Shape::Partitioned);
    push(
        index,
        root,
        node,
        core.name.clone(),
        kind,
        shape,
        ident,
        path,
    );
}

#[allow(clippy::too_many_arguments)]
fn push(
    index: &mut Index,
    root: &Path,
    node: &Node,
    core: String,
    kind: String,
    shape: Shape,
    ident: String,
    path: &Path,
) {
    let rel_path = path.strip_prefix(root).unwrap_or(path).to_path_buf();
    let resolution = Resolution {
        reference: Ref {
            core: core.clone(),
            slug: ident.clone(),
        },
        kind,
        shape,
        home: node.code.clone(),
        rel_path,
    };
    index.entry((core, ident)).or_default().push(resolution);
}
