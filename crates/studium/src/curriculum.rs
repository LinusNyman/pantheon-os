//! The curriculum file (§19.3) — the one reference datum a lens reads.
//!
//! A grade is a symbol; the GPA needs a number, and `A = 5.0` is a fact about the
//! *school*, not a reading of a life — so it is neither a core's record nor a knob on
//! Studium's behaviour (§18's carve-out). It is declared in a per-programme
//! `[code]_curriculum.toml` that **governs its node and everything under it** (§6.3),
//! exactly as a rule's scope is where its file sits (§9.1).
//!
//! Studium only ever **reads** it (I1): the file is hand-edited like an annotation
//! (§6.6), parsed with the one TOML parser the workspace allows (`toml_edit`, §18), and
//! never rewritten. A malformed or absent file yields no scale, so a grade governed by
//! it simply cannot be valued and falls out of the fold — the honest absence, never a
//! guessed number.

use std::collections::HashMap;
use std::path::Path;

use pantheon::{Code, Node, TreeRoot, build_tree};

/// One grading scale — `af`, `pf` — as the GPA needs it (§19.3).
#[derive(Clone, Debug, Default)]
pub struct Scale {
    /// Each grade symbol's GPA value. Which scale a grade belongs to is decided by
    /// *which one holds the symbol* (§19.4), so this map is also the membership test.
    grades: HashMap<String, f64>,
    /// The passing symbols — a failing grade earns no credits and enters no sum (§19.4).
    passing: Vec<String>,
    /// Whether this scale contributes to the GPA mean at all: a pass/fail scale does
    /// not, though its credits still count as completed (§19.4).
    pub counts_in_gpa: bool,
}

impl Scale {
    #[must_use]
    pub fn value(&self, grade: &str) -> Option<f64> {
        self.grades.get(grade).copied()
    }

    #[must_use]
    pub fn holds(&self, grade: &str) -> bool {
        self.grades.contains_key(grade)
    }

    #[must_use]
    pub fn is_passing(&self, grade: &str) -> bool {
        self.passing.iter().any(|p| p == grade)
    }
}

/// A parsed `[code]_curriculum.toml`: the scales a grade is weighed against (§19.3).
///
/// The academic calendar (`terms`/`periods`, §19.5) is declared here too; this MVP folds
/// the GPA and leaves the period-label derivation to a later pass, so only the scales are
/// read for now.
#[derive(Clone, Debug, Default)]
pub struct Curriculum {
    default_scale: Option<String>,
    scales: HashMap<String, Scale>,
}

impl Curriculum {
    /// The scale that governs a grade symbol — the one that *holds* it (§19.4).
    ///
    /// The default scale wins where a symbol appears in more than one (an `F` sits in
    /// both an `af` and a `pf` scale), which keeps a grade's reading stable; otherwise
    /// the first scale carrying the symbol answers.
    #[must_use]
    pub fn scale_holding(&self, grade: &str) -> Option<&Scale> {
        if let Some(name) = &self.default_scale {
            if let Some(scale) = self.scales.get(name) {
                if scale.holds(grade) {
                    return Some(scale);
                }
            }
        }
        self.scales.values().find(|s| s.holds(grade))
    }

    /// Parse a curriculum's TOML text (§19.3), tolerating everything but a broken parse:
    /// a scale missing a field simply reads as empty for that field.
    #[must_use]
    pub fn parse(text: &str) -> Option<Curriculum> {
        let doc = text.parse::<toml_edit::DocumentMut>().ok()?;
        let default_scale = doc
            .get("default_scale")
            .and_then(toml_edit::Item::as_str)
            .map(str::to_owned);

        let mut scales = HashMap::new();
        if let Some(table) = doc.get("scale").and_then(toml_edit::Item::as_table_like) {
            for (name, item) in table.iter() {
                if let Some(scale) = parse_scale(item) {
                    scales.insert(name.to_owned(), scale);
                }
            }
        }
        Some(Curriculum {
            default_scale,
            scales,
        })
    }
}

