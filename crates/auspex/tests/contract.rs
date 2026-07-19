//! `aus`'s frozen JSON contract (I4), taken from the real binary rather than the
//! library behind it.
//!
//! Auspex mints **no plan token** — it is code, governed by I2 and its own `plan`
//! (§7.3) — so unlike every core's contract test there is nothing here to redact.
//! Nothing reads the wall clock either: a rule's discovery is a walk, and `now` enters
//! only with the propose protocol.
//!
//! What this file pins is the **read half**: discovery, the header, and the verb
//! surface. Executing a rule lands with `plan`/`test`; applying a proposal with `run`.

#![allow(clippy::format_push_string)]

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU32, Ordering};

use pantheon::mint::NewSpec;
use pantheon::plan_new;
use serde_json::{Value, json};

static COUNTER: AtomicU32 = AtomicU32::new(0);

/// Run the real `aus`, returning its exit code and the JSON it emitted — stdout when
/// it produced a value, stderr for the `{"error":…}` envelope (§7.3).
fn aus(root: &Path, args: &[&str]) -> (i32, Value) {
    aus_env(root, args, &[])
}

/// [`aus`] with environment set for the child — never for this process (§7.3).
fn aus_env(root: &Path, args: &[&str], env: &[(&str, &str)]) -> (i32, Value) {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_aus"));
    cmd.arg("-C")
        .arg(root)
        .args(args)
        .env_remove("PANTHEON_ROOT");
    for (key, value) in env {
        cmd.env(key, value);
    }
    let out = cmd.output().unwrap();
    let code = out.status.code().unwrap_or(-1);
    let bytes = if out.stdout.is_empty() {
        out.stderr
    } else {
        out.stdout
    };
    (code, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
}

fn pretty(value: &Value) -> String {
    serde_json::to_string_pretty(value).unwrap()
}

/// A tree with two nodes to scope rules at: `c` → `cs` → `csa`, and `a` → `ac`.
fn fresh_root() -> PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("aus-snap-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    for (parent, ch, label) in [
        ("root", "c", "contextus"),
        ("c", "s", "societas"),
        ("cs", "a", "amicitia"),
        ("root", "a", "actio"),
        ("a", "c", "cura"),
    ] {
        let (plan, _) = plan_new(&dir, parent, NewSpec::Triple { ch, label }).unwrap();
        plan.apply(&dir).unwrap();
    }
    dir
}

/// Write a rule into a node's meta dir, minting the dir if it is not there.
///
/// **Written by hand, not through a binary** — and that is the point rather than a
/// shortcut: a rule *is* a hand-authored file (§9.1). `touch` mints one and `rm`
/// removes one, so there is no tool to route this through, and a fixture that faked
/// one would be testing something the system does not have.
fn write_rule(root: &Path, node_path: &str, code: &str, file: &str, body: &str) {
    let meta = root.join(node_path).join(format!("{code}__"));
    std::fs::create_dir_all(&meta).unwrap();
    std::fs::write(meta.join(file), body).unwrap();
}

const CSA: &str = "c_contextus/c_s_societas/cs_a_amicitia";
const AC: &str = "a_actio/a_c_cura";

/// A tree carrying one of each header shape the parser must meet.
fn seeded() -> PathBuf {
    let root = fresh_root();

    // A shebang takes line 1, so the header is line 2 (§9.2).
    write_rule(
        &root,
        CSA,
        "csa",
        "csa__function__stale_contact.py",
        "#!/usr/bin/env python3\n# auspex: watch=annales \
         writes=pensum@acm:add;annales@csa/stale_contact:add desc=nudge me about quiet friends\n\
         print(\"{}\")\n",
    );
    // No shebang: the header is line 1, and `//` is the leader for JS/Rust (§9.2).
    write_rule(
        &root,
        CSA,
        "csa",
        "csa__function__js_style.js",
        "// auspex: watch=pensum writes=annales@csa:add\n",
    );
    // No header at all — a legal rule that declares nothing, so it is read-only by
    // the default-deny rule (§9.2). Also extensionless: the shebang names the
    // language, and `classify` ignores the extension either way (§9.1).
    write_rule(
        &root,
        CSA,
        "csa",
        "csa__function__silent",
        "#!/bin/sh\n# just a comment, not a declaration\necho '{}'\n",
    );
    // A name carrying `__`: `classify` rejoins everything after the token (§9.1).
    write_rule(
        &root,
        AC,
        "ac",
        "ac__function__weigh__in.sh",
        "# auspex: watch=annales writes=pensum@ac:add\n",
    );
    root
}

