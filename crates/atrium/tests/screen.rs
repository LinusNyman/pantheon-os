//! Atrium's **real** screen, driven — with the cores **absent** (§12).
//!
//! A lens reads whatever cores it finds on `PATH` and links none of them (I5). So the
//! interesting case is not only the happy one: §12 says a missing core makes its action
//! *unavailable* rather than a relay that fails when tried, and §15.5 calls that
//! graceful degradation "the whole of what makes installing one app real".
//!
//! This file pins the degraded half — no core binaries reachable, the lens still
//! standing. The relay itself is `relay.rs`, which needs a `PATH` and so needs a test
//! process of its own.

#![cfg(feature = "tui")]

use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};

use atrium::Atrium;
use pantheon::mint::NewSpec;
use pantheon::plan_new;

static COUNTER: AtomicU32 = AtomicU32::new(0);

fn fresh_root() -> PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("atr-screen-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    for (parent, ch, label) in [("root", "a", "actio"), ("a", "c", "cura")] {
        let (plan, _) = plan_new(&dir, parent, NewSpec::Triple { ch, label }).unwrap();
        plan.apply(&dir).unwrap();
    }
    dir
}

/// The lens leads with its mosaic — the dashboard, not the tree (P§3) — and stands in
/// full even where no core answers.
#[test]
fn the_lens_stands_with_no_cores_on_path() {
    let root = fresh_root();
    let text =
        porticus::drive(&mut Atrium::new(&root), &root, &porticus::keys(""), 90, 20).unwrap();

    assert!(text.contains("A T R I U M"), "the name-word: {text}");
    assert!(
        text.contains("mosaic"),
        "the tab strip names the mosaic: {text}"
    );
    // Its tiles are still drawn — a tile with nothing to count says so rather than
    // vanishing, because absence is calm and never an error (I7, P§4).
    for tile in ["open tasks", "people", "documents"] {
        assert!(text.contains(tile), "the tile {tile} is drawn: {text}");
    }
}

/// A lineup is checked before a terminal is ever taken (P§3) — and Atrium's was checked
/// nowhere at all until now, having had no test directory.
#[test]
fn the_lineup_is_legal() {
    let root = fresh_root();
    let text =
        porticus::drive(&mut Atrium::new(&root), &root, &porticus::keys("2"), 90, 20).unwrap();
    assert!(
        text.contains("agenda"),
        "the second view is the agenda: {text}"
    );
}
