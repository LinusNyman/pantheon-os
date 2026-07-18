//! The editor form's spawn contract (§7.3): the editor is the environment's, and
//! what comes back decides whether anything is written at all.
//!
//! These drive [`edit_text_in`] rather than `$VISUAL`/`$EDITOR` — a test may not
//! mutate the process environment (it is `unsafe` in edition 2024, and these run in
//! parallel), and the environment is the hand's to set anyway.

use pantheon::contract::{Edited, edit_text_in};

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
