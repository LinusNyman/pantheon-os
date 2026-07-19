//! The frozen contract (§7.2): insta snapshots of `fas`'s own JSON — the only thing
//! that crosses a component boundary (I4), so these are taken from the real binary
//! rather than the library behind it.
//!
//! Plan tokens are redacted (they hash the computed change). Slugs and date keys are
//! **not**: a slug is the record's identity and its name at once, and a date key is the
//! occurrence's identity, stable across an edit (§5.4). **Every span bound and every
//! occurrence key is passed explicitly** — Fasti is the core most tempted to read the
//! wall clock, and one that read `now` where a snapshot could see it would make the
//! suite fail tomorrow. Regenerate these deliberately, never blindly.

// These tests build human-readable snapshot text; `push_str(&format!(...))` reads
// clearest here and the allocation is irrelevant in a test.
#![allow(clippy::format_push_string)]
// `exit_codes` is one table of cases and reads as one; splitting it to satisfy a line
// count would scatter the exit-code contract across several snapshots (§7.3).
#![allow(clippy::too_many_lines)]

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU32, Ordering};

use serde_json::{Value, json};

use pantheon::mint::NewSpec;
use pantheon::plan_new;

static COUNTER: AtomicU32 = AtomicU32::new(0);

/// A tree with two nodes to place at — `aof` (Fabrica) and `aoa` (Ars) — plus a
/// definition-prefix node under `aof`, which is where an entity-as-node lives (§5.1).
fn fresh_root() -> PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("fas-snap-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    for (parent, ch, label) in [
        ("root", "a", "actio"),
        ("a", "o", "opus"),
        ("ao", "f", "fabrica"),
        ("ao", "a", "ars"),
    ] {
        let (plan, _) = plan_new(&dir, parent, NewSpec::Triple { ch, label }).unwrap();
        plan.apply(&dir).unwrap();
    }
    let (plan, _) = plan_new(
        &dir,
        "aof",
        NewSpec::Def {
            definition: "mvp_phase",
        },
    )
    .unwrap();
    plan.apply(&dir).unwrap();
    dir
}

/// Run the real `fas`, returning its exit code and the JSON it emitted — stdout when it
/// produced a value, stderr for the `{"error":…}` envelope (§7.3).
fn fas(root: &Path, args: &[&str]) -> (i32, Value) {
    fas_env(root, args, &[]).0
}

