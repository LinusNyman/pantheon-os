//! **The propose protocol, end to end (§9.3).** A rule is a child process: context on
//! its stdin, proposals on its stdout, and `PANTHEON_RULE=1` in its environment.
//!
//! Its own file rather than more of `contract.rs`, because every fixture here is an
//! **executable script** and so the whole file is `#![cfg(unix)]`. `contract.rs` stays
//! portable: discovery and the header need no exec bit.
//!
//! Nothing here is snapshotted. `now` is today's date by construction (§9.3), so a
//! frozen copy would fail tomorrow — the one thing CLAUDE.md's wall-clock rule
//! forbids. Where `now` is asserted at all it is asserted by **shape**, which is the
//! workspace's existing move for an unavoidable clock value.

#![cfg(unix)]

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};

use pantheon::mint::NewSpec;
use pantheon::plan_new;
use serde_json::Value;

static COUNTER: AtomicU32 = AtomicU32::new(0);

const CSA: &str = "c_contextus/c_s_societas/cs_a_amicitia";

/// Run the real `aus` with a fixture on its stdin.
fn aus_stdin(root: &Path, args: &[&str], stdin: Option<&str>) -> (i32, Value) {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_aus"));
    cmd.arg("-C")
        .arg(root)
        .args(args)
        .env_remove("PANTHEON_ROOT")
        .stdin(match stdin {
            // A null stdin is an immediate EOF; inheriting the runner's would hang.
            None => Stdio::null(),
            Some(_) => Stdio::piped(),
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = cmd.spawn().unwrap();
    if let Some(text) = stdin {
        use std::io::Write;
        child
            .stdin
            .take()
            .unwrap()
            .write_all(text.as_bytes())
            .unwrap();
    }
    let out = child.wait_with_output().unwrap();
    let code = out.status.code().unwrap_or(-1);
    let bytes = if out.stdout.is_empty() {
        out.stderr
    } else {
        out.stdout
    };
    (code, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
}

fn aus(root: &Path, args: &[&str]) -> (i32, Value) {
    aus_stdin(root, args, None)
}

fn fresh_root() -> PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("aus-propose-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    for (parent, ch, label) in [
        ("root", "c", "contextus"),
        ("c", "s", "societas"),
        ("cs", "a", "amicitia"),
    ] {
        let (plan, _) = plan_new(&dir, parent, NewSpec::Triple { ch, label }).unwrap();
        plan.apply(&dir).unwrap();
    }
    dir
}

/// A rule on disk, executable — which is what §9.1 means by a rule being run directly:
/// the shebang names the interpreter, so the file *is* the program.
fn rule(root: &Path, name: &str, body: &str) {
    use std::os::unix::fs::PermissionsExt;

    let meta = root.join(CSA).join("csa__");
    std::fs::create_dir_all(&meta).unwrap();
    let path = meta.join(format!("csa__function__{name}.sh"));
    std::fs::write(&path, body).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

/// The same, without the exec bit.
fn unrunnable_rule(root: &Path, name: &str, body: &str) {
    let meta = root.join(CSA).join("csa__");
    std::fs::create_dir_all(&meta).unwrap();
    std::fs::write(meta.join(format!("csa__function__{name}.sh")), body).unwrap();
}

const PROPOSES: &str = "#!/bin/sh\n# auspex: writes=pensum@csa:add\ncat > /dev/null\n\
     echo '{\"writes\":[{\"core\":\"pensum\",\"verb\":\"add\",\"home\":\"csa\",\
     \"name\":\"Reach out to Alex\"}]}'\n";

fn row<'a>(rules: &'a Value, name: &str) -> &'a Value {
    rules
        .as_array()
        .expect("plan emits an array of per-rule rows")
        .iter()
        .find(|r| r["rule"] == name)
        .unwrap_or_else(|| panic!("{name} is in the report: {rules}"))
}

// ── what a rule proposes (§9.3) ──────────────────────────────────────────────

#[test]
fn a_rule_proposes_and_auspex_reports_it() {
    let root = fresh_root();
    rule(&root, "proposes", PROPOSES);

    let (code, rules) = aus(&root, &["plan"]);
    assert_eq!(code, 0, "every rule ran");
    let writes = &row(&rules, "proposes")["writes"];
    assert_eq!(writes[0]["core"], "pensum");
    assert_eq!(writes[0]["verb"], "add");
    assert_eq!(
        writes[0]["name"], "Reach out to Alex",
        "a fresh add carries a name, never a key — Auspex normalizes it (§9.3)"
    );
}

/// Most wakes find nothing to say, so proposing nothing is the ordinary case and not a
/// failure. Empty and absent read the same (§9.5 step 1) — both are `writes: []`, a
/// real empty result rather than a missing key.
#[test]
fn a_rule_may_propose_nothing() {
    let root = fresh_root();
    rule(
        &root,
        "silent",
        "#!/bin/sh\n# auspex: writes=pensum@csa:add\nexit 0\n",
    );
    rule(
        &root,
        "empty_object",
        "#!/bin/sh\n# auspex: writes=pensum@csa:add\necho '{}'\n",
    );

    let (code, rules) = aus(&root, &["plan"]);
    assert_eq!(code, 0, "proposing nothing is success");
    assert_eq!(row(&rules, "silent")["writes"], serde_json::json!([]));
    assert_eq!(row(&rules, "empty_object")["writes"], serde_json::json!([]));
}

/// **Nothing is applied.** `aus plan` runs the rule and prints what it proposed; the
/// tree is untouched. This is what makes `plan` safe to run against a rule you have
/// not read yet, and it is why the capability check can wait for `run`.
#[test]
fn planning_writes_nothing_to_the_tree() {
    let root = fresh_root();
    rule(&root, "proposes", PROPOSES);
    let before = std::fs::read_dir(root.join(CSA).join("csa__"))
        .unwrap()
        .count();

    let (code, _) = aus(&root, &["plan"]);
    assert_eq!(code, 0);

    let after = std::fs::read_dir(root.join(CSA).join("csa__"))
        .unwrap()
        .count();
    assert_eq!(before, after, "no record was written (§9.6)");
    assert!(
        !root
            .join(CSA)
            .join("csa__")
            .join("csa__task.jsonl")
            .exists(),
        "the proposed task did not land — only `run` applies"
    );
}

// ── the child's environment (§9.3) ───────────────────────────────────────────

/// Both variables, asserted from **inside** a rule, which is the only place they can
/// be seen. `PANTHEON_ROOT` is the one that would otherwise fail silently: a rule
/// reads the tree through the core CLIs, and without it a rule under `aus -C` would
/// read whichever tree the ambient environment named.
#[test]
fn a_rule_runs_under_pantheon_rule_and_is_told_the_root() {
    let root = fresh_root();
    rule(
        &root,
        "echoenv",
        "#!/bin/sh\n# auspex: writes=pensum@csa:add\ncat > /dev/null\n\
         printf '{\"writes\":[{\"rule_env\":\"%s\",\"root_env\":\"%s\"}]}\\n' \
         \"$PANTHEON_RULE\" \"$PANTHEON_ROOT\"\n",
    );

    let (code, rules) = aus(&root, &["plan"]);
    assert_eq!(code, 0);
    let w = &row(&rules, "echoenv")["writes"][0];
    assert_eq!(
        w["rule_env"], "1",
        "every core refuses a write under this (§9.3)"
    );
    assert_eq!(
        w["root_env"],
        root.to_str().unwrap(),
        "the rule reads the tree `aus` was given, not the ambient one (§6.2)"
    );
}

/// A rule may not borrow a hand's authority (I2). The enforcement is the core's, and
/// it is already pinned in all seven — this asserts the *channel*: that a rule really
/// meets that refusal when it tries to write.
#[test]
fn a_rules_own_write_is_refused_by_the_core_it_calls() {
    let root = fresh_root();
    let pen = PathBuf::from(env!("CARGO_BIN_EXE_aus"))
        .parent()
        .unwrap()
        .join("pen");
    assert!(
        pen.exists(),
        "`pen` is not built; run `cargo build --workspace --bins` first"
    );
    rule(
        &root,
        "tries_to_write",
        &format!(
            "#!/bin/sh\n# auspex: writes=pensum@csa:add\ncat > /dev/null\n\
             {} -H csa sneaky -y >/dev/null 2>&1\n\
             printf '{{\"writes\":[{{\"core_said\":%s}}]}}\\n' \"$?\"\n",
            pen.display()
        ),
    );

    let (code, rules) = aus(&root, &["plan"]);
    assert_eq!(code, 0);
    assert_eq!(
        row(&rules, "tries_to_write")["writes"][0]["core_said"],
        6,
        "the core refused the rule's write with exit 6 (§9.3)"
    );
    assert!(
        !root
            .join(CSA)
            .join("csa__")
            .join("csa__task.jsonl")
            .exists(),
        "and nothing landed"
    );
}

// ── a rule that errors is skipped and reported (§9.5) ────────────────────────

#[test]
fn each_failure_mode_is_reported_against_its_own_rule() {
    let root = fresh_root();
    rule(&root, "proposes", PROPOSES);
    rule(
        &root,
        "angry",
        "#!/bin/sh\n# auspex: writes=pensum@csa:add\necho 'it broke' >&2\nexit 3\n",
    );
    rule(
        &root,
        "garbage",
        "#!/bin/sh\n# auspex: writes=pensum@csa:add\necho 'not json'\n",
    );
    unrunnable_rule(
        &root,
        "noexec",
        "#!/bin/sh\n# auspex: writes=pensum@csa:add\necho '{\"writes\":[]}'\n",
    );

    let (code, rules) = aus(&root, &["plan"]);
    assert_eq!(
        code, 1,
        "a command that met a broken rule does not claim success"
    );

    // The healthy rule is unaffected — that is the whole of §9.5's "others are
    // unaffected", and it is why one bad rule cannot silence a tree.
    assert_eq!(row(&rules, "proposes")["writes"][0]["core"], "pensum");

    let says = |name: &str| row(&rules, name)["error"].as_str().unwrap().to_string();
    assert!(says("angry").contains("exit 3"), "{}", says("angry"));
    assert!(
        says("angry").contains("it broke"),
        "the rule's own last word is quoted: {}",
        says("angry")
    );
    assert!(
        says("garbage").contains("did not emit JSON"),
        "{}",
        says("garbage")
    );
    assert!(
        says("noexec").contains("exec bit"),
        "the likely cause is named: {}",
        says("noexec")
    );
}

// ── `aus test` (§9.6) ────────────────────────────────────────────────────────

/// The fixture is handed to the rule unchanged, which is the point: drive a rule with
/// a `now` and a `trigger` you chose and assert on what comes back.
#[test]
fn test_hands_a_rule_the_fixture_from_stdin() {
    let root = fresh_root();
    rule(
        &root,
        "echoes",
        "#!/bin/sh\n# auspex: writes=pensum@csa:add\nCTX=$(cat)\n\
         printf '{\"writes\":[{\"got\":%s}]}\\n' \"$CTX\"\n",
    );

    let fixture = r#"{"sign":"hook","rule":"echoes","scope":"csa","now":"260101",
                      "trigger":{"core":"annales","home":"csa"}}"#;
    let (code, rules) = aus_stdin(&root, &["test", "echoes"], Some(fixture));
    assert_eq!(code, 0);

    let got = &row(&rules, "echoes")["writes"][0]["got"];
    assert_eq!(
        got["now"], "260101",
        "the fixture's own date reached the rule"
    );
    assert_eq!(got["sign"], "hook");
    assert_eq!(got["trigger"]["core"], "annales");
}

