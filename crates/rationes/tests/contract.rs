//! The frozen contract (§7.2): insta snapshots of `rat`'s own JSON — the only thing
//! that crosses a component boundary (I4), so these are taken from the real binary
//! rather than the library behind it.
//!
//! Plan tokens are redacted (they hash the computed change). Slugs and keys are
//! **not**: a slug is the record's identity and its name at once, and a balance's key
//! is the day it was read (§5.4). Nothing here depends on the wall clock — every
//! reading is given an explicit `-a`, which is the price of a series keyed by date.
//! Regenerate these deliberately, never blindly.

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

/// A tree with the three Res nodes a holding files under — `crb` (Bona), `crp`
/// (Pecunia), `cri` (Iura) — plus a definition-prefix node under `crp`, which is
/// where an entity-as-node lives (§5.1). The homes are the reference tree's, an
/// illustration and never a shape the tools impose (§8, §6.2).
fn fresh_root() -> PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("rat-snap-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    for (parent, ch, label) in [
        ("root", "c", "contextus"),
        ("c", "r", "res"),
        ("cr", "b", "bona"),
        ("cr", "p", "pecunia"),
        ("cr", "i", "iura"),
    ] {
        let (plan, _) = plan_new(&dir, parent, NewSpec::Triple { ch, label }).unwrap();
        plan.apply(&dir).unwrap();
    }
    let (plan, _) = plan_new(
        &dir,
        "crp",
        NewSpec::Def {
            definition: "some_bank",
        },
    )
    .unwrap();
    plan.apply(&dir).unwrap();
    dir
}

/// Run the real `rat`, returning its exit code and the JSON it emitted — stdout when
/// it produced a value, stderr for the `{"error":…}` envelope (§7.3).
fn rat(root: &Path, args: &[&str]) -> (i32, Value) {
    rat_env(root, args, &[]).0
}

