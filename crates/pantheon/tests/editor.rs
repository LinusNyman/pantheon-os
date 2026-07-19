//! The editor form's spawn contract (§7.3): the editor is the environment's, and
//! what comes back decides whether anything is written at all.
//!
//! These drive [`edit_text_in`] rather than `$VISUAL`/`$EDITOR` — a test may not
//! mutate the process environment (it is `unsafe` in edition 2024, and these run in
//! parallel), and the environment is the hand's to set anyway.

use pantheon::contract::{Edited, edit_file_in, edit_text_in};

#[test]
fn text_that_comes_back_changed_is_returned() {
    let edited = edit_text_in(r#"sh -c 'printf "79.9\n" > "$0"'"#, "78.4\n").unwrap();
    match edited {
        Edited::Changed(text) => assert_eq!(text, "79.9\n"),
        Edited::Unchanged => panic!("the editor rewrote the buffer; that is a change"),
    }
}

#[test]
fn text_that_comes_back_unchanged_writes_nothing() {
    // `true` opens nothing and saves nothing — the `:q!` case (§7.3).
    let edited = edit_text_in("true", "78.4\n").unwrap();
    assert!(
        matches!(edited, Edited::Unchanged),
        "an untouched buffer is not a change (§7.3)"
    );
}

#[test]
fn an_editor_exiting_non_zero_writes_nothing() {
    // Exit `1`: a runtime failure, and the record is left exactly as it was (§7.3).
    let failed = edit_text_in("false", "78.4\n").unwrap_err();
    assert_eq!(failed.exit_code().as_u8(), 1);
}

#[test]
fn an_unparseable_editor_command_is_a_usage_error() {
    let failed = edit_text_in("sh -c 'unbalanced", "78.4\n").unwrap_err();
    assert_eq!(failed.exit_code().as_u8(), 2);
}

// ── the in-place form (§7.3, §8.7) ──────────────────────────────────────────
//
// What opens follows the shape (§6.1). An entity field or a series line opens a
// scratch buffer holding only that value; a **document is opened in place**, since it
// already *is* the text. So these drive the real file and there is no fold-back — the
// editor itself commits it.

fn scratch(name: &str, contents: &str) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!("pan-editfile-{}-{name}.md", std::process::id()));
    std::fs::write(&path, contents).unwrap();
    path
}

#[test]
fn a_file_that_comes_back_changed_is_returned_and_already_saved() {
    let path = scratch("changed", "+++\ntype = \"nota\"\n+++\n\nBefore.\n");
    let edited = edit_file_in(r#"sh -c 'printf "After.\n" > "$0"'"#, &path).unwrap();
    match edited {
        Edited::Changed(text) => assert_eq!(text, "After.\n"),
        Edited::Unchanged => panic!("the editor rewrote the file; that is a change"),
    }
    // The editor committed it — that is what "in place" means (§7.3).
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "After.\n");
    let _ = std::fs::remove_file(&path);
}

#[test]
fn a_file_that_comes_back_unchanged_writes_nothing() {
    let path = scratch("unchanged", "+++\ntype = \"nota\"\n+++\n\nProse.\n");
    // `true` opens nothing and saves nothing — the `:q!` case (§7.3).
    let edited = edit_file_in("true", &path).unwrap();
    assert!(matches!(edited, Edited::Unchanged));
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "+++\ntype = \"nota\"\n+++\n\nProse.\n",
        "an untouched file is byte-identical (§7.3)"
    );
    let _ = std::fs::remove_file(&path);
}

#[test]
fn an_editor_exiting_non_zero_leaves_the_file_alone() {
    let path = scratch("failed", "Prose.\n");
    let failed = edit_file_in("false", &path).unwrap_err();
    assert_eq!(failed.exit_code().as_u8(), 1);
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "Prose.\n");
    let _ = std::fs::remove_file(&path);
}
