//! The frozen contract (§7.2): insta snapshots of `pen`'s own JSON — the only thing
//! that crosses a component boundary (I4), so these are taken from the real binary
//! rather than the library behind it.
//!
//! Plan tokens are redacted (they hash the computed change). Keys are **not**: a
//! task's key is its identity and its name at once (§5.4). Nothing here depends on
//! the wall clock — every `--done` is given an explicit date, which is the price of
//! `done` carrying one. Regenerate these deliberately, never blindly.

// These tests build human-readable snapshot text; `push_str(&format!(...))` reads
// clearest here and the allocation is irrelevant in a test.
#![allow(clippy::format_push_string)]

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU32, Ordering};

use serde_json::{Value, json};

use pantheon::mint::NewSpec;
use pantheon::plan_new;

static COUNTER: AtomicU32 = AtomicU32::new(0);

/// A tree with three nodes to file tasks at: `acm` (Mentis, under Cura) and `ao`
/// (Opus), plus their parents. A task homes where the *doing* lives (§8.5).
fn fresh_root() -> PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("pen-snap-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    for (parent, ch, label) in [
        ("root", "a", "actio"),
        ("a", "c", "cura"),
        ("ac", "m", "mentis"),
        ("a", "o", "opus"),
    ] {
        let (plan, _) = plan_new(&dir, parent, NewSpec::Triple { ch, label }).unwrap();
        plan.apply(&dir).unwrap();
    }
    dir
}

/// Run the real `pen`, returning its exit code and the JSON it emitted — stdout when
/// it produced a value, stderr for the `{"error":…}` envelope (§7.3).
fn pen(root: &Path, args: &[&str]) -> (i32, Value) {
    pen_env(root, args, &[]).0
}

/// [`pen`] with environment set for the child — never for this process (§7.3).
/// Returns stderr separately: a soft finding rides there while the record itself
/// goes to stdout (§5.4).
fn pen_env(root: &Path, args: &[&str], env: &[(&str, &str)]) -> ((i32, Value), String) {
    pen_full(root, args, env, None)
}

/// [`pen`] run from *inside* the tree, which is what makes the locus resolve: with
/// no home token the tool walks up from `$PWD` to the nearest node (§7.3).
fn pen_at(root: &Path, cwd: &Path, args: &[&str]) -> (i32, Value) {
    pen_full(root, args, &[], Some(cwd)).0
}

fn pen_full(
    root: &Path,
    args: &[&str],
    env: &[(&str, &str)],
    working_dir: Option<&Path>,
) -> ((i32, Value), String) {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_pen"));
    if let Some(working_dir) = working_dir {
        cmd.current_dir(working_dir);
    }
    cmd.arg("-C")
        .arg(root)
        .args(args)
        .env_remove("PANTHEON_ROOT");
    for (key, value) in env {
        cmd.env(key, value);
    }
    let out = cmd.output().unwrap();
    let code = out.status.code().unwrap_or(-1);
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    let bytes = if out.stdout.is_empty() {
        out.stderr
    } else {
        out.stdout
    };
    (
        (code, serde_json::from_slice(&bytes).unwrap_or(Value::Null)),
        stderr,
    )
}

/// The plan token hashes the computed change; redact it before freezing (§7.3).
fn redact(mut value: Value) -> Value {
    if value.get("token").is_some() {
        value["token"] = json!("[redacted]");
    }
    value
}

fn pretty(value: &Value) -> String {
    serde_json::to_string_pretty(value).unwrap()
}

/// A node's register file, whether or not it exists.
fn register(root: &Path, code: &str) -> PathBuf {
    let node = match code {
        "acm" => root.join("a_actio/a_c_cura/ac_m_mentis"),
        "ac" => root.join("a_actio/a_c_cura"),
        "ao" => root.join("a_actio/a_o_opus"),
        other => panic!("no node dir mapped for {other}"),
    };
    node.join(format!("{code}__"))
        .join(format!("{code}__task.jsonl"))
}

// ── the shape of the core ───────────────────────────────────────────────────

