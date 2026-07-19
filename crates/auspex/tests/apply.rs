//! **Applying proposals, end to end (§9.5).** The write half of the reactive loop.
//!
//! A rule proposes, Auspex checks the proposal against the rule's grant, and — if it
//! passes — spawns the core CLI that lands it. So these tests drive the *real* `aus`
//! and read the result back through the *real* `pen`: a pass means a rule's proposal
//! became a record in another process, which is the whole of I2.
//!
//! `aus` finds the cores on `PATH` (`CoreRegistry::discover`, and the spawn that
//! applies), so `PATH` must carry the built binaries. It is set **on each child**, never
//! on this process — the `pensum/tests/hook.rs` move — so the file needs no
//! `unsafe set_var` and its tests may run in parallel.

#![cfg(unix)]

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU32, Ordering};

use pantheon::mint::NewSpec;
use pantheon::plan_new;
use serde_json::Value;

static COUNTER: AtomicU32 = AtomicU32::new(0);

/// The directory Cargo built the workspace binaries into — `aus` and its sibling
/// cores. Found from `aus` itself, so the cores sit beside it.
fn bin_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_aus"))
        .parent()
        .expect("a binary has a directory")
        .to_path_buf()
}

fn path_with(front: Option<&Path>) -> String {
    let mut dirs = Vec::new();
    if let Some(front) = front {
        dirs.push(front.to_path_buf());
    }
    dirs.push(bin_dir());
    if let Some(existing) = std::env::var_os("PATH") {
        dirs.extend(std::env::split_paths(&existing));
    }
    std::env::join_paths(dirs)
        .expect("a joinable PATH")
        .into_string()
        .expect("PATH is UTF-8")
}

/// Run the real `aus` (by absolute path, so it is always the real engine), with the
/// cores discoverable on the child's `PATH`.
fn aus(root: &Path, args: &[&str]) -> (i32, Value) {
    aus_with_path(root, args, &path_with(None))
}

fn aus_with_path(root: &Path, args: &[&str], path: &str) -> (i32, Value) {
    let out = Command::new(env!("CARGO_BIN_EXE_aus"))
        .arg("-C")
        .arg(root)
        .args(args)
        .env_remove("PANTHEON_ROOT")
        .env("PATH", path)
        .stdin(std::process::Stdio::null())
        .output()
        .expect("aus runs");
    let code = out.status.code().unwrap_or(-1);
    let bytes = if out.stdout.is_empty() {
        out.stderr
    } else {
        out.stdout
    };
    (code, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
}

/// Read the tree back through the real `pen`.
fn pen(root: &Path, args: &[&str]) -> (i32, Value) {
    let out = Command::new(bin_dir().join("pen"))
        .arg("-C")
        .arg(root)
        .args(args)
        .env_remove("PANTHEON_ROOT")
        .output()
        .expect("pen runs");
    let code = out.status.code().unwrap_or(-1);
    let bytes = if out.stdout.is_empty() {
        out.stderr
    } else {
        out.stdout
    };
    (code, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
}

fn fresh_root() -> PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("aus-apply-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    for (parent, ch, label) in [("root", "a", "actio"), ("a", "c", "cura")] {
        let (plan, _) = plan_new(&dir, parent, NewSpec::Triple { ch, label }).unwrap();
        plan.apply(&dir).unwrap();
    }
    dir
}

/// A rule on disk, executable (§9.1).
fn rule(root: &Path, name: &str, body: &str) {
    use std::os::unix::fs::PermissionsExt;

    let meta = root.join("a_actio/a_c_cura").join("ac__");
    std::fs::create_dir_all(&meta).unwrap();
    let path = meta.join(format!("ac__function__{name}.sh"));
    std::fs::write(&path, body).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

/// A rule proposing one Pensum task, granted at `grant`.
fn proposing_task(grant: &str, name: &str) -> String {
    format!(
        "#!/bin/sh\n# auspex: writes={grant}\ncat > /dev/null\n\
         printf '{{\"writes\":[{{\"core\":\"pensum\",\"verb\":\"add\",\"home\":\"ac\",\
         \"name\":\"{name}\"}}]}}\\n'\n"
    )
}

fn tasks_at_ac(root: &Path) -> Vec<String> {
    let (code, list) = pen(root, &["list", "-H", "ac"]);
    assert_eq!(code, 0, "pen list reads back: {list}");
    list.as_array()
        .map(|rows| {
            rows.iter()
                .map(|r| r["key"].as_str().unwrap_or_default().to_string())
                .collect()
        })
        .unwrap_or_default()
}

fn find<'a>(rows: &'a Value, rule: &str) -> &'a Value {
    rows.as_array()
        .expect("run emits an array")
        .iter()
        .find(|r| r["rule"] == rule)
        .unwrap_or_else(|| panic!("{rule} is in the report: {rows}"))
}

