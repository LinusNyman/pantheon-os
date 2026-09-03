//! **§12's cross-process relay, through Speculum, end to end.**
//!
//! A core's own TUI relays *in-process* — the write goes through the very code the CLI
//! runs. A **lens** cannot do that: it links no core (I5), so its relay shells out to
//! the core binary on `PATH` and the write crosses the JSON boundary (I4). Everything
//! about that crossing — `PATH` discovery, `-C <root>`, the mandatory `-y`, the plan
//! token — is a lens's own, and Speculum is the first lens to relay *across* cores: the
//! horizon folds several, and the row under the cursor names its own.
//!
//! So: seed a tree with a dated reading through the real `ann`, put the built binaries
//! on `PATH`, drive Speculum's horizon, press `x`, confirm, and read the file back with
//! `ann`. A pass means a keystroke in one process became a write by another — and that
//! Speculum routed it to the right core with nothing but the row's home and key.
//!
//! **One test, alone in its own test binary, on purpose.** It mutates `PATH`, which is
//! process-global; Cargo gives each integration-test file its own process, so a lone
//! test here cannot race anything.

#![cfg(feature = "tui")]

use std::path::{Path, PathBuf};
use std::process::Command;

use pantheon::mint::NewSpec;
use pantheon::plan_new;
use speculum::Speculum;

/// Where Cargo put the workspace's binaries — the directory `spe` itself is in, so the
/// sibling cores sit beside it. Found from `spe` rather than from a core's
/// `CARGO_BIN_EXE_*`, because **Speculum depends on no core** and could not name one (I5).
fn bin_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_spe"))
        .parent()
        .expect("a binary has a directory")
        .to_path_buf()
}

fn ann(root: &Path, args: &[&str]) -> std::process::Output {
    let ann = bin_dir().join("ann");
    assert!(
        ann.exists(),
        "`ann` is not built. A lens's contract test drives another tool's binary, so \
         `cargo build --workspace --bins` has to run first — cargo builds no bin for a \
         crate that is not under test."
    );
    Command::new(ann)
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .expect("ann runs")
}

#[test]
fn x_on_a_speculum_row_removes_a_reading_in_another_process() {
    let root = std::env::temp_dir().join(format!("spe-relay-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    for (parent, ch, label) in [("root", "a", "actio"), ("a", "c", "cura")] {
        let (plan, _) = plan_new(&root, parent, NewSpec::Triple { ch, label }).unwrap();
        plan.apply(&root).unwrap();
    }

    // Seed a dated reading through the real core, by absolute path — this half needs no
    // `PATH`. With no `--at`, the reading is keyed *today*, so it falls in the horizon's
    // default week window whatever day the suite runs (today is always within its own
    // week). `-c` mints the log before the first reading.
    let seeded = ann(&root, &["-H", "ac", "weight", "78.4", "-c"]);
    assert!(
        seeded.status.success(),
        "the fixture reading must file: {}",
        String::from_utf8_lossy(&seeded.stderr)
    );

    // Now make the cores discoverable the way a lens finds them: on `PATH` (§12).
    // SAFETY: this is the only test in this binary, so nothing else can be reading the
    // environment concurrently. Cargo gives every integration-test file its own process.
    let path = std::env::var_os("PATH").unwrap_or_default();
    let mut dirs = vec![bin_dir()];
    dirs.extend(std::env::split_paths(&path));
    let joined = std::env::join_paths(dirs).expect("a joinable PATH");
    unsafe { std::env::set_var("PATH", &joined) };

    // It is on the horizon to start: `2` switches to it, and the week window holds today.
    let before = porticus::drive(
        &mut Speculum::new(&root),
        &root,
        &porticus::keys("2"),
        100,
        24,
    )
    .expect("the lens drives");
    assert!(
        before.contains("weight"),
        "the reading is on the horizon: {before}"
    );

    // Remove it: `2` → horizon, `x` → the Confirm overlay (over a `--dry-run` relay),
    // `y` → commit. The write leaves this process entirely: Porticus builds `ann rm
    // -H ac <key>`, adds `-C`, `-y` and the plan token, and spawns it (P§7). Speculum
    // routed it by the core the row remembers being folded from (G8, I5).
    let _ = porticus::drive(
        &mut Speculum::new(&root),
        &root,
        &porticus::keys("2xy"),
        100,
        24,
    )
    .expect("the lens drives");

    // Read it back with the binary: the log has no readings left.
    let out = ann(&root, &["list", "-H", "ac"]);
    let listed: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let empty = listed.as_array().is_some_and(Vec::is_empty);
    assert!(
        empty,
        "`x` in the lens must reach the file through `ann`: {}",
        String::from_utf8_lossy(&out.stdout)
    );

    // ── N3: survey yourself — log a reading from the mirror ──────────────────
    a_reading_is_logged_through_annales(&root);
}

/// **N3 — the mirror logs a reading** (§8.6, §12).
///
/// Speculum could fix and drop readings and mint none, so "survey myself" had no key at
/// all: `on_action` matched only a row. Logging is a relay like any other — the hand
/// asks, Annales writes — and the two things it needs beyond a row are the core (the
/// view declares it) and the *date*, which the horizon already names: a reading logged
/// while reviewing a window belongs to that window, exactly as a Calendar cell dates its
/// add (§7.3).
fn a_reading_is_logged_through_annales(root: &Path) {
    // `2` → the horizon; `A` → the pick-a-home modal, `<down>` onto `ac`, `<enter>` to
    // take it; then the survey form: log · value · note · new log. The log does not
    // exist yet, so `y` on the switch mints it — `add` fills a container and never mints
    // one (§7.3), and the hand says so rather than the lens inferring it.
    let form = porticus::drive(
        &mut Speculum::new(root),
        root,
        &porticus::keys("2A<down><enter>"),
        100,
        24,
    )
    .expect("the lens drives");
    assert!(
        form.contains("log") && form.contains("new log"),
        "the survey form opens with the log, its value, and the mint switch: {form}"
    );

    porticus::drive(
        &mut Speculum::new(root),
        root,
        &porticus::keys("2A<down><enter>mood<tab>7<tab><tab>y<enter>"),
        100,
        24,
    )
    .expect("the lens drives the log");

    // Read it back through the core: the reading is filed at `ac`, in a log the relay
    // minted, keyed by the horizon's own anchor — today, on a freshly opened screen.
    let out = ann(root, &["series", "mood", "-H", "ac"]);
    let listed: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap_or_default();
    let logged = listed.as_array().is_some_and(|lines| {
        lines
            .iter()
            .any(|line| line["data"]["values"][0].as_str() == Some("7"))
    });
    assert!(
        logged,
        "`a` in the mirror must log the reading through `ann`: {}",
        String::from_utf8_lossy(&out.stdout)
    );

    let today = jiff::Zoned::now().strftime("%Y%m%d").to_string();
    let dated = listed
        .as_array()
        .and_then(|lines| lines.first())
        .and_then(|line| line["key"].as_str().map(str::to_owned));
    assert_eq!(
        dated.as_deref(),
        Some(today.as_str()),
        "the horizon's window dates the reading (§7.3): {listed}"
    );

    // And it folds straight back onto the horizon it was logged from (I1).
    let after = porticus::drive(
        &mut Speculum::new(root),
        root,
        &porticus::keys("2"),
        100,
        24,
    )
    .expect("the lens drives");
    assert!(
        after.contains("mood"),
        "the new reading is on the horizon: {after}"
    );
}