#[test]
fn schema_surface() {
    let (code, value) = pen(&fresh_root(), &["schema"]);
    assert_eq!(code, 0);
    // The first `"named": false` in the workspace: the bit that says this file's
    // name slot carries no identity, and its lines' keys carry them instead (§7.1).
    insta::assert_snapshot!("schema_surface", pretty(&value));
}

// ── writing ─────────────────────────────────────────────────────────────────

#[test]
fn the_first_task_mints_the_register_without_c() {
    let root = fresh_root();
    assert!(!register(&root, "acm").exists());

    let (code, value) = pen(&root, &["acm", "reach_out_to_alex", "-r", "album:alex"]);
    assert_eq!(code, 0);
    assert!(
        register(&root, "acm").exists(),
        "a node's first task mints its register (§7.3, §8.5)"
    );
    insta::assert_snapshot!("add_mints_the_register", pretty(&value));
}

#[test]
fn a_second_add_on_one_key_is_an_overwrite_and_confirms() {
    let root = fresh_root();
    // A fresh key runs free: a new record can't destroy an existing one (§7.3).
    assert_eq!(pen(&root, &["acm", "buy_milk"]).0, 0);

    // The same key again is a mutation: piped, exit 5 with the change to show.
    let (code, pending) = pen(&root, &["acm", "buy_milk", "the 2% one"]);
    assert_eq!(code, 5);
    // And with `-y` it applies.
    assert_eq!(pen(&root, &["acm", "buy_milk", "the 2% one", "-y"]).0, 0);

    insta::assert_snapshot!("add_overwrite_pending", pretty(&redact(pending)));
}

#[test]
fn dry_run_emits_a_plan_and_mints_nothing() {
    let root = fresh_root();
    let (code, value) = pen(&root, &["acm", "reach_out_to_alex", "-n"]);
    assert_eq!(code, 0);
    assert!(
        !register(&root, "acm").exists(),
        "a plan that left a file behind would not be one (§7.2)"
    );
    insta::assert_snapshot!("add_dry_run", pretty(&redact(value)));
}

#[test]
fn edit_marks_a_task_done_and_undone() {
    let root = fresh_root();
    pen(&root, &["acm", "buy_milk", "the 2% one"]);

    let (code, done) = pen(&root, &["edit", "buy_milk", "--done", "260719", "-y"]);
    assert_eq!(code, 0);
    // What a hand did not give, the task keeps (I1) — the note survives.
    let (_, undone) = pen(&root, &["edit", "buy_milk", "--undone", "-y"]);

    let mut out = String::new();
    out.push_str(&format!("done:   {}\n", pretty(&done)));
    out.push_str(&format!("undone: {}\n", pretty(&undone)));
    insta::assert_snapshot!("edit_done_then_undone", out);
}

#[test]
fn rename_moves_the_key_and_cascades_every_ref() {
    let root = fresh_root();
    pen(&root, &["acm", "reach_out_to_alex"]);
    // Another node's task pointing at it — the edge §8.5 says a task is reached by.
    pen(
        &root,
        &["ao", "chase_it_up", "-r", "pensum:reach_out_to_alex"],
    );

    let (code, value) = pen(&root, &["rename", "reach_out_to_alex", "ring_alex", "-y"]);
    assert_eq!(code, 0);
    let follower = std::fs::read_to_string(register(&root, "ao")).unwrap();
    assert!(
        follower.contains("pensum:ring_alex"),
        "the ref followed the rename (§5.4): {follower}"
    );
    insta::assert_snapshot!("rename_cascades", pretty(&value));
}

#[test]
fn rename_refuses_a_key_another_task_already_holds() {
    let root = fresh_root();
    pen(&root, &["acm", "reach_out_to_alex"]);
    pen(&root, &["acm", "call_alex"]);
    pen(&root, &["ao", "email_alex"]);

    let mut out = String::new();
    // Within one register, and across nodes: the check is tree-wide (§7.2).
    for onto in ["call_alex", "email_alex"] {
        let (code, value) = pen(&root, &["rename", "reach_out_to_alex", onto, "-y"]);
        out.push_str(&format!("onto {onto} => exit {code}: {value}\n"));
    }
    insta::assert_snapshot!("rename_refuses_occupied_key", out);
}

