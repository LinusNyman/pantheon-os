//! The table half of "format follows the hand" (§7.3, I8): stdout to a TTY → table,
//! piped → JSON. Same data, same code path — this renders the *emitted contract
//! value*, never a record, so there is one renderer rather than one per core (I4).
//!
//! It lives in the spine and not in Porticus on purpose. A TUI-bearing bin built
//! `--no-default-features` drops the chrome and is still a CLI that must table at a
//! terminal (§14, §12) — so the renderer cannot sit behind the `tui` feature.
//!
//! **No per-core knowledge** (I5). The columns are whatever keys the value carries;
//! the spine reads `data` opaquely here exactly as it does on disk.

use serde_json::Value;

/// The envelope keys the contract adds around a record (§7.2), in the order a reader
/// wants them rather than the order they serialize in.
///
/// A key not listed falls in after these, in the object's own key order — which is
/// `serde_json`'s, alphabetical, the spine taking no `preserve_order` feature. The
/// list is a *preference*, never a filter: a new contract key shows up on its own
/// rather than being silently dropped.
const ENVELOPE_ORDER: &[&str] = &[
    "core", "home", "kind", "series", "slug", "ext", "key", "type", "tags", "refs",
];

/// Render a contract value for a reader.
///
/// Three shapes, because the contract emits three (§7.2):
/// - an **array of objects** (a fold, a series) → a column per key, a row per element;
/// - a **single object** (one record, `schema`, `version`) → a field list, since one
///   record as a one-row table puts its widest column off the screen;
/// - anything else → pretty JSON, which is what a scalar or an array of scalars is.
pub fn render(value: &Value) -> String {
    match value {
        Value::Array(rows) if rows.is_empty() => String::new(),
        Value::Array(rows) if rows.iter().all(is_flat_record) => grid(rows),
        Value::Object(_) => fields(value),
        _ => pretty(value),
    }
}

/// A row a grid can render: an object whose every value outside `data` fits one cell.
///
/// **Deliberately not recursive.** If a row holding a list of rows counted as flat,
/// `pan tree` would qualify — its nodes nest nodes — and render as a grid whose
/// `children` column is the JSON the table was meant to spare you. One level of
/// nesting is a sub-grid ([`is_sub_grid`]); two is a structure, and pretty JSON is the
/// honest rendering of a structure.
fn is_flat_record(value: &Value) -> bool {
    value.as_object().is_some_and(|object| {
        object
            .iter()
            .all(|(key, value)| key == "data" || is_cell(value))
    })
}

/// A value that renders as one cell: a scalar, or a list of scalars (`refs`, `tags`).
fn is_cell(value: &Value) -> bool {
    match value {
        Value::Object(_) => false,
        Value::Array(items) => !items.iter().any(|i| i.is_object() || i.is_array()),
        _ => true,
    }
}

/// A value that renders as a labeled sub-grid below a field list: a non-empty list of
/// flat rows — `pan resolve`'s `resolved`, a cascade's rewrites.
fn is_sub_grid(value: &Value) -> bool {
    matches!(value, Value::Array(items) if !items.is_empty() && items.iter().all(is_flat_record))
}

// ── the grid: a fold, a series, a resolve ────────────────────────────────────

/// One column per key across every row, one row per element.
fn grid(rows: &[Value]) -> String {
    let columns = columns(rows);
    let cells: Vec<Vec<String>> = rows.iter().map(|row| render_row(row, &columns)).collect();

    let widths: Vec<usize> = columns
        .iter()
        .enumerate()
        .map(|(i, col)| {
            cells
                .iter()
                .map(|row| width(&row[i]))
                .chain(std::iter::once(width(col)))
                .max()
                .unwrap_or(0)
        })
        .collect();

    let mut out = String::new();
    push_line(&mut out, &upcased(&columns), &widths);
    for row in &cells {
        push_line(&mut out, row, &widths);
    }
    out
}