fn parse_scale(item: &toml_edit::Item) -> Option<Scale> {
    let table = item.as_table_like()?;
    let counts_in_gpa = table
        .get("counts_in_gpa")
        .and_then(toml_edit::Item::as_bool)
        .unwrap_or(true);

    let mut grades = HashMap::new();
    if let Some(map) = table.get("grades").and_then(toml_edit::Item::as_table_like) {
        for (symbol, value) in map.iter() {
            if let Some(v) = value.as_float().or_else(|| value.as_integer().map(as_f64)) {
                grades.insert(symbol.to_owned(), v);
            }
        }
    }

    let passing = table
        .get("passing")
        .and_then(toml_edit::Item::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default();

    Some(Scale {
        grades,
        passing,
        counts_in_gpa,
    })
}

/// An integer grade value (`A = 5`) as an `f64`. A grade table's values are small
/// whole numbers, so the widening is exact.
#[allow(clippy::cast_precision_loss)]
fn as_f64(i: i64) -> f64 {
    i as f64
}

/// Every curriculum in the tree, paired with the node code it governs (§6.3).
///
/// Walked, never cached (§18, §5.0): the file sits at its node, named `[code]_curriculum
/// .toml`, and the walk is the whole of how one is found. A parse failure drops that one
/// file — a governed grade then cannot be valued and leaves the fold, which is the calm
/// absence, not a crash.
#[must_use]
pub fn discover(root: &Path) -> Vec<(Code, Curriculum)> {
    let mut out = Vec::new();
    let Ok(tree) = build_tree(root, None) else {
        return out;
    };
    match tree {
        TreeRoot::Forest(nodes) => {
            for node in &nodes {
                collect(node, &mut out);
            }
        }
        TreeRoot::Subtree(node) => collect(&node, &mut out),
    }
    out
}

fn collect(node: &Node, out: &mut Vec<(Code, Curriculum)>) {
    let file = node
        .path
        .join(format!("{}_curriculum.toml", node.code.as_str()));
    if let Ok(text) = std::fs::read_to_string(&file) {
        if let Some(curriculum) = Curriculum::parse(&text) {
            out.push((node.code.clone(), curriculum));
        }
    }
    for child in &node.children {
        collect(child, out);
    }
}

/// The curriculum that governs a fact's home (§19.4): the deepest one whose node is an
/// ancestor-or-self of the home code (§6.3).
///
/// A node's descendants share its code as a prefix — directly for a compact code, past a
/// `_` boundary for a definition-prefix one — the same reach `mv`-into-own-subtree
/// guards (§5.1). The **longest** governing node wins, so a programme's own scale beats a
/// sphere-wide one placed above it.
#[must_use]
pub fn governing<'a>(curricula: &'a [(Code, Curriculum)], home: &str) -> Option<&'a Curriculum> {
    curricula
        .iter()
        .filter(|(code, _)| governs(code, home))
        .max_by_key(|(code, _)| code.as_str().len())
        .map(|(_, curriculum)| curriculum)
}

/// Whether `code`'s subtree contains (or is) `home` (§6.3, §5.1).
fn governs(code: &Code, home: &str) -> bool {
    let c = code.as_str();
    if home == c {
        return true;
    }
    if code.is_compact() {
        home.starts_with(c)
    } else {
        home.starts_with(&format!("{c}_"))
    }
}

#[cfg(test)]
mod tests {
    // Grade values are exact small constants; comparing them is the assertion.
    #![allow(clippy::float_cmp)]
    use super::*;

    const KTH: &str = r#"
university    = "kth"
default_scale = "af"
periods_per_year = 5

[scale.af]
counts_in_gpa = true
grades  = { A = 5, B = 4, C = 3, D = 2, E = 1, Fx = 0, F = 0 }
passing = ["A", "B", "C", "D", "E"]

[scale.pf]
counts_in_gpa = false
grades  = { P = 0, F = 0 }
passing = ["P"]
"#;

    #[test]
    fn a_symbol_resolves_to_the_scale_that_holds_it() {
        let c = Curriculum::parse(KTH).expect("parses");
        // `A` is on `af`; `P` is on `pf` (§19.4).
        assert!(c.scale_holding("A").unwrap().counts_in_gpa);
        assert!(!c.scale_holding("P").unwrap().counts_in_gpa);
        assert_eq!(c.scale_holding("A").unwrap().value("A"), Some(5.0));
        assert!(c.scale_holding("A").unwrap().is_passing("A"));
        assert!(!c.scale_holding("Fx").unwrap().is_passing("Fx"));
        // A symbol no scale holds cannot be valued.
        assert!(c.scale_holding("Z").is_none());
    }

    #[test]
    fn the_deepest_governing_node_wins() {
        let broad = Curriculum::parse(KTH).unwrap();
        let narrow = Curriculum::parse(KTH).unwrap();
        let curricula = vec![
            (Code::parse("as").unwrap(), broad),
            (Code::parse("asd").unwrap(), narrow),
        ];
        // A fact homed at `asd` (or below) is governed by `asd`, the longer prefix.
        assert!(governing(&curricula, "asd").is_some());
        assert!(governs(&Code::parse("asd").unwrap(), "asd"));
        assert!(governs(&Code::parse("asd").unwrap(), "asdf"));
        assert!(!governs(&Code::parse("asd").unwrap(), "asx"));
    }
}
