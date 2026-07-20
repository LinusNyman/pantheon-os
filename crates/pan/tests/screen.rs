//! `pan`'s **real** screen, driven (§10, P§3).
//!
//! `pan` is not a core — it owns no primitive and files no record. Its two tabs are the
//! *ontology* and what is wrong with it, so what this pins is different in kind from a
//! core's: that the tree draws, that `validate` reports through the same chrome, and
//! that the six structural mutators are honestly **dark** rather than quietly broken.

#![cfg(feature = "tui")]

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use pan::PanApp;
use pantheon::mint::NewSpec;
use pantheon::plan_new;

static COUNTER: AtomicU32 = AtomicU32::new(0);

fn fresh_root() -> PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("pan-screen-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    for (parent, ch, label) in [
        ("root", "a", "actio"),
        ("a", "c", "cura"),
        ("root", "c", "contextus"),
    ] {
        let (plan, _) = plan_new(&dir, parent, NewSpec::Triple { ch, label }).unwrap();
        plan.apply(&dir).unwrap();
    }
    dir
}

fn frame(root: &Path, script: &str) -> String {
    porticus::drive(
        &mut PanApp::new(root),
        root,
        &porticus::keys(script),
        90,
        18,
    )
    .unwrap()
}

/// The tree tab draws the ontology, and the header carries the tracked name-word (P§8).
#[test]
fn the_tree_tab_draws_the_ontology() {
    let root = fresh_root();
    let text = frame(&root, "");
    assert!(text.contains("P A N T H E O N"), "{text}");
    // Both spheres and the child, by code and label (§5.1).
    for node in ["a actio", "ac cura", "c contextus"] {
        assert!(text.contains(node), "the tree names {node}: {text}");
    }
}

/// The second tab is `validate`, reached by its number key (P§4, P§5).
#[test]
fn the_validate_tab_reports_through_the_same_chrome() {
    let root = fresh_root();
    let text = frame(&root, "2");
    assert!(text.contains("validate"), "the tab strip names it: {text}");
    // A clean tree has nothing to report, and absence is calm (I7, P§4) — the chrome
    // stands in full around one dim line.
    assert!(text.contains("P A N T H E O N"), "{text}");
}

/// **`x` removes the selected node, through the spine, on disk (§10.1).**
///
/// The rail cursor opens on the first sphere; `<down>` lands on its child `ac`, `x` asks,
/// `<enter>` confirms, and Porticus relays `pan rm ac -C <root> -y`. `ac` is an empty
/// leaf, so it goes; its parent stays.
#[test]
fn x_removes_the_selected_node() {
    let root = fresh_root();
    assert!(root.join("a_actio/a_c_cura").is_dir(), "the leaf is there");

    frame(&root, "<down>x<enter>");

    assert!(
        !root.join("a_actio/a_c_cura").exists(),
        "`x` must reach the file, not just the frame"
    );
    assert!(root.join("a_actio").is_dir(), "the parent survives");
}

/// **`r` renames the selected node's label, on disk (§10.1).**
///
/// `<down>` selects `ac`, `r` opens Porticus's rename prompt; the typed label is appended
/// to `pan rename ac --label …`. The first `<enter>` submits the prompt, the second
/// confirms the computed change (a rename confirms, P§5). The code is unchanged (a label
/// rename), so only the directory's label part moves.
#[test]
fn r_renames_the_selected_node_label() {
    let root = fresh_root();

    frame(&root, "<down>rkinship<enter><enter>");

    assert!(
        root.join("a_actio/a_c_kinship").is_dir(),
        "`r` must rename the label on disk, not just the frame"
    );
    assert!(!root.join("a_actio/a_c_cura").exists());
}

/// **`d` on the validate tab applies a finding's fix, on disk (§10.2).**
///
/// A non-normalized node label is a finding whose single legal correction is `pan rename
/// <code> --label <normalized>`. `2` opens the tab, `<tab>` focuses the findings, `d`
/// relays the fix and the label is normalized on disk — step 9's 2b, unblocked now that
/// `pan rename --label` exists.
#[test]
fn d_on_the_validate_tab_applies_a_finding_fix() {
    let root = fresh_root();
    // Minting normalizes (§5.1), so a non-normal label is written by hand. A hyphen (not
    // case) so the before/after paths differ on a case-insensitive filesystem too.
    std::fs::create_dir_all(root.join("a_actio/a_x_bad-label")).unwrap();

    frame(&root, "2<tab>d");

    assert!(
        root.join("a_actio/a_x_bad_label").is_dir(),
        "`d` applied the normalizing fix on disk"
    );
    assert!(!root.join("a_actio/a_x_bad-label").exists());
}

/// **`m` (move) stays dark — a no-op, not a relay that fails.**
///
/// Porticus has no destination prompt for a node move yet, so `pan` does not offer the
/// action; the key draws nothing and touches nothing (P§7).
#[test]
fn move_stays_dark() {
    let root = fresh_root();
    let before = frame(&root, "");
    let after = frame(&root, "<down>m");
    assert_eq!(
        before.lines().count(),
        after.lines().count(),
        "a dark key draws no overlay: {after}"
    );
    assert!(root.join("a_actio/a_c_cura").is_dir(), "nothing moved");
}