/// With nothing piped, the context `plan` would have built is synthesized rather than
/// blocking on a stdin that will never close (§7.3's TTY rule, I8). A blank fixture
/// counts as none: `aus test foo </dev/null` is a hand asking to run the rule.
#[test]
fn test_synthesizes_a_context_when_no_fixture_is_given() {
    let root = fresh_root();
    rule(
        &root,
        "echoes",
        "#!/bin/sh\n# auspex: writes=pensum@csa:add\nCTX=$(cat)\n\
         printf '{\"writes\":[{\"got\":%s}]}\\n' \"$CTX\"\n",
    );

    let (code, rules) = aus(&root, &["test", "echoes"]);
    assert_eq!(code, 0);

    let got = &row(&rules, "echoes")["writes"][0]["got"];
    assert_eq!(got["sign"], "manual", "a hand ran this, not a hook (§9.3)");
    assert_eq!(got["rule"], "echoes");
    assert_eq!(got["scope"], "csa");
    // `now` is today's date by construction, so its **shape** is what can be asserted;
    // the value would make this test fail tomorrow.
    let now = got["now"].as_str().expect("now is a string");
    assert_eq!(now.len(), 6, "YYMMDD (§9.3): {now}");
    assert!(
        now.chars().all(|c| c.is_ascii_digit()),
        "digits only: {now}"
    );
    assert!(
        got.get("trigger").is_none(),
        "no single write authored this wake, so there is no trigger to name (§9.3)"
    );
}

