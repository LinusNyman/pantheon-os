//! The tree walk (§5.0, §6.3): build the ontology (sub)tree from directories, and
//! resolve a code to its path by descending one directory per level. Resolution is
//! a walk, not a cache — there is no index and no `pan reindex` (§5.0, §18).

use std::path::{Path, PathBuf};

use serde_json::json;

use crate::code::{CharToken, Code, CodeForm, NodeName, parse_node_dirname};
use crate::{Error, Result};

/// A node in the ontology tree, its identity read off its directory (§5.1).
#[derive(Clone, Debug)]
pub struct Node {
    pub code: Code,
    pub form: CodeForm,
    /// The defining char — `None` for a definition-prefix node.
    pub ch: Option<CharToken>,
    pub label: String,
    pub path: PathBuf,
    pub children: Vec<Node>,
}

impl Node {
    /// The contract JSON for a node (§5.5). Paths are omitted — codes and labels are
    /// the contract; a def-prefix node's `char` is `null`.
    #[must_use]
    pub fn to_json(&self) -> serde_json::Value {
        json!({
            "code": self.code.as_str(),
            "char": self.ch.as_ref().map(CharToken::as_code_str),
            "label": self.label,
            "form": self.form.as_str(),
            "children": self.children.iter().map(Node::to_json).collect::<Vec<_>>(),
        })
    }
}

/// The result of a walk: the whole forest (spheres and below) or one subtree.
#[derive(Clone, Debug)]
pub enum TreeRoot {
    Forest(Vec<Node>),
    Subtree(Node),
}

impl TreeRoot {
    #[must_use]
    pub fn to_json(&self) -> serde_json::Value {
        match self {
            TreeRoot::Forest(nodes) => {
                json!({ "nodes": nodes.iter().map(Node::to_json).collect::<Vec<_>>() })
            }
            TreeRoot::Subtree(node) => node.to_json(),
        }
    }
}

/// Build the ontology (sub)tree by walking node directories only — meta dirs, loose
/// documents, and bulk are passed over (§6.3). `at = None` walks the whole forest
/// from the root; `at = Some(code)` walks that node's subtree.
pub fn build_tree(root: &Path, at: Option<&Code>) -> Result<TreeRoot> {
    match at {
        None => {
            let mut nodes = Vec::new();
            for (nn, path) in read_child_nodes(root, None)? {
                nodes.push(build_node(nn, path)?);
            }
            nodes.sort_by(|a, b| a.code.as_str().cmp(b.code.as_str()));
            Ok(TreeRoot::Forest(nodes))
        }
        Some(code) => {
            let (nn, path) = descend(root, code)?;
            Ok(TreeRoot::Subtree(build_node(nn, path)?))
        }
    }
}

/// Resolve a code to its directory path by descending one level per step (§5.1),
/// globbing a single directory at each level. A compact code pre-tokenizes; a
/// definition-prefix code is matched as a prefix of the remaining string per level.
pub fn resolve_code(root: &Path, code: &Code) -> Result<PathBuf> {
    descend(root, code).map(|(_, path)| path)
}

fn build_node(nn: NodeName, path: PathBuf) -> Result<Node> {
    let mut children = Vec::new();
    for (child_nn, child_path) in read_child_nodes(&path, Some(&nn.code))? {
        children.push(build_node(child_nn, child_path)?);
    }
    children.sort_by(|a, b| a.code.as_str().cmp(b.code.as_str()));
    Ok(Node {
        code: nn.code,
        form: nn.form,
        ch: nn.ch,
        label: nn.label,
        path,
        children,
    })
}

/// The child node names directly under `dir`, parsed against `parent` (skipping the
/// meta dir and non-node entries). The collision check `mint` runs before a mint.
pub fn child_node_names(dir: &Path, parent: Option<&Code>) -> Result<Vec<NodeName>> {
    Ok(read_child_nodes(dir, parent)?
        .into_iter()
        .map(|(nn, _)| nn)
        .collect())
}

/// The child node directories of `dir` (each parsed against `parent`), skipping the
/// meta dir, loose files, and any dir that does not parse as a child node.
fn read_child_nodes(dir: &Path, parent: Option<&Code>) -> Result<Vec<(NodeName, PathBuf)>> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.ends_with("__") {
            continue; // a meta dir, not a child node
        }
        if let Ok(nn) = parse_node_dirname(parent, &name) {
            out.push((nn, entry.path()));
        }
    }
    Ok(out)
}

/// Descend from the root to the node named by `target`, returning its identity and
/// path. At each level exactly one child's code is a prefix of the remaining string
/// (§5.0); an exact match ends the walk. A missing node is `NotFound` (exit `4`); two
/// siblings both prefixing the target is a code collision (§5.3).
fn descend(root: &Path, target: &Code) -> Result<(NodeName, PathBuf)> {
    let tgt = target.as_str();
    let mut dir = root.to_path_buf();
    let mut parent: Option<Code> = None;
    loop {
        let children = read_child_nodes(&dir, parent.as_ref())?;
        let mut candidates: Vec<(NodeName, PathBuf)> = Vec::new();
        let mut exact: Option<(NodeName, PathBuf)> = None;
        for (nn, path) in children {
            let ccode = nn.code.as_str();
            if ccode == tgt {
                exact = Some((nn, path));
                break;
            }
            if tgt.starts_with(ccode) {
                candidates.push((nn, path));
            }
        }
        if let Some(found) = exact {
            return Ok(found);
        }
        match candidates.len() {
            0 => return Err(Error::not_found(format!("no node with code {tgt:?}"))),
            1 => {
                let (nn, path) = candidates.pop().expect("one candidate");
                parent = Some(nn.code);
                dir = path;
            }
            _ => {
                return Err(Error::validation(format!(
                    "code {tgt:?} is ambiguous: a prefix collision among siblings (§5.3)"
                )));
            }
        }
    }
}