// ── the surface (§9.6) ───────────────────────────────────────────────────────

#[test]
fn the_verb_surface_is_frozen() {
    let root = fresh_root();
    let (code, help) = aus(&root, &["help"]);
    assert_eq!(code, 0);
    insta::assert_snapshot!("help_surface", pretty(&help));

    // A bare short emits `help` down a pipe — a TUI has nothing to draw there (§7.3).
    let (code, bare) = aus(&root, &[]);
    assert_eq!(code, 0);
    assert_eq!(bare, help, "a bare short piped is `help` (§7.3)");
}

/// `pan doctor` probes every app's `version -f json` and reads `format_version` off
/// it; a mismatch is what §15.5 makes the check for. `aus` is already in `pan`'s
/// `KNOWN_SHORTS`, so this is the surface that turns it from *absent* into *installed*.
#[test]
fn version_is_what_doctor_reads() {
    let root = fresh_root();
    let (code, version) = aus(&root, &["version"]);
    assert_eq!(code, 0);
    assert_eq!(version["short"], "aus");
    assert_eq!(version["name"], "auspex");
    assert_eq!(
        version["format_version"], 1,
        "a disagreement here is what `pan doctor` exists to report (§15.5)"
    );
    insta::assert_snapshot!("version", pretty(&version));
}

// ── discovery and the header (§9.1, §9.2) ────────────────────────────────────

#[test]
fn every_rule_is_listed_with_its_scope_and_grant() {
    let root = seeded();
    let (code, rules) = aus(&root, &["ls"]);
    assert_eq!(code, 0);
    insta::assert_snapshot!("ls", pretty(&rules));
}

/// §9.1: "Where the file sits is the whole of its scope." The filename's code segment
/// is *classify*'s reading of the name, not an authority — a rule moved by hand
/// without its prefix rewritten is misfiled, and the meta dir still wins.
#[test]
fn the_meta_dir_wins_over_a_filenames_own_code() {
    let root = fresh_root();
    write_rule(
        &root,
        CSA,
        "csa",
        "cs__function__misfiled.sh",
        "# auspex: watch=album\n",
    );
    let (code, rules) = aus(&root, &["ls"]);
    assert_eq!(code, 0);
    assert_eq!(
        rules[0]["scope"], "csa",
        "the scope is where the file sits (§9.1)"
    );
    assert_eq!(
        rules[0]["misfiled_as"], "cs",
        "and the filename's disagreement is reported, not honoured"
    );
}

/// A header Auspex cannot read leaves the rule **default-deny** rather than dropping
/// it from the listing: `writes` is what stands between a rule and your records
/// (§9.2, §9.5), so the unreadable case must fail closed and say so.
#[test]
fn an_unreadable_header_fails_closed_and_is_reported() {
    let root = fresh_root();
    write_rule(
        &root,
        CSA,
        "csa",
        "csa__function__typo.sh",
        "# auspex: watch=annales bogus writes=pensum@acm:add\n",
    );
    let (code, rules) = aus(&root, &["ls"]);
    assert_eq!(code, 0);
    assert_eq!(rules[0]["name"], "typo", "the rule is still listed");
    assert!(
        rules[0]["error"].as_str().unwrap().contains("bogus"),
        "and names what it could not read: {}",
        rules[0]["error"]
    );
}