/// [`rat`] with environment set for the child — never for this process (§7.3).
/// Returns stderr separately: a soft finding rides there while the record itself
/// goes to stdout (§5.4).
fn rat_env(root: &Path, args: &[&str], env: &[(&str, &str)]) -> ((i32, Value), String) {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_rat"));
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

fn node_dir(root: &Path, code: &str) -> PathBuf {
    let res = root.join("c_contextus").join("c_r_res");
    match code {
        "crb" => res.join("cr_b_bona"),
        "crp" => res.join("cr_p_pecunia"),
        "cri" => res.join("cr_i_iura"),
        other => panic!("no node dir mapped for {other}"),
    }
}

fn meta_dir(root: &Path, code: &str) -> PathBuf {
    node_dir(root, code).join(format!("{code}__"))
}

/// Where a holding's balance series sits, whether or not it exists (§5.2, §8.3).
fn balance_file(root: &Path, code: &str, slug: &str) -> PathBuf {
    meta_dir(root, code).join(format!("{code}__balance__{slug}.jsonl"))
}

/// A `crp` account with one reading on it — the fixture most tests start from.
fn with_checking(root: &Path) {
    assert_eq!(rat(root, &["crp", "checking", "--currency", "usd"]).0, 0);
    assert_eq!(rat(root, &["crp", "checking", "4200", "-a", "260718"]).0, 0);
}

// ── the discovery surface (§7.2) ────────────────────────────────────────────

/// The declaration that makes Rationes the first two-shape core (§7.1): three
/// `partitioned` tokens and one `series` whose `named` bit is **false**.
#[test]
fn schema_surface() {
    let root = fresh_root();
    let (code, schema) = rat(&root, &["schema"]);
    assert_eq!(code, 0);
    insta::assert_snapshot!("schema_surface", pretty(&schema));
}

#[test]
fn help_and_version() {
    let root = fresh_root();
    let (code, help) = rat(&root, &["help"]);
    assert_eq!(code, 0);
    insta::assert_snapshot!("help", pretty(&help));
    let (code, version) = rat(&root, &["version"]);
    assert_eq!(code, 0);
    insta::assert_snapshot!("version", pretty(&version));
}

// ── the write verbs (§7.2, §7.3) ────────────────────────────────────────────

#[test]
fn verb_add_files_a_holding_then_overwrites_it() {
    let root = fresh_root();
    // A fresh `add` runs free: it creates the record it *is* (§7.3, §18).
    let (code, fresh) = rat(&root, &["crp", "Checking", "--currency", "usd"]);
    assert_eq!(code, 0);
    insta::assert_snapshot!("verb_add_holding_fresh", pretty(&fresh));

    // Landing on a slug that exists is an overwrite — a mutation, so piped and
    // without -y it is exit 5 with the change to review (§7.3).
    let (code, pending) = rat(&root, &["crp", "checking", "--note", "day to day"]);
    assert_eq!(code, 5);
    insta::assert_snapshot!(
        "verb_add_holding_overwrite_pending",
        pretty(&redact(pending))
    );

    let (code, applied) = rat(&root, &["crp", "checking", "--note", "day to day", "-y"]);
    assert_eq!(code, 0);
    insta::assert_snapshot!("verb_add_holding_overwrite_applied", pretty(&applied));
}

/// The amount is the fork (§8.3): a second positional makes the write a balance
/// reading on a holding that already exists, and its first reading mints the series.
#[test]
fn verb_add_writes_a_balance_reading() {
    let root = fresh_root();
    assert_eq!(rat(&root, &["crp", "checking", "--currency", "usd"]).0, 0);
    assert!(!balance_file(&root, "crp", "checking").exists());

    let (code, fresh) = rat(&root, &["crp", "checking", "4200", "-a", "260718"]);
    assert_eq!(code, 0);
    insta::assert_snapshot!("verb_add_balance_fresh", pretty(&fresh));
    // The determinant's first reading mints the series, without `-c` (§7.3, §8.3).
    assert!(balance_file(&root, "crp", "checking").exists());

    // A second reading on the same key is an overwrite — I1's correction path for a
    // figure read wrong, shown and confirmed before it commits (§6.1, §7.3).
    let (code, pending) = rat(&root, &["crp", "checking", "4250", "-a", "260718"]);
    assert_eq!(code, 5);
    insta::assert_snapshot!(
        "verb_add_balance_overwrite_pending",
        pretty(&redact(pending))
    );

    // A different key is a different reading and runs free.
    let (code, next) = rat(&root, &["crp", "checking", "4310", "-a", "260719"]);
    assert_eq!(code, 0);
    insta::assert_snapshot!("verb_add_balance_next_key", pretty(&next));
}

/// The holding is found tree-wide, so a reading needs no home: the lookup that proves
/// the determinant exists is the lookup that says which node the series lives at.
#[test]
fn a_reading_finds_its_holding_without_a_home() {
    let root = fresh_root();
    assert_eq!(rat(&root, &["crb", "bicycle", "-k", "asset"]).0, 0);
    let (code, written) = rat(&root, &["bicycle", "900", "-a", "260718"]);
    assert_eq!(code, 0);
    assert_eq!(written["home"], "crb");
    assert!(balance_file(&root, "crb", "bicycle").exists());
}

#[test]
fn verb_edit_corrects_a_holding_in_place() {
    let root = fresh_root();
    with_checking(&root);
    let (code, pending) = rat(&root, &["edit", "checking", "--note", "joint"]);
    assert_eq!(code, 5);
    insta::assert_snapshot!("verb_edit_pending", pretty(&redact(pending)));

    let (code, applied) = rat(&root, &["edit", "checking", "--note", "joint", "-y"]);
    assert_eq!(code, 0);
    // What a hand does not give, the record keeps (I1) — the currency survives.
    insta::assert_snapshot!("verb_edit_applied", pretty(&applied));
}

/// `edit -k` renames the file: changing what a holding fundamentally *is* is a
/// visible structural act, not a silent field flip (§7.2).
#[test]
fn verb_edit_kind_renames_the_file() {
    let root = fresh_root();
    assert_eq!(rat(&root, &["crb", "bicycle"]).0, 0);
    assert!(
        meta_dir(&root, "crb")
            .join("crb__account__bicycle.json")
            .exists()
    );

    let (code, edited) = rat(&root, &["edit", "bicycle", "-k", "asset", "-y"]);
    assert_eq!(code, 0);
    insta::assert_snapshot!("verb_edit_kind", pretty(&edited));
    assert!(
        meta_dir(&root, "crb")
            .join("crb__asset__bicycle.json")
            .exists()
    );
    assert!(
        !meta_dir(&root, "crb")
            .join("crb__account__bicycle.json")
            .exists()
    );
}

/// A rename carries the balance series with it: the series' name *is* the holding's
/// slug, so leaving it behind would strand it (§7.2, §8.3, §10.2). The refs follow
/// too — the same cascade every core's `rename` runs (§5.4).
#[test]
fn verb_rename_carries_the_balance_series_and_cascades() {
    let root = fresh_root();
    with_checking(&root);
    // Something that points at it, so the cascade has work to do (§5.4).
    assert_eq!(
        rat(
            &root,
            &["crb", "toolbox", "-k", "asset", "-r", "rationes:checking"]
        )
        .0,
        0
    );

    let (code, renamed) = rat(&root, &["rename", "checking", "current", "-y"]);
    assert_eq!(code, 0);
    insta::assert_snapshot!("verb_rename", pretty(&renamed));

    assert!(balance_file(&root, "crp", "current").exists());
    assert!(!balance_file(&root, "crp", "checking").exists());
    // The readings themselves came along, unread by the rename.
    let (code, trend) = rat(&root, &["series", "current"]);
    assert_eq!(code, 0);
    assert_eq!(trend.as_array().unwrap().len(), 1);
    // And the ref that pointed at the old slug now points at the new one.
    let (_, toolbox) = rat(&root, &["get", "toolbox"]);
    assert_eq!(toolbox["refs"], json!(["rationes:current"]));
}

/// A holding with no readings has nothing to carry, and says so by omitting the key
/// rather than reporting a move that did not happen.
#[test]
fn verb_rename_of_a_holding_with_no_readings_carries_nothing() {
    let root = fresh_root();
    assert_eq!(rat(&root, &["cri", "passport", "-k", "claim"]).0, 0);
    let (code, renamed) = rat(&root, &["rename", "passport", "travel_document", "-y"]);
    assert_eq!(code, 0);
    assert!(renamed.get("series").is_none());
}

/// A re-home carries the series in the same transaction — that series exists only
/// because the holding does, and would otherwise strand at a node its determinant has
/// left (§7.2, §10.2).
#[test]
fn verb_move_carries_the_balance_series() {
    let root = fresh_root();
    with_checking(&root);
    let (code, moved) = rat(&root, &["mv", "checking", "--to", "crb", "-y"]);
    assert_eq!(code, 0);
    insta::assert_snapshot!("verb_move", pretty(&moved));

    assert!(balance_file(&root, "crb", "checking").exists());
    assert!(!balance_file(&root, "crp", "checking").exists());
    let (code, trend) = rat(&root, &["series", "checking"]);
    assert_eq!(code, 0);
    assert_eq!(trend[0]["home"], "crb");
}

#[test]
fn verb_rm_takes_the_series_with_the_holding() {
    let root = fresh_root();
    with_checking(&root);
    let (code, removed) = rat(&root, &["rm", "checking", "-y"]);
    assert_eq!(code, 0);
    insta::assert_snapshot!("verb_rm_holding", pretty(&removed));
    assert!(
        !meta_dir(&root, "crp")
            .join("crp__account__checking.json")
            .exists()
    );
    // Left behind it would be a stranded series — a `pan validate` finding rather
    // than a record (§10.2).
    assert!(!balance_file(&root, "crp", "checking").exists());
}

/// `-a` names one reading: the only way a reading leaves the record, since an
/// overwrite corrects a figure but nothing rewrites a day away (I1, §6.1, §7.2).
#[test]
fn verb_rm_at_a_key_drops_one_reading() {
    let root = fresh_root();
    with_checking(&root);
    assert_eq!(
        rat(&root, &["crp", "checking", "4310", "-a", "260719"]).0,
        0
    );

    let (code, removed) = rat(&root, &["rm", "checking", "-a", "260719", "-y"]);
    assert_eq!(code, 0);
    insta::assert_snapshot!("verb_rm_reading", pretty(&removed));
    // The holding and its earlier reading both stand.
    assert_eq!(rat(&root, &["get", "checking"]).0, 0);
    let (_, trend) = rat(&root, &["series", "checking"]);
    assert_eq!(trend.as_array().unwrap().len(), 1);
}

// ── the read verbs (§7.2) ───────────────────────────────────────────────────

/// Every read that derives, in one snapshot: `list` carries each holding's latest
/// balance, `get` the same for one, and `series` the trend whole (I1, §8.3).
#[test]
fn verbs_read() {
    let root = fresh_root();
    with_checking(&root);
    assert_eq!(
        rat(&root, &["crp", "checking", "4310", "-a", "260719"]).0,
        0
    );
    assert_eq!(
        rat(
            &root,
            &["crb", "bicycle", "-k", "asset", "--currency", "usd"]
        )
        .0,
        0
    );
    assert_eq!(rat(&root, &["crb", "bicycle", "900", "-a", "260718"]).0, 0);
    assert_eq!(
        rat(
            &root,
            &["cri", "passport", "-k", "claim", "--expires", "300101"]
        )
        .0,
        0
    );

    let mut out = String::new();
    for (label, args) in [
        ("list", vec!["list"]),
        ("list -k asset", vec!["list", "-k", "asset"]),
        ("get checking", vec!["get", "checking"]),
        ("series checking", vec!["series", "checking"]),
        (
            "series checking --from 260719",
            vec!["series", "checking", "--from", "260719"],
        ),
        ("where passport", vec!["where", "passport"]),
    ] {
        let (code, value) = rat(&root, &args);
        out.push_str(&format!("$ rat {label}\n{code} {}\n\n", pretty(&value)));
    }
    insta::assert_snapshot!("verbs_read", out);
}

/// Net worth folds the latest balance of the kinds that *have* one, and is never
/// stored (I1, §8.3). A `claim` — the passport — is not part of it, and the fold is
/// **by currency**: adding dollars to shares would be a figure precise and false.
#[test]
fn net_worth_folds_only_what_carries_a_balance() {
    let root = fresh_root();
    with_checking(&root);
    assert_eq!(
        rat(
            &root,
            &["crb", "bicycle", "-k", "asset", "--currency", "usd"]
        )
        .0,
        0
    );
    assert_eq!(rat(&root, &["crb", "bicycle", "900", "-a", "260718"]).0, 0);
    // Priced in another unit, so it folds into its own bucket rather than the total.
    assert_eq!(
        rat(
            &root,
            &["crb", "vanguard", "-k", "asset", "--currency", "shares"]
        )
        .0,
        0
    );
    assert_eq!(rat(&root, &["crb", "vanguard", "12", "-a", "260718"]).0, 0);
    // A claim carries no balance and so reaches the fold not at all.
    assert_eq!(
        rat(
            &root,
            &["cri", "passport", "-k", "claim", "--expires", "300101"]
        )
        .0,
        0
    );

    let (code, net) = rat(&root, &["list", "--net"]);
    assert_eq!(code, 0);
    insta::assert_snapshot!("list_net", pretty(&net));
}

/// A holding whose series was never minted is not found, and says which command
/// would mint it (§7.3).
#[test]
fn series_of_a_holding_with_no_readings_is_not_found() {
    let root = fresh_root();
    assert_eq!(rat(&root, &["crp", "checking"]).0, 0);
    let (code, err) = rat(&root, &["series", "checking"]);
    assert_eq!(code, 4);
    insta::assert_snapshot!("refusal_series_without_readings", pretty(&err));
}

// ── the refusals ────────────────────────────────────────────────────────────

/// **The determined-series trap** (§7.3, I5).
///
/// `Store::write_line` mints any `Shape::Series { named: false }` on first write,
/// because a determined series is minted by its determinant. Pensum's determinant is
/// the *node*, which the store resolves anyway; Rationes' is a holding **entity**,
/// which the store cannot see — it links no core. So `rat` proves the holding exists
/// in its own bin first, and a missing one is **not found (exit `4`)**, the same code
/// §7.3 gives an `add` that would append to a series that does not exist.
///
/// The assertion that matters is the second one: nothing was minted.
#[test]
fn refusal_a_balance_without_its_determinant() {
    let root = fresh_root();
    let (code, err) = rat(&root, &["crp", "nonexistent", "4200", "-a", "260718"]);
    assert_eq!(code, 4, "a missing determinant is not found (§7.3)");
    insta::assert_snapshot!("refusal_balance_without_determinant", pretty(&err));
    assert!(
        !balance_file(&root, "crp", "nonexistent").exists(),
        "the store would have minted this; `rat` is what stops it (I5, §7.3)"
    );
    // And a typo on a holding that *does* exist is the same not-found, not a second
    // series beside the first.
    assert_eq!(rat(&root, &["crp", "checking"]).0, 0);
    let (code, _) = rat(&root, &["crp", "chekcing", "4200", "-a", "260718"]);
    assert_eq!(code, 4);
    assert!(!balance_file(&root, "crp", "chekcing").exists());
}

/// A `claim` carries no balance series (§8.3). The holding is there and the write is
/// well-formed, so this is a **validation** failure (exit `3`) rather than a
/// not-found: legality within a core's own vocabulary is that core's check on write
/// (§6.4).
#[test]
fn refusal_a_balance_on_a_claim() {
    let root = fresh_root();
    assert_eq!(
        rat(
            &root,
            &["cri", "passport", "-k", "claim", "--expires", "300101"]
        )
        .0,
        0
    );
    let (code, err) = rat(&root, &["cri", "passport", "100", "-a", "260718"]);
    assert_eq!(code, 3);
    insta::assert_snapshot!("refusal_balance_on_a_claim", pretty(&err));
    assert!(!balance_file(&root, "cri", "passport").exists());
}

/// A holding that carries readings may not become a kind that carries none: the
/// series would outlive the reason it exists (§8.3, §10.2).
#[test]
fn refusal_a_kind_change_that_would_strand_a_series() {
    let root = fresh_root();
    with_checking(&root);
    let (code, err) = rat(&root, &["edit", "checking", "-k", "claim", "-y"]);
    assert_eq!(code, 3);
    insta::assert_snapshot!("refusal_kind_change_strands_a_series", pretty(&err));
    assert!(balance_file(&root, "crp", "checking").exists());
}

/// A cascade onto an occupied slug spends the token that told the two apart, and §18
/// keeps no history to recover it (§7.2). The same walk that finds the refs finds it.
#[test]
fn refusal_rename_onto_an_occupied_slug() {
    let root = fresh_root();
    assert_eq!(rat(&root, &["crp", "checking"]).0, 0);
    assert_eq!(rat(&root, &["crb", "bicycle", "-k", "asset"]).0, 0);

    let (code, err) = rat(&root, &["rename", "bicycle", "checking", "-y"]);
    assert_eq!(code, 3, "an occupied slug is refused tree-wide (§7.2)");
    insta::assert_snapshot!("refusal_rename_onto_an_occupied_slug", pretty(&err));
    assert_eq!(rat(&root, &["get", "checking"]).0, 0);
    assert_eq!(rat(&root, &["get", "bicycle"]).0, 0);
}

/// The universal flags that mean nothing to this core, and the ones that mean
/// something only on one of its two forms (§7.3).
#[test]
fn refusals_of_the_universal_flags() {
    let root = fresh_root();
    with_checking(&root);

    let mut out = String::new();
    for (label, args) in [
        // A determined series is minted by its determinant, never by a hand (§7.3).
        ("-c on an add", vec!["crp", "savings", "-c"]),
        // `-k` selects within a shape, never across it (§7.2, verbatim).
        (
            "-k balance on the entity form",
            vec!["crp", "savings", "-k", "balance"],
        ),
        ("-k balance on a read", vec!["list", "-k", "balance"]),
        // `-a` dates a reading, and there is no reading here to date.
        ("-a with no amount", vec!["crp", "checking", "-a", "260718"]),
        // A reading is corrected by writing its key again (I1, §7.3).
        (
            "-a on an edit",
            vec!["edit", "checking", "--note", "x", "-a", "260718"],
        ),
        // Arity decides the form; content never does (§5.1, §7.3).
        (
            "a second token that is no figure",
            vec!["crp", "checking", "four"],
        ),
        ("three tokens", vec!["crp", "checking", "4200", "extra"]),
        // The holding's fields are not a reading's (I1).
        (
            "--currency on a reading",
            vec!["crp", "checking", "4200", "--currency", "usd"],
        ),
        // A determined series has no name of its own to be given (§5.4).
        ("series with no holding", vec!["series"]),
    ] {
        let (code, value) = rat(&root, &args);
        out.push_str(&format!("$ rat {label}\n{code} {}\n\n", pretty(&value)));
    }
    insta::assert_snapshot!("refusals_of_the_universal_flags", out);
}

/// Neither `rename` nor `move` may touch an entity-as-node: its slug *is* its node's
/// definition and its home *is* its node, so either would be a node operation, which
/// no core may perform (§5.2, §7.2, I3).
#[test]
fn an_entity_as_node_refuses_both_structural_verbs() {
    let root = fresh_root();
    assert_eq!(rat(&root, &["-H", "crp_some_bank", "some_bank"]).0, 0);

    let (code, rename) = rat(&root, &["rename", "some_bank", "the_bank", "-y"]);
    assert_eq!(code, 2);
    let (code2, moved) = rat(&root, &["move", "some_bank", "--to", "crb", "-y"]);
    assert_eq!(code2, 2);
    insta::assert_snapshot!(
        "refusals_entity_as_node",
        format!("{}\n\n{}", pretty(&rename), pretty(&moved))
    );
}

// ── the review path (§7.3) ──────────────────────────────────────────────────

/// Every write verb takes `--dry-run`, fresh or not — and a dry run of a first
/// reading mints nothing, because a plan that left a file behind would not be one.
#[test]
fn dry_run_writes_nothing_and_mints_nothing() {
    let root = fresh_root();
    assert_eq!(rat(&root, &["crp", "checking"]).0, 0);

    let (code, plan) = rat(&root, &["crp", "checking", "4200", "-a", "260718", "-n"]);
    assert_eq!(code, 0);
    insta::assert_snapshot!("dry_run_first_reading", pretty(&redact(plan)));
    assert!(!balance_file(&root, "crp", "checking").exists());
}

/// The plan token guards against acting on a stale review (§7.3).
#[test]
fn a_stale_plan_token_is_refused() {
    let root = fresh_root();
    with_checking(&root);
    let (code, plan) = rat(&root, &["crp", "checking", "4250", "-a", "260718", "-n"]);
    assert_eq!(code, 0);
    let token = plan["token"].as_str().unwrap().to_string();

    // Honored while the record still hashes the same.
    assert_eq!(
        rat(
            &root,
            &[
                "crp", "checking", "4250", "-a", "260718", "-y", "-p", &token
            ]
        )
        .0,
        0
    );
    // And refused once it does not: the reading it reviewed is gone.
    let (code, err) = rat(
        &root,
        &[
            "crp", "checking", "4999", "-a", "260718", "-y", "-p", &token,
        ],
    );
    assert_eq!(code, 3);
    insta::assert_snapshot!("refusal_stale_plan_token", pretty(&err));
}

/// A write verb is refused outright under `PANTHEON_RULE=1` (exit `6`, §9.3): the one
/// reactive writer is Auspex, and a rule may not borrow a hand's authority (I2).
#[test]
fn a_rule_may_plan_but_never_write() {
    let root = fresh_root();
    with_checking(&root);
    let rule = [("PANTHEON_RULE", "1")];

    let ((code, err), _) = rat_env(&root, &["crp", "checking", "4310", "-a", "260719"], &rule);
    assert_eq!(code, 6);
    insta::assert_snapshot!("refusal_under_rule", pretty(&err));

    // `--dry-run` computes without writing, so a rule may still plan (§7.3).
    let ((code, _), _) = rat_env(
        &root,
        &["crp", "checking", "4310", "-a", "260719", "-n"],
        &rule,
    );
    assert_eq!(code, 0);
    // Reads are untouched.
    let ((code, _), _) = rat_env(&root, &["list"], &rule);
    assert_eq!(code, 0);
}

/// A slug held at another node is a *soft* finding: the record still goes to stdout,
/// the warning to stderr in the shape `pan validate` emits (§5.4, §18, I4).
#[test]
fn a_cross_node_duplicate_warns_on_stderr() {
    let root = fresh_root();
    assert_eq!(rat(&root, &["crp", "checking"]).0, 0);
    let ((code, record), stderr) = rat_env(&root, &["crb", "checking"], &[]);
    assert_eq!(code, 0, "the record is still written (§5.4)");
    assert_eq!(record["home"], "crb");
    let findings: Value = serde_json::from_str(stderr.trim()).unwrap();
    insta::assert_snapshot!("warning_duplicate_slug", pretty(&findings));
}

// ── the shape on disk (§5.2, §6.1) ──────────────────────────────────────────

/// A determined series carrying a name splits into three segments like any other
/// (§5.2); only the `named` bit tells it from a hand-named one. And no variant tag is
/// ever written — the filename's token already says which record a file holds (§7.1,
/// §18).
#[test]
fn the_files_wear_the_names_the_spec_gives_them() {
    let root = fresh_root();
    with_checking(&root);
    assert!(
        meta_dir(&root, "crp")
            .join("crp__account__checking.json")
            .exists()
    );
    assert!(balance_file(&root, "crp", "checking").exists());

    let holding =
        std::fs::read_to_string(meta_dir(&root, "crp").join("crp__account__checking.json"))
            .unwrap();
    let reading = std::fs::read_to_string(balance_file(&root, "crp", "checking")).unwrap();
    insta::assert_snapshot!(
        "on_disk",
        format!("{}--- balance ---\n{}", holding, reading)
    );
}
