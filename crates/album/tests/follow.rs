//! **G5 — following a ref chip, within one core** (P§3, I5).
//!
//! A card's chips were legible and inert. `f` now follows the focused one: Porticus
//! resolves the `core:slug` token through the spine (§5.4), checks it names *this*
//! instrument's own core, takes the tree to the record's node, and pins it — the same
//! landing an `Enter`-drill makes, reached from a reference instead of a row.
//!
//! The same-core rule is the point, not a shortfall. `alb` links Album and nothing else,
//! so a `mappa:` chip is a record it cannot draw; pressing `f` there must *say where the
//! record lives*, never silently do nothing and never pretend to render it.
//!
//! **One test binary, on purpose.** The follow asks `CoreRegistry::discover()` which
//! binary owns a core, so it needs the built cores on `PATH` — which is process-global.
//! Cargo gives each integration-test file its own process, so these tests race nothing.

#![cfg(feature = "tui")]

use std::path::{Path, PathBuf};
use std::process::Command;

use album::AlbumApp;
use pantheon::mint::NewSpec;
use pantheon::plan_new;

fn bin_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_alb"))
        .parent()
        .expect("a binary has a directory")
        .to_path_buf()
}

fn alb(root: &Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_alb"))
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .expect("running alb")
}

/// A tree with two people at one node, one referencing the other.
fn seeded(tag: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("alb-follow-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    for (parent, ch, label) in [
        ("root", "c", "contextus"),
        ("c", "s", "societas"),
        ("cs", "a", "amicitia"),
    ] {
        let (plan, _) = plan_new(&root, parent, NewSpec::Triple { ch, label }).unwrap();
        plan.apply(&root).unwrap();
    }
    assert!(
        alb(&root, &["-H", "csa", "Mara Vidal", "-y"])
            .status
            .success(),
        "the referenced record must file first — a chip to nothing would not resolve"
    );
    assert!(
        alb(
            &root,
            &["-H", "csa", "Alex Roth", "--ref", "album:mara_vidal", "-y"],
        )
        .status
        .success(),
        "the referring record must file"
    );
    root
}

/// Make the cores discoverable the way the follow finds them: on `PATH` (§5.5, §12).
///
/// # Safety
/// The tests in this file run on threads of one process, and each calls this with the
/// same value before reading it. Cargo gives this file its own process, so nothing
/// outside it observes the change.
fn cores_on_path() {
    let path = std::env::var_os("PATH").unwrap_or_default();
    if std::env::split_paths(&path).any(|dir| dir == bin_dir()) {
        return;
    }
    let mut dirs = vec![bin_dir()];
    dirs.extend(std::env::split_paths(&path));
    let joined = std::env::join_paths(dirs).expect("a joinable PATH");
    unsafe { std::env::set_var("PATH", &joined) };
}

/// **`f` on a same-core chip lands on that record's card.**
///
/// `<down>`×2 puts the rail on `csa`, `<tab>` takes content focus, `<down>` lands on
/// `alex_roth` (the records list is alphabetical), `<enter>` drills into its card, and
/// `f` follows its one chip to `mara_vidal` — whose own card is then what the screen
/// draws.
#[test]
fn f_follows_a_same_core_chip_to_its_record() {
    let root = seeded("same");
    cores_on_path();

    let card = porticus::drive(
        &mut AlbumApp::new(&root),
        &root,
        &porticus::keys("<down><down><tab><enter>"),
        90,
        20,
    )
    .expect("the screen drives");
    assert!(
        card.contains("alex_roth"),
        "the drill lands on the referring record's card: {card}"
    );
    assert!(
        card.contains("album:mara_vidal"),
        "whose chip names the reference (P§3): {card}"
    );

    let followed = porticus::drive(
        &mut AlbumApp::new(&root),
        &root,
        &porticus::keys("<down><down><tab><enter>f"),
        90,
        20,
    )
    .expect("the screen drives");
    assert!(
        followed.contains("mara_vidal"),
        "`f` must land on the referenced record's own card: {followed}"
    );
    assert!(
        !followed.contains("album:mara_vidal"),
        "and it is the *referenced* card now — Mara references nobody, so no chip: \
         {followed}"
    );
}

/// **A cross-core chip is answered, never followed** (I5).
///
/// `alb` links Album alone, so a `mappa:` chip is a record it has no card for. The status
/// line says which binary owns it; the screen does not move.
#[test]
fn f_on_a_cross_core_chip_says_where_to_go() {
    let root = seeded("cross");
    // A ref into another core. Written by hand rather than through `alb --ref`, which
    // refuses a reference that resolves to nothing (§5.4) — and the point here is the
    // *core* check, which runs before resolution.
    let file =
        root.join("c_contextus/c_s_societas/cs_a_amicitia/csa__/csa__person__alex_roth.json");
    let text = std::fs::read_to_string(&file).unwrap();
    std::fs::write(&file, text.replace("album:mara_vidal", "mappa:stockholm")).unwrap();
    cores_on_path();

    let frame = porticus::drive(
        &mut AlbumApp::new(&root),
        &root,
        &porticus::keys("<down><down><tab><enter>f"),
        90,
        20,
    )
    .expect("the screen drives");
    assert!(
        frame.contains("alex_roth"),
        "the card did not move: {frame}"
    );
    assert!(
        frame.contains("map"),
        "the status line names the binary that owns it (I4, I5): {frame}"
    );
}