/// [`fas`] with environment set for the child — never for this process (§7.3). Returns
/// stderr separately: a soft finding rides there while the record itself goes to stdout
/// (§5.4).
fn fas_env(root: &Path, args: &[&str], env: &[(&str, &str)]) -> ((i32, Value), String) {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_fas"));
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

fn meta_dir(root: &Path, node_dir: &str, code: &str) -> PathBuf {
    root.join("a_actio")
        .join("a_o_opus")
        .join(node_dir)
        .join(format!("{code}__"))
}

/// The `aof` meta dir, where most of these records land.
fn aof_meta(root: &Path) -> PathBuf {
    meta_dir(root, "ao_f_fabrica", "aof")
}

// ── the discovery surface (§7.2) ────────────────────────────────────────────

/// The one surface that shows both shapes at once: `span` partitioned beside `event`
/// series-and-hand-named, and a record schema that is the untagged union of the two
/// (§7.1). This is what `pan doctor` and resolution read over PATH (§5.0).
#[test]
fn schema_surface() {
    let root = fresh_root();
    let (code, schema) = fas(&root, &["schema"]);
    assert_eq!(code, 0);
    insta::assert_snapshot!("schema_surface", pretty(&schema));
}

// ── the span half: a partitioned entity (§6.1, §8.4) ────────────────────────

#[test]
fn verb_add_span_fresh_then_overwrite() {
    let root = fresh_root();
    // A fresh `add` runs free: it creates the record it *is* (§7.3, §18).
    let (code, fresh) = fas(&root, &["aof", "MVP Phase", "--from", "260101"]);
    assert_eq!(code, 0);
    insta::assert_snapshot!("verb_add_span_fresh", pretty(&fresh));

    // Landing on a slug that exists is an overwrite — a mutation, so piped and without
    // -y it is exit 5 with the change to review (§7.3).
    let (code, pending) = fas(&root, &["aof", "mvp_phase", "--to", "260901"]);
    assert_eq!(code, 5);
    insta::assert_snapshot!("verb_add_span_overwrite_pending", pretty(&redact(pending)));

    let (code, applied) = fas(&root, &["aof", "mvp_phase", "--to", "260901", "-y"]);
    assert_eq!(code, 0);
    insta::assert_snapshot!("verb_add_span_overwrite_applied", pretty(&applied));
}

/// Closing an open span is an ordinary `edit` that corrects the one object in place
/// (I1) — there is no `close` verb, and `to`'s absence *was* the open state (§8.4).
#[test]
fn verb_edit_closes_an_open_span() {
    let root = fresh_root();
    let (_, open) = fas(&root, &["aof", "employment", "--from", "240301"]);
    assert!(
        open["data"].get("to").is_none(),
        "an open span has no `to` at all, not a null one (§8.4)"
    );

    let (code, closed) = fas(&root, &["edit", "employment", "--to", "260630", "-y"]);
    assert_eq!(code, 0);
    insta::assert_snapshot!("verb_edit_span_close", pretty(&closed));
    // What a hand did not give, the record keeps (I1).
    assert_eq!(closed["data"]["from"], "240301");
}

#[test]
fn dry_run_emits_a_plan_and_writes_nothing() {
    let root = fresh_root();
    let (code, plan) = fas(&root, &["aof", "sabbatical", "--from", "260401", "-n"]);
    assert_eq!(code, 0);
    insta::assert_snapshot!("verb_add_span_dry_run", pretty(&redact(plan)));
    assert_eq!(
        fas(&root, &["get", "sabbatical"]).0,
        4,
        "nothing was written"
    );
}

#[test]
fn verb_rm_span() {
    let root = fresh_root();
    fas(&root, &["aof", "sabbatical", "--from", "260401"]);
    let (code, deleted) = fas(&root, &["rm", "sabbatical", "-y"]);
    assert_eq!(code, 0);
    insta::assert_snapshot!("verb_rm_span", pretty(&deleted));
    assert_eq!(fas(&root, &["get", "sabbatical"]).0, 4);
}

#[test]
fn verb_move_span_carries_no_refs_with_it() {
    let root = fresh_root();
    fas(&root, &["aof", "sabbatical", "--from", "260401"]);
    fas(&root, &["aof", "standups", "-c"]);
    fas(
        &root,
        &[
            "aof",
            "standups",
            "kickoff",
            "-a",
            "260401",
            "-r",
            "fasti:sabbatical",
        ],
    );

    let (code, moved) = fas(&root, &["move", "sabbatical", "--to", "aoa", "-y"]);
    assert_eq!(code, 0);
    insta::assert_snapshot!("verb_move_span", pretty(&moved));

    // A ref carries no path, so it survives a re-home untouched (§5.4).
    let (_, series) = fas(&root, &["series", "standups"]);
    assert_eq!(series[0]["refs"], json!(["fasti:sabbatical"]));
    // The file wears its new home's code, not a stale one carried across (§5.2).
    assert!(
        meta_dir(&root, "ao_a_ars", "aoa")
            .join("aoa__span__sabbatical.json")
            .exists()
    );
    assert!(!aof_meta(&root).join("aof__span__sabbatical.json").exists());
}

#[test]
fn an_entity_as_node_refuses_both_structural_verbs() {
    let root = fresh_root();
    fas(
        &root,
        &["-H", "aof_mvp_phase", "mvp_phase", "--from", "260101"],
    );
    let path = root
        .join("a_actio/a_o_opus/ao_f_fabrica/aof_mvp_phase_/aof_mvp_phase__")
        .join("aof_mvp_phase__span.json");
    assert!(path.exists(), "the filename carries only the kind (§5.2)");

    let (code, renamed) = fas(&root, &["rename", "mvp_phase", "mvp", "-y"]);
    assert_eq!(code, 2);
    insta::assert_snapshot!("refusal_rename_entity_node", pretty(&renamed));

    let (code, moved) = fas(&root, &["move", "mvp_phase", "--to", "aoa", "-y"]);
    assert_eq!(code, 2);
    insta::assert_snapshot!("refusal_move_entity_node", pretty(&moved));
}

// ── the event half: a hand-named series (§6.1, §7.3) ────────────────────────

#[test]
fn verb_add_event_fresh_then_overwrite() {
    let root = fresh_root();
    // A hand-named series is minted explicitly; a plain `add` never conjures one, so a
    // timeline cannot be born of a typo (§7.3, §18).
    let (code, created) = fas(&root, &["aof", "standups", "-c"]);
    assert_eq!(code, 0);
    insta::assert_snapshot!("verb_add_created_series", pretty(&created));

    // "A meeting 4–5pm" is one record: the key is the start, `--until` the end (§8.4).
    let (code, fresh) = fas(
        &root,
        &[
            "aof",
            "standups",
            "sprint review",
            "-a",
            "260719T1600",
            "--until",
            "1700",
        ],
    );
    assert_eq!(code, 0);
    insta::assert_snapshot!("verb_add_event_fresh", pretty(&fresh));

    let (code, pending) = fas(
        &root,
        &["aof", "standups", "sprint retro", "-a", "260719T1600"],
    );
    assert_eq!(code, 5, "an overwrite must stop at the checkpoint (§7.3)");
    insta::assert_snapshot!("verb_add_event_overwrite_pending", pretty(&redact(pending)));

    let (code, applied) = fas(
        &root,
        &["aof", "standups", "sprint retro", "-a", "260719T1600", "-y"],
    );
    assert_eq!(code, 0);
    insta::assert_snapshot!("verb_add_event_overwrite_applied", pretty(&applied));
}

#[test]
fn verb_edit_event_keeps_the_key() {
    let root = fresh_root();
    fas(&root, &["aof", "standups", "-c"]);
    fas(&root, &["aof", "standups", "kickoff", "-a", "260719"]);

    // A correction rewrites the keyed line in place; the date key is the occurrence's
    // own identity and does not move (I1, §5.4).
    let (code, edited) = fas(&root, &["edit", "260719", "kickoff moved", "-y"]);
    assert_eq!(code, 0);
    insta::assert_snapshot!("verb_edit_event", pretty(&edited));

    // One line, not two: a correction never stacks a second (I1).
    let (_, whole) = fas(&root, &["series", "standups"]);
    assert_eq!(whole.as_array().map(Vec::len), Some(1));
}

#[test]
fn verb_rm_event() {
    let root = fresh_root();
    fas(&root, &["aof", "standups", "-c"]);
    fas(&root, &["aof", "standups", "kickoff", "-a", "260719"]);
    let (code, deleted) = fas(&root, &["rm", "260719", "--series", "standups", "-y"]);
    assert_eq!(code, 0);
    insta::assert_snapshot!("verb_rm_event", pretty(&deleted));
}

// ── one verb over two shapes (§7.2) ─────────────────────────────────────────

/// `edit` and `rm` reach a span by slug and an occurrence by key, and the shape is the
/// one that *answers* rather than one guessed from the key's spelling (§5.4).
#[test]
fn edit_dispatches_on_which_shape_answers() {
    let root = fresh_root();
    fas(&root, &["aof", "mvp", "--from", "260101"]);
    fas(&root, &["aof", "standups", "-c"]);
    fas(&root, &["aof", "standups", "kickoff", "-a", "260719"]);

    let mut out = String::new();
    for (label, args) in [
        (
            "edit mvp --to 260901",
            vec!["edit", "mvp", "--to", "260901", "-y"],
        ),
        (
            "edit 260719 'kickoff done'",
            vec!["edit", "260719", "kickoff done", "-y"],
        ),
    ] {
        let (code, value) = fas(&root, &args);
        assert_eq!(code, 0, "{label}");
        out.push_str(&format!("$ fas {label}\n{}\n\n", pretty(&value)));
    }
    insta::assert_snapshot!("dispatch_edit_by_shape", out);
}

/// A span and an event series are two files and **one** `fasti:<slug>` (§5.4) — the
/// kind trap of §6.1 in the dimension a two-shape core adds. Within a node the check is
/// cheap, so it is hard, and it runs from both sides.
#[test]
fn add_refuses_a_name_the_other_shape_holds() {
    let root = fresh_root();
    fas(&root, &["aof", "standups", "-c"]);
    let (code, err) = fas(&root, &["aof", "standups", "--from", "260101"]);
    assert_eq!(code, 3);
    insta::assert_snapshot!("refusal_span_onto_event_series", pretty(&err));

    fas(&root, &["aof", "mvp", "--from", "260101"]);
    let (code, err) = fas(&root, &["aof", "mvp", "-c"]);
    assert_eq!(code, 3);
    insta::assert_snapshot!("refusal_series_onto_span", pretty(&err));
}

/// One cascade over two shapes. Renaming a span rewrites the `fasti:` ref an occurrence
/// holds — a core's own record pointing at its own record, through the spine's cascade
/// (§5.4, I5) — and renaming the series moves the collection a ref names as a whole.
#[test]
fn verb_rename_cascades_across_both_shapes() {
    let root = fresh_root();
    fas(&root, &["aof", "mvp_phse", "--from", "260101"]);
    fas(&root, &["aof", "standups", "-c"]);
    fas(
        &root,
        &[
            "aof",
            "standups",
            "kickoff",
            "-a",
            "260719",
            "-r",
            "fasti:mvp_phse",
        ],
    );

    let (code, renamed) = fas(&root, &["rename", "mvp_phse", "mvp_phase", "-y"]);
    assert_eq!(code, 0);
    insta::assert_snapshot!("verb_rename_span", pretty(&renamed));

    let series_file = aof_meta(&root).join("aof__event__standups.jsonl");
    let after = std::fs::read_to_string(&series_file).unwrap();
    assert!(after.contains("fasti:mvp_phase"), "{after}");
    assert!(!after.contains("mvp_phse"), "the old slug is gone: {after}");

    // A hand-named series is itself a ref target, so it renames the same way (§5.4).
    let (code, renamed) = fas(&root, &["rename", "standups", "dailies", "-y"]);
    assert_eq!(code, 0);
    insta::assert_snapshot!("verb_rename_event_series", pretty(&renamed));
    assert!(aof_meta(&root).join("aof__event__dailies.jsonl").exists());
    // The occurrences survived the file rename.
    let (_, whole) = fas(&root, &["series", "dailies"]);
    assert_eq!(whole[0]["data"]["values"], json!(["kickoff"]));
}

/// A rename onto a name the *other shape* already holds is refused tree-wide and hard
/// (§7.2) — the spine's own check, which sees across the shape boundary because Fasti
/// hands `plan_cascade` both of its tokens.
#[test]
fn rename_refuses_a_name_the_other_shape_holds() {
    let root = fresh_root();
    fas(&root, &["aof", "mvp", "--from", "260101"]);
    fas(&root, &["aoa", "standups", "-c"]);

    let (code, err) = fas(&root, &["rename", "mvp", "standups", "-y"]);
    assert_eq!(code, 3);
    insta::assert_snapshot!("refusal_rename_onto_other_shape", pretty(&err));
}

#[test]
fn add_warns_softly_on_a_cross_node_duplicate() {
    let root = fresh_root();
    assert_eq!(fas(&root, &["aof", "review", "--from", "260101"]).0, 0);
    // Across nodes the check is a walk, so it stays soft: the write succeeds, the record
    // goes to stdout, and the warning rides stderr (§5.4, §18).
    let ((code, record), stderr) = fas_env(&root, &["aoa", "review", "--from", "260201"], &[]);
    assert_eq!(code, 0, "a cross-node duplicate is never refused");
    assert_eq!(record["home"], "aoa");
    let findings: Value = serde_json::from_str(stderr.trim()).unwrap();
    insta::assert_snapshot!("warn_duplicate_slug", pretty(&findings));
}

// ── the derived surfaces: nothing here is stored (I1, §8.4) ─────────────────

/// An event with no span is **legal** — not a finding, not a stored flag — and the set
/// is surfaced on demand (§8.4).
///
/// `--unspanned` deliberately ranges over *every* occurrence rather than each series'
/// present: an unspanned occurrence that is not its timeline's most recent line would
/// be invisible to a present-fold, and a check that quietly omits its own members is
/// worse than none — which matters exactly because §8.4 keeps this off the validator.
#[test]
fn list_unspanned_is_derived_and_nothing_nags() {
    let root = fresh_root();
    fas(&root, &["aof", "mvp_phase", "--from", "260101"]);
    fas(&root, &["aof", "standups", "-c"]);
    fas(
        &root,
        &[
            "aof",
            "standups",
            "kickoff",
            "-a",
            "260719",
            "-r",
            "fasti:mvp_phase",
        ],
    );
    fas(&root, &["aof", "standups", "stray", "-a", "260720"]);
    // A `fasti:` ref naming the *series* spans nothing: a collection is not a period.
    fas(
        &root,
        &[
            "aof",
            "standups",
            "self referential",
            "-a",
            "260721",
            "-r",
            "fasti:standups",
        ],
    );

    // Writing an unspanned occurrence is not a warning and not a finding.
    let ((code, _), stderr) = fas_env(&root, &["aof", "standups", "quiet", "-a", "260722"], &[]);
    assert_eq!(code, 0);
    assert!(stderr.is_empty(), "nothing nags (§8.4): {stderr}");
    assert_eq!(
        fas(&root, &["-H", "aof", "list"]).0,
        0,
        "and nothing is a validation failure"
    );

    let (code, unspanned) = fas(&root, &["-H", "aof", "list", "--unspanned"]);
    assert_eq!(code, 0);
    insta::assert_snapshot!("derived_unspanned", pretty(&unspanned));
}

// ── the read verbs (§7.2) ───────────────────────────────────────────────────

#[test]
fn verbs_read() {
    let root = fresh_root();
    fas(
        &root,
        &["aof", "mvp_phase", "--from", "260101", "--to", "260630"],
    );
    fas(
        &root,
        &[
            "aof",
            "employment",
            "--from",
            "240301",
            "-r",
            "album:dare_robotics",
        ],
    );
    fas(&root, &["aof", "standups", "-c"]);
    fas(
        &root,
        &[
            "aof",
            "standups",
            "kickoff",
            "-a",
            "260719",
            "-r",
            "fasti:mvp_phase",
        ],
    );
    fas(
        &root,
        &[
            "aof",
            "standups",
            "demo",
            "-a",
            "260720T1400",
            "--until",
            "1500",
        ],
    );

    let mut out = String::new();
    for (label, args) in [
        ("get mvp_phase", vec!["get", "mvp_phase"]),
        ("get standups", vec!["get", "standups"]),
        ("series standups", vec!["series", "standups"]),
        (
            "series standups --from 260720",
            vec!["series", "standups", "--from", "260720"],
        ),
        ("list", vec!["-H", "aof", "list"]),
        ("list -k span", vec!["-H", "aof", "list", "-k", "span"]),
        ("list -k event", vec!["-H", "aof", "list", "-k", "event"]),
        ("where mvp_phase", vec!["where", "mvp_phase"]),
        ("where standups", vec!["where", "standups"]),
    ] {
        let (code, value) = fas(&root, &args);
        assert_eq!(code, 0, "{label}");
        out.push_str(&format!("$ fas {}\n{}\n\n", label, pretty(&value)));
    }
    insta::assert_snapshot!("verbs_read", out);
}

// ── the shapes on disk (I3, §18) ────────────────────────────────────────────

/// The snapshot that proves §7.1's "no tag is ever written": a two-token core's `Record`
/// is a dispatch type, and the filename already names the variant (§5.2, §18).
#[test]
fn on_disk_neither_shape_stores_its_location_or_its_variant() {
    let root = fresh_root();
    fas(
        &root,
        &["aof", "mvp_phase", "--from", "260101", "--to", "260630"],
    );
    fas(&root, &["aof", "standups", "-c"]);
    fas(
        &root,
        &[
            "aof",
            "standups",
            "kickoff",
            "-a",
            "260719",
            "-r",
            "fasti:mvp_phase",
        ],
    );
    fas(
        &root,
        &[
            "aof",
            "standups",
            "demo",
            "-a",
            "260720T1400",
            "--until",
            "1500",
        ],
    );

    let span = std::fs::read_to_string(aof_meta(&root).join("aof__span__mvp_phase.json")).unwrap();
    insta::assert_snapshot!("on_disk_span_file", span.clone());
    let events =
        std::fs::read_to_string(aof_meta(&root).join("aof__event__standups.jsonl")).unwrap();
    insta::assert_snapshot!("on_disk_event_file", events.clone());

    for (raw, what) in [(&span, "span"), (&events, "event")] {
        // No kind, no home, no slug, no series — all four are the file's location and
        // name (I3) — and no variant tag, which is the two-shape core's own trap (§18).
        for absent in ["kind", "home", "slug", "series", "Span", "Event", "aof"] {
            assert!(!raw.contains(absent), "{what}: {absent:?} is stored: {raw}");
        }
    }
    // A span stores no key either: its *name* is the key (§5.4, §18).
    assert!(!span.contains("key"));
}

// ── the hand, the rule, and the editor (§7.3, §9.3) ─────────────────────────

#[test]
fn the_editor_form_piped_prints_a_path_and_spawns_nothing() {
    let root = fresh_root();
    fas(
        &root,
        &["aof", "mvp_phase", "--from", "260101", "--note", "a remark"],
    );
    fas(&root, &["aof", "standups", "-c"]);
    fas(&root, &["aof", "standups", "kickoff", "-a", "260719"]);

    // `false` would fail if it ran; piped, nothing is spawned at all (§7.3, I8). Both
    // shapes take the editor form, each opening the value its shape has.
    let ((code, span), _) = fas_env(
        &root,
        &["edit", "mvp_phase", "--note"],
        &[("EDITOR", "false"), ("VISUAL", "false")],
    );
    assert_eq!(code, 0);
    assert!(
        span["path"]
            .as_str()
            .unwrap()
            .ends_with("aof__span__mvp_phase.json"),
        "{span}"
    );

    let ((code, event), _) = fas_env(
        &root,
        &["edit", "260719"],
        &[("EDITOR", "false"), ("VISUAL", "false")],
    );
    assert_eq!(code, 0);
    assert!(
        event["path"]
            .as_str()
            .unwrap()
            .ends_with("aof__event__standups.jsonl"),
        "{event}"
    );
}

#[test]
fn write_verbs_are_refused_under_a_rule() {
    let root = fresh_root();
    fas(&root, &["aof", "mvp_phase", "--from", "260101"]);
    fas(&root, &["aof", "standups", "-c"]);
    fas(&root, &["aof", "standups", "kickoff", "-a", "260719"]);
    let rule = [("PANTHEON_RULE", "1")];

    for args in [
        vec!["aof", "sabbatical", "--from", "260401"],
        vec!["aof", "standups", "another", "-a", "260720"],
        vec!["edit", "mvp_phase", "--to", "260901", "-y"],
        vec!["edit", "260719", "corrected", "-y"],
        vec!["rename", "mvp_phase", "mvp", "-y"],
        vec!["move", "mvp_phase", "--to", "aoa", "-y"],
        vec!["rm", "260719", "-y"],
        vec!["rm", "mvp_phase", "-y"],
    ] {
        let ((code, _), _) = fas_env(&root, &args, &rule);
        assert_eq!(code, 6, "{args:?} must be refused under a rule (§9.3, I2)");
    }
    // Reads run free, and `--dry-run` still computes: a rule may plan (§7.3).
    assert_eq!(fas_env(&root, &["get", "mvp_phase"], &rule).0.0, 0);
    assert_eq!(fas_env(&root, &["list", "--unspanned"], &rule).0.0, 0);
    assert_eq!(
        fas_env(
            &root,
            &["aof", "sabbatical", "--from", "260401", "-n"],
            &rule
        )
        .0
        .0,
        0
    );
}

// ── the hand decides what a bare short means (§7.3) ─────────────────────────

/// A bare short opens the screen at a terminal and emits `help` down a pipe — a screen
/// has nothing to draw there, and `add` is the default verb, so there is no fold for a
/// bare short to mean instead (§7.3).
///
/// This is as far as a test can reach into the screen: `screen.rs` is a module of the
/// **bin**, and an integration test links the lib, so `FastiApp` is not nameable here.
/// Driving it with `porticus::drive` — which step 6 found catches what a green suite
/// misses — would need the screen to move into the lib, and that is a shape question
/// for all seven cores at once rather than one this core settles alone (§14).
#[test]
fn a_bare_short_piped_emits_help() {
    let root = fresh_root();
    let (code, help) = fas(&root, &[]);
    assert_eq!(code, 0);
    assert_eq!(help["short"], "fas");
    assert_eq!(help["name"], "fasti");
    // Both tokens are named, since a hand reading `help` is choosing between shapes.
    assert_eq!(help["kinds"], json!(["span", "event"]));
    // The twelve verbs and no thirteenth (§18).
    let verbs = help["verbs"].as_array().unwrap();
    for verb in [
        "add", "edit", "rename", "move", "rm", "list", "get", "series", "where", "schema", "help",
        "version",
    ] {
        assert!(verbs.iter().any(|v| v == verb), "missing {verb}");
    }
}

// ── the integration gate: `pan` reads what `fas` wrote (§5.0, §16) ──────────

/// Both shapes must resolve through the spine, which learns Fasti's tokens by running
/// `fas schema` over PATH — it never links a core (I5, §5.0). This is the gate that
/// proves a two-shape core is legible to a resolver that knows no core, across all three
/// filename forms §5.2 admits: a partitioned `span`, an `event` series by *its* filename
/// as a whole collection (§5.4), and a span promoted to its own node, whose slug is the
/// node's definition and so is not in the filename at all.
#[test]
fn records_resolve_through_pan() {
    let root = fresh_root();
    fas(&root, &["aof", "employment", "--from", "240301"]);
    fas(&root, &["aof", "standups", "-c"]);
    fas(
        &root,
        &["-H", "aof_mvp_phase", "mvp_phase", "--from", "260101"],
    );

    let fas_bin = PathBuf::from(env!("CARGO_BIN_EXE_fas"));
    let bin_dir = fas_bin.parent().unwrap();
    let pan_bin = bin_dir.join("pan");
    // Cargo builds `fas` for this test (CARGO_BIN_EXE_fas) but not `pan`, which belongs
    // to another crate — so the workspace bins must be built first. CI does that in the
    // `nextest + insta` job; locally, `cargo build --workspace --bins`. Failing loudly
    // beats skipping: a gate that quietly passes is not a gate.
    assert!(
        pan_bin.exists(),
        "this gate needs `pan` built beside `fas`: run `cargo build --workspace --bins` \
         (looked in {})",
        bin_dir.display()
    );

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
            "fasti:employment",
            "fasti:standups",
            "fasti:mvp_phase",
        ])
        .env("PATH", path)
        .env_remove("PANTHEON_ROOT")
        .output()
        .unwrap();
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let resolved: Value = serde_json::from_slice(&out.stdout).unwrap();
    insta::assert_snapshot!("resolve_through_pan", pretty(&resolved));
}

