//! Mappa's **real** screen, driven (P§2, P§3).
//!
//! Not a fake app and not a pty. [`porticus::drive`] runs the same `handle` the loop
//! runs, over the same lineup `map`'s bare short opens, with the same in-process
//! relay — so a key here goes all the way to a file on disk and back, and the frame
//! returned is the frame a hand would see.
//!
//! Before the crate's shape changed, `screen.rs` was a module of the *bin* and an
//! integration test links the *lib*, so Mappa's `App` was not nameable from a test
//! and none of this could be written. The chrome was the only thing a test could
//! drive; this is the other half — Mappa's own lineup, its folds, and the
//! invocations its actions build.

#![cfg(feature = "tui")]

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU32, Ordering};

use mappa::MappaApp;
use pantheon::mint::NewSpec;
use pantheon::plan_new;

static COUNTER: AtomicU32 = AtomicU32::new(0);

fn fresh_root() -> PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("map-screen-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    for (parent, ch, label) in [
        ("root", "c", "contextus"),
        ("c", "l", "locus"),
        ("cl", "u", "urbs"),
    ] {
        let (plan, _) = plan_new(&dir, parent, NewSpec::Triple { ch, label }).unwrap();
        plan.apply(&dir).unwrap();
    }
    dir
}

/// Set the tree up through the real binary, so the screen reads what `map` wrote —
/// the same crossing a hand makes (I8).
fn map(root: &Path, args: &[&str]) -> i32 {
    Command::new(env!("CARGO_BIN_EXE_map"))
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
    let out = Command::new(env!("CARGO_BIN_EXE_map"))
        .arg("-C")
        .arg(root)
        .args(["list", "-H", "clu"])
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

fn seeded() -> PathBuf {
    let root = fresh_root();
    assert_eq!(
        map(&root, &["clu", "kafe_esaias", "--address", "Frejgatan 1"]),
        0,
        "the fixture record must file"
    );
    root
}

/// The node's records are on screen, folded by Mappa's own fold.
#[test]
fn the_screen_draws_the_nodes_records() {
    let root = seeded();
    let frame = porticus::drive(
        &mut MappaApp::new(&root),
        &root,
        &porticus::keys(""),
        90,
        18,
    )
    .unwrap();
    assert!(
        frame.contains("M A P P A"),
        "the name-word is the header (P§8): {frame}"
    );
    assert!(frame.contains("kafe_esaias"), "{frame}");
}

/// A lineup is checked before a terminal is ever taken (P§3), and Mappa's was only
/// checked where something built it — which until now was launch, in a hand's terminal.
#[test]
fn the_lineup_is_legal_and_named() {
    let root = fresh_root();
    // `drive` runs `check_lineup` first, so an illegal lineup fails here rather than in
    // front of someone.
    let frame = porticus::drive(
        &mut MappaApp::new(&root),
        &root,
        &porticus::keys(""),
        90,
        18,
    )
    .unwrap();
    // The tab strip names the lineup in order, and `1`–`9` switch by position (P§4).
    for view in ["records", "card", "insights"] {
        assert!(frame.contains(view), "the tab strip names {view}: {frame}");
    }
}

/// An empty node keeps its chrome and says so calmly (I7, P§4) — in Mappa's own
/// wording, reached through its own `empty` on the catalog view.
#[test]
fn an_empty_node_says_so_calmly() {
    let root = fresh_root();
    let frame = porticus::drive(
        &mut MappaApp::new(&root),
        &root,
        &porticus::keys(""),
        90,
        18,
    )
    .unwrap();
    assert!(frame.contains("nowhere filed here"), "{frame}");
}

/// **`x` removes the focused record, through the core, on disk.**
///
/// The relay §12 and I2 describe, end to end: Porticus resolves the key to
/// `Action::Remove` on the focused row, Mappa's `on_action` builds `map rm …`,
/// Porticus opens the Confirm overlay over a computed `--dry-run` (a remove always
/// confirms, P§5), `<enter>` commits, and Porticus adds `-C <root>` and `-y` — a
/// relay's child writes down a pipe, where a mutation without `-y` exits `5` (§7.3).
#[test]
fn x_on_a_row_removes_the_record_from_disk() {
    let root = seeded();
    assert_eq!(
        listed(&root),
        ["kafe_esaias"],
        "the fixture record is there to remove"
    );

    // `<tab>` moves focus to the content, `x` asks, `<enter>` confirms (P§5, P§6).
    porticus::drive(
        &mut MappaApp::new(&root),
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

/// **`a` opens the add form, fills it, and mints a record through the core, on disk.**
///
/// The defect this closes: `a` used to relay a nameless `add` the spine refused (§7.3),
/// so no prompt appeared and nothing was created. Now `a` opens the multi-field form;
/// the hand types a name and a coordinate and `<enter>` relays
/// `map add -H clu <name> --coordinates … -y` in-process (P§7).
#[test]
fn a_opens_the_add_form_and_mints_a_record() {
    let root = fresh_root();
    assert!(listed(&root).is_empty(), "clu starts empty");

    // At launch the rail cursor is on the sphere; descend to `clu`, then `a` opens the
    // form. Type the name, `<tab>` to the coordinates field, type a point, `<enter>`.
    porticus::drive(
        &mut MappaApp::new(&root),
        &root,
        &porticus::keys("<down><right><down>atorg<tab>59.33,18.06<enter>"),
        90,
        18,
    )
    .unwrap();

    assert_eq!(
        listed(&root),
        ["torg"],
        "`a` must reach the file, not just the frame: {:?}",
        listed(&root)
    );

    // The coordinate field reached the record too — a multi-field add, not a name alone.
    let out = Command::new(env!("CARGO_BIN_EXE_map"))
        .arg("-C")
        .arg(&root)
        .args(["list", "-H", "clu"])
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        text.contains("59.33") && text.contains("18.06"),
        "the coordinate the form collected is on disk: {text}"
    );
}
