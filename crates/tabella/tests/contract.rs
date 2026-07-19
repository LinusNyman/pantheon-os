//! The frozen contract (§7.2): insta snapshots of `tab`'s own JSON — the only thing
//! that crosses a component boundary (I4), so these are taken from the real binary
//! rather than the library behind it.
//!
//! Plan tokens are redacted (they hash the computed change). Slugs are **not**: a slug
//! is the record's identity and its name at once (§5.4). Nothing here depends on the
//! wall clock — a document has no key to date (§7.1), so there is no `-a` anywhere.
//! Regenerate these deliberately, never blindly.

// These tests build human-readable snapshot text; `push_str(&format!(...))` reads
// clearest here and the allocation is irrelevant in a test.
#![allow(clippy::format_push_string)]

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};

use serde_json::{Value, json};

use pantheon::mint::NewSpec;
use pantheon::plan_new;

static COUNTER: AtomicU32 = AtomicU32::new(0);

/// A tree with two nodes to file at — `ecv` (Valetudo) and `eam` (Mores) — plus a
/// definition-prefix node under `eam`, which is the case §8.7 names: a note about a
/// person homes *in* them, which is what promotes them to their own node (§5.1).
fn fresh_root() -> PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("tab-snap-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    for (parent, ch, label) in [
        ("root", "e", "ego"),
        ("e", "c", "corpus"),
        ("ec", "v", "valetudo"),
        ("e", "a", "anima"),
        ("ea", "m", "mores"),
    ] {
        let (plan, _) = plan_new(&dir, parent, NewSpec::Triple { ch, label }).unwrap();
        plan.apply(&dir).unwrap();
    }
    let (plan, _) = plan_new(
        &dir,
        "eam",
        NewSpec::Def {
            definition: "marcus_aurelius",
        },
    )
    .unwrap();
    plan.apply(&dir).unwrap();
    dir
}

/// Run the real `tab`, returning its exit code and the JSON it emitted — stdout when
/// it produced a value, stderr for the `{"error":…}` envelope (§7.3).
fn tab(root: &Path, args: &[&str]) -> (i32, Value) {
    tab_env(root, args, &[]).0
}

/// [`tab`] with environment set for the child — never for this process (§7.3).
/// Returns stderr separately: a soft finding rides there while the record itself goes
/// to stdout (§5.4).
fn tab_env(root: &Path, args: &[&str], env: &[(&str, &str)]) -> ((i32, Value), String) {
    let (code, stdout, stderr) = tab_raw(root, args, env);
    let text = if stdout.is_empty() { &stderr } else { &stdout };
    (
        (code, serde_json::from_str(text).unwrap_or(Value::Null)),
        stderr,
    )
}