// ── the exit codes are contract (§7.3) ──────────────────────────────────────

#[test]
fn exit_codes() {
    // A node holding exactly one event series: inference has a single answer.
    let one = fresh_root();
    fas(&one, &["aof", "standups", "-c"]);
    fas(&one, &["aof", "standups", "kickoff", "-a", "260719"]);
    fas(&one, &["aof", "mvp_phase", "--from", "260101"]);

    // A node holding two: inference must list them and stop, never guess (§7.3).
    let two = fresh_root();
    fas(&two, &["aof", "standups", "-c"]);
    fas(&two, &["aof", "reviews", "-c"]);

    let cases: &[(&PathBuf, &str, &[&str])] = &[
        // The form picks the shape, and one write is one shape (§7.2).
        (
            &one,
            "a span's bound and an event's date at once",
            &["aof", "x", "--from", "260101", "-a", "260101"],
        ),
        (
            &one,
            "-k never selects across a shape",
            &["add", "-k", "event", "aof", "y"],
        ),
        (
            &one,
            "-k span cannot ride the series form",
            &["aof", "z", "-k", "span", "-c"],
        ),
        (&one, "a span needs a --from", &["aof", "unbounded"]),
        (
            &one,
            "a span may not end before it starts",
            &["aof", "w", "--from", "260301", "--to", "260101"],
        ),
        (&one, "a malformed day", &["aof", "v", "--from", "26010"]),
        (
            &one,
            "a malformed --until",
            &["aof", "standups", "q", "-a", "260720", "--until", "5pm"],
        ),
        // A span is not a collection, and an occurrence is not a ref target (§5.4).
        (
            &one,
            "a span is read with get",
            &["series", "-k", "span", "standups"],
        ),
        (
            &one,
            "--unspanned is about events",
            &["list", "--unspanned", "-k", "span"],
        ),
        // The container is found, never invented (§7.3, §18).
        (
            &one,
            "a typo cannot conjure a timeline",
            &["aof", "standps", "1", "-a", "260721"],
        ),
        (
            &one,
            "-c is refused on an inference form",
            &["standups", "1", "-c"],
        ),
        (
            &two,
            "two series at the node: ambiguous",
            &["aof", "1", "-a", "260719"],
        ),
        // Neither shape answers, or both do.
        (
            &one,
            "no such record",
            &["edit", "nothing_here", "--note", "x"],
        ),
        (&one, "no such ref target", &["get", "nothing_here"]),
        (&one, "a token fasti does not own", &["list", "-k", "task"]),
        (&one, "a name is one token", &["get", "mvp", "phase"]),
        (
            &one,
            "a blank remark",
            &["aof", "u", "--from", "260101", "--note", "   "],
        ),
        (
            &one,
            "a malformed reference",
            &["aof", "t", "--from", "260101", "-r", "not-a-ref"],
        ),
        // A second occurrence on a key that exists is an overwrite awaiting review.
        (
            &one,
            "an overwrite, piped, without -y",
            &["aof", "standups", "again", "-a", "260719"],
        ),
    ];

    let mut out = String::new();
    for (root, why, args) in cases {
        let (code, value) = fas(root, args);
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
            "fas {}\n  # {why}\n  => exit {code}: {outcome}\n",
            args.join(" ")
        ));
    }
    insta::assert_snapshot!("exit_codes", out);
}
