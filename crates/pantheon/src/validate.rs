//! The tree's own lint (§5.5, §10.2): the cross-cutting checks the spine owns —
//! code collisions, malformed directories, files owned by no installed core, and
//! dangling references. Per-core kind legality is the owning core's check, not
//! `pan`'s (I5). Runs on demand — nothing watches the tree (§18).

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use serde_json::json;

use crate::Result;
use crate::classify::{FileClass, classify};
use crate::code::{Code, NodeName, parse_node_dirname};
use crate::core::CoreRegistry;
use crate::envelope::{RawEntity, RawLine, Ref};
use crate::name::is_normalized;

/// A validation finding, reported by path (§10.2).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Finding {
    pub code: FindingCode,
    pub severity: Severity,
    pub rel_path: PathBuf,
    pub msg: String,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Severity {
    Error,
    Warning,
}

impl Severity {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Severity::Error => "error",
            Severity::Warning => "warning",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FindingCode {
    /// A directory name that does not parse as a node (§5.1).
    MalformedDir,
    /// Two siblings resolving to the same code, or one prefix-shadowing another (§5.3).
    CodeCollision,
    /// A file the tools own no shape for (§5.5).
    UnclassifiableFile,
    /// A record whose kind is declared by no installed core (§5.0).
    KindOwnedByNoCore,
    /// A record whose envelope will not deserialize (§6.4).
    MalformedRecord,
    /// A `core:slug` reference that resolves to nothing (§5.4).
    DanglingRef,
    /// One core's slug held at two nodes (§5.4). Soft by design: finding it is a
    /// tree walk, which is the cost the softness exists to avoid, so `add` warns
    /// and you fix it at the source (§18).
    DuplicateSlug,
    /// A typed token not in normal form (§5.1).
    NonNormalizedName,
}

impl FindingCode {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            FindingCode::MalformedDir => "malformed_dir",
            FindingCode::CodeCollision => "code_collision",
            FindingCode::UnclassifiableFile => "unclassifiable_file",
            FindingCode::KindOwnedByNoCore => "kind_owned_by_no_core",
            FindingCode::MalformedRecord => "malformed_record",
            FindingCode::DanglingRef => "dangling_ref",
            FindingCode::DuplicateSlug => "duplicate_slug",
            FindingCode::NonNormalizedName => "non_normalized_name",
        }
    }
}

impl Finding {
    #[must_use]
    pub fn to_json(&self) -> serde_json::Value {
        json!({
            "code": self.code.as_str(),
            "severity": self.severity.as_str(),
            "path": self.rel_path.to_string_lossy(),
            "msg": self.msg,
        })
    }
}

/// The `pan validate` contract JSON (§5.5).
#[must_use]
pub fn findings_json(findings: &[Finding]) -> serde_json::Value {
    json!({ "findings": findings.iter().map(Finding::to_json).collect::<Vec<_>>() })
}

/// Validate the tree, reporting findings by path (§5.5). Clean or warnings-only is
/// success; any `Error`-severity finding is a validation failure (exit `3`, decided
/// by the caller).
pub fn validate(root: &Path, reg: &CoreRegistry) -> Result<Vec<Finding>> {
    let ids = crate::resolve::identifier_set(root, reg)?;
    let mut findings = Vec::new();

    // Spheres are the root's children, parsed with no parent (§5.1).
    let mut spheres: Vec<(NodeName, PathBuf)> = Vec::new();
    for entry in std::fs::read_dir(root)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue; // stray files at the root are bulk, not the tree's concern
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.ends_with("__") {
            continue;
        }
        match parse_node_dirname(None, &name) {
            Ok(nn) => spheres.push((nn, entry.path())),
            Err(e) => push(
                &mut findings,
                FindingCode::MalformedDir,
                Severity::Error,
                root,
                &entry.path(),
                e.to_string(),
            ),
        }
    }
    check_collisions(&spheres, root, &mut findings);
    for (nn, path) in &spheres {
        walk_node(root, &nn.code, &nn.label, path, reg, &ids, &mut findings)?;
    }

    findings.sort_by(|a, b| {
        (a.rel_path.to_string_lossy(), a.msg.as_str())
            .cmp(&(b.rel_path.to_string_lossy(), b.msg.as_str()))
    });
    Ok(findings)
}

