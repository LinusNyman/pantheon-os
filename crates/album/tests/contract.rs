//! The frozen contract (§7.2): insta snapshots of `alb`'s own JSON — the only thing
//! that crosses a component boundary (I4), so these are taken from the real binary
//! rather than the library behind it.
//!
//! Plan tokens are redacted (they hash the computed change). Slugs are **not**: a
//! slug is the record's identity and its name at once (§5.4). Nothing here depends
//! on the wall clock — an entity has no key to date (§7.1). Regenerate these
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

/// A tree with two nodes to file at — `csa` (Amicitia) and `cso` (Officium) — plus a
/// definition-prefix node under `csa`, which is where an entity-as-node lives (§5.1).
fn fresh_root() -> PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("alb-snap-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    for (parent, ch, label) in [
        ("root", "c", "contextus"),
        ("c", "s", "societas"),
        ("cs", "a", "amicitia"),
        ("cs", "o", "officium"),
    ] {
        let (plan, _) = plan_new(&dir, parent, NewSpec::Triple { ch, label }).unwrap();
        plan.apply(&dir).unwrap();
    }
    let (plan, _) = plan_new(
        &dir,
        "csa",
        NewSpec::Def {
            definition: "john_appleseed",
        },
    )
    .unwrap();
    plan.apply(&dir).unwrap();
    dir
}

/// Run the real `alb`, returning its exit code and the JSON it emitted — stdout when
/// it produced a value, stderr for the `{"error":…}` envelope (§7.3).
fn alb(root: &Path, args: &[&str]) -> (i32, Value) {
    alb_env(root, args, &[]).0
}

/// [`alb`] with environment set for the child — never for this process (§7.3).
/// Returns stderr separately: a soft finding rides there while the record itself
/// goes to stdout (§5.4).
fn alb_env(root: &Path, args: &[&str], env: &[(&str, &str)]) -> ((i32, Value), String) {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_alb"));
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
    root.join("c_contextus")
        .join("c_s_societas")
        .join(node_dir)
        .join(format!("{code}__"))
}

/// The `csa` meta dir, where most of these records land.
fn csa_meta(root: &Path) -> PathBuf {
    meta_dir(root, "cs_a_amicitia", "csa")
}

// ── the discovery surface (§7.2) ────────────────────────────────────────────

#[test]
fn schema_surface() {
    let root = fresh_root();
    let (code, schema) = alb(&root, &["schema"]);
    assert_eq!(code, 0);
    insta::assert_snapshot!("schema_surface", pretty(&schema));
}

// ── the write verbs (§7.2, §7.3) ────────────────────────────────────────────

#[test]
fn verb_add_fresh_then_overwrite() {
    let root = fresh_root();
    // A fresh `add` runs free: it creates the record it *is* (§7.3, §18).
    let (code, fresh) = alb(&root, &["csa", "John Appleseed", "--closeness", "close"]);
    assert_eq!(code, 0);
    insta::assert_snapshot!("verb_add_fresh", pretty(&fresh));

    // Landing on a slug that exists is an overwrite — a mutation, so piped and
    // without -y it is exit 5 with the change to review (§7.3).
    let (code, pending) = alb(&root, &["csa", "john_appleseed", "--role", "friend"]);
    assert_eq!(code, 5);
    insta::assert_snapshot!("verb_add_overwrite_pending", pretty(&redact(pending)));

    let (code, applied) = alb(&root, &["csa", "john_appleseed", "--role", "friend", "-y"]);
    assert_eq!(code, 0);
    insta::assert_snapshot!("verb_add_overwrite_applied", pretty(&applied));
}

#[test]
fn dry_run_emits_a_plan_and_writes_nothing() {
    let root = fresh_root();
    let (code, plan) = alb(&root, &["csa", "mara", "-n"]);
    assert_eq!(code, 0);
    insta::assert_snapshot!("verb_add_dry_run", pretty(&redact(plan)));
    // Nothing was written (§7.2).
    assert_eq!(alb(&root, &["get", "mara"]).0, 4);
}

#[test]
fn add_refuses_a_slug_another_kind_holds() {
    let root = fresh_root();
    assert_eq!(alb(&root, &["csa", "book_club", "-k", "group"]).0, 0);
    // One `read_dir`, and hard: two files, one ref (§5.4, §18).
    let (code, err) = alb(&root, &["csa", "book_club"]);
    assert_eq!(code, 3);
    insta::assert_snapshot!("refusal_slug_held_by_another_kind", pretty(&err));
}

