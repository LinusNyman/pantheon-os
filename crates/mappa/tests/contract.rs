//! The frozen contract (§7.2): insta snapshots of `map`'s own JSON — the only thing
//! that crosses a component boundary (I4), so these are taken from the real binary
//! rather than the library behind it.
//!
//! Plan tokens are redacted (they hash the computed change). Slugs are **not**: a
//! slug is the record's identity and its name at once (§5.4). Nothing here depends
//! on the wall clock — a place has no key to date (§7.1). Regenerate these
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

/// A tree with two nodes to file at — `clh` (Habitat) and `clu` (Urbs) — plus a
/// definition-prefix node under `clh`, which is where an entity-as-node lives (§5.1):
/// a house you model as its own node, holding its own record and its own documents.
fn fresh_root() -> PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("map-snap-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    for (parent, ch, label) in [
        ("root", "c", "contextus"),
        ("c", "l", "locus"),
        ("cl", "h", "habitat"),
        ("cl", "u", "urbs"),
    ] {
        let (plan, _) = plan_new(&dir, parent, NewSpec::Triple { ch, label }).unwrap();
        plan.apply(&dir).unwrap();
    }
    let (plan, _) = plan_new(
        &dir,
        "clh",
        NewSpec::Def {
            definition: "old_mill",
        },
    )
    .unwrap();
    plan.apply(&dir).unwrap();
    dir
}

/// Run the real `map`, returning its exit code and the JSON it emitted — stdout when
/// it produced a value, stderr for the `{"error":…}` envelope (§7.3).
fn map(root: &Path, args: &[&str]) -> (i32, Value) {
    map_env(root, args, &[]).0
}

/// [`map`] with environment set for the child — never for this process (§7.3).
/// Returns stderr separately: a soft finding rides there while the record itself
/// goes to stdout (§5.4).
fn map_env(root: &Path, args: &[&str], env: &[(&str, &str)]) -> ((i32, Value), String) {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_map"));
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
        .join("c_l_locus")
        .join(node_dir)
        .join(format!("{code}__"))
}

/// The `clh` meta dir, where most of these records land.
fn clh_meta(root: &Path) -> PathBuf {
    meta_dir(root, "cl_h_habitat", "clh")
}

// ── the discovery surface (§7.2) ────────────────────────────────────────────

#[test]
fn schema_surface() {
    let root = fresh_root();
    let (code, schema) = map(&root, &["schema"]);
    assert_eq!(code, 0);
    insta::assert_snapshot!("schema_surface", pretty(&schema));
}

// ── the write verbs (§7.2, §7.3) ────────────────────────────────────────────

#[test]
fn verb_add_fresh_then_overwrite() {
    let root = fresh_root();
    // A fresh `add` runs free: it creates the record it *is* (§7.3, §18).
    let (code, fresh) = map(
        &root,
        &[
            "clu",
            "Kafe Esaias",
            "--coordinates",
            "59.3293,18.0686",
            "--timezone",
            "Europe/Stockholm",
        ],
    );
    assert_eq!(code, 0);
    insta::assert_snapshot!("verb_add_fresh", pretty(&fresh));

    // Landing on a slug that exists is an overwrite — a mutation, so piped and
    // without -y it is exit 5 with the change to review (§7.3).
    let (code, pending) = map(&root, &["clu", "kafe_esaias", "--address", "Frejgatan 1"]);
    assert_eq!(code, 5);
    insta::assert_snapshot!("verb_add_overwrite_pending", pretty(&redact(pending)));

    let (code, applied) = map(
        &root,
        &["clu", "kafe_esaias", "--address", "Frejgatan 1", "-y"],
    );
    assert_eq!(code, 0);
    insta::assert_snapshot!("verb_add_overwrite_applied", pretty(&applied));
}

#[test]
fn dry_run_emits_a_plan_and_writes_nothing() {
    let root = fresh_root();
    let (code, plan) = map(&root, &["clu", "the_moor", "-k", "region", "-n"]);
    assert_eq!(code, 0);
    insta::assert_snapshot!("verb_add_dry_run", pretty(&redact(plan)));
    // Nothing was written (§7.2).
    assert_eq!(map(&root, &["get", "the_moor"]).0, 4);
}