#[test]
fn test_refuses_to_guess_between_two_rules_of_one_name() {
    let root = fresh_root();
    rule(&root, "twin", PROPOSES);
    // The same rule name at a second node — legal, since a name is unique per node.
    let meta = root.join("c_contextus").join("c_s_societas").join("cs__");
    std::fs::create_dir_all(&meta).unwrap();
    std::fs::write(meta.join("cs__function__twin.sh"), PROPOSES).unwrap();

    let (code, err) = aus(&root, &["test", "twin"]);
    assert_eq!(
        code, 2,
        "ambiguity is listed and refused, never guessed (§7.3)"
    );
    let msg = err["error"]["msg"].as_str().unwrap();
    assert!(msg.contains("cs"), "the candidates are named: {msg}");
    assert!(msg.contains("csa"), "both of them: {msg}");
}

#[test]
fn test_on_a_rule_that_is_not_there_is_not_found() {
    let root = fresh_root();
    let (code, err) = aus(&root, &["test", "nosuch"]);
    assert_eq!(code, 4);
    assert_eq!(err["error"]["code"], 4);
}

// ── scope (§9.1) ─────────────────────────────────────────────────────────────

#[test]
fn planning_narrows_to_a_scope() {
    let root = fresh_root();
    rule(&root, "here", PROPOSES);
    let meta = root.join("c_contextus").join("c__");
    std::fs::create_dir_all(&meta).unwrap();
    std::fs::write(meta.join("c__function__elsewhere.sh"), PROPOSES).unwrap();

    let (code, rules) = aus(&root, &["plan", "csa"]);
    assert_eq!(
        code, 0,
        "the out-of-scope rule is not run, so its exec bit never matters"
    );
    assert_eq!(
        rules.as_array().unwrap().len(),
        1,
        "only the rule scoped under csa evaluated: {rules}"
    );
    assert_eq!(rules[0]["rule"], "here");
}
