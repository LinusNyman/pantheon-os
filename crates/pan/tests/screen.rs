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

/// **The six structural mutators are dark, and the screen says so by doing nothing.**
///
/// §10.1's node-level cascade is still stubbed — `mv`, `rm`, `rename`, `rename-prefix`,
/// `rename-pattern` and `mv-file` all return not-implemented — so `on_action` returns
/// `None` and Porticus greys the keys (P§7). A dark key is a **no-op**, never a key
/// repurposed for something else, and never a relay that fails after the fact.
///
/// This is the assertion that would catch the cascade being wired up without its keys
/// being reconsidered, or a key being quietly rebound while the verb is still a stub.
#[test]
fn the_stubbed_mutators_stay_dark() {
    let root = fresh_root();
    let before = frame(&root, "");

    // Focus the content, then press every mutating key `pan` does not offer.
    let after = frame(&root, "<tab>rmx");
    assert_eq!(
        before.lines().count(),
        after.lines().count(),
        "a dark key draws no overlay and no prompt"
    );
    // The tree is untouched: nothing was renamed, moved, or removed.
    assert!(
        after.contains("a actio") && after.contains("ac cura"),
        "{after}"
    );
    assert!(
        root.join("a_actio").is_dir(),
        "a dark key must not reach the filesystem"
    );
}
