//! Auspex's real screen, driven headless (P§2).
//!
//! Built with `AuspexApp::new(&root)` and driven with `porticus::drive` — the same
//! `handle` the loop runs — so a lineup that `check_lineup` would refuse fails here
//! rather than in a hand's terminal.
//!
//! The load-bearing test is the last one. The rules browser **offers no actions**
//! (§9.1: a rule is a file, and `touch`/`rm` are the hand's), and "offers none" is
//! invisible in a rendered frame — it looks exactly like a screen whose actions
//! happen not to have fired. Pressing the keys is the only way to know.

#![cfg(feature = "tui")]

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use auspex::AuspexApp;
use pantheon::mint::NewSpec;
use pantheon::plan_new;

static COUNTER: AtomicU32 = AtomicU32::new(0);

const CSA: &str = "c_contextus/c_s_societas/cs_a_amicitia";

fn fresh_root() -> PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("aus-screen-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    for (parent, ch, label) in [
        ("root", "c", "contextus"),
        ("c", "s", "societas"),
        ("cs", "a", "amicitia"),
    ] {
        let (plan, _) = plan_new(&dir, parent, NewSpec::Triple { ch, label }).unwrap();
        plan.apply(&dir).unwrap();
    }
    dir
}

/// A rule on disk, written by hand as §9.1 says a rule is authored.
fn write_rule(root: &Path, file: &str, body: &str) {
    let meta = root.join(CSA).join("csa__");
    std::fs::create_dir_all(&meta).unwrap();
    std::fs::write(meta.join(file), body).unwrap();
}

fn seeded() -> PathBuf {
    let root = fresh_root();
    write_rule(
        &root,
        "csa__function__stale_contact.py",
        "#!/usr/bin/env python3\n# auspex: watch=annales writes=pensum@acm:add\n",
    );
    root
}

fn frame(root: &Path, script: &str) -> String {
    porticus::drive(
        &mut AuspexApp::new(root),
        root,
        &porticus::keys(script),
        90,
        18,
    )
    .unwrap()
}

#[test]
fn the_screen_draws_the_trees_rules() {
    let root = seeded();
    let frame = frame(&root, "");
    assert!(
        frame.contains("A U S P E X"),
        "the name-word is the header (P§8): {frame}"
    );
    assert!(
        frame.contains("stale_contact"),
        "the rule is listed: {frame}"
    );
    assert!(
        frame.contains("writes=pensum@acm:add"),
        "with its grant, which is the whole guard (§9.2): {frame}"
    );
}

#[test]
fn the_lineup_is_legal_and_named() {
    let root = seeded();
    // Driving at all runs `check_lineup`, which is otherwise only checked at launch.
    let frame = frame(&root, "");
    assert!(frame.contains("rules"), "the view names itself: {frame}");
}

#[test]
fn a_tree_with_no_rules_says_so_calmly() {
    let root = fresh_root();
    let frame = frame(&root, "");
    assert!(
        frame.contains("nothing watches this tree"),
        "absence is calm, never an error (I7): {frame}"
    );
}

/// §9.1 leaves minting and removing a rule to the hand — `touch` and `rm` — so the
/// browser offers no actions and every Tier-2 key is inert.
///
/// **This is what a frame assertion cannot see.** A screen that silently did nothing
/// and a screen that offered a write it could not perform render identically; the
/// difference only shows when the key is pressed. `x`, `d`, `a` and `r` here would be
/// remove, done, add and rename on any core's screen.
#[test]
fn the_action_keys_are_dark_and_change_nothing() {
    let root = seeded();
    let before = frame(&root, "");
    let after = frame(&root, "xdar");
    assert_eq!(
        before, after,
        "an unoffered action's key is a no-op, not a rebind (P§5)"
    );

    // And the rule is still on disk: the keys did not reach a file either.
    let meta = root.join(CSA).join("csa__");
    assert!(
        meta.join("csa__function__stale_contact.py").exists(),
        "the rule file is untouched"
    );
}