/// The column set: every key any row carries, `data`'s own keys hoisted into columns
/// of their own.
///
/// Hoisting is what makes the table worth having — `data` is where a core's actual
/// record lives, and one cell of nested JSON is the thing a reader came to avoid. A
/// hoisted key that collides with an envelope key keeps its `data.` prefix, so no
/// column is ever two different things.
fn columns(rows: &[Value]) -> Vec<String> {
    let mut envelope: Vec<String> = Vec::new();
    let mut hoisted: Vec<String> = Vec::new();

    for row in rows {
        let Some(object) = row.as_object() else {
            continue;
        };
        for (key, value) in object {
            if key == "data" {
                for inner in value.as_object().into_iter().flatten().map(|(k, _)| k) {
                    let name = if object.contains_key(inner.as_str()) {
                        format!("data.{inner}")
                    } else {
                        inner.clone()
                    };
                    if !hoisted.contains(&name) {
                        hoisted.push(name);
                    }
                }
            } else if !envelope.contains(key) {
                envelope.push(key.clone());
            }
        }
    }

    envelope.sort_by_key(|key| {
        ENVELOPE_ORDER
            .iter()
            .position(|known| known == key)
            .unwrap_or(usize::MAX)
    });
    envelope.extend(hoisted);
    envelope
}

/// This row's cell for every column. A key the row does not carry is blank, not
/// `null` — a fold spans nodes and a column absent here is not a value withheld.
fn render_row(row: &Value, columns: &[String]) -> Vec<String> {
    let object = row.as_object();
    columns
        .iter()
        .map(|column| {
            let direct = object.and_then(|o| o.get(column.as_str()));
            let inner = || {
                let key = column.strip_prefix("data.").unwrap_or(column);
                object?.get("data")?.as_object()?.get(key)
            };
            direct.or_else(inner).map_or(String::new(), scalar)
        })
        .collect()
}

// ── the field list: one record ───────────────────────────────────────────────

/// One record as `label  value` lines, `data`'s keys hoisted alongside the envelope's
/// exactly as the grid hoists them.
///
/// A **multi-line** value — a document's body, the one the contract carries whole
/// (§7.2) — is printed as a block below the fields rather than crushed onto one line.
///
/// A value that is itself a **list of records** — `pan resolve`'s `resolved`, a
/// `--dry-run`'s `cascade` — becomes a labeled sub-grid below the fields, since that
/// is what it is. One level only.
///
/// Not every emitted object is a record. `pan tree` nests nodes inside nodes and
/// `schema` nests a JSON Schema; neither is a row of fields, and squeezing one into a
/// cell would render it *less* legible than the JSON it came from. So a structure this
/// shape declines to flatten falls back to pretty JSON whole (see [`flattenable`]) —
/// a table where the contract is tabular, and the source where it is not.
fn fields(value: &Value) -> String {
    let Some(object) = value.as_object() else {
        return pretty(value);
    };
    if !flattenable(object) {
        return pretty(value);
    }

    let mut pairs: Vec<(String, &Value)> = Vec::new();
    let mut blocks: Vec<(String, String)> = Vec::new();

    for (key, inner) in object {
        if key == "data" {
            for (name, nested) in inner.as_object().into_iter().flatten() {
                let label = if object.contains_key(name.as_str()) {
                    format!("data.{name}")
                } else {
                    name.clone()
                };
                sort_into(&mut pairs, &mut blocks, label, nested);
            }
        } else {
            sort_into(&mut pairs, &mut blocks, key.clone(), inner);
        }
    }

    pairs.sort_by_key(|(label, _)| {
        ENVELOPE_ORDER
            .iter()
            .position(|known| known == label)
            .unwrap_or(usize::MAX)
    });

    let label_width = pairs.iter().map(|(l, _)| width(l)).max().unwrap_or(0);
    let mut out = String::new();
    for (label, value) in &pairs {
        let line = format!("{}  {}", pad(&upcase(label), label_width), scalar(value));
        out.push_str(line.trim_end());
        out.push('\n');
    }
    for (label, block) in blocks {
        out.push('\n');
        out.push_str(&upcase(&label));
        out.push('\n');
        out.push_str(&block);
        if !block.ends_with('\n') {
            out.push('\n');
        }
    }
    out
}

