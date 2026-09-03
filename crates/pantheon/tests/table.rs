//! The table renderer (§7.3, I8). These pin *shape*, not prettiness: what becomes a
//! column, what becomes blank, and what the renderer declines to flatten.
//!
//! The contract's own snapshots cannot cover this — every one of them pipes, so they
//! all take the JSON path. A table is only ever seen by the other hand.

use pantheon::table::render;
use serde_json::json;

/// A fold is a grid: envelope keys first in reading order, `data`'s keys hoisted into
/// columns of their own — which is the whole reason to render a table at all.
///
/// The `core`/`home`/`kind` columns are **constant** across both rows here, so they
/// carry no information and are elided at the TTY (P3, see [`a_constant_column_is_dropped`]);
/// what is left is the identity `KEY`, the varying `REFS`, and the hoisted `DONE`.
#[test]
fn a_fold_hoists_data_into_columns() {
    let value = json!([
        {"core":"pensum","home":"ac","kind":"task","key":"buy_milk","refs":[],"data":{"done":"20260719"}},
        {"core":"pensum","home":"ac","kind":"task","key":"call_alex","refs":["album:alex"],"data":{}},
    ]);
    assert_eq!(
        render(&value),
        "\
KEY        REFS        DONE
buy_milk               20260719
call_alex  album:alex
"
    );
}

/// A column constant across every row carries no information and is dropped at the TTY
/// (P3): `pen ls` shows only what varies. The **identity** column stays even were it
/// constant, and a single-row list keeps everything (that is what
/// [`a_single_row_keeps_its_constant_columns`] pins). Piped output is JSON and unaffected.
#[test]
fn a_constant_column_is_dropped() {
    let value = json!([
        {"core":"pensum","home":"ac","kind":"task","key":"buy_milk"},
        {"core":"pensum","home":"ac","kind":"task","key":"call_alex"},
    ]);
    let out = render(&value);
    assert!(
        !out.contains("CORE"),
        "the constant core column is dropped: {out}"
    );
    assert!(
        !out.contains("KIND"),
        "the constant kind column is dropped: {out}"
    );
    assert!(out.contains("KEY"), "the identity column stays: {out}");
    assert!(
        out.contains("buy_milk") && out.contains("call_alex"),
        "the varying values remain: {out}"
    );
}

/// The drop is **general, never per-core** (I5): a two-shape core's `kind` varies across
/// rows, so it stays — for free, no core knowledge in the spine.
#[test]
fn a_varying_column_is_kept() {
    let value = json!([
        {"core":"fasti","kind":"span","slug":"enrol"},
        {"core":"fasti","kind":"event","slug":"exam"},
    ]);
    let out = render(&value);
    assert!(out.contains("KIND"), "a varying kind is kept: {out}");
    assert!(
        !out.contains("CORE"),
        "the constant core still drops: {out}"
    );
}

/// A **single-row** list keeps every column even when each is trivially "constant" — a
/// one-record list must still show what it is (P3), so the drop only fires with rows to
/// compare.
#[test]
fn a_single_row_keeps_its_constant_columns() {
    let value = json!([{"core":"pensum","home":"ac","kind":"task","key":"buy_milk"}]);
    let out = render(&value);
    assert!(out.contains("CORE"), "one row shows what it is: {out}");
    assert!(out.contains("KIND"), "{out}");
}

/// A column a row does not carry is **blank**, not `null`. A fold spans nodes, so an
/// absent key is a record that never had one — not a value withheld.
#[test]
fn an_absent_value_is_blank_not_null() {
    let value = json!([
        {"slug":"a","type":"principium"},
        {"slug":"b","type":null},
    ]);
    assert_eq!(
        render(&value),
        "\
SLUG  TYPE
a     principium
b
"
    );
}

/// `refs` and `tags` are lists of scalars and read as one cell, comma-joined.
#[test]
fn a_scalar_list_joins_into_one_cell() {
    let value = json!([{"slug":"note","tags":["mores","vocatio"]}]);
    assert_eq!(
        render(&value),
        "\
SLUG  TAGS
note  mores, vocatio
"
    );
}

/// One record is a field list, not a one-row table: its widest column would otherwise
/// run off the screen.
#[test]
fn one_record_is_a_field_list() {
    let value = json!({
        "core":"album","home":"csa","kind":"person","slug":"mara",
        "refs":["album:alex"],"data":{"gender":"f"}
    });
    assert_eq!(
        render(&value),
        "\
CORE    album
HOME    csa
KIND    person
SLUG    mara
REFS    album:alex
GENDER  f
"
    );
}

/// A document's body is the one value the contract carries whole (§7.2). It is a block
/// below the fields, never crushed onto a line.
#[test]
fn a_multiline_value_becomes_a_block() {
    let value = json!({
        "core":"tabella","home":"ecv","slug":"note","ext":"md",
        "type":"reflexio","tags":[],"body":"First line.\nSecond line.\n"
    });
    assert_eq!(
        render(&value),
        "\
CORE  tabella
HOME  ecv
SLUG  note
EXT   md
TYPE  reflexio
TAGS

BODY
First line.
Second line.
"
    );
}

/// A list of records nested in a record is a sub-grid — one level, indented.
///
/// Columns the envelope does not name (`path`, `ref`) fall in the object's own key
/// order, which is `serde_json`'s: alphabetical, since the spine takes no
/// `preserve_order` feature.
#[test]
fn a_nested_list_of_records_is_a_sub_grid() {
    let value = json!({
        "verb":"rename",
        "cascade":[{"ref":"album:john","path":"csa__/x.json"}],
    });
    assert_eq!(
        render(&value),
        "\
VERB  rename

CASCADE
  PATH          REF
  csa__/x.json  album:john
"
    );
}

/// `pan tree` nests nodes inside nodes; a `schema` nests a JSON Schema. Neither is a
/// row of fields, so the renderer declines and hands back the source (§7.3) rather
/// than squeezing a structure into a cell.
#[test]
fn a_structure_falls_back_to_json() {
    let value = json!({"nodes":[{"code":"a","children":[{"code":"ac","children":[]}]}]});
    assert_eq!(
        render(&value),
        serde_json::to_string_pretty(&value).unwrap()
    );
}

/// An empty fold renders as nothing at all — absence is calm (I7), and a header over
/// no rows claims a shape the answer does not have.
#[test]
fn an_empty_fold_renders_nothing() {
    assert_eq!(render(&json!([])), "");
}

/// `data` is the core's own record and the spine reads it opaquely (I5). A nested
/// value inside it is one cell of JSON — the right size for one field — and does not
/// disqualify the record from being a row.
#[test]
fn nesting_inside_data_stays_a_row() {
    let value = json!([{"slug":"alex","data":{"away":[{"from":"20260601"}]}}]);
    assert_eq!(
        render(&value),
        "\
SLUG  AWAY
alex  [{\"from\":\"20260601\"}]
"
    );
}
