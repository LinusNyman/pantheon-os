//! Annales's **real** screen, driven (P§2, P§3).
//!
//! Not a fake app and not a pty. [`porticus::drive`] runs the same `handle` the loop
//! runs, over the same lineup `ann`'s bare short opens, with the same in-process
//! relay — so a key here goes all the way to a file on disk and back, and the frame
//! returned is the frame a hand would see.
//!
//! Before the crate's shape changed, `screen.rs` was a module of the *bin* and an
//! integration test links the *lib*, so Annales's `App` was not nameable from a test
//! and none of this could be written. The chrome was the only thing a test could
//! drive; this is the other half — Annales's own lineup, its folds, and the
//! invocations its actions build.

#![cfg(feature = "tui")]

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU32, Ordering};

use annales::AnnalesApp;
use pantheon::mint::NewSpec;
use pantheon::plan_new;

static COUNTER: AtomicU32 = AtomicU32::new(0);

fn fresh_root() -> PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("ann-screen-{}-{n}", std::process::id()));
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

/// Set the tree up through the real binary, so the screen reads what `ann` wrote —
/// the same crossing a hand makes (I8).
fn ann(root: &Path, args: &[&str]) -> i32 {
    Command::new(env!("CARGO_BIN_EXE_ann"))
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .unwrap()
        .status
        .code()
        .unwrap_or(1)
}

/// What `list` reports at the fixture's node, read back through the **binary** — so a
/// pass means the file moved, not a field in memory (I1, I4).
fn listed(root: &Path) -> Vec<String> {
    let out = Command::new(env!("CARGO_BIN_EXE_ann"))
        .arg("-C")
        .arg(root)
        .args(["list", "-H", "ecv"])
        .output()
        .unwrap();
    let value: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap_or_default();
    value
        .as_array()
        .map(|rows| {
            rows.iter()
                // A partitioned entity is named by its `slug` and a series line by its
                // `key` — the same identity under the name its shape gives it (§5.4).
                .filter_map(|r| {
                    r["slug"]
                        .as_str()
                        .or_else(|| r["key"].as_str())
                        .map(str::to_owned)
                })
                .collect()
        })
        .unwrap_or_default()
}

/// A log **and a reading in it**. `-c` mints the series and nothing else — a hand-named
/// series is minted explicitly and then appended to (§7.3, §8.6) — so a fixture that
/// stops at `-c` has a file with no records in it and nothing for a screen to draw.
fn seeded() -> PathBuf {
    let root = fresh_root();
    assert_eq!(ann(&root, &["ecv", "weight", "-c"]), 0, "the log must mint");
    assert_eq!(
        ann(&root, &["ecv", "weight", "82", "-a", "260718"]),
        0,
        "the reading must land"
    );
    root
}

/// The node's records are on screen, folded by Annales's own fold.
#[test]
fn the_screen_draws_the_nodes_records() {
    let root = seeded();
    let frame = porticus::drive(
        &mut AnnalesApp::new(&root),
        &root,
        &porticus::keys(""),
        90,
        18,
    )
    .unwrap();
    assert!(
        frame.contains("A N N A L E S"),
        "the name-word is the header (P§8): {frame}"
    );
    assert!(frame.contains("weight"), "the log names itself: {frame}");
}

/// A lineup is checked before a terminal is ever taken (P§3), and Annales's was only
/// checked where something built it — which until now was launch, in a hand's terminal.
#[test]
fn the_lineup_is_legal_and_named() {
    let root = fresh_root();
    // `drive` runs `check_lineup` first, so an illegal lineup fails here rather than in
    // front of someone.
    let frame = porticus::drive(
        &mut AnnalesApp::new(&root),
        &root,
        &porticus::keys(""),
        90,
        18,
    )
    .unwrap();
    // The tab strip names the lineup in order, and `1`–`9` switch by position (P§4).
    for view in ["records", "insights"] {
        assert!(frame.contains(view), "the tab strip names {view}: {frame}");
    }
}

/// An empty node keeps its chrome and says so calmly (I7, P§4) — in Annales's own
/// wording, reached through its own `empty` on the catalog view.
#[test]
fn an_empty_node_says_so_calmly() {
    let root = fresh_root();
    let frame = porticus::drive(
        &mut AnnalesApp::new(&root),
        &root,
        &porticus::keys(""),
        90,
        18,
    )
    .unwrap();
    assert!(frame.contains("nothing logged here"), "{frame}");
}

/// **`x` removes the focused record, through the core, on disk.**
///
/// The relay §12 and I2 describe, end to end: Porticus resolves the key to
/// `Action::Remove` on the focused row, Annales's `on_action` builds `ann rm …`,
/// Porticus opens the Confirm overlay over a computed `--dry-run` (a remove always
/// confirms, P§5), `<enter>` commits, and Porticus adds `-C <root>` and `-y` — a
/// relay's child writes down a pipe, where a mutation without `-y` exits `5` (§7.3).
#[test]
fn x_on_a_row_removes_the_record_from_disk() {
    let root = seeded();
    assert_eq!(
        listed(&root),
        ["260718"],
        "a reading is keyed by its date (I1, §6.1)"
    );

    // `<tab>` moves focus to the content, `x` asks, `<enter>` confirms (P§5, P§6).
    porticus::drive(
        &mut AnnalesApp::new(&root),
        &root,
        &porticus::keys("<tab>x<enter>"),
        90,
        18,
    )
    .unwrap();

    assert!(
        listed(&root).is_empty(),
        "`x` must reach the file, not just the frame: {:?}",
        listed(&root)
    );
}