#[test]
fn add_refuses_a_slug_another_kind_holds() {
    let root = fresh_root();
    assert_eq!(map(&root, &["clu", "sodermalm", "-k", "region"]).0, 0);
    // One `read_dir`, and hard: two files, one ref (§5.4, §18).
    let (code, err) = map(&root, &["clu", "sodermalm"]);
    assert_eq!(code, 3);
    insta::assert_snapshot!("refusal_slug_held_by_another_kind", pretty(&err));
}

#[test]
fn add_warns_softly_on_a_cross_node_duplicate() {
    let root = fresh_root();
    assert_eq!(map(&root, &["clh", "the_studio"]).0, 0);
    // Across nodes the check is a walk, so it stays soft: the write succeeds, the
    // record goes to stdout, and the warning rides stderr (§5.4, §18).
    let ((code, record), stderr) = map_env(&root, &["clu", "the_studio"], &[]);
    assert_eq!(code, 0, "a cross-node duplicate is never refused");
    assert_eq!(record["home"], "clu");
    let findings: Value = serde_json::from_str(stderr.trim()).unwrap();
    insta::assert_snapshot!("warn_duplicate_slug", pretty(&findings));
    // Both records exist; a resolve meeting two lists them rather than guessing.
    assert_eq!(map(&root, &["get", "the_studio"]).0, 2);
}

#[test]
fn verb_edit_and_edit_k_renames_the_file() {
    let root = fresh_root();
    map(
        &root,
        &["clu", "sodermalm", "--timezone", "Europe/Stockholm"],
    );

    // What a hand does not give, the record keeps (I1).
    let (code, edited) = map(
        &root,
        &["edit", "sodermalm", "--note", "the island south", "-y"],
    );
    assert_eq!(code, 0);
    insta::assert_snapshot!("verb_edit", pretty(&edited));

    // Changing what it *is* — a point corrected to an area — renames the file: a
    // visible structural act, not a silent field flip (§7.2).
    let (code, rekinded) = map(
        &root,
        &[
            "edit",
            "sodermalm",
            "-k",
            "region",
            "--bounds",
            "59.30,18.03,59.32,18.11",
            "-y",
        ],
    );
    assert_eq!(code, 0);
    insta::assert_snapshot!("verb_edit_kind", pretty(&rekinded));
    assert!(
        meta_dir(&root, "cl_u_urbs", "clu")
            .join("clu__region__sodermalm.json")
            .exists(),
        "the file wears its new kind"
    );
    assert!(
        !meta_dir(&root, "cl_u_urbs", "clu")
            .join("clu__location__sodermalm.json")
            .exists(),
        "and not its old one"
    );
}

#[test]
fn verb_rename_cascades_refs() {
    let root = fresh_root();
    map(&root, &["clh", "the_milll"]);
    map(&root, &["clh", "the_yard", "-r", "mappa:the_milll"]);
    let referrer = clh_meta(&root).join("clh__location__the_yard.json");
    let before = std::fs::read_to_string(&referrer).unwrap();

    let (code, renamed) = map(&root, &["rename", "the_milll", "the_mill", "-y"]);
    assert_eq!(code, 0);
    insta::assert_snapshot!("verb_rename", pretty(&renamed));

    let after = std::fs::read_to_string(&referrer).unwrap();
    insta::assert_snapshot!("on_disk_cascaded_ref", after.clone());
    assert!(
        !after.contains("the_milll"),
        "the old slug is gone from the ref"
    );
    // The rewrite touches the envelope's refs and nothing else (I5).
    assert_eq!(
        before.replace("mappa:the_milll", "mappa:the_mill"),
        after,
        "only the ref changed"
    );
}

#[test]
fn verb_rename_refuses_an_occupied_slug() {
    let root = fresh_root();
    map(&root, &["clh", "the_milll"]);
    map(&root, &["clu", "the_mill"]);
    // Tree-wide and hard, unlike `add`'s cross-node warning (§7.2).
    let (code, err) = map(&root, &["rename", "the_milll", "the_mill", "-y"]);
    assert_eq!(code, 3);
    insta::assert_snapshot!("refusal_rename_onto_occupied", pretty(&err));
}

