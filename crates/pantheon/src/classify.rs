//! The file→core map (§5.2): classify a directory entry using only its extension
//! and a split of its stem on `__` — never by opening it or asking a core (I5).
//! This structural parse is what the whole tree walk and resolution rest on.

use crate::code::{Code, NodeName, parse_node_dirname};

/// The reserved kind token naming an Auspex rule (§9.1). Checked before extension:
/// a rule's language extension may be absent (a shebang names it), and a rule
/// wearing a document's extension is still a rule.
pub const RESERVED_KIND_FUNCTION: &str = "function";

/// A document's text extension — the small fixed set classified by extension alone.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DocExt {
    Md,
    Txt,
    Mdx,
}

impl DocExt {
    /// The whole set, in the order an error message should list them.
    pub const ALL: &'static [DocExt] = &[DocExt::Md, DocExt::Txt, DocExt::Mdx];

    /// The payload is prose, not a machine format, so the set is open across these
    /// three — but classification still rests on extension alone (§6.1).
    #[must_use]
    pub fn from_ext(ext: &str) -> Option<DocExt> {
        match ext {
            "md" => Some(DocExt::Md),
            "txt" => Some(DocExt::Txt),
            "mdx" => Some(DocExt::Mdx),
            _ => None,
        }
    }

    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            DocExt::Md => "md",
            DocExt::Txt => "txt",
            DocExt::Mdx => "mdx",
        }
    }
}

impl std::fmt::Display for DocExt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// What a directory entry is, structurally (§5.2). The `kind` segment always names
/// the owning core; the spine never needs a core's meaning to get this far.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum FileClass {
    /// `[code]__.toml` — node annotations.
    Annotation { code: Code },
    /// `[code]__[kind]__[slug].json` — a partitioned entity.
    Partitioned {
        code: Code,
        kind: String,
        slug: String,
    },
    /// `[code]__[kind].json` — an entity promoted to its own node (slug = node definition).
    EntityNode { code: Code, kind: String },
    /// `[code]__[kind]__[name].jsonl` — a hand-named series.
    NamedSeries {
        code: Code,
        kind: String,
        name: String,
    },
    /// `[code]__[kind].jsonl` — a determined-name (nameless) series.
    DeterminedSeries { code: Code, kind: String },
    /// `[code]_[slug].{md,txt,mdx}` — a loose document in the open node dir.
    Document {
        code: Code,
        slug: String,
        ext: DocExt,
    },
    /// `[code]__function__[name].*` — an Auspex rule (extension may be absent).
    Rule { code: Code, name: String },
    /// `[code]__` — a node's meta directory.
    MetaDir { code: Code },
    /// A child node directory.
    NodeDir { name: NodeName },
    /// Homed media / PDF / binary — not a record (§6.5).
    Bulk,
    /// A file the tools own no shape for — a `pan validate` finding (§5.5).
    Unclassifiable { reason: String },
}

/// Classify one directory entry, given the code of the node whose directory is being
/// walked (used to strip a document's single-`_` code prefix and to parent a child
/// node dir). Pure name analysis — no disk access, no core knowledge.
#[must_use]
pub fn classify(file_name: &str, is_dir: bool, node: &Code) -> FileClass {
    if is_dir {
        classify_dir(file_name, node)
    } else {
        classify_file(file_name, node)
    }
}

fn classify_dir(name: &str, node: &Code) -> FileClass {
    if let Some(code_str) = name.strip_suffix("__") {
        return match Code::parse(code_str) {
            Ok(code) => FileClass::MetaDir { code },
            Err(e) => FileClass::Unclassifiable {
                reason: e.to_string(),
            },
        };
    }
    match parse_node_dirname(Some(node), name) {
        Ok(nn) => FileClass::NodeDir { name: nn },
        Err(e) => FileClass::Unclassifiable {
            reason: e.to_string(),
        },
    }
}

fn classify_file(name: &str, node: &Code) -> FileClass {
    let (stem, ext) = split_ext(name);
    let segments: Vec<&str> = stem.split("__").collect();

    // Reserved `function` in the kind slot, checked first — before extension (§5.2).
    if segments.len() >= 2 && segments[1] == RESERVED_KIND_FUNCTION {
        let Ok(code) = Code::parse(segments[0]) else {
            return unclassifiable(name);
        };
        let rule_name = segments[2..].join("__");
        if rule_name.is_empty() {
            return unclassifiable(name);
        }
        return FileClass::Rule {
            code,
            name: rule_name,
        };
    }

    match ext {
        Some("toml") => classify_toml(name, &segments),
        Some("json") => classify_json(name, &segments),
        Some("jsonl") => classify_jsonl(name, &segments),
        Some(e) => match DocExt::from_ext(e) {
            Some(de) => classify_document(name, stem, de, node),
            None => FileClass::Bulk,
        },
        None => FileClass::Bulk,
    }
}

fn classify_toml(name: &str, segments: &[&str]) -> FileClass {
    // Annotation is `[code]__.toml`: the stem `[code]__` splits to `[code, ""]`.
    if segments.len() == 2 && segments[1].is_empty() {
        return match Code::parse(segments[0]) {
            Ok(code) => FileClass::Annotation { code },
            Err(_) => unclassifiable(name),
        };
    }
    unclassifiable(name)
}

fn classify_json(name: &str, segments: &[&str]) -> FileClass {
    match segments {
        [code, kind, slug] if !kind.is_empty() && !slug.is_empty() => match Code::parse(code) {
            Ok(code) => FileClass::Partitioned {
                code,
                kind: (*kind).to_string(),
                slug: (*slug).to_string(),
            },
            Err(_) => unclassifiable(name),
        },
        [code, kind] if !kind.is_empty() => match Code::parse(code) {
            Ok(code) => FileClass::EntityNode {
                code,
                kind: (*kind).to_string(),
            },
            Err(_) => unclassifiable(name),
        },
        _ => unclassifiable(name),
    }
}

fn classify_jsonl(name: &str, segments: &[&str]) -> FileClass {
    match segments {
        [code, kind, series] if !kind.is_empty() && !series.is_empty() => match Code::parse(code) {
            Ok(code) => FileClass::NamedSeries {
                code,
                kind: (*kind).to_string(),
                name: (*series).to_string(),
            },
            Err(_) => unclassifiable(name),
        },
        [code, kind] if !kind.is_empty() => match Code::parse(code) {
            Ok(code) => FileClass::DeterminedSeries {
                code,
                kind: (*kind).to_string(),
            },
            Err(_) => unclassifiable(name),
        },
        _ => unclassifiable(name),
    }
}

fn classify_document(name: &str, stem: &str, ext: DocExt, node: &Code) -> FileClass {
    let prefix = format!("{}_", node.as_str());
    match stem.strip_prefix(&prefix) {
        Some(slug) if !slug.is_empty() => FileClass::Document {
            code: node.clone(),
            slug: slug.to_string(),
            ext,
        },
        _ => FileClass::Unclassifiable {
            reason: format!("document {name:?} does not carry its node's `{prefix}` prefix"),
        },
    }
}

/// Split a filename into `(stem, extension)` at the last `.`; a dotfile or a
/// no-`.` name has no extension.
fn split_ext(name: &str) -> (&str, Option<&str>) {
    match name.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() => (stem, Some(ext)),
        _ => (name, None),
    }
}

fn unclassifiable(name: &str) -> FileClass {
    FileClass::Unclassifiable {
        reason: format!("no record shape matches {name:?}"),
    }
}