#[test]
fn move_relocates_the_line_between_registers() {
    let root = fresh_root();
    pen(&root, &["acm", "reach_out_to_alex"]);
    pen(&root, &["acm", "buy_milk"]);

    let (code, value) = pen(&root, &["mv", "reach_out_to_alex", "--to", "ao", "-y"]);
    assert_eq!(code, 0);
    // A line moved between two files, not a file renamed (§7.2, §6.4).
    let source = std::fs::read_to_string(register(&root, "acm")).unwrap();
    let dest = std::fs::read_to_string(register(&root, "ao")).unwrap();
    assert!(!source.contains("reach_out_to_alex"), "{source}");
    assert!(dest.contains("reach_out_to_alex"), "{dest}");
    assert!(
        source.contains("buy_milk"),
        "the line it keeps is untouched"
    );
    insta::assert_snapshot!("move_relocates", pretty(&value));
}

#[test]
fn rm_drops_one_line() {
    let root = fresh_root();
    pen(&root, &["acm", "buy_milk"]);
    pen(&root, &["acm", "file_taxes"]);
    let (code, value) = pen(&root, &["rm", "buy_milk", "-y"]);
    assert_eq!(code, 0);
    let left = std::fs::read_to_string(register(&root, "acm")).unwrap();
    assert!(!left.contains("buy_milk") && left.contains("file_taxes"));
    insta::assert_snapshot!("rm", pretty(&value));
}

// ── reading ─────────────────────────────────────────────────────────────────

#[test]
fn the_read_verbs() {
    let root = fresh_root();
    pen(&root, &["acm", "reach_out_to_alex", "-r", "album:alex"]);
    pen(&root, &["acm", "buy_milk", "the 2% one"]);
    pen(&root, &["ao", "file_taxes"]);
    pen(&root, &["edit", "buy_milk", "--done", "260719", "-y"]);

    let mut out = String::new();
    for (label, args) in [
        // A fold across the subtree: every task is its own present (I1, §5.4),
        // minus the done ones — a task list is what is not yet done.
        ("list -H a", &["list", "-H", "a"][..]),
        ("list -H a --all", &["list", "-H", "a", "--all"][..]),
        // One node's register, read whole: done and open alike.
        ("series acm", &["series", "acm"][..]),
        // One task, found by its key anywhere in the tree (§5.4).
        ("get buy_milk", &["get", "buy_milk"][..]),
        ("where file_taxes", &["where", "file_taxes"][..]),
    ] {
        let (code, value) = pen(&root, args);
        out.push_str(&format!("$ pen {}\n  exit {code}\n", args.join(" ")));
        out.push_str(&format!("{}\n\n", pretty(&value)));
        let _ = label;
    }
    insta::assert_snapshot!("read_verbs", out);
}

#[test]
fn the_leading_token_is_probed_for_a_node_code() {
    let root = fresh_root();
    // Run from `ac`, so the readings that fall through to the locus have one and a
    // wrong probe shows as the wrong home rather than as an error (§7.3).
    let cwd = root.join("a_actio/a_c_cura");
    let mut out = String::new();
    // §7.3's rule, all five readings. `-H` forces either direction, because the
    // probe never runs once a home is stated.
    for args in [
        &["ao", "file_taxes"][..],
        &["book_flights", "aisle seat"][..],
        &["acm", "call_bank", "about the fee"][..],
        &["ring_alex"][..],
        &["-H", "ao", "ao", "a task really named ao"][..],
    ] {
        let (code, value) = pen_at(&root, &cwd, args);
        out.push_str(&format!(
            "$ pen {}\n  exit {code}  home={} key={} data={}\n",
            args.join(" "),
            value["home"],
            value["key"],
            value["data"],
        ));
    }
    insta::assert_snapshot!("leading_token_probe", out);
}

// ── the rules that bound a write ────────────────────────────────────────────

#[test]
fn a_cross_node_duplicate_key_warns_softly() {
    let root = fresh_root();
    pen(&root, &["acm", "call_bank"]);
    // The same key at another node: the record still lands, the warning rides
    // stderr in `pan validate`'s own shape (§5.4, §18).
    let ((code, value), stderr) = pen_env(&root, &["ao", "call_bank"], &[]);
    assert_eq!(code, 0);

    let mut out = String::new();
    out.push_str(&format!("stdout: {}\n", pretty(&value)));
    out.push_str(&format!(
        "stderr: {}",
        pretty(&serde_json::from_str::<Value>(stderr.trim()).unwrap())
    ));
    insta::assert_snapshot!("duplicate_key_warns", out);
}