#[test]
fn verb_move_carries_no_refs_with_it() {
    let root = fresh_root();
    map(&root, &["clh", "the_studio"]);
    map(&root, &["clh", "the_yard", "-r", "mappa:the_studio"]);

    let (code, moved) = map(&root, &["move", "the_studio", "--to", "clu", "-y"]);
    assert_eq!(code, 0);
    insta::assert_snapshot!("verb_move", pretty(&moved));

    // A ref carries no path, so it survives a re-home untouched (§5.4).
    let (_, yard) = map(&root, &["get", "the_yard"]);
    assert_eq!(yard["refs"], json!(["mappa:the_studio"]));
    // The file wears its new home's code, not a stale one carried across (§5.2).
    assert!(
        !meta_dir(&root, "cl_u_urbs", "clu")
            .join("clh__location__the_studio.json")
            .exists()
    );
    assert!(
        meta_dir(&root, "cl_u_urbs", "clu")
            .join("clu__location__the_studio.json")
            .exists()
    );
}

#[test]
fn an_entity_as_node_refuses_both_structural_verbs() {
    let root = fresh_root();
    map(&root, &["-H", "clh_old_mill", "old_mill"]);

    let (code, rename) = map(&root, &["rename", "old_mill", "the_mill", "-y"]);
    assert_eq!(code, 2);
    insta::assert_snapshot!("refusal_rename_entity_node", pretty(&rename));

    let (code, moved) = map(&root, &["move", "old_mill", "--to", "clu", "-y"]);
    assert_eq!(code, 2);
    insta::assert_snapshot!("refusal_move_entity_node", pretty(&moved));
}

#[test]
fn verb_rm() {
    let root = fresh_root();
    map(&root, &["clh", "the_studio"]);
    let (code, deleted) = map(&root, &["rm", "the_studio", "-y"]);
    assert_eq!(code, 0);
    insta::assert_snapshot!("verb_rm", pretty(&deleted));
    assert_eq!(map(&root, &["get", "the_studio"]).0, 4);
}

// ── the two kinds (§8.2) ────────────────────────────────────────────────────

#[test]
fn a_region_may_straddle_the_antimeridian() {
    let root = fresh_root();
    // East below west is a wrap, not an error: Fiji is a real place, and refusing it
    // would refuse the world to keep the arithmetic tidy (§8.2).
    let (code, fiji) = map(
        &root,
        &[
            "clu",
            "fiji",
            "-k",
            "region",
            "--bounds",
            "-20.7,176.9,-12.4,-178.2",
        ],
    );
    assert_eq!(code, 0);
    insta::assert_snapshot!("verb_add_region_wrapping", pretty(&fiji));

    // North below south is an inverted extent, and is refused (§8.2).
    let (code, err) = map(
        &root,
        &[
            "clu",
            "upside_down",
            "-k",
            "region",
            "--bounds",
            "10,0,5,10",
        ],
    );
    assert_eq!(code, 3);
    insta::assert_snapshot!("refusal_bounds_inverted", pretty(&err));
}

#[test]
fn a_kind_is_the_filename_and_validate_never_sees_it() {
    let root = fresh_root();
    // `validate` is handed a record and not its token (§7.1), so a `location`
    // carrying `bounds` is stored, not refused. The kind is the filename's, and
    // `edit -k` is what moves it — a check here would need the very thing the trait
    // withholds. This test pins that, so a later hand does not "fix" it into a
    // refusal that `edit -k` would then have to route around.
    let (code, odd) = map(
        &root,
        &["clu", "an_odd_point", "--bounds", "59.30,18.03,59.32,18.11"],
    );
    assert_eq!(code, 0);
    assert_eq!(odd["kind"], "location");
    assert!(odd["data"]["bounds"].is_object());
}

// ── the shapes on disk (I3, §18) ────────────────────────────────────────────

