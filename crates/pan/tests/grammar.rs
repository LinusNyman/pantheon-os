//! `pan`'s share of the shared grammar (§7.3) — the parts it had been the exception to.
//!
//! Asserted structurally rather than snapshotted, like `doctor` beside it: what is
//! stable is the *rule*, not the prose that satisfies it.

use std::path::PathBuf;
use std::process::Command;

use serde_json::Value;

fn pan(args: &[&str]) -> (i32, Value, String) {
    let bin = PathBuf::from(env!("CARGO_BIN_EXE_pan"));
    let out = Command::new(bin)
        .args(args)
        .env_remove("PANTHEON_ROOT")
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let value = serde_json::from_str(&stdout).unwrap_or(Value::Null);
    (
        out.status.code().unwrap_or(1),
        value,
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

/// **A bare short piped emits, rather than opening** (§7.3).
///
/// `pan` had been the one exception in the suite: it answered a pipe with prose where
/// every core answers with the contract. A screen has nothing to draw down a pipe, and
/// the TTY rule governs `pan` too.
#[test]
fn a_piped_bare_short_emits_the_surface_as_json() {
    let (code, value, _) = pan(&[]);
    assert_eq!(code, 0);
    assert_eq!(value["short"], "pan");
    assert!(
        value["verbs"].as_array().is_some_and(|v| !v.is_empty()),
        "the surface names its verbs: {value}"
    );
}

/// The seven placement rules (§2), emitted so a human and an LLM file alike (§5.5, I8).
#[test]
fn constitution_emits_the_seven_rules() {
    let (code, value, stderr) = pan(&["constitution"]);
    assert_eq!(code, 0, "{stderr}");
    let rules = value["rules"].as_array().expect("rules is an array");
    assert_eq!(rules.len(), 7, "§2 states seven placement rules");
    // Numbered, so a caller can cite one.
    for (i, rule) in rules.iter().enumerate() {
        assert_eq!(rule["n"], i + 1);
        assert!(rule["name"].as_str().is_some_and(|s| !s.is_empty()));
        assert!(rule["rule"].as_str().is_some_and(|s| !s.is_empty()));
    }
    // With no node named there is no node half — null, not an empty object: the
    // question was not asked.
    assert_eq!(value["node"], Value::Null);
}

/// **`pan <code>` no longer swallows the rest of the line.**
///
/// It was a clap `external_subcommand`, which hands the subcommand every remaining word
/// as opaque text — so `pan csa -f table` parsed no `-f` at all and silently emitted
/// JSON. A universal flag quietly dropped is worse than one refused (§7.3). The implicit
/// verb is now inserted by a pre-pass, exactly as a core inserts `add`.
#[test]
fn a_trailing_flag_after_a_code_is_not_swallowed() {
    // `-f json` after the code must be *parsed*, not absorbed. Without a root this
    // fails at root resolution (exit 2) rather than at flag parsing — which is itself
    // the proof the flag was understood and the code reached its verb.
    let (code, _, stderr) = pan(&["csa", "-f", "json"]);
    assert_eq!(code, 2, "no root named is a usage error (§6.2): {stderr}");
    assert!(
        stderr.contains("PANTHEON_ROOT") || stderr.contains("root"),
        "it should fail on the root, not on the flag: {stderr}"
    );
}

/// A verb still wins over a node code — the ambiguity rule the pre-pass must preserve
/// (§7.3). `doctor` is a verb, never a code to look up.
#[test]
fn a_verb_still_wins_over_a_code() {
    let (code, value, _) = pan(&["doctor"]);
    assert_eq!(code, 0);
    assert!(
        value["apps"].is_array(),
        "`doctor` ran as a verb rather than being looked up as a code: {value}"
    );
}