/// The bytes `tab` actually wrote. `-f raw` emits prose rather than JSON, so the
/// `cat` case has to be read as text (§7.2).
fn tab_raw(root: &Path, args: &[&str], env: &[(&str, &str)]) -> (i32, String, String) {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_tab"));
    cmd.arg("-C")
        .arg(root)
        .args(args)
        .env_remove("PANTHEON_ROOT")
        // `add` reads its body from stdin when none is given and stdin is not a
        // terminal (§7.3); a null stdin makes that an immediate EOF rather than a hang.
        .stdin(Stdio::null());
    for (key, value) in env {
        cmd.env(key, value);
    }
    let out = cmd.output().unwrap();
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
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

/// A document lives loose in the **open** node dir, never the meta dir (§6.1).
fn node_dir(root: &Path) -> PathBuf {
    root.join("e_ego").join("e_c_corpus").join("ec_v_valetudo")
}

fn read_doc(root: &Path, file: &str) -> String {
    std::fs::read_to_string(node_dir(root).join(file)).unwrap()
}

// ── the discovery surface (§7.2) ────────────────────────────────────────────

/// The step's headline artifact: a core declaring **no tokens at all**. That emptiness
/// is what names it a Document core (§7.1), and it is what `pan doctor` and the
/// resolver read to route every loose document here by extension alone (§5.0, §5.5).
#[test]
fn schema_surface() {
    let root = fresh_root();
    let (code, schema) = tab(&root, &["schema"]);
    assert_eq!(code, 0);
    assert_eq!(schema["tokens"], json!([]), "a Document core declares none");
    insta::assert_snapshot!("schema_surface", pretty(&schema));
}

// ── the write verbs (§7.2, §7.3) ────────────────────────────────────────────

#[test]
fn verb_add_fresh_then_overwrite() {
    let root = fresh_root();
    // A fresh `add` runs free: it creates the record it *is* (§7.3, §18).
    let (code, fresh) = tab(
        &root,
        &[
            "ecv",
            "Trip Idea",
            "Two weeks in Rome.",
            "--type",
            "nota",
            "--tag",
            "travel",
        ],
    );
    assert_eq!(code, 0);
    insta::assert_snapshot!("verb_add_fresh", pretty(&fresh));

    // Landing on an existing document is an overwrite — a mutation, so piped it exits
    // `5` with the change and its plan token (§7.3).
    let (code, pending) = tab(&root, &["ecv", "trip_idea", "Rewritten."]);
    assert_eq!(code, 5);
    insta::assert_snapshot!("verb_add_overwrite_pending", pretty(&redact(pending)));

    let (code, applied) = tab(&root, &["ecv", "trip_idea", "Rewritten.", "-y"]);
    assert_eq!(code, 0);
    insta::assert_snapshot!("verb_add_overwrite_applied", pretty(&applied));
}

#[test]
fn verb_add_dry_run_writes_nothing() {
    let root = fresh_root();
    let (code, plan) = tab(&root, &["ecv", "trip_idea", "Rome.", "-n"]);
    assert_eq!(code, 0);
    insta::assert_snapshot!("verb_add_dry_run", pretty(&redact(plan)));
    // Every write verb takes `--dry-run`, fresh or not — and it wrote nothing.
    assert_eq!(tab(&root, &["get", "trip_idea"]).0, 4);
}

#[test]
fn verb_add_writes_the_extension_it_is_given() {
    let root = fresh_root();
    assert_eq!(tab(&root, &["ecv", "a_note", "x", "--ext", "txt"]).0, 0);
    assert!(node_dir(&root).join("ecv_a_note.txt").exists());
    let (code, doc) = tab(&root, &["get", "a_note"]);
    assert_eq!(code, 0);
    assert_eq!(doc["ext"], "txt");
}

/// `ecv_note.md` and `ecv_note.txt` are two files and one `tabella:note` — §5.4's
/// kind trap in the extension dimension, so the refusal is hard (exit `3`).
#[test]
fn refusal_add_under_another_extension() {
    let root = fresh_root();
    assert_eq!(tab(&root, &["ecv", "note", "x"]).0, 0);
    let (code, err) = tab(&root, &["ecv", "note", "y", "--ext", "txt"]);
    assert_eq!(code, 3);
    insta::assert_snapshot!("refusal_add_under_another_extension", pretty(&err));
}

#[test]
fn verb_edit_changes_frontmatter_and_confirms() {
    let root = fresh_root();
    assert_eq!(
        tab(&root, &["ecv", "trip_idea", "Rome.", "--type", "nota"]).0,
        0
    );

    // Piped, an inline edit exits `5` carrying the change and its plan token — and
    // writes nothing until the caller re-runs with `-y` (§7.3).
    let (code, pending) = tab(&root, &["edit", "trip_idea", "--type", "principium"]);
    assert_eq!(code, 5, "an inline edit is an ordinary mutation (§7.3)");
    assert!(
        pending["token"].is_string(),
        "a pending change is reviewable"
    );
    assert_eq!(
        tab(&root, &["get", "trip_idea"]).1["type"],
        "nota",
        "exit 5 must write nothing"
    );

    let (code, edited) = tab(
        &root,
        &[
            "edit",
            "trip_idea",
            "--type",
            "principium",
            "--tag",
            "mores",
            "-y",
        ],
    );
    assert_eq!(code, 0);
    insta::assert_snapshot!("verb_edit_frontmatter", pretty(&edited));

    // `--type ""` clears the key rather than writing a blank one.
    let (code, cleared) = tab(&root, &["edit", "trip_idea", "--type", "", "-y"]);
    assert_eq!(code, 0);
    assert_eq!(cleared["type"], Value::Null);
}

/// Piped, the editor form spawns nothing and prints the file's path — the LLM hand
/// gets a path to open with its own tools rather than a blocked process (§7.3, I8).
#[test]
fn the_editor_form_piped_prints_a_path() {
    let root = fresh_root();
    assert_eq!(tab(&root, &["ecv", "trip_idea", "Rome."]).0, 0);
    // `false` as the editor: if anything spawned it, this would fail rather than pass.
    let (code, out) = tab_env(&root, &["edit", "trip_idea"], &[("EDITOR", "false")]).0;
    assert_eq!(code, 0);
    let path = out["path"].as_str().unwrap();
    assert!(path.ends_with("ecv_trip_idea.md"), "{path}");
    assert_eq!(out.as_object().unwrap().len(), 1, "only a path (§7.3)");
}

#[test]
fn verb_rename_cascades_refs_and_moves_the_file() {
    let root = fresh_root();
    assert_eq!(tab(&root, &["ecv", "trip_idea", "Rome."]).0, 0);
    // Another core's record pointing at the document. Hand-written rather than driven
    // through `alb`: the cascade is spine machinery over the envelope, so the test
    // stays hermetic and does not turn into a second core's contract test (I5).
    let meta = node_dir(&root).join("ecv__");
    std::fs::create_dir_all(&meta).unwrap();
    std::fs::write(
        meta.join("ecv__person__mara.json"),
        "{\n  \"refs\": [\n    \"tabella:trip_idea\"\n  ],\n  \"data\": {}\n}\n",
    )
    .unwrap();

    let (code, renamed) = tab(&root, &["rename", "trip_idea", "journey", "-y"]);
    assert_eq!(code, 0);
    insta::assert_snapshot!("verb_rename", pretty(&renamed));

    assert!(node_dir(&root).join("ecv_journey.md").exists());
    assert!(!node_dir(&root).join("ecv_trip_idea.md").exists());
    let cascaded = std::fs::read_to_string(meta.join("ecv__person__mara.json")).unwrap();
    assert!(cascaded.contains("tabella:journey"), "{cascaded}");
    insta::assert_snapshot!("on_disk_cascaded_ref", cascaded);
}

/// **The regression test for a hole that would otherwise be silent.**
///
/// `plan_cascade` gates its occupied-slug refusal on the caller's own tokens, and a
/// Document core declares none (§7.1) — and it walks meta dirs, where no document
/// lives (§5.2). Neither gate can see a document, so without Tabella's own tree-wide
/// check this rename would succeed at exit `0` and leave two `tabella:journey`,
/// indistinguishable and unrecoverable (§18 keeps no history).
#[test]
fn refusal_rename_onto_an_occupied_slug() {
    let root = fresh_root();
    assert_eq!(tab(&root, &["ecv", "journey", "one"]).0, 0);
    assert_eq!(tab(&root, &["eam", "trip_idea", "two"]).0, 0);

    let (code, err) = tab(&root, &["rename", "trip_idea", "journey", "-y"]);
    assert_eq!(code, 3, "an occupied slug is refused tree-wide (§7.2)");
    insta::assert_snapshot!("refusal_rename_onto_an_occupied_slug", pretty(&err));
    // Both documents still stand, under their own names.
    assert_eq!(tab(&root, &["get", "journey"]).0, 0);
    assert_eq!(tab(&root, &["get", "trip_idea"]).0, 0);
}

/// A document's `move` is a `mv` between **node dirs**, not meta dirs (§7.2) — the
/// one place the Document shape's placement is visible in a verb.
#[test]
fn verb_move_relocates_between_node_dirs() {
    let root = fresh_root();
    assert_eq!(tab(&root, &["ecv", "trip_idea", "Rome."]).0, 0);
    let (code, moved) = tab(&root, &["mv", "trip_idea", "--to", "eam", "-y"]);
    assert_eq!(code, 0);
    insta::assert_snapshot!("verb_move", pretty(&moved));

    let to = root.join("e_ego").join("e_a_anima").join("ea_m_mores");
    // The filename's code prefix is rewritten, because it is derived from the home.
    assert!(to.join("eam_trip_idea.md").exists());
    assert!(!node_dir(&root).join("ecv_trip_idea.md").exists());
    // And it did *not* land in a meta dir.
    assert!(!to.join("eam__").join("eam_trip_idea.md").exists());
}

#[test]
fn verb_rm_deletes_the_file() {
    let root = fresh_root();
    assert_eq!(tab(&root, &["ecv", "trip_idea", "Rome."]).0, 0);
    let (code, removed) = tab(&root, &["rm", "trip_idea", "-y"]);
    assert_eq!(code, 0);
    insta::assert_snapshot!("verb_rm", pretty(&removed));
    assert!(!node_dir(&root).join("ecv_trip_idea.md").exists());
}

// ── the read verbs (§7.2) ───────────────────────────────────────────────────

#[test]
fn verbs_read() {
    let root = fresh_root();
    assert_eq!(
        tab(
            &root,
            &[
                "ecv",
                "trip_idea",
                "Rome.",
                "--type",
                "nota",
                "--tag",
                "travel"
            ]
        )
        .0,
        0
    );
    assert_eq!(
        tab(&root, &["eam", "on_anger", "Seneca.", "--type", "quote"]).0,
        0
    );

    let mut out = String::new();
    for (why, args) in [
        (
            "one document, frontmatter and body (§7.2)",
            vec!["get", "trip_idea"],
        ),
        (
            "the fold — frontmatter only, never a body (§7.1)",
            vec!["list"],
        ),
        ("a slug to its home code (§7.3)", vec!["where", "on_anger"]),
    ] {
        let (code, value) = tab(&root, &args);
        out.push_str(&format!(
            "tab {}\n  # {why}\n  => exit {code}\n{}\n\n",
            args.join(" "),
            pretty(&value)
        ));
    }
    insta::assert_snapshot!("verbs_read", out);
}

/// `-f raw` emits the bare body — the `cat` case, for a pager or `$EDITOR` (§7.2).
/// Byte-for-byte the prose: no frontmatter, and no newline added.
#[test]
fn get_raw_emits_the_bare_body() {
    let root = fresh_root();
    assert_eq!(
        tab(&root, &["ecv", "trip_idea", "Rome.", "--type", "nota"]).0,
        0
    );
    let (code, stdout, _) = tab_raw(&root, &["get", "trip_idea", "-f", "raw"], &[]);
    assert_eq!(code, 0);
    assert_eq!(stdout, "Rome.\n", "the body alone, verbatim");
    insta::assert_snapshot!("get_raw", stdout);
}

// ── the file on disk (§6.1, §6.6) ───────────────────────────────────────────

/// Loose in the open node dir as `[code]_[slug].[ext]` — a **single** underscore and
/// no `__` segment, because a Document core declares no token to put in one (§7.1).
/// The record stores neither its home nor its slug: both are the file itself (I3).
#[test]
fn on_disk_document_file() {
    let root = fresh_root();
    assert_eq!(
        tab(
            &root,
            &[
                "ecv",
                "trip_idea",
                "Two weeks in Rome.",
                "--type",
                "nota",
                "--tag",
                "travel"
            ]
        )
        .0,
        0
    );
    let text = read_doc(&root, "ecv_trip_idea.md");
    for absent in ["home", "slug", "ecv", "kind"] {
        assert!(
            !text.contains(absent),
            "{absent:?} is the path's, not the record's (I3)"
        );
    }
    insta::assert_snapshot!("on_disk_document_file", text);
}

/// §8.7: Tabella handles *every* loose `[code]_*` document in place — including one a
/// hand wrote with no fence at all. It reads as an empty envelope over a whole-file
/// body, and an edit grows a fence above prose it leaves untouched.
#[test]
fn a_document_a_hand_wrote_without_frontmatter() {
    let root = fresh_root();
    std::fs::write(
        node_dir(&root).join("ecv_handmade.md"),
        "just prose, no fence\n\nand a second paragraph\n",
    )
    .unwrap();

    let (code, got) = tab(&root, &["get", "handmade"]);
    assert_eq!(code, 0);
    assert_eq!(got["type"], Value::Null);
    assert_eq!(got["tags"], json!([]));
    insta::assert_snapshot!("get_a_document_without_frontmatter", pretty(&got));

    // It folds like any other document.
    let (_, listed) = tab(&root, &["list"]);
    assert_eq!(listed.as_array().unwrap().len(), 1);

    assert_eq!(
        tab(&root, &["edit", "handmade", "--type", "nota", "-y"]).0,
        0
    );
    insta::assert_snapshot!(
        "on_disk_frontmatter_grown_over_existing_prose",
        read_doc(&root, "ecv_handmade.md")
    );
}

/// §6.6: all TOML is `toml_edit`'s, so comments and key ordering survive a rewrite by
/// code or LLM (I6, I8). This is what forbids a serde round-trip of the frontmatter,
/// and the reason a document carries its fence's raw TOML rather than reconstructing
/// it from the two fields Tabella reads.
#[test]
fn on_disk_a_hands_comment_and_unread_keys_survive_an_edit() {
    let root = fresh_root();
    std::fs::write(
        node_dir(&root).join("ecv_meditationes.md"),
        "+++\n# why this note exists\ntags = [\"mores\"]\nauthor = \"a hand\"\ntype = \"principium\"\n+++\n\nProse.\n",
    )
    .unwrap();

    assert_eq!(
        tab(&root, &["edit", "meditationes", "--tag", "vocatio", "-y"]).0,
        0
    );
    let text = read_doc(&root, "ecv_meditationes.md");
    assert!(
        text.contains("# why this note exists"),
        "comment lost:\n{text}"
    );
    assert!(
        text.contains("author = \"a hand\""),
        "unread key lost:\n{text}"
    );
    assert!(
        text.find("tags").unwrap() < text.find("type").unwrap(),
        "order lost"
    );
    insta::assert_snapshot!("on_disk_hand_written_frontmatter_survives", text);
}

/// A note about a person homes *in* them — the case §8.7 says promotes a person to
/// their own node (§5.1, I3). The document's prefix is the node's whole
/// definition-prefix code, which `classify` strips to find the slug.
#[test]
fn a_document_at_a_definition_prefix_node() {
    let root = fresh_root();
    let (code, added) = tab(
        &root,
        &[
            "-H",
            "eam_marcus_aurelius",
            "interview_notes",
            "What he said.",
        ],
    );
    assert_eq!(code, 0);
    assert_eq!(added["home"], "eam_marcus_aurelius");
    let at = root
        .join("e_ego")
        .join("e_a_anima")
        .join("ea_m_mores")
        .join("eam_marcus_aurelius_");
    assert!(at.join("eam_marcus_aurelius_interview_notes.md").exists());
    assert_eq!(tab(&root, &["get", "interview_notes"]).0, 0);
}

// ── the soft half of uniqueness (§5.4) ──────────────────────────────────────

#[test]
fn warn_duplicate_slug_across_nodes() {
    let root = fresh_root();
    assert_eq!(tab(&root, &["ecv", "trip_idea", "one"]).0, 0);
    let ((code, record), stderr) = tab_env(&root, &["eam", "trip_idea", "two"], &[]);
    // The record itself still goes to stdout; the warning rides stderr (§5.4, §18).
    assert_eq!(code, 0);
    assert_eq!(record["home"], "eam");
    assert!(stderr.contains("duplicate_slug"), "{stderr}");
}

// ── refusals and exit codes (§7.3) ──────────────────────────────────────────

#[test]
fn write_verbs_are_refused_under_a_rule() {
    let root = fresh_root();
    assert_eq!(tab(&root, &["ecv", "trip_idea", "Rome."]).0, 0);
    let rule = [("PANTHEON_RULE", "1")];
    for args in [
        vec!["ecv", "another", "x"],
        vec!["edit", "trip_idea", "--type", "nota"],
        vec!["rename", "trip_idea", "journey"],
        vec!["mv", "trip_idea", "--to", "eam"],
        vec!["rm", "trip_idea"],
    ] {
        let (code, _) = tab_env(&root, &args, &rule).0;
        assert_eq!(code, 6, "{args:?} must be refused under a rule (§9.3)");
    }
    // Reads still run, and so does a dry-run: a rule may plan (§7.3).
    assert_eq!(tab_env(&root, &["get", "trip_idea"], &rule).0.0, 0);
    assert_eq!(tab_env(&root, &["ecv", "x", "y", "-n"], &rule).0.0, 0);
}

#[test]
fn exit_codes() {
    let root = fresh_root();
    assert_eq!(tab(&root, &["ecv", "trip_idea", "Rome."]).0, 0);
    assert_eq!(tab(&root, &["eam", "twin", "a"]).0, 0);
    assert_eq!(tab(&root, &["ecv", "twin", "b"]).0, 0);

    let cases: &[(&str, Vec<&str>)] = &[
        ("no such document", vec!["get", "nobody"]),
        (
            "a slug at two nodes is listed, not guessed",
            vec!["get", "twin"],
        ),
        (
            "a document's frontmatter carries no refs",
            vec!["ecv", "x", "-r", "album:mara"],
        ),
        (
            "a Document core declares no tokens",
            vec!["-k", "person", "get", "trip_idea"],
        ),
        ("no series to mint", vec!["ecv", "x", "-c"]),
        ("no key to date", vec!["ecv", "x", "-a", "260718"]),
        ("no series to read", vec!["series"]),
        (
            "raw is one document's body, so only `get` has one",
            vec!["list", "-f", "raw"],
        ),
        (
            "only `add` creates a file",
            vec!["--ext", "txt", "get", "trip_idea"],
        ),
        (
            "the extension set is closed at three",
            vec!["ecv", "x", "--ext", "pdf"],
        ),
        ("a document needs a name", vec!["add"]),
        ("a name that normalizes to empty (§5.1)", vec!["ecv", "!!!"]),
        (
            "prose and -e name two sources for one buffer",
            vec!["ecv", "x", "some prose", "-e"],
        ),
        (
            "an occupied slug, tree-wide",
            vec!["rename", "trip_idea", "twin", "-y"],
        ),
        (
            "an overwrite, piped, without -y",
            vec!["ecv", "trip_idea", "again"],
        ),
    ];

    let mut out = String::new();
    for (why, args) in cases {
        let (code, value) = tab(&root, args);
        let msg = value
            .get("error")
            .and_then(|e| e.get("msg"))
            .and_then(Value::as_str)
            .unwrap_or("(a value, not an error)");
        out.push_str(&format!(
            "tab {}\n  # {why}\n  => exit {code}: {msg}\n\n",
            args.join(" ")
        ));
    }
    insta::assert_snapshot!("exit_codes", out);
}

// ── the file→core map, end to end (§5.0, §5.5) ──────────────────────────────

/// **The integration gate for the whole step.** A document has no token, so it reaches
/// its core by *extension alone* — the resolver finds the Document core by looking for
/// the one that declares no kinds (§7.1). Until Tabella existed, no core did, so this
/// path had never run. Driving `pan` over `PATH` with `tab` beside it is what proves
/// §5.5's totality claim demonstrable rather than merely stated (§16).
#[test]
fn documents_resolve_through_pan_by_extension_alone() {
    let root = fresh_root();
    assert_eq!(
        tab(&root, &["ecv", "trip_idea", "Rome.", "--type", "nota"]).0,
        0
    );

    let bin_dir = PathBuf::from(env!("CARGO_BIN_EXE_tab"))
        .parent()
        .unwrap()
        .to_path_buf();
    let pan = bin_dir.join("pan");
    assert!(
        pan.exists(),
        "`pan` must be built beside `tab`: run `cargo build --workspace --bins`"
    );
    let path = std::env::var("PATH").unwrap_or_default();
    let out = Command::new(&pan)
        .arg("-C")
        .arg(&root)
        .args(["resolve", "tabella:trip_idea"])
        .env("PATH", format!("{}:{path}", bin_dir.display()))
        .env_remove("PANTHEON_ROOT")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0));
    let resolved: Value = serde_json::from_slice(&out.stdout).unwrap();
    let one = &resolved["resolved"][0];
    assert_eq!(one["core"], "tabella");
    // No kind: there is no token to name, which is the whole point (§7.1).
    assert_eq!(one["kind"], "");
    assert_eq!(one["shape"], json!({ "shape": "document" }));
    insta::assert_snapshot!("documents_resolve_through_pan", pretty(&resolved));
}
