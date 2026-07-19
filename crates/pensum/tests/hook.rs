//! **The Auspex wake, end to end (§9.4).** The hook every core carries.
//!
//! After a successful write a core spawns `aus run --trigger <core>@<home>` detached
//! and forgets it. Until step 8 there was nothing to wake, and — as this file was
//! written to prove — there was also no spawn: the hook did not exist, though the
//! repo's notes said it did. So it is pinned here from the outside, through the real
//! `pen` binary, against a **fake `aus`** that records the argv it was called with.
//!
//! Four facts, and the last two are the ones that keep the suite honest: a write
//! wakes Auspex naming the write; `PANTHEON_NO_HOOKS=1` silences it (which is what
//! Auspex's own applies carry, so a rule cannot recurse); a *read* wakes nothing; and
//! a tree with no `aus` on `PATH` writes exactly as before — cores depend on Auspex
//! not at all (I5), which is the state steps 1–7 ran in.
//!
//! **Unix only.** The fake `aus` is a `/bin/sh` script; CI runs its test job on
//! ubuntu, and the author's machine is darwin.

#![cfg(unix)]

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU32, Ordering};

use pantheon::mint::NewSpec;
use pantheon::plan_new;

static COUNTER: AtomicU32 = AtomicU32::new(0);

/// A tree with one node to file a task at: `a` → `ac` (§5.1).
fn fresh_root() -> PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("pen-hook-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    for (parent, ch, label) in [("root", "a", "actio"), ("a", "c", "cura")] {
        let (plan, _) = plan_new(&dir, parent, NewSpec::Triple { ch, label }).unwrap();
        plan.apply(&dir).unwrap();
    }
    dir
}

/// A directory holding a fake `aus` that appends its argv to `$AUS_LOG`, and the log
/// path it writes to. Put the directory on a child's `PATH` and the hook finds it.
fn fake_aus(root: &Path) -> (PathBuf, PathBuf) {
    use std::os::unix::fs::PermissionsExt;

    let bin = root.join("fakebin");
    std::fs::create_dir_all(&bin).unwrap();
    let log = root.join("aus.log");
    let aus = bin.join("aus");
    // `$@` and not `$*`: the trigger is one argument and must stay one.
    std::fs::write(&aus, "#!/bin/sh\necho \"$@\" >> \"$AUS_LOG\"\n").unwrap();
    std::fs::set_permissions(&aus, std::fs::Permissions::from_mode(0o755)).unwrap();
    (bin, log)
}

/// Run the real `pen` with `PATH` and the environment stated for the **child** — never
/// for this process, so these tests may run in parallel with every other (§7.3).
fn pen(root: &Path, args: &[&str], env: &[(&str, &str)]) -> i32 {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_pen"));
    cmd.arg("-C")
        .arg(root)
        .args(args)
        .env_remove("PANTHEON_ROOT");
    for (key, value) in env {
        cmd.env(key, value);
    }
    cmd.output().expect("pen runs").status.code().unwrap_or(-1)
}

/// `PATH` with `dir` in front, as a string a child can be handed.
fn path_with(dir: &Path) -> String {
    let existing = std::env::var_os("PATH").unwrap_or_default();
    let mut dirs = vec![dir.to_path_buf()];
    dirs.extend(std::env::split_paths(&existing));
    std::env::join_paths(dirs)
        .expect("a joinable PATH")
        .into_string()
        .expect("PATH is UTF-8")
}

/// Wait for the detached `aus` to land its line.
///
/// **The hook is spawned and forgotten** (§9.4) — `pen` exits without waiting, so the
/// child may not have run yet when `pen` returns. Polling is what a detached spawn
/// costs a test; the alternative would be waiting on the child, which is the one
/// thing the contract says a core must not do.
fn wake_log(log: &Path) -> String {
    for _ in 0..200 {
        if let Ok(text) = std::fs::read_to_string(log) {
            if !text.trim().is_empty() {
                return text;
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    String::new()
}

/// The log after giving a wake every chance to appear — for the cases that assert
/// *silence*, where there is no arrival to poll for.
fn no_wake_log(log: &Path) -> String {
    std::thread::sleep(std::time::Duration::from_millis(300));
    std::fs::read_to_string(log).unwrap_or_default()
}

#[test]
fn a_write_wakes_auspex_naming_the_write() {
    let root = fresh_root();
    let (bin, log) = fake_aus(&root);
    let path = path_with(&bin);
    let env = [("PATH", path.as_str()), ("AUS_LOG", log.to_str().unwrap())];

    assert_eq!(pen(&root, &["-H", "ac", "buy_milk", "-y"], &env), 0);

    assert_eq!(
        wake_log(&log).trim(),
        "run --trigger pensum@ac",
        "a successful write spawns `aus run --trigger <core>@<home>` (§9.4)"
    );
}

#[test]
fn a_read_wakes_nothing() {
    let root = fresh_root();
    let (bin, log) = fake_aus(&root);
    let path = path_with(&bin);
    let env = [("PATH", path.as_str()), ("AUS_LOG", log.to_str().unwrap())];

    // Seed with the hook silenced, so the only wake a failure could show is the read's.
    let seeded = [
        ("PATH", path.as_str()),
        ("AUS_LOG", log.to_str().unwrap()),
        ("PANTHEON_NO_HOOKS", "1"),
    ];
    assert_eq!(pen(&root, &["-H", "ac", "buy_milk", "-y"], &seeded), 0);
    assert_eq!(pen(&root, &["list", "-H", "ac"], &env), 0);

    assert!(
        no_wake_log(&log).trim().is_empty(),
        "a fold writes nothing, so there is nothing to wake for (§9.4)"
    );
}

#[test]
fn no_hooks_silences_the_wake() {
    let root = fresh_root();
    let (bin, log) = fake_aus(&root);
    let path = path_with(&bin);
    let env = [
        ("PATH", path.as_str()),
        ("AUS_LOG", log.to_str().unwrap()),
        ("PANTHEON_NO_HOOKS", "1"),
    ];

    assert_eq!(pen(&root, &["-H", "ac", "buy_milk", "-y"], &env), 0);

    assert!(
        no_wake_log(&log).trim().is_empty(),
        "Auspex's own applies carry PANTHEON_NO_HOOKS=1, and a core seeing it skips \
         the hook — no recursion (§9.4)"
    );
}

#[test]
fn a_tree_with_no_aus_installed_writes_exactly_as_before() {
    let root = fresh_root();
    // No fake `aus`, and `PATH` emptied so the real one cannot be found either: the
    // hook must be a silent no-op. Cores do not depend on Auspex (I5).
    let env = [("PATH", "")];

    assert_eq!(
        pen(&root, &["-H", "ac", "buy_milk", "-y"], &env),
        0,
        "a write succeeds with nothing to wake — the state steps 1–7 ran in (§9.4)"
    );

    // And the record really landed, rather than the write being skipped along with it.
    let code = pen(&root, &["get", "buy_milk", "-H", "ac"], &[]);
    assert_eq!(code, 0, "the task is there to read back");
}