// ── the reactive write lands (§9.5, I2) ──────────────────────────────────────

#[test]
fn a_granted_proposal_becomes_a_record_in_another_process() {
    let root = fresh_root();
    rule(
        &root,
        "nudge",
        &proposing_task("pensum@ac:add", "Reach out to Alex"),
    );

    let (code, report) = aus(&root, &["run"]);
    assert_eq!(code, 0, "the rule ran and applied: {report}");
    assert_eq!(find(&report, "nudge")["applied"][0], "pensum@ac:add");

    assert_eq!(
        tasks_at_ac(&root),
        ["reach_out_to_alex"],
        "the proposed task is a real Pensum record now — the whole of I2"
    );
}

/// A rule run twice keeps one record, not two: Auspex upserts on the key it derives
/// from the proposed name (§9.3), and an overwrite is the same `add` with `-y` (§9.5).
#[test]
fn applying_the_same_rule_twice_is_idempotent() {
    let root = fresh_root();
    rule(
        &root,
        "nudge",
        &proposing_task("pensum@ac:add", "Reach out to Alex"),
    );

    assert_eq!(aus(&root, &["run"]).0, 0);
    assert_eq!(aus(&root, &["run"]).0, 0);

    assert_eq!(
        tasks_at_ac(&root).len(),
        1,
        "the second run upserts the one record rather than stacking a second (§9.3)"
    );
}

// ── the grant is the whole guard (§9.2, §9.5) ────────────────────────────────

/// An ungranted proposal rejects the **whole batch**, not just itself: a rule that got
/// one write wrong is not trusted with the rest (§9.5). So the *granted* proposal
/// beside it does not land either.
#[test]
fn one_ungranted_proposal_rejects_the_whole_batch() {
    let root = fresh_root();
    // Granted at `ac`, but the batch also proposes a write at `xyz`, which the grant
    // does not name.
    rule(
        &root,
        "greedy",
        "#!/bin/sh\n# auspex: writes=pensum@ac:add\ncat > /dev/null\n\
         printf '{\"writes\":[{\"core\":\"pensum\",\"verb\":\"add\",\"home\":\"ac\",\"name\":\"allowed\"},\
         {\"core\":\"pensum\",\"verb\":\"add\",\"home\":\"xyz\",\"name\":\"forbidden\"}]}'\n",
    );

    let (code, report) = aus(&root, &["run"]);
    assert_eq!(code, 1, "a rejected batch is not success");
    assert!(
        find(&report, "greedy")["rejected"]
            .as_str()
            .unwrap()
            .contains("xyz"),
        "the rejection names what was refused: {report}"
    );
    assert!(
        tasks_at_ac(&root).is_empty(),
        "the granted proposal did not land either — the batch is the unit of trust (§9.5)"
    );
}

/// A rule granting nothing is read-only: it may propose, but nothing lands (§9.2).
#[test]
fn a_rule_that_grants_nothing_writes_nothing() {
    let root = fresh_root();
    rule(
        &root,
        "readonly",
        "#!/bin/sh\ncat > /dev/null\n\
         printf '{\"writes\":[{\"core\":\"pensum\",\"verb\":\"add\",\"home\":\"ac\",\"name\":\"x\"}]}'\n",
    );

    let (code, report) = aus(&root, &["run"]);
    assert_eq!(code, 1);
    assert!(find(&report, "readonly").get("rejected").is_some());
    assert!(tasks_at_ac(&root).is_empty(), "default-deny (§9.2)");
}

/// A `data`-bearing proposal has no faithful route into a core (I5), so it is refused
/// loudly rather than mis-stored (§9.3).
#[test]
fn a_data_bearing_proposal_is_refused() {
    let root = fresh_root();
    rule(
        &root,
        "hasdata",
        "#!/bin/sh\n# auspex: writes=pensum@ac:add\ncat > /dev/null\n\
         printf '{\"writes\":[{\"core\":\"pensum\",\"verb\":\"add\",\"home\":\"ac\",\"name\":\"x\",\"data\":{\"k\":1}}]}'\n",
    );

    let (code, report) = aus(&root, &["run"]);
    assert_eq!(code, 1);
    assert!(
        find(&report, "hasdata")["errors"][0]
            .as_str()
            .unwrap()
            .contains("data"),
        "the refusal names the wall: {report}"
    );
    assert!(tasks_at_ac(&root).is_empty());
}