#[test]
fn the_editor_form_piped_prints_a_path_and_spawns_nothing() {
    let root = fresh_root();
    pen(&root, &["acm", "buy_milk"]);
    // An `edit` given no new value, with stdout a pipe: it spawns nothing and hands
    // back the file's path, so the LLM hand opens it with its own tools rather than
    // a process it cannot drive. `$EDITOR` is set to prove nothing is run (§7.3).
    let ((code, value), _) = pen_env(
        &root,
        &["edit", "buy_milk"],
        &[("EDITOR", "false"), ("VISUAL", "false")],
    );
    assert_eq!(code, 0, "the editor form is not a failure (§7.3)");
    // Not snapshotted: the path is absolute and the root is a fresh temp dir, so
    // there is nothing stable to freeze. What matters is which file it names.
    let path = value["path"].as_str().expect("a path (§7.3)");
    assert!(path.ends_with("acm__task.jsonl"), "got {path}");

    // Nothing was written: the task is untouched.
    let (_, present) = pen(&root, &["get", "buy_milk"]);
    assert_eq!(present["data"], json!({}));
}

#[test]
fn write_verbs_are_refused_under_a_rule() {
    let root = fresh_root();
    pen(&root, &["acm", "buy_milk"]);
    let rule = [("PANTHEON_RULE", "1")];

    let mut out = String::new();
    for args in [
        &["acm", "file_taxes"][..],
        &["edit", "buy_milk", "--done", "260719", "-y"][..],
        &["rename", "buy_milk", "get_milk", "-y"][..],
        &["mv", "buy_milk", "--to", "ao", "-y"][..],
        &["rm", "buy_milk", "-y"][..],
        // Reads run free, and so does a dry run: a rule may still plan (§9.3).
        &["list", "-H", "a"][..],
        &["acm", "file_taxes", "-n"][..],
    ] {
        let ((code, _), _) = pen_env(&root, args, &rule);
        out.push_str(&format!("exit {code}: pen {}\n", args.join(" ")));
    }
    insta::assert_snapshot!("refused_under_a_rule", out);
}

#[test]
fn the_on_disk_record_stores_no_location() {
    let root = fresh_root();
    pen(&root, &["acm", "reach_out_to_alex", "-r", "album:alex"]);
    pen(&root, &["acm", "buy_milk", "the 2% one"]);
    pen(&root, &["edit", "buy_milk", "--done", "260719", "-y"]);
    // A record's home, core, and kind are its file's location and name (I3), so
    // none of them appears inside — and the filename carries no series name.
    let text = std::fs::read_to_string(register(&root, "acm")).unwrap();
    insta::assert_snapshot!("on_disk_register", text);
}

#[test]
fn exit_codes() {
    let root = fresh_root();
    pen(&root, &["acm", "buy_milk"]);
    pen(&root, &["ao", "buy_milk"]);

    let cases: &[(&str, &[&str])] = &[
        (
            "a determined series is minted by its determinant",
            &["acm", "x", "-c"],
        ),
        (
            "a task keys by its name, not a date",
            &["acm", "x", "-a", "260719"],
        ),
        (
            "pensum's register is nameless",
            &["series", "acm", "weight"],
        ),
        ("a token this core does not own", &["list", "-k", "log"]),
        ("no task by that key", &["get", "never_written"]),
        ("no register at that node yet", &["series", "ac"]),
        (
            "one key at two nodes is listed, not guessed",
            &["get", "buy_milk"],
        ),
        ("naming nothing", &["get"]),
        (
            "a mutation piped without -y",
            &["rm", "-H", "acm", "buy_milk"],
        ),
    ];

    let mut out = String::new();
    for (why, args) in cases {
        let (code, value) = pen(&root, args);
        // Exit 5 is not a failure: it emits the change for a caller to show and
        // re-run with `-y`, so it carries a plan rather than an error (§7.3).
        let msg = value
            .pointer("/error/msg")
            .and_then(Value::as_str)
            .map_or_else(
                || {
                    format!(
                        "(a {} plan awaiting -y, not an error)",
                        value["plan"].as_str().unwrap_or("?")
                    )
                },
                str::to_string,
            );
        out.push_str(&format!(
            "pen {}\n  # {why}\n  => exit {code}: {msg}\n",
            args.join(" ")
        ));
    }
    insta::assert_snapshot!("exit_codes", out);
}