#[test]
fn add_warns_softly_on_a_cross_node_duplicate() {
    let root = fresh_root();
    assert_eq!(alb(&root, &["csa", "alex"]).0, 0);
    // Across nodes the check is a walk, so it stays soft: the write succeeds, the
    // record goes to stdout, and the warning rides stderr (§5.4, §18).
    let ((code, record), stderr) = alb_env(&root, &["cso", "alex"], &[]);
    assert_eq!(code, 0, "a cross-node duplicate is never refused");
    assert_eq!(record["home"], "cso");
    let findings: Value = serde_json::from_str(stderr.trim()).unwrap();
    insta::assert_snapshot!("warn_duplicate_slug", pretty(&findings));
    // Both records exist; a resolve meeting two lists them rather than guessing.
    assert_eq!(alb(&root, &["get", "alex"]).0, 2);
}

#[test]
fn verb_edit_and_edit_k_renames_the_file() {
    let root = fresh_root();
    alb(&root, &["csa", "dare_robotics", "--role", "employer"]);

    // What a hand does not give, the record keeps (I1).
    let (code, edited) = alb(
        &root,
        &["edit", "dare_robotics", "--origin", "a job fair", "-y"],
    );
    assert_eq!(code, 0);
    insta::assert_snapshot!("verb_edit", pretty(&edited));

    // Changing what it *is* renames the file — a visible structural act (§7.2).
    let (code, rekinded) = alb(
        &root,
        &["edit", "dare_robotics", "-k", "organization", "-y"],
    );
    assert_eq!(code, 0);
    insta::assert_snapshot!("verb_edit_kind", pretty(&rekinded));
    assert!(
        csa_meta(&root)
            .join("csa__organization__dare_robotics.json")
            .exists(),
        "the file wears its new kind"
    );
    assert!(
        !csa_meta(&root)
            .join("csa__person__dare_robotics.json")
            .exists(),
        "and not its old one"
    );
}

#[test]
fn verb_rename_cascades_refs() {
    let root = fresh_root();
    alb(&root, &["csa", "johnn"]);
    alb(
        &root,
        &["csa", "mara", "-r", "album:johnn", "--closeness", "close"],
    );
    let referrer = csa_meta(&root).join("csa__person__mara.json");
    let before = std::fs::read_to_string(&referrer).unwrap();

    let (code, renamed) = alb(&root, &["rename", "johnn", "john", "-y"]);
    assert_eq!(code, 0);
    insta::assert_snapshot!("verb_rename", pretty(&renamed));

    let after = std::fs::read_to_string(&referrer).unwrap();
    insta::assert_snapshot!("on_disk_cascaded_ref", after.clone());
    assert!(
        !after.contains("johnn"),
        "the old slug is gone from the ref"
    );
    // The rewrite touches the envelope's refs and nothing else (I5).
    assert_eq!(
        before.replace("album:johnn", "album:john"),
        after,
        "only the ref changed"
    );
}

#[test]
fn verb_rename_refuses_an_occupied_slug() {
    let root = fresh_root();
    alb(&root, &["csa", "johnn"]);
    alb(&root, &["cso", "john"]);
    // Tree-wide and hard, unlike `add`'s cross-node warning (§7.2).
    let (code, err) = alb(&root, &["rename", "johnn", "john", "-y"]);
    assert_eq!(code, 3);
    insta::assert_snapshot!("refusal_rename_onto_occupied", pretty(&err));
}

#[test]
fn verb_move_carries_no_refs_with_it() {
    let root = fresh_root();
    alb(&root, &["csa", "alex"]);
    alb(&root, &["csa", "mara", "-r", "album:alex"]);

    let (code, moved) = alb(&root, &["move", "alex", "--to", "cso", "-y"]);
    assert_eq!(code, 0);
    insta::assert_snapshot!("verb_move", pretty(&moved));

    // A ref carries no path, so it survives a re-home untouched (§5.4).
    let (_, mara) = alb(&root, &["get", "mara"]);
    assert_eq!(mara["refs"], json!(["album:alex"]));
    // The file wears its new home's code, not a stale one carried across (§5.2).
    assert!(
        !meta_dir(&root, "cs_o_officium", "cso")
            .join("csa__person__alex.json")
            .exists()
    );
    assert!(
        meta_dir(&root, "cs_o_officium", "cso")
            .join("cso__person__alex.json")
            .exists()
    );
}

