//! **§12's cross-process relay, end to end.** The test the repo has never had.
//!
//! A core's own TUI relays *in-process* — the write goes through the very code the CLI
//! runs, so validation and the plan token are one implementation. A **lens** cannot do
//! that: it links no core (I5), so its relay shells out to the core binary on `PATH`
//! and the write crosses the JSON boundary (I4). Everything about that crossing —
//! `PATH` discovery, `-C <root>`, the mandatory `-y`, the child's exit code coming back
//! as a status line — was until now exercised only by a hand.
//!
//! So: seed a tree with the real `pen`, put the built binaries on `PATH`, drive
//! Atrium's agenda, press `d`, and read the file back with `pen` again. A pass means a
//! keystroke in one process became a write by another.
//!
//! **One test, alone in its own test binary, on purpose.** It mutates `PATH`, which is
//! process-global; Cargo runs the tests *within* one binary on threads but gives each
//! integration-test file its own process, so a lone test here cannot race anything.

#![cfg(feature = "tui")]

use std::path::{Path, PathBuf};
use std::process::Command;

use atrium::Atrium;
use pantheon::mint::NewSpec;
use pantheon::plan_new;

/// Where Cargo put the workspace's binaries — the directory `atr` itself is in, so the
/// sibling cores sit beside it. Found from `atr` rather than from a core's
/// `CARGO_BIN_EXE_*`, because **Atrium depends on no core** and could not name one (I5).
fn bin_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_atr"))
        .parent()
        .expect("a binary has a directory")
        .to_path_buf()
}

fn pen(root: &Path, args: &[&str]) -> std::process::Output {
    let pen = bin_dir().join("pen");
    assert!(
        pen.exists(),
        "`pen` is not built. A lens's contract test drives another tool's binary, so \
         `cargo build --workspace --bins` has to run first — cargo builds no bin for a \
         crate that is not under test."
    );
    Command::new(pen)
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .expect("pen runs")
}

#[test]
fn d_on_an_atrium_row_marks_a_task_done_in_another_process() {
    let root = std::env::temp_dir().join(format!("atr-relay-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    for (parent, ch, label) in [("root", "a", "actio"), ("a", "c", "cura")] {
        let (plan, _) = plan_new(&root, parent, NewSpec::Triple { ch, label }).unwrap();
        plan.apply(&root).unwrap();
    }

    // Seed through the real core, by absolute path — this half needs no `PATH`.
    assert!(
        pen(&root, &["-H", "ac", "buy_milk", "-y"]).status.success(),
        "the fixture task must file"
    );

    // Now make the cores discoverable the way a lens finds them: on `PATH` (§12).
    // SAFETY: this is the only test in this binary, so nothing else can be reading the
    // environment concurrently. Cargo gives every integration-test file its own process.
    let path = std::env::var_os("PATH").unwrap_or_default();
    let mut dirs = vec![bin_dir()];
    dirs.extend(std::env::split_paths(&path));
    let joined = std::env::join_paths(dirs).expect("a joinable PATH");
    unsafe { std::env::set_var("PATH", &joined) };

    // `2` switches to the agenda, `d` marks the focused row done (P§4, P§5). The write
    // leaves this process entirely: Porticus builds `pen edit … --done`, adds `-C` and
    // `-y`, and spawns it (P§7).
    let frame = porticus::drive(
        &mut Atrium::new(&root),
        &root,
        &porticus::keys("2d"),
        90,
        20,
    )
    .expect("the lens drives");
    assert!(
        frame.contains("buy_milk"),
        "the task is on the agenda: {frame}"
    );

    // Read it back with the binary. `--all` is required: a plain `list` is every *open*
    // task, so a done one is gone from it.
    let out = pen(&root, &["list", "--all", "-H", "ac"]);
    let listed: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let done = listed
        .as_array()
        .and_then(|rows| rows.first())
        .is_some_and(|row| row["data"]["done"].is_string());
    assert!(
        done,
        "`d` in the lens must reach the file through `pen`: {}",
        String::from_utf8_lossy(&out.stdout)
    );
}