/// Whether a field list can render this object: every value outside `data` is either
/// one cell or one sub-grid.
///
/// `data` is exempt and always admitted: it is the core's own record, the spine carries
/// it opaquely (I5), and its keys are hoisted into fields whose values may be anything
/// — an Album `away` list renders as its own sub-grid, which is the right size for one
/// field of one record. What disqualifies an object is a nesting **outside** `data`
/// that is neither: `pan tree`'s `nodes`, a `schema`'s definitions.
fn flattenable(object: &serde_json::Map<String, Value>) -> bool {
    object
        .iter()
        .all(|(key, value)| key == "data" || is_cell(value) || is_sub_grid(value))
}

/// Where each value belongs: a multi-line string and a list of records are blocks
/// below the fields; everything else is a field.
fn sort_into<'a>(
    pairs: &mut Vec<(String, &'a Value)>,
    blocks: &mut Vec<(String, String)>,
    label: String,
    value: &'a Value,
) {
    match value {
        Value::String(text) if text.contains('\n') => blocks.push((label, text.clone())),
        Value::Array(items) if is_sub_grid(value) => blocks.push((label, indent(&grid(items)))),
        _ => pairs.push((label, value)),
    }
}

/// Two spaces, so a sub-grid reads as belonging to the field above it.
fn indent(block: &str) -> String {
    use std::fmt::Write as _;
    block.lines().fold(String::new(), |mut out, line| {
        let _ = writeln!(out, "  {line}");
        out
    })
}

// ── cells ────────────────────────────────────────────────────────────────────

/// One value as one cell.
///
/// `null` renders **blank**, not `"null"`: an absent `type` on a document (§7.2) is
/// nothing to read, and the word would be louder than the value. An array of scalars
/// joins on `, ` — that is what `refs` and `tags` are — and anything still nested
/// falls back to compact JSON rather than being flattened into a guess.
fn scalar(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::String(text) => text.replace('\n', " "),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        Value::Array(items) if items.iter().all(|i| !i.is_object() && !i.is_array()) => {
            items.iter().map(scalar).collect::<Vec<_>>().join(", ")
        }
        other => other.to_string(),
    }
}

fn upcased(columns: &[String]) -> Vec<String> {
    columns.iter().map(|c| upcase(c)).collect()
}

fn upcase(label: &str) -> String {
    label.to_uppercase()
}

/// Trailing whitespace is stripped: a table that is copied out of a terminal should
/// not carry padding into whatever reads it next.
fn push_line(out: &mut String, cells: &[String], widths: &[usize]) {
    let mut line = String::new();
    for (i, cell) in cells.iter().enumerate() {
        if i + 1 == cells.len() {
            line.push_str(cell);
        } else {
            line.push_str(&pad(cell, widths[i]));
            line.push_str("  ");
        }
    }
    out.push_str(line.trim_end());
    out.push('\n');
}

fn pad(cell: &str, to: usize) -> String {
    let mut out = cell.to_string();
    for _ in width(cell)..to {
        out.push(' ');
    }
    out
}

/// Display width, counted in `char`s.
///
/// Not grapheme clusters and not East-Asian width: §13 lists no width crate, and
/// names normalize to NFC (§5.1), so a composed `ö` is one `char` and columns line up
/// for the text this tree actually holds. A CJK label will sit a column proud — a
/// cosmetic cost the spine does not spend a dependency to close.
fn width(cell: &str) -> usize {
    cell.chars().count()
}

fn pretty(value: &Value) -> String {
    serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string())
}