#[test]
fn an_entity_as_node_refuses_both_structural_verbs() {
    let root = fresh_root();
    alb(&root, &["-H", "csa_john_appleseed", "john_appleseed"]);

    let (code, rename) = alb(&root, &["rename", "john_appleseed", "jack", "-y"]);
    assert_eq!(code, 2);
    insta::assert_snapshot!("refusal_rename_entity_node", pretty(&rename));

    let (code, moved) = alb(&root, &["move", "john_appleseed", "--to", "cso", "-y"]);
    assert_eq!(code, 2);
    insta::assert_snapshot!("refusal_move_entity_node", pretty(&moved));
}

#[test]
fn verb_rm() {
    let root = fresh_root();
    alb(&root, &["csa", "alex"]);
    let (code, deleted) = alb(&root, &["rm", "alex", "-y"]);
    assert_eq!(code, 0);
    insta::assert_snapshot!("verb_rm", pretty(&deleted));
    assert_eq!(alb(&root, &["get", "alex"]).0, 4);
}

// ── the read verbs (§7.2) ───────────────────────────────────────────────────

#[test]
fn verbs_read() {
    let root = fresh_root();
    alb(
        &root,
        &[
            "csa",
            "mara",
            "--gender",
            "female",
            "--away",
            "260601..260615",
        ],
    );
    alb(&root, &["csa", "book_club", "-k", "group"]);
    alb(&root, &["cso", "dare_robotics", "-k", "organization"]);
    alb(&root, &["-H", "csa_john_appleseed", "john_appleseed"]);

    let mut out = String::new();
    for (label, args) in [
        ("get mara", vec!["get", "mara"]),
        ("get john_appleseed", vec!["get", "john_appleseed"]),
        ("list", vec!["list"]),
        ("list -k group", vec!["list", "-k", "group"]),
        ("where dare_robotics", vec!["where", "dare_robotics"]),
        ("where john_appleseed", vec!["where", "john_appleseed"]),
    ] {
        let (code, value) = alb(&root, &args);
        out.push_str(&format!("$ alb {}\n{}\n\n", label, pretty(&value)));
        assert_eq!(code, 0, "{label}");
    }
    insta::assert_snapshot!("verbs_read", out);
}

// ── the shapes on disk (I3, §18) ────────────────────────────────────────────

#[test]
fn on_disk_entity_stores_no_location() {
    let root = fresh_root();
    alb(
        &root,
        &[
            "csa",
            "mara",
            "--closeness",
            "close",
            "-r",
            "album:book_club",
        ],
    );
    let raw = std::fs::read_to_string(csa_meta(&root).join("csa__person__mara.json")).unwrap();
    // No kind, no home, no slug, no key: all four are the file's location and name
    // (I3, §18). This snapshot is what proves it.
    insta::assert_snapshot!("on_disk_entity_file", raw.clone());
    for absent in ["kind", "home", "slug", "key", "person", "csa"] {
        assert!(!raw.contains(absent), "{absent:?} is not stored: {raw}");
    }
}

#[test]
fn an_entity_as_node_drops_its_slug_segment() {
    let root = fresh_root();
    alb(
        &root,
        &[
            "-H",
            "csa_john_appleseed",
            "john_appleseed",
            "--note",
            "his own node",
        ],
    );
    let path = root
        .join("c_contextus/c_s_societas/cs_a_amicitia/csa_john_appleseed_/csa_john_appleseed__")
        .join("csa_john_appleseed__person.json");
    assert!(path.exists(), "the filename carries only the kind (§5.2)");
    // And the walk supplies the slug the filename does not carry.
    let (code, got) = alb(&root, &["get", "john_appleseed"]);
    assert_eq!(code, 0);
    insta::assert_snapshot!("entity_as_node", pretty(&got));
}

// ── the hand, the rule, and the editor (§7.3, §9.3) ─────────────────────────