/// A rule declaring no `writes` is read-only — it may propose, but nothing it proposes
/// lands (§9.2). The listing must show that as an empty grant, never as a missing key
/// a reader could mistake for "not checked".
#[test]
fn a_rule_with_no_header_declares_nothing() {
    let root = seeded();
    let (_, rules) = aus(&root, &["ls"]);
    let silent = rules
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["name"] == "silent")
        .expect("the headerless rule is listed");
    assert_eq!(silent["writes"], json!([]), "default-deny (§9.2)");
    assert_eq!(silent["watch"], json!([]));
    assert!(silent.get("error").is_none(), "no header is not an error");
}

/// A scope is `build_tree`'s own subtree argument, so `aus ls <code>` narrows exactly
/// as `pan tree <code>` does — this node and everything under it (§6.3, §9.1).
#[test]
fn a_scope_narrows_to_a_subtree() {
    let root = seeded();
    let (code, all) = aus(&root, &["ls"]);
    assert_eq!(code, 0);
    assert_eq!(all.as_array().unwrap().len(), 4);

    let (code, at_c) = aus(&root, &["ls", "c"]);
    assert_eq!(code, 0);
    let names: Vec<&str> = at_c
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["name"].as_str().unwrap())
        .collect();
    assert_eq!(
        names,
        ["js_style", "silent", "stale_contact"],
        "the `a` sphere's rule is out of scope"
    );
}

// ── a rule may not re-enter the engine (§9.3) ────────────────────────────────

#[test]
fn the_verbs_that_evaluate_a_rule_are_refused_to_a_rule() {
    let root = seeded();
    let rule = [("PANTHEON_RULE", "1")];

    for args in [&["run"][..], &["plan"][..], &["test", "stale_contact"][..]] {
        let (code, out) = aus_env(&root, args, &rule);
        assert_eq!(code, 6, "aus {args:?} must be refused under a rule (§9.3)");
        assert_eq!(out["error"]["code"], 6);
    }

    // Reads run free: a rule may look at what exists, it simply may not evaluate.
    for args in [&["ls"][..], &["help"][..], &["version"][..]] {
        let (code, _) = aus_env(&root, args, &rule);
        assert_eq!(code, 0, "aus {args:?} reads and is not refused (§9.3)");
    }

    insta::assert_snapshot!(
        "refusal_under_rule",
        pretty(&aus_env(&root, &["plan"], &rule).1)
    );
}

// ── exit codes are contract (§7.3) ───────────────────────────────────────────

/// Why · the argv · the environment the child runs under.
type Case<'a> = (&'a str, &'a [&'a str], &'a [(&'a str, &'a str)]);

#[test]
fn exit_codes() {
    let root = seeded();
    let rule = [("PANTHEON_RULE", "1")];

    let cases: &[Case] = &[
        ("every rule in the tree", &["ls"], &[]),
        ("a scope that is a real node", &["ls", "csa"], &[]),
        ("a scope naming no node", &["ls", "nosuch"], &[]),
        ("rule execution is not built yet", &["run"], &[]),
        ("nor is planning", &["plan"], &[]),
        ("a rule may not re-enter the engine", &["run"], &rule),
        ("but it may read", &["ls"], &rule),
    ];

    let mut out = String::new();
    for (why, args, env) in cases {
        let (code, value) = aus_env(&root, args, env);
        let outcome = value
            .pointer("/error/msg")
            .and_then(Value::as_str)
            .map_or_else(
                || {
                    let n = value.as_array().map_or(0, Vec::len);
                    format!("{n} rule(s)")
                },
                str::to_string,
            );
        let under = if env.is_empty() {
            String::new()
        } else {
            " (PANTHEON_RULE=1)".to_string()
        };
        out.push_str(&format!(
            "aus {}{under}\n  # {why}\n  => exit {code}: {outcome}\n",
            args.join(" ")
        ));
    }
    insta::assert_snapshot!("exit_codes", out);
}