#[test]
fn on_disk_entity_stores_no_path_fields() {
    let root = fresh_root();
    map(
        &root,
        &[
            "clh",
            "the_yard",
            "--coordinates",
            "59.3293,18.0686",
            "-r",
            "mappa:the_moor",
        ],
    );
    let raw =
        std::fs::read_to_string(clh_meta(&root).join("clh__location__the_yard.json")).unwrap();
    // No kind, no home, no slug, no key: all four are the file's location and name
    // (I3, §18). This snapshot is what proves it.
    insta::assert_snapshot!("on_disk_entity_file", raw.clone());
    for absent in ["kind", "home", "slug", "key", "location", "clh"] {
        assert!(!raw.contains(absent), "{absent:?} is not stored: {raw}");
    }
}

#[test]
fn an_entity_as_node_drops_its_slug_segment() {
    let root = fresh_root();
    map(
        &root,
        &[
            "-H",
            "clh_old_mill",
            "old_mill",
            "--note",
            "a house of its own",
        ],
    );
    let path = root
        .join("c_contextus/c_l_locus/cl_h_habitat/clh_old_mill_/clh_old_mill__")
        .join("clh_old_mill__location.json");
    assert!(path.exists(), "the filename carries only the kind (§5.2)");
    // And the walk supplies the slug the filename does not carry.
    let (code, got) = map(&root, &["get", "old_mill"]);
    assert_eq!(code, 0);
    insta::assert_snapshot!("entity_as_node", pretty(&got));
}

// ── the read verbs (§7.2) ───────────────────────────────────────────────────

#[test]
fn verbs_read() {
    let root = fresh_root();
    map(
        &root,
        &[
            "clu",
            "kafe_esaias",
            "--coordinates",
            "59.3293,18.0686",
            "--address",
            "Frejgatan 1",
            "--timezone",
            "Europe/Stockholm",
        ],
    );
    map(
        &root,
        &[
            "clu",
            "sodermalm",
            "-k",
            "region",
            "--bounds",
            "59.30,18.03,59.32,18.11",
        ],
    );
    map(
        &root,
        &[
            "clh",
            "standup_room",
            "--url",
            "https://example.invalid/j/42",
        ],
    );
    map(&root, &["-H", "clh_old_mill", "old_mill"]);

    let mut out = String::new();
    for (label, args) in [
        ("get kafe_esaias", vec!["get", "kafe_esaias"]),
        ("get old_mill", vec!["get", "old_mill"]),
        ("list", vec!["list"]),
        ("list -k region", vec!["list", "-k", "region"]),
        ("where sodermalm", vec!["where", "sodermalm"]),
        ("where old_mill", vec!["where", "old_mill"]),
    ] {
        let (code, value) = map(&root, &args);
        out.push_str(&format!("$ map {}\n{}\n\n", label, pretty(&value)));
        assert_eq!(code, 0, "{label}");
    }
    insta::assert_snapshot!("verbs_read", out);
}

// ── the hand, the rule, and the editor (§7.3, §9.3) ─────────────────────────

#[test]
fn the_editor_form_piped_prints_a_path_and_spawns_nothing() {
    let root = fresh_root();
    map(&root, &["clh", "the_yard", "--note", "a remark"]);
    // `false` would fail if it ran; piped, nothing is spawned at all (§7.3, I8).
    let ((code, value), _) = map_env(
        &root,
        &["edit", "the_yard", "--note"],
        &[("EDITOR", "false")],
    );
    assert_eq!(code, 0);
    assert!(
        value["path"]
            .as_str()
            .unwrap()
            .ends_with("clh__location__the_yard.json")
    );
}

#[test]
fn the_editor_form_opens_one_field_at_a_time() {
    let root = fresh_root();
    map(&root, &["clh", "the_yard"]);
    // A buffer holds one value (§7.3), so naming two is a usage error.
    let (code, err) = map(&root, &["edit", "the_yard", "--note", "--address"]);
    assert_eq!(code, 2);
    insta::assert_snapshot!("refusal_editor_two_fields", pretty(&err));
}