#[test]
fn the_editor_form_piped_prints_a_path_and_spawns_nothing() {
    let root = fresh_root();
    alb(&root, &["csa", "mara", "--note", "a remark"]);
    // `false` would fail if it ran; piped, nothing is spawned at all (§7.3, I8).
    let ((code, value), _) = alb_env(&root, &["edit", "mara", "--note"], &[("EDITOR", "false")]);
    assert_eq!(code, 0);
    assert!(
        value["path"]
            .as_str()
            .unwrap()
            .ends_with("csa__person__mara.json")
    );
}

#[test]
fn the_editor_form_opens_one_field_at_a_time() {
    let root = fresh_root();
    alb(&root, &["csa", "mara"]);
    // A buffer holds one value (§7.3), so naming two is a usage error.
    let (code, err) = alb(&root, &["edit", "mara", "--note", "--role"]);
    assert_eq!(code, 2);
    insta::assert_snapshot!("refusal_editor_two_fields", pretty(&err));
}

#[test]
fn write_verbs_are_refused_under_a_rule() {
    let root = fresh_root();
    alb(&root, &["csa", "mara"]);
    let rule = [("PANTHEON_RULE", "1")];

    for args in [
        vec!["csa", "alex"],
        vec!["edit", "mara", "--role", "friend", "-y"],
        vec!["rename", "mara", "mara_k", "-y"],
        vec!["move", "mara", "--to", "cso", "-y"],
        vec!["rm", "mara", "-y"],
    ] {
        let ((code, _), _) = alb_env(&root, &args, &rule);
        assert_eq!(code, 6, "{args:?} must be refused under a rule (§9.3, I2)");
    }
    // Reads run free, and `--dry-run` still computes: a rule may plan (§7.3).
    assert_eq!(alb_env(&root, &["get", "mara"], &rule).0.0, 0);
    assert_eq!(alb_env(&root, &["csa", "alex", "-n"], &rule).0.0, 0);
}

// ── the integration gate: `pan` reads what `alb` wrote (§5.0, §16) ──────────

#[test]
fn records_resolve_through_pan() {
    let root = fresh_root();
    alb(&root, &["csa", "mara"]);
    alb(&root, &["-H", "csa_john_appleseed", "john_appleseed"]);

    // `pan` discovers Album over PATH by running `alb schema` — it never links a
    // core (I5, §5.0). Both filename forms must resolve.
    let alb_bin = PathBuf::from(env!("CARGO_BIN_EXE_alb"));
    let bin_dir = alb_bin.parent().unwrap();
    let pan_bin = bin_dir.join("pan");
    assert!(
        pan_bin.exists(),
        "this gate needs `pan` built beside `alb`: run `cargo build --workspace` \
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
        .args(["resolve", "album:mara", "album:john_appleseed"])
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
    let root = fresh_root();
    alb(&root, &["csa", "mara"]);
    alb(&root, &["csa", "book_club", "-k", "group"]);
    alb(&root, &["cso", "mara"]);

    let cases: Vec<(&str, Vec<&str>)> = vec![
        ("no such slug", vec!["get", "nobody"]),
        ("a slug at two nodes", vec!["get", "mara"]),
        ("-c mints nothing", vec!["csa", "x", "-c"]),
        ("-a keys nothing", vec!["csa", "x", "-a", "260718"]),
        ("no series to read", vec!["series"]),
        ("a token album lacks", vec!["csa", "x", "-k", "task"]),
        ("a name is one token", vec!["csa", "john", "appleseed"]),
        ("add needs a name", vec!["add"]),
        ("a blank field", vec!["csa", "x", "--note", "   "]),
        ("a malformed away", vec!["csa", "x", "--away", "26060"]),
        ("a slug another kind holds", vec!["csa", "book_club"]),
        ("a malformed ref", vec!["csa", "x", "-r", "not-a-ref"]),
        ("an overwrite, piped", vec!["cso", "mara", "--role", "x"]),
    ];

    let mut out = String::new();
    for (label, args) in cases {
        let (code, value) = alb(&root, &args);
        let msg = value
            .get("error")
            .and_then(|e| e.get("msg"))
            .and_then(Value::as_str)
            .unwrap_or("<a pending change>");
        out.push_str(&format!("exit {code}: {label} — {msg}\n"));
    }
    insta::assert_snapshot!("exit_codes", out);
}