// ── the cross-binary gate ───────────────────────────────────────────────────

/// The single test that proves the whole `named: false` path end to end.
///
/// For Album this gate showed the resolver's *filename* path. Pensum's is stronger:
/// a task's key lives inside its series, so this is the one resolution in the system
/// that opens record files rather than resting on their names (§5.0).
#[test]
fn tasks_resolve_through_pan() {
    let root = fresh_root();
    pen(&root, &["acm", "reach_out_to_alex", "-r", "album:alex"]);
    pen(&root, &["ao", "file_taxes"]);
    // A date-keyed line hand-written into the same register: a sample, not an
    // identity, so it must resolve to nothing (I1, §5.4).
    let mut text = std::fs::read_to_string(register(&root, "ao")).unwrap();
    text.push_str("{\"key\":\"260718\",\"refs\":[],\"data\":{}}\n");
    std::fs::write(register(&root, "ao"), text).unwrap();

    let self_bin = PathBuf::from(env!("CARGO_BIN_EXE_pen"));
    let bin_dir = self_bin.parent().unwrap();
    let pan_bin = bin_dir.join("pan");
    // Cargo builds `pen` for this test (CARGO_BIN_EXE_pen) but NOT `pan`, which
    // belongs to another crate — so the workspace bins must be built first. Failing
    // loudly beats skipping: a gate that quietly passes is not a gate.
    assert!(
        pan_bin.exists(),
        "this gate needs `pan` built beside `pen`: run \
         `cargo build --workspace --bins` (looked in {})",
        bin_dir.display()
    );

    // Prepend the bin dir to PATH so `pan` discovers Pensum by running `pen schema`
    // (§5.0, I5) — never by linking it.
    let path = match std::env::var_os("PATH") {
        Some(existing) => {
            let mut dirs = vec![bin_dir.to_path_buf()];
            dirs.extend(std::env::split_paths(&existing));
            std::env::join_paths(dirs).unwrap()
        }
        None => bin_dir.as_os_str().to_owned(),
    };
    let out = Command::new(&pan_bin)
        .arg("-C")
        .arg(&root)
        .args([
            "resolve",
            "pensum:reach_out_to_alex",
            "pensum:file_taxes",
            "pensum:260718",
            "pensum:never_written",
        ])
        .env("PATH", path)
        .env_remove("PANTHEON_ROOT")
        .output()
        .unwrap();
    // Any unresolved ref exits 4 — `pensum:260718` and `pensum:never_written` are.
    assert_eq!(
        out.status.code(),
        Some(4),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    insta::assert_snapshot!(
        "resolve_through_pan",
        pretty(&serde_json::from_slice::<Value>(&out.stdout).unwrap())
    );
}

/// The file a detached hook and a hand contend for (§6.4, §16 step 4) — here as
/// whole processes, which is how it actually happens.
#[test]
fn concurrent_adds_at_one_node_all_land() {
    let root = fresh_root();
    assert!(
        !register(&root, "acm").exists(),
        "every writer races to mint it"
    );

    let mut children: Vec<_> = (0..8)
        .map(|i| {
            let mut cmd = Command::new(env!("CARGO_BIN_EXE_pen"));
            cmd.arg("-C")
                .arg(&root)
                .args(["acm", &format!("task_{i}")])
                .env_remove("PANTHEON_ROOT");
            cmd.spawn().unwrap()
        })
        .collect();
    for child in &mut children {
        assert!(child.wait().unwrap().success());
    }

    let text = std::fs::read_to_string(register(&root, "acm")).unwrap();
    let keys: std::collections::HashSet<&str> =
        text.lines().filter(|l| !l.trim().is_empty()).collect();
    assert_eq!(keys.len(), 8, "every writer's task must survive:\n{text}");
}