#[test]
fn write_verbs_are_refused_under_a_rule() {
    let root = fresh_root();
    map(&root, &["clh", "the_yard"]);
    let rule = [("PANTHEON_RULE", "1")];

    for args in [
        vec!["clh", "the_studio"],
        vec!["edit", "the_yard", "--note", "a remark", "-y"],
        vec!["rename", "the_yard", "the_court", "-y"],
        vec!["move", "the_yard", "--to", "clu", "-y"],
        vec!["rm", "the_yard", "-y"],
    ] {
        let ((code, _), _) = map_env(&root, &args, &rule);
        assert_eq!(code, 6, "{args:?} must be refused under a rule (§9.3, I2)");
    }
    // Reads run free, and `--dry-run` still computes: a rule may plan (§7.3).
    assert_eq!(map_env(&root, &["get", "the_yard"], &rule).0.0, 0);
    assert_eq!(map_env(&root, &["clh", "the_studio", "-n"], &rule).0.0, 0);
}

// ── the integration gate: `pan` reads what `map` wrote (§5.0, §16) ──────────

#[test]
fn records_resolve_through_pan() {
    let root = fresh_root();
    map(&root, &["clh", "the_yard"]);
    map(&root, &["-H", "clh_old_mill", "old_mill"]);

    // `pan` discovers Mappa over PATH by running `map schema` — it never links a
    // core (I5, §5.0). Both filename forms must resolve.
    let map_bin = PathBuf::from(env!("CARGO_BIN_EXE_map"));
    let bin_dir = map_bin.parent().unwrap();
    let pan_bin = bin_dir.join("pan");
    // Cargo builds `map` for this test (CARGO_BIN_EXE_map) but not `pan`, which
    // belongs to another crate — so the workspace bins must be built first. CI does
    // that in the `nextest + insta` job; locally, `cargo build --workspace --bins`.
    // Failing loudly beats skipping: a gate that quietly passes is not a gate.
    assert!(
        pan_bin.exists(),
        "this gate needs `pan` built beside `map`: run `cargo build --workspace --bins` \
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
        .args(["resolve", "mappa:the_yard", "mappa:old_mill"])
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
    map(&root, &["clh", "the_yard"]);
    map(&root, &["clu", "sodermalm", "-k", "region"]);
    map(&root, &["clu", "the_yard"]);

    let cases: Vec<(&str, Vec<&str>)> = vec![
        ("no such slug", vec!["get", "nowhere"]),
        ("a slug at two nodes", vec!["get", "the_yard"]),
        ("-c mints nothing", vec!["clh", "x", "-c"]),
        ("-a keys nothing", vec!["clh", "x", "-a", "260718"]),
        ("no series to read", vec!["series"]),
        ("a token mappa lacks", vec!["clh", "x", "-k", "person"]),
        ("a name is one token", vec!["clh", "the", "yard"]),
        ("add needs a name", vec!["add"]),
        ("a blank field", vec!["clh", "x", "--address", "   "]),
        ("a url with whitespace", vec!["clh", "x", "--url", "a link"]),
        (
            "coordinates that are one token",
            vec!["clh", "x", "--coordinates", "59.3293"],
        ),
        (
            "coordinates that are not numbers",
            vec!["clh", "x", "--coordinates", "north,east"],
        ),
        (
            "a latitude off the globe",
            vec!["clh", "x", "--coordinates", "91,0"],
        ),
        (
            "bounds with three corners' worth",
            vec!["clh", "x", "--bounds", "1,2,3"],
        ),
        (
            "bounds inverted",
            vec!["clh", "x", "-k", "region", "--bounds", "10,0,5,10"],
        ),
        ("a slug another kind holds", vec!["clu", "sodermalm"]),
        ("a malformed ref", vec!["clh", "x", "-r", "not-a-ref"]),
        (
            "an overwrite, piped",
            vec!["clu", "the_yard", "--address", "x"],
        ),
    ];

    let mut out = String::new();
    for (label, args) in cases {
        let (code, value) = map(&root, &args);
        let msg = value
            .get("error")
            .and_then(|e| e.get("msg"))
            .and_then(Value::as_str)
            .unwrap_or("<a pending change>");
        out.push_str(&format!("exit {code}: {label} — {msg}\n"));
    }
    insta::assert_snapshot!("exit_codes", out);
}
