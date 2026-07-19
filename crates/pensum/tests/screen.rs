//! Pensum's **real** screen, driven (P§2, P§3).
//!
//! Not a fake app and not a pty. [`porticus::drive`] runs the same `handle` the loop
//! runs, over the same lineup `pen`'s bare short opens, with the same in-process relay
//! — so a key here goes all the way to a file on disk and back, and the frame returned
//! is the frame a hand would see.
//!
//! This is what the crate's shape was changed for. Before it, `screen.rs` was a module
//! of the *bin* and an integration test links the *lib*, so no core's `App` was
//! nameable from a test and none of this could be written. Step 6 found three defects
//! that reached `main` past a full green suite and were caught only by driving a
//! screen; every one of them was in the chrome, because the chrome was the only thing
//! a test could drive. What follows is the other half — Pensum's own contribution: its
//! lineup, its folds, and the invocations its actions build.

#![cfg(feature = "tui")]

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU32, Ordering};

use pantheon::mint::NewSpec;
use pantheon::plan_new;
use pensum::PensumApp;

static COUNTER: AtomicU32 = AtomicU32::new(0);

fn fresh_root() -> PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("pen-screen-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    for (parent, ch, label) in [("root", "a", "actio"), ("a", "c", "cura")] {
        let (plan, _) = plan_new(&dir, parent, NewSpec::Triple { ch, label }).unwrap();
        plan.apply(&dir).unwrap();
    }
    dir
}

/// Set the tree up through the real binary, so the screen reads what `pen` wrote —
/// the same crossing a hand makes (I8).
fn pen(root: &Path, args: &[&str]) -> i32 {
    Command::new(env!("CARGO_BIN_EXE_pen"))
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .unwrap()
        .status
        .code()
        .unwrap_or(1)
}

/// Every task at `ac` and whether it is done, read back through the **binary** — so a
/// pass means the file moved, not a field in memory (I1, I4).
///
/// `--all` is required: a plain `list` is every *open* task, so a done one vanishes from
/// it, which is the correct behaviour and a trap for a test that assumes otherwise.
/// `done` carries the **date** it was finished, not a flag — a task says when, and the
/// bool is derived from whether that is there at all (I1).
fn done_flags(root: &Path) -> Vec<(String, bool)> {
    let out = Command::new(env!("CARGO_BIN_EXE_pen"))
        .arg("-C")
        .arg(root)
        .args(["list", "--all", "-H", "ac"])
        .output()
        .unwrap();
    let value: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    value
        .as_array()
        .expect("`list` emits a bare array (§7.2)")
        .iter()
        .map(|r| {
            (
                r["key"].as_str().unwrap().to_owned(),
                // An open task carries no `done` at all — absent rather than empty, so
                // the record says only what is true of it (I1). The date itself is the
                // wall clock's, so presence is what this asserts and never the value.
                r["data"]["done"].is_string(),
            )
        })
        .collect()
}

/// The node's tasks are on screen, folded by Pensum's own `rows_at`.
#[test]
fn the_screen_draws_the_nodes_tasks() {
    let root = fresh_root();
    assert_eq!(pen(&root, &["-H", "ac", "buy_milk", "-y"]), 0);
    assert_eq!(pen(&root, &["-H", "ac", "call_the_dentist", "-y"]), 0);

    let frame = porticus::drive(
        &mut PensumApp::new(&root),
        &root,
        &porticus::keys(""),
        80,
        16,
    )
    .unwrap();
    assert!(frame.contains("P E N S U M"), "{frame}");
    assert!(frame.contains("buy_milk"), "{frame}");
    assert!(frame.contains("call_the_dentist"), "{frame}");
}

/// **`d` marks a task done, through the core, on disk.**
///
/// The write §12 and I2 describe, end to end and in one process: Porticus resolves the
/// key to `Action::Done` on the focused row, Pensum's `on_action` builds
/// `pen edit … --done`, Porticus adds `-C <root>` and `-y` (a relay's child writes down
/// a pipe, where a mutation without `-y` exits `5`, §7.3), and the core's own verb runs.
/// Reading it back with the *binary* proves the file moved rather than a field in
/// memory (I1).
#[test]
fn d_on_a_row_marks_the_task_done_on_disk() {
    let root = fresh_root();
    assert_eq!(pen(&root, &["-H", "ac", "buy_milk", "-y"]), 0);
    assert_eq!(
        done_flags(&root),
        vec![("buy_milk".to_string(), false)],
        "the task starts open"
    );

    // `<tab>` moves focus to the content, then `d` acts on the focused row (P§5, P§6).
    porticus::drive(
        &mut PensumApp::new(&root),
        &root,
        &porticus::keys("<tab>d"),
        80,
        16,
    )
    .unwrap();

    assert_eq!(
        done_flags(&root),
        vec![("buy_milk".to_string(), true)],
        "`d` must reach the file, not just the frame"
    );
}

/// A lineup is checked before a terminal is ever taken (P§3), and Pensum's is only
/// checked where something builds it — which until now was launch, in a hand's terminal.
#[test]
fn the_lineup_is_legal() {
    let root = fresh_root();
    // `drive` runs `check_lineup` first, so an illegal lineup fails here rather than in
    // front of someone.
    let frame = porticus::drive(
        &mut PensumApp::new(&root),
        &root,
        &porticus::keys(""),
        80,
        16,
    )
    .unwrap();
    // The tab strip names the lineup in order, and `1`–`9` switch by position (P§4).
    for view in ["records", "agenda", "insights"] {
        assert!(frame.contains(view), "the tab strip names {view}: {frame}");
    }
}

/// An empty node keeps its chrome and says so calmly (I7, P§4) — Pensum's own wording,
/// reached through its own `empty` on the catalog view.
#[test]
fn an_empty_node_says_so_in_pensums_words() {
    let root = fresh_root();
    let frame = porticus::drive(
        &mut PensumApp::new(&root),
        &root,
        &porticus::keys(""),
        80,
        16,
    )
    .unwrap();
    assert!(frame.contains("no todos here"), "{frame}");
}
