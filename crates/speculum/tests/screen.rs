//! Speculum's **real** screen, driven — with the cores **absent** (§12).
//!
//! A lens reads whatever cores it finds on `PATH` and links none of them (I5). So the
//! interesting case is not only the happy one: §12 says a missing core makes its action
//! *unavailable* rather than a relay that fails when tried, and §15.5 calls that
//! graceful degradation "the whole of what makes installing one app real".
//!
//! This file pins the degraded half — no core binaries reachable, the lens still
//! standing — and the horizon control, which needs no core at all: widening and
//! narrowing the window is the view's own state (I1), independent of what any core
//! answers. The cross-process relay itself is `relay.rs`, which needs a `PATH` and so
//! needs a test process of its own.

#![cfg(feature = "tui")]

use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};

use pantheon::mint::NewSpec;
use pantheon::plan_new;
use speculum::Speculum;

static COUNTER: AtomicU32 = AtomicU32::new(0);

fn fresh_root() -> PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("spe-screen-{}-{n}", std::process::id()));
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
    let text = porticus::drive(
        &mut Speculum::new(&root),
        &root,
        &porticus::keys(""),
        90,
        24,
    )
    .unwrap();

    assert!(text.contains("S P E C U L U M"), "the name-word: {text}");
    assert!(
        text.contains("mosaic"),
        "the tab strip names the mosaic: {text}"
    );
    assert!(
        text.contains("horizon"),
        "the tab strip names the horizon: {text}"
    );
    // Its tiles are still drawn — a tile with nothing to count says so rather than
    // vanishing, because absence is calm and never an error (I7, P§4).
    for tile in ["open tasks", "people", "logs", "documents"] {
        assert!(text.contains(tile), "the tile {tile} is drawn: {text}");
    }
}

/// A lineup is checked before a terminal is ever taken (P§3) — the horizon is the
/// second view.
#[test]
fn the_lineup_is_legal() {
    let root = fresh_root();
    let text = porticus::drive(
        &mut Speculum::new(&root),
        &root,
        &porticus::keys("2"),
        90,
        24,
    )
    .unwrap();
    assert!(
        text.contains("horizon"),
        "the second view is the horizon: {text}"
    );
}

/// **The horizon control** (§12): `w` widens the window and `n` narrows it, day → week
/// → month → year, and back. The view carries the span itself, so this needs no core
/// on `PATH` — the locator names the width whatever the fold returns.
///
/// Off the wall clock by construction: the *span word* the locator carries is decided
/// by the keystrokes, not the date. The anchor's date moves with today, which is why
/// the boundaries are not asserted — only the width the hand set.
#[test]
fn the_horizon_widens_and_narrows() {
    let root = fresh_root();
    let drive = |script: &str| {
        porticus::drive(
            &mut Speculum::new(&root),
            &root,
            &porticus::keys(script),
            90,
            24,
        )
        .unwrap()
    };

    // `2` opens the horizon; it defaults to the week.
    let week = drive("2");
    assert!(
        week.contains("week"),
        "the horizon opens on the week: {week}"
    );

    // `n` narrows week → day.
    let day = drive("2n");
    assert!(day.contains("day"), "`n` narrows to the day: {day}");

    // `w` widens week → month; `ww` climbs on to the year.
    let month = drive("2w");
    assert!(month.contains("month"), "`w` widens to the month: {month}");
    let year = drive("2ww");
    assert!(year.contains("year"), "`ww` widens to the year: {year}");

    // The Tier-3 keys are listed in the status hint, so they appear in Help too (P§4).
    assert!(
        week.contains("widen") && week.contains("narrow"),
        "the horizon control is on the hint line: {week}"
    );
}
