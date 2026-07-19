//! Node annotations (§5.2, §6.6): the optional `[code]__.toml` in a node's meta dir
//! (symbol, keywords, deity, explanation). Read or written in place via `toml_edit`
//! so hand comments and ordering survive (§6.6). Annotation touches the node, never
//! `data` and never the tree shape.

use std::path::{Path, PathBuf};

use serde_json::json;
use toml_edit::{Array, DocumentMut, Item};

use crate::code::Code;
use crate::lock::with_record_lock;
use crate::tree::resolve_code;
use crate::{Error, Result};

/// A node's annotations (§5.2). Every field is optional — annotations are optional.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Annotations {
    pub symbol: Option<String>,
    pub keywords: Vec<String>,
    pub deity: Option<String>,
    pub explanation: Option<String>,
}

impl Annotations {
    #[must_use]
    pub fn to_json(&self) -> serde_json::Value {
        json!({
            "symbol": self.symbol,
            "keywords": self.keywords,
            "deity": self.deity,
            "explanation": self.explanation,
        })
    }
}

/// The annotation file path: `<node>/<code>__/<code>__.toml`.
fn annotation_path(root: &Path, code: &Code) -> Result<PathBuf> {
    let node = resolve_code(root, code)?;
    let meta = node.join(format!("{}__", code.as_str()));
    Ok(meta.join(format!("{}__.toml", code.as_str())))
}

/// Read a node's annotations, or the empty set if the file is absent (§5.2).
pub fn read_annotations(root: &Path, code: &Code) -> Result<Annotations> {
    let path = annotation_path(root, code)?;
    if !path.exists() {
        return Ok(Annotations::default());
    }
    let text = std::fs::read_to_string(&path)?;
    let doc: DocumentMut = text
        .parse()
        .map_err(|e| Error::validation(format!("annotations {}: {e}", path.display())))?;
    Ok(Annotations {
        symbol: get_str(&doc, "symbol"),
        keywords: get_str_array(&doc, "keywords"),
        deity: get_str(&doc, "deity"),
        explanation: get_str(&doc, "explanation"),
    })
}

/// Set annotation keys in place (§6.6). `keywords` takes a comma-separated value and
/// becomes a TOML array; every other key is a string. Comments and key order in an
/// existing file survive.
pub fn set_annotations(root: &Path, code: &Code, sets: &[(String, String)]) -> Result<()> {
    let path = annotation_path(root, code)?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    with_record_lock(&path, |prev| {
        let mut doc: DocumentMut = match prev {
            Some(bytes) => std::str::from_utf8(bytes)
                .map_err(|e| Error::runtime(format!("annotations: {e}")))?
                .parse()
                .map_err(|e| Error::validation(format!("annotations: {e}")))?,
            None => DocumentMut::new(),
        };
        for (key, value) in sets {
            if key == "keywords" {
                let mut arr = Array::new();
                for kw in value.split(',').map(str::trim).filter(|s| !s.is_empty()) {
                    arr.push(kw);
                }
                doc[key.as_str()] = toml_edit::value(arr);
            } else {
                doc[key.as_str()] = toml_edit::value(value.as_str());
            }
        }
        Ok(doc.to_string().into_bytes())
    })
}

/// Shared with [`crate::document`]: annotations and document frontmatter are the two
/// hand-written TOML surfaces (§6.6), and both read their scalars the same way.
pub(crate) fn get_str(doc: &DocumentMut, key: &str) -> Option<String> {
    doc.get(key).and_then(Item::as_str).map(ToOwned::to_owned)
}

pub(crate) fn get_str_array(doc: &DocumentMut, key: &str) -> Vec<String> {
    doc.get(key)
        .and_then(Item::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(ToOwned::to_owned))
                .collect()
        })
        .unwrap_or_default()
}
