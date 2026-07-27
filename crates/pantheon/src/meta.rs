//! Node annotations (§5.2, §6.6): the optional `[code]__.toml` in a node's meta dir
//! (symbol, keywords, deity, explanation, and a node's open `[fields]`). Read or written
//! in place via `toml_edit` so hand comments and ordering survive (§6.6). Annotation
//! touches the node, never `data` and never the tree shape.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde_json::json;
use toml_edit::{Array, DocumentMut, Item};

use crate::code::Code;
use crate::lock::with_record_lock;
use crate::tree::resolve_code;
use crate::{Error, Result};

/// The annotation keys with a shape of their own (§5.2). Anything else a hand sets is a
/// field, and lands in `[fields]`.
const TYPED_KEYS: &[&str] = &["symbol", "keywords", "deity", "explanation"];

/// The table an open field lives in (§5.2). Namespaced so the four typed keys keep their
/// meaning and a field can never shadow one.
const FIELDS: &str = "fields";

/// A node's annotations (§5.2). Every field is optional — annotations are optional.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Annotations {
    pub symbol: Option<String>,
    pub keywords: Vec<String>,
    pub deity: Option<String>,
    pub explanation: Option<String>,
    /// A node's open fields — the `[fields]` table, any key a hand cares to write
    /// (§5.2, placement rule 4).
    ///
    /// Placement rule 4 is "fields, not nodes": closeness, role, motive, obligation,
    /// origin and format *colour* a record and are never branches. Before this there was
    /// nowhere to put one — the key set was closed at four — so the only home for a
    /// warrant or a role was `keywords`, which is documented as search hints for an LLM
    /// and would have made the field indistinguishable from one.
    ///
    /// **A field is annotation, never behaviour** (§18, §6.6). Every value is a plain
    /// string, unvalidated and untyped, and **no tool may branch on one** — a field that
    /// tuned a tool would be the config file §18 forbids, whatever it were called.
    pub fields: BTreeMap<String, String>,
}

impl Annotations {
    #[must_use]
    pub fn to_json(&self) -> serde_json::Value {
        json!({
            "symbol": self.symbol,
            "keywords": self.keywords,
            "deity": self.deity,
            "explanation": self.explanation,
            "fields": self.fields,
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
        fields: get_fields(&doc),
    })
}

/// The `[fields]` table, read as strings (§5.2). `as_table_like` so a hand may write it
/// either way — `[fields]` on its own line, or `fields = { … }` inline. A value that is
/// not a string is passed over rather than coerced: what a hand meant by it is not the
/// spine's to guess.
fn get_fields(doc: &DocumentMut) -> BTreeMap<String, String> {
    doc.get(FIELDS)
        .and_then(Item::as_table_like)
        .map(|table| {
            table
                .iter()
                .filter_map(|(k, v)| v.as_str().map(|s| (k.to_owned(), s.to_owned())))
                .collect()
        })
        .unwrap_or_default()
}

/// Set annotation keys in place (§6.6). `keywords` takes a comma-separated value and
/// becomes a TOML array; the other three typed keys are strings; **any other key is a
/// field** and lands in `[fields]` (§5.2). Comments and key order in an existing file
/// survive.
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
            } else if TYPED_KEYS.contains(&key.as_str()) {
                doc[key.as_str()] = toml_edit::value(value.as_str());
            } else {
                // An open field. The table is minted on first use and left implicit, so
                // a file that has never carried one grows a `[fields]` header and no
                // more; `toml_edit` keeps everything already written around it (§6.6).
                if doc.get(FIELDS).is_none() {
                    doc[FIELDS] = Item::Table(toml_edit::Table::new());
                }
                doc[FIELDS][key.as_str()] = toml_edit::value(value.as_str());
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
