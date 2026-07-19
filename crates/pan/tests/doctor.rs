//! `pan doctor` (§5.5): what is installed, whether the apps agree about the format
//! version (§15.5), and whether the file→core map is total.
//!
//! Asserted **structurally rather than frozen in a snapshot**, deliberately. Doctor's
//! output is a picture of what is installed, so it changes every time a step turns a
//! scaffold into a real binary (§16) — a snapshot here would churn at steps 7, 8, and
//! 9 without ever catching a contract change. What is stable is the *claim*: every
//! token owned by exactly one core, and one Document core taking the rest by
//! extension. That is what these check.

use std::path::PathBuf;
use std::process::Command;

use serde_json::Value;

fn doctor() -> Value {
    let bin_dir = PathBuf::from(env!("CARGO_BIN_EXE_pan"))
        .parent()
        .unwrap()
        .to_path_buf();
    for short in ["alb", "ann", "pen", "tab"] {
        assert!(
            bin_dir.join(short).exists(),
            "`{short}` must be built beside `pan`: run `cargo build --workspace --bins`"
        );
    }
    let path = std::env::var("PATH").unwrap_or_default();
    let out = Command::new(bin_dir.join("pan"))
        .arg("doctor")
        .env("PATH", format!("{}:{path}", bin_dir.display()))
        .env_remove("PANTHEON_ROOT")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0));
    serde_json::from_slice(&out.stdout).unwrap()
}

/// The totality claim itself (§5.5): "every token has exactly one owning core and one
/// shape, which is what lets resolution read a name without importing a core".
#[test]
fn a_clean_run_means_the_file_to_core_map_is_total() {
    let d = doctor();
    assert_eq!(d["collisions"], serde_json::json!([]));
    assert_eq!(d["map_total"], true);
}

/// **What Tabella is what makes demonstrable** (§16). Every other core reaches its
/// files through a globally-unique `[kind]` token; a Document core declares none, so
/// its files reach it by extension alone. Doctor showing one such core beside a
/// collision-free token map is the map being `extension ∪ token` rather than token
/// alone — the claim stated rather than merely asserted.
#[test]
fn exactly_one_document_core_takes_the_rest_by_extension() {
    let d = doctor();
    assert_eq!(
        d["document_cores"],
        serde_json::json!(["tabella"]),
        "a Document core is named by declaring no tokens (§7.1), and the mapping is \
         unambiguous only while there is one of them (§7.1)"
    );
    let tokens = d["tokens"].as_array().unwrap();
    assert!(
        tokens.iter().all(|t| t["core"] != "tabella"),
        "tabella contributes no token, which is the whole point (§7.1)"
    );
    // The cores built by step 5 own these; each is claimed once.
    for token in ["person", "organization", "group", "log", "task"] {
        assert_eq!(
            tokens.iter().filter(|t| t["token"] == token).count(),
            1,
            "{token} must have exactly one owner (§7.1)"
        );
    }
}

/// A format bump is a breaking change for every app and gets a migration; crate
/// versions drift freely beneath it (§15.5). So a *disagreement* is what doctor flags.
#[test]
fn the_installed_apps_agree_about_the_format_version() {
    let d = doctor();
    assert_eq!(d["format"]["agreed"], true);
    let shorts: Vec<&str> = d["apps"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a["short"].as_str().unwrap())
        .collect();
    for built in ["pan", "alb", "ann", "pen", "tab"] {
        assert!(shorts.contains(&built), "{built} is built and must be seen");
    }
    // A scaffold prints a line rather than emitting `version -f json`, so it reads as
    // absent — which is the honest answer, and the same tolerance discovery shows a
    // missing core (§5.0).
    assert!(
        d["absent"]
            .as_array()
            .unwrap()
            .contains(&Value::from("spe")),
        "an unbuilt app is absent, not an error"
    );
}