/// Two proposals landing on one key would upsert one over the other with nothing to
/// show for the first, so the batch is a rule error (§9.5 step 4).
#[test]
fn two_proposals_on_one_key_reject_the_batch() {
    let root = fresh_root();
    rule(
        &root,
        "twice",
        "#!/bin/sh\n# auspex: writes=pensum@ac:add\ncat > /dev/null\n\
         printf '{\"writes\":[{\"core\":\"pensum\",\"verb\":\"add\",\"home\":\"ac\",\"name\":\"Same Thing\"},\
         {\"core\":\"pensum\",\"verb\":\"add\",\"home\":\"ac\",\"name\":\"same_thing\"}]}'\n",
    );

    let (code, report) = aus(&root, &["run"]);
    assert_eq!(code, 1);
    assert!(
        find(&report, "twice")["rejected"]
            .as_str()
            .unwrap()
            .contains("same_thing"),
        "both names normalize to one key, and the collision is named: {report}"
    );
    assert!(tasks_at_ac(&root).is_empty(), "the batch was refused whole");
}

// ── the trigger filters, and applied writes do not recurse (§9.3, §9.4) ──────

#[test]
fn the_watch_filter_narrows_by_the_triggers_core() {
    let root = fresh_root();
    rule(&root, "nudge", &proposing_task("pensum@ac:add", "task"));

    // `nudge` watches annales (via the grant's core? no — watch is its own key).
    rule(
        &root,
        "watcher",
        "#!/bin/sh\n# auspex: watch=annales writes=pensum@ac:add\ncat > /dev/null\n\
         printf '{\"writes\":[]}'\n",
    );

    // A trigger from album evaluates neither the annales-watcher nor... `nudge` has no
    // watch, so it always evaluates. The watcher does not.
    let (_, report) = aus(&root, &["run", "--trigger", "album@x"]);
    let ran: Vec<&str> = report
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["rule"].as_str().unwrap())
        .collect();
    assert!(
        ran.contains(&"nudge"),
        "a rule with no watch always evaluates: {report}"
    );
    assert!(
        !ran.contains(&"watcher"),
        "a rule watching annales is not woken by an album write (§9.3): {report}"
    );

    // A trigger from annales does wake the watcher.
    let (_, report) = aus(&root, &["run", "--trigger", "annales@x"]);
    let ran: Vec<&str> = report
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["rule"].as_str().unwrap())
        .collect();
    assert!(
        ran.contains(&"watcher"),
        "annales wakes its watcher: {report}"
    );
}

/// Auspex's applied write carries `PANTHEON_NO_HOOKS=1`, so the `pen add` it spawns
/// does not itself wake `aus` — no recursion (§9.4).
///
/// A **fake `aus`** in front of the real one on the child's `PATH` records any call.
/// The real engine is still driven (by absolute path); only the cores' hook-spawn
/// resolves through `PATH` and would hit the fake — if `NO_HOOKS` were missing.
#[test]
fn an_applied_write_does_not_wake_auspex_again() {
    use std::os::unix::fs::PermissionsExt;

    let root = fresh_root();
    rule(&root, "nudge", &proposing_task("pensum@ac:add", "task"));

    let fake = root.join("fakebin");
    std::fs::create_dir_all(&fake).unwrap();
    let log = root.join("aus-calls.log");
    let aus_stub = fake.join("aus");
    std::fs::write(&aus_stub, format!("#!/bin/sh\necho called >> {log:?}\n")).unwrap();
    std::fs::set_permissions(&aus_stub, std::fs::Permissions::from_mode(0o755)).unwrap();

    let (code, report) = aus_with_path(&root, &["run"], &path_with(Some(&fake)));
    assert_eq!(code, 0, "applied: {report}");
    assert_eq!(find(&report, "nudge")["applied"][0], "pensum@ac:add");

    // Give any detached wake time to land, then confirm none did.
    std::thread::sleep(std::time::Duration::from_millis(300));
    assert!(
        std::fs::read_to_string(&log)
            .unwrap_or_default()
            .trim()
            .is_empty(),
        "the applied `pen add` carried PANTHEON_NO_HOOKS=1, so it woke no `aus` (§9.4)"
    );
}
