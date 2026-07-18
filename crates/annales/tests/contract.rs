//! The frozen contract (§7.2): insta snapshots of `ann`'s own JSON — the only thing
//! that crosses a component boundary (I4), so these are taken from the real binary
//! rather than the library behind it.
//!
//! Plan tokens are redacted (they hash the computed change). Date keys are **not**:
//! a key is the reading's identity, stable across an edit (§5.4). Every write passes
//! an explicit `-a`, so no snapshot depends on the wall clock. Regenerate these
//! deliberately, never blindly.

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

/// A tree with one node to log at: `e` → `ec` → `ecv` (§5.1).
fn fresh_root() -> PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("ann-snap-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    for (parent, ch, label) in [
        ("root", "e", "ego"),
        ("e", "c", "corpus"),
        ("ec", "v", "valetudo"),
    ] {
        let (plan, _) = plan_new(&dir, parent, NewSpec::Triple { ch, label }).unwrap();
        plan.apply(&dir).unwrap();
    }
    dir
}

/// Run the real `ann`, returning its exit code and the JSON it emitted — stdout when
/// it produced a value, stderr for the `{"error":…}` envelope (§7.3).
fn ann(root: &Path, args: &[&str]) -> (i32, Value) {
    ann_env(root, args, &[])
}

/// [`ann`] with environment set for the child — never for this process (§7.3).
fn ann_env(root: &Path, args: &[&str], env: &[(&str, &str)]) -> (i32, Value) {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_ann"));
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

// ── the discovery surface (§7.2) ────────────────────────────────────────────

#[test]
fn schema_surface() {
    let root = fresh_root();
    let (code, schema) = ann(&root, &["schema"]);
    assert_eq!(code, 0);
    insta::assert_snapshot!("schema_surface", pretty(&schema));
}

// ── the write verbs (§7.2, §7.3) ────────────────────────────────────────────

#[test]
fn verb_add_fresh_then_overwrite() {
    let root = fresh_root();
    // A hand-named series is minted explicitly; a plain `add` never conjures one.
    let (code, created) = ann(&root, &["ecv", "weight", "-c"]);
    assert_eq!(code, 0);
    insta::assert_snapshot!("verb_add_created_series", pretty(&created));

    // A fresh key runs free (§7.3).
    let (code, fresh) = ann(&root, &["ecv", "weight", "78.4", "-a", "260718"]);
    assert_eq!(code, 0);
    insta::assert_snapshot!("verb_add_fresh", pretty(&fresh));

    // A second reading on the same key is an overwrite — a mutation. Piped and
    // without `-y`, it exits 5 and prints the change for the caller to review.
    let (code, pending) = ann(&root, &["ecv", "weight", "78.9", "-a", "260718"]);
    assert_eq!(code, 5, "an overwrite must stop at the checkpoint (§7.3)");
    insta::assert_snapshot!("verb_add_overwrite_pending", pretty(&redact(pending)));

    // Re-run with `-y` and it commits.
    let (code, applied) = ann(&root, &["ecv", "weight", "78.9", "-a", "260718", "-y"]);
    assert_eq!(code, 0);
    insta::assert_snapshot!("verb_add_overwrite_applied", pretty(&applied));
}

#[test]
fn verb_edit_keeps_the_key() {
    let root = fresh_root();
    ann(&root, &["ecv", "weight", "-c"]);
    ann(&root, &["ecv", "weight", "78.4", "-a", "260718"]);

    // A correction rewrites the keyed line in place; the date key is the reading's
    // own identity and does not move (I1, §5.4).
    let (code, edited) = ann(
        &root,
        &["edit", "260718", "79.0", "--series", "weight", "-y"],
    );
    assert_eq!(code, 0);
    insta::assert_snapshot!("verb_edit", pretty(&edited));

    // One line, not two: a correction never stacks a second (I1).
    let (_, whole) = ann(&root, &["series", "weight"]);
    assert_eq!(whole.as_array().map(Vec::len), Some(1));
}

#[test]
fn verb_rm() {
    let root = fresh_root();
    ann(&root, &["ecv", "weight", "-c"]);
    ann(&root, &["ecv", "weight", "78.4", "-a", "260718"]);
    let (code, deleted) = ann(&root, &["rm", "260718", "--series", "weight", "-y"]);
    assert_eq!(code, 0);
    insta::assert_snapshot!("verb_rm", pretty(&deleted));
}

#[test]
fn dry_run_emits_a_plan_and_writes_nothing() {
    let root = fresh_root();
    ann(&root, &["ecv", "weight", "-c"]);
    let (code, plan) = ann(&root, &["ecv", "weight", "78.4", "-a", "260718", "-n"]);
    assert_eq!(code, 0, "--dry-run is not a failure");
    insta::assert_snapshot!("verb_add_dry_run", pretty(&redact(plan)));

    // Nothing was written: the series is still empty.
    let (_, whole) = ann(&root, &["series", "weight"]);
    assert_eq!(whole.as_array().map(Vec::len), Some(0));
}

// ── the read verbs (§7.2) ───────────────────────────────────────────────────

#[test]
fn verbs_list_get_series() {
    let root = fresh_root();
    ann(&root, &["ecv", "weight", "-c"]);
    ann(&root, &["ecv", "places", "-c"]);
    ann(&root, &["ecv", "weight", "78.4", "-a", "260718"]);
    ann(&root, &["ecv", "weight", "79.1", "-a", "260719"]);
    // A fact carried entirely by its references is still a fact (§8.6, I9).
    ann(
        &root,
        &["ecv", "places", "-a", "260718", "-r", "mappa:office"],
    );

    let mut out = String::new();
    // `series` is the whole collection — the trend across keys.
    let (_, whole) = ann(&root, &["series", "weight"]);
    out.push_str(&format!("series weight:\n{}\n", pretty(&whole)));
    // A window is a filter on it, never a second verb.
    let (_, windowed) = ann(&root, &["series", "weight", "--from", "260719"]);
    out.push_str(&format!(
        "series weight --from 260719:\n{}\n",
        pretty(&windowed)
    ));
    // `get` is the present: the reading at the latest key (I1).
    let (_, present) = ann(&root, &["get", "weight"]);
    out.push_str(&format!("get weight:\n{}\n", pretty(&present)));
    // `list` folds every log in the subtree to its present.
    let (_, folded) = ann(&root, &["list"]);
    out.push_str(&format!("list:\n{}\n", pretty(&folded)));
    // `where` resolves a log to its home by walking Annales' own files.
    let (_, located) = ann(&root, &["where", "weight"]);
    out.push_str(&format!("where weight:\n{}\n", pretty(&located)));

    insta::assert_snapshot!("verbs_read", out);
}

// ── the editor follows the hand too (§7.3, I8) ──────────────────────────────

#[test]
fn the_editor_form_piped_prints_a_path_and_spawns_nothing() {
    let root = fresh_root();
    ann(&root, &["ecv", "weight", "-c"]);
    ann(&root, &["ecv", "weight", "78.4", "-a", "260718"]);

    // An `edit` given no new value, with stdout a pipe: it spawns nothing and hands
    // back the file's path, so the LLM hand opens it with its own tools rather than
    // a process it cannot drive. `$EDITOR` is set to prove nothing is run.
    let (code, out) = ann_env(
        &root,
        &["edit", "260718", "--series", "weight"],
        &[("EDITOR", "false"), ("VISUAL", "false")],
    );
    assert_eq!(code, 0, "the editor form is not a failure (§7.3)");
    let path = out["path"].as_str().expect("a path (§7.3)");
    assert!(path.ends_with("ecv__log__weight.jsonl"), "got {path}");

    // Nothing was written: the reading still reads 78.4.
    let (_, present) = ann(&root, &["get", "weight"]);
    assert_eq!(present["data"]["values"][0], "78.4");
}

// ── a rule may not borrow a hand's authority (I2, §9.3) ─────────────────────

#[test]
fn write_verbs_are_refused_under_a_rule() {
    let root = fresh_root();
    ann(&root, &["ecv", "weight", "-c"]);
    ann(&root, &["ecv", "weight", "78.4", "-a", "260718"]);

    let rule = [("PANTHEON_RULE", "1")];
    for args in [
        &["ecv", "weight", "79.0", "-a", "260719"][..],
        &["edit", "260718", "79.0", "--series", "weight", "-y"][..],
        &["rm", "260718", "--series", "weight", "-y"][..],
    ] {
        let (code, out) = ann_env(&root, args, &rule);
        assert_eq!(
            code, 6,
            "a write verb under a rule is refused: ann {args:?}"
        );
        assert_eq!(out["error"]["code"], 6);
    }

    // Reads run free under a rule — a rule that wants a value uses `get` (§9.3).
    let (code, present) = ann_env(&root, &["get", "weight"], &rule);
    assert_eq!(code, 0);
    assert_eq!(present["data"]["values"][0], "78.4");

    // And `--dry-run` still computes, since it writes nothing (§7.3).
    let (code, _) = ann_env(
        &root,
        &["ecv", "weight", "79.0", "-a", "260719", "-n"],
        &rule,
    );
    assert_eq!(code, 0);
}

// ── the structural verbs, over the spine's cascade (§7.2, §5.4) ─────────────

#[test]
fn verb_rename_cascades_refs() {
    let root = fresh_root();
    ann(&root, &["ecv", "wieght", "-c"]);
    ann(&root, &["ecv", "wieght", "78.4", "-a", "260718"]);
    // A referrer written by hand rather than by a core: the cascade is the spine's,
    // and it rewrites whatever record points at the name — including, in the real
    // tree, another core's (§5.4, I5).
    let referrer = root.join("e_ego/e_c_corpus/ec_v_valetudo/ecv__/ecv__log__mood.jsonl");
    std::fs::write(
        &referrer,
        "{\"key\":\"260718\",\"refs\":[\"annales:wieght\"],\"data\":{\"values\":[\"ok\"]}}\n",
    )
    .unwrap();

    let (code, renamed) = ann(&root, &["rename", "wieght", "weight", "-y"]);
    assert_eq!(code, 0);
    insta::assert_snapshot!("verb_rename", pretty(&renamed));

    // The ref followed, and the line beside it is untouched.
    let after = std::fs::read_to_string(&referrer).unwrap();
    assert!(after.contains("annales:weight"), "{after}");
    assert!(after.contains(r#""values":["ok"]"#), "{after}");
    // The readings survived the file rename.
    let (code, series) = ann(&root, &["series", "weight"]);
    assert_eq!(code, 0);
    assert_eq!(series[0]["data"]["values"], json!(["78.4"]));
}

#[test]
fn verb_move_rehomes_without_touching_refs() {
    let root = fresh_root();
    ann(&root, &["ecv", "weight", "-c"]);
    ann(&root, &["ecv", "weight", "78.4", "-a", "260718"]);

    let (code, moved) = ann(&root, &["move", "weight", "--to", "ec", "-y"]);
    assert_eq!(code, 0);
    insta::assert_snapshot!("verb_move", pretty(&moved));
    // The file wears its new home's code (§5.2).
    assert!(
        root.join("e_ego/e_c_corpus/ec__/ec__log__weight.jsonl")
            .exists()
    );
    assert!(
        !root
            .join("e_ego/e_c_corpus/ec_v_valetudo/ecv__/ecv__log__weight.jsonl")
            .exists()
    );
}

// ── the path is the home (I3) ───────────────────────────────────────────────

#[test]
fn on_disk_envelope_stores_no_location() {
    let root = fresh_root();
    ann(&root, &["ecv", "weight", "-c"]);
    ann(&root, &["ecv", "weight", "78.4", "-a", "260718"]);
    ann(
        &root,
        &[
            "ecv",
            "weight",
            "79.1",
            "-a",
            "260719",
            "--note",
            "after a run",
        ],
    );

    let file = root.join("e_ego/e_c_corpus/ec_v_valetudo/ecv__/ecv__log__weight.jsonl");
    let on_disk = std::fs::read_to_string(&file).unwrap();
    // The record carries its key, its refs, and its data — and nothing about where
    // it lives: home, core, kind, and series are the file's location and name (I3).
    insta::assert_snapshot!("on_disk_series_file", on_disk);
}

// ── exit codes are contract (§7.3) ──────────────────────────────────────────

#[test]
fn exit_codes() {
    // A node holding exactly one series: inference has a single answer.
    let one = fresh_root();
    ann(&one, &["ecv", "weight", "-c"]);
    ann(&one, &["ecv", "weight", "78.4", "-a", "260718"]);

    // A node holding two: inference must list them and stop, never guess (§7.3).
    let two = fresh_root();
    ann(&two, &["ecv", "weight", "-c"]);
    ann(&two, &["ecv", "places", "-c"]);

    let cases: &[(&PathBuf, &str, &[&str])] = &[
        // A typo names a series that isn't there — it is not a new log (§7.3).
        (
            &one,
            "a typo cannot conjure a series",
            &["ecv", "wieght", "1", "-a", "260718"],
        ),
        (
            &one,
            "no such series, tree-wide",
            &["nosuch", "1", "-a", "260718"],
        ),
        (
            &one,
            "-c is refused on an inference form",
            &["weight", "1", "-c"],
        ),
        (
            &one,
            "no line at that key",
            &["rm", "999999", "--series", "weight", "-y"],
        ),
        (
            &one,
            "a token this core does not own",
            &["list", "-k", "task"],
        ),
        (
            &one,
            "a blank reading value",
            &["ecv", "weight", "  ", "-a", "260722"],
        ),
        // A second reading on a key that exists is an overwrite awaiting review.
        (
            &one,
            "an overwrite, piped, without -y",
            &["ecv", "weight", "1", "-a", "260718"],
        ),
        (
            &two,
            "two series at the node: ambiguous",
            &["ecv", "1", "-a", "260718"],
        ),
        (
            &one,
            "a malformed --at",
            &["ecv", "weight", "1", "-a", "sometime"],
        ),
        (
            &one,
            "a malformed reference",
            &["ecv", "weight", "1", "-a", "260723", "-r", "office"],
        ),
    ];

    let mut out = String::new();
    for (root, why, args) in cases {
        let (code, value) = ann(root, args);
        let outcome = value
            .pointer("/error/msg")
            .and_then(Value::as_str)
            .map_or_else(
                || {
                    format!(
                        "a pending {} awaiting review",
                        value["plan"].as_str().unwrap_or("change")
                    )
                },
                str::to_string,
            );
        out.push_str(&format!(
            "ann {}\n  # {why}\n  => exit {code}: {outcome}\n",
            args.join(" ")
        ));
    }
    insta::assert_snapshot!("exit_codes", out);
}