fn walk_node(
    root: &Path,
    node_code: &Code,
    node_label: &str,
    node_path: &Path,
    reg: &CoreRegistry,
    ids: &HashSet<(String, String)>,
    findings: &mut Vec<Finding>,
) -> Result<()> {
    if !is_normalized(node_label) {
        push(
            findings,
            FindingCode::NonNormalizedName,
            Severity::Warning,
            root,
            node_path,
            format!("label {node_label:?} is not in normal form (§5.1)"),
        );
    }

    // Records and rules live in the meta dir.
    let meta = node_path.join(format!("{}__", node_code.as_str()));
    if meta.is_dir() {
        for entry in std::fs::read_dir(&meta)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().into_owned();
            let class = classify(&name, false, node_code);
            check_record(root, &class, &entry.path(), reg, ids, findings);
        }
    }

    // The open dir holds child node directories and loose documents.
    let mut children: Vec<(NodeName, PathBuf)> = Vec::new();
    for entry in std::fs::read_dir(node_path)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        let is_dir = entry.file_type()?.is_dir();
        if is_dir {
            if name.ends_with("__") {
                continue; // the meta dir, handled above
            }
            match classify(&name, true, node_code) {
                FileClass::NodeDir { name: nn } => children.push((nn, entry.path())),
                FileClass::Unclassifiable { reason } => push(
                    findings,
                    FindingCode::MalformedDir,
                    Severity::Error,
                    root,
                    &entry.path(),
                    reason,
                ),
                _ => {}
            }
        } else if let FileClass::Unclassifiable { reason } = classify(&name, false, node_code) {
            push(
                findings,
                FindingCode::UnclassifiableFile,
                Severity::Warning,
                root,
                &entry.path(),
                reason,
            );
        }
    }

    check_collisions(&children, root, findings);
    for (nn, path) in &children {
        walk_node(root, &nn.code, &nn.label, path, reg, ids, findings)?;
    }
    Ok(())
}

fn check_record(
    root: &Path,
    class: &FileClass,
    path: &Path,
    reg: &CoreRegistry,
    ids: &HashSet<(String, String)>,
    findings: &mut Vec<Finding>,
) {
    let (kind, is_series) = match class {
        FileClass::Partitioned { kind, .. } | FileClass::EntityNode { kind, .. } => (kind, false),
        FileClass::NamedSeries { kind, .. } | FileClass::DeterminedSeries { kind, .. } => {
            (kind, true)
        }
        FileClass::Unclassifiable { reason } => {
            push(
                findings,
                FindingCode::UnclassifiableFile,
                Severity::Warning,
                root,
                path,
                reason.clone(),
            );
            return;
        }
        // Not a record the spine reads here: annotations and rules are structural,
        // documents/dirs are handled in the open-dir walk.
        FileClass::Annotation { .. }
        | FileClass::Rule { .. }
        | FileClass::Bulk
        | FileClass::Document { .. }
        | FileClass::MetaDir { .. }
        | FileClass::NodeDir { .. } => return,
    };

    if reg.core_of_kind(kind).is_none() {
        push(
            findings,
            FindingCode::KindOwnedByNoCore,
            Severity::Warning,
            root,
            path,
            format!("kind {kind:?} is declared by no installed core"),
        );
        return;
    }

    match record_refs(path, is_series) {
        Err(e) => push(
            findings,
            FindingCode::MalformedRecord,
            Severity::Error,
            root,
            path,
            e,
        ),
        Ok(refs) => {
            for r in refs {
                if !ids.contains(&(r.core.clone(), r.slug.clone())) {
                    push(
                        findings,
                        FindingCode::DanglingRef,
                        Severity::Error,
                        root,
                        path,
                        format!("reference {r} resolves to nothing (§5.4)"),
                    );
                }
            }
        }
    }
}

/// Read a record's envelope refs. An entity is one object; a series is many lines.
/// The `data` half is never parsed past `RawValue` — the spine carries a core's
/// record opaquely (I5).
pub(crate) fn record_refs(path: &Path, is_series: bool) -> std::result::Result<Vec<Ref>, String> {
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    if is_series {
        let mut out = Vec::new();
        for line in bytes.split(|b| *b == b'\n') {
            if line.iter().all(u8::is_ascii_whitespace) {
                continue;
            }
            let l: RawLine = serde_json::from_slice(line).map_err(|e| e.to_string())?;
            out.extend(l.refs);
        }
        Ok(out)
    } else {
        let e: RawEntity = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
        Ok(e.refs)
    }
}

/// Flag two siblings that resolve to the same code or where one prefix-shadows
/// another's walk (§5.3).
fn check_collisions(siblings: &[(NodeName, PathBuf)], root: &Path, findings: &mut Vec<Finding>) {
    let mut by_code: HashMap<&str, usize> = HashMap::new();
    for (nn, path) in siblings {
        let code = nn.code.as_str();
        if by_code.insert(code, 0).is_some() {
            push(
                findings,
                FindingCode::CodeCollision,
                Severity::Error,
                root,
                path,
                format!("code {code:?} is claimed by two sibling directories (§5.3)"),
            );
        }
    }
    for (a_nn, _) in siblings {
        for (b_nn, b_path) in siblings {
            let (a, b) = (a_nn.code.as_str(), b_nn.code.as_str());
            if a != b && b.starts_with(a) && b.as_bytes().get(a.len()) == Some(&b'_') {
                push(
                    findings,
                    FindingCode::CodeCollision,
                    Severity::Error,
                    root,
                    b_path,
                    format!("code {b:?} is prefix-shadowed by sibling {a:?} (§5.3)"),
                );
            }
        }
    }
}

fn push(
    findings: &mut Vec<Finding>,
    code: FindingCode,
    severity: Severity,
    root: &Path,
    path: &Path,
    msg: impl Into<String>,
) {
    let rel_path = path.strip_prefix(root).unwrap_or(path).to_path_buf();
    findings.push(Finding {
        code,
        severity,
        rel_path,
        msg: msg.into(),
    });
}
