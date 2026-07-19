//! What the chrome guarantees, tested without a terminal.
//!
//! These are the rules P§5 and P§7 call *reserved* and *mandatory* — the ones a later
//! hand could quietly relax while every screen still rendered. A test is the only
//! thing that notices.

use porticus::action::{Invocation, Relayed};
use porticus::keymap;
use porticus::{Action, Ident};

/// Every Tier-2 action round-trips to exactly one key and back.
///
/// The binding is what makes muscle memory one across the suite (P§5), so it must be
/// a bijection — two actions sharing a key would make one unreachable.
#[test]
fn every_action_binds_to_one_key() {
    let actions = [
        Action::Add,
        Action::Edit,
        Action::Done,
        Action::Remove,
        Action::Rename,
        Action::Move,
        Action::QuickAdd,
        Action::DoneAll,
        Action::RemoveAll,
    ];
    let mut seen = std::collections::HashSet::new();
    for action in actions {
        let key = keymap::key_for(action);
        assert!(seen.insert(key), "key `{key}` is bound twice");
        assert_eq!(keymap::action(key), Some(action));
    }
}

/// Tier 1 and Tier 2 never claim the same key.
#[test]
fn the_tiers_do_not_overlap() {
    for key in ('!'..='~').chain(['?', '+', '/', '.']) {
        let chrome = keymap::chrome(key).is_some();
        let action = keymap::action(key).is_some();
        assert!(
            !(chrome && action),
            "`{key}` is claimed by both Tier 1 and Tier 2"
        );
    }
}

/// A reserved key stays reserved even in a view that does not offer its action — which
/// is exactly why the reservation is suite-wide rather than per-view (P§5).
#[test]
fn reserved_keys_are_closed_to_tier_three() {
    for key in [
        '?', '+', '/', '.', 'q', '1', '9', 'a', 'e', 'd', 'x', 'r', 'm', 'A', 'D', 'X',
    ] {
        assert!(keymap::is_reserved(key), "`{key}` should be reserved");
    }
    // The motion keys too — a view that rebound `j` would break navigation everywhere.
    for key in ['h', 'j', 'k', 'l'] {
        assert!(keymap::is_reserved(key), "`{key}` should be reserved");
    }
    // A genuine Tier-3 key is free: Calendar's `t` for today, `[`/`]` for months.
    for key in ['t', '[', ']', 'g'] {
        assert!(!keymap::is_reserved(key), "`{key}` should be free");
    }
}

/// The confirm policy, pinned (P§5).
///
/// A single focused-row change is itself the acknowledgement §7.3 requires and relays
/// direct; every remove and every bulk action opens the overlay first. Changing this
/// changes the feel of all twelve instruments at once, so it should be a visible diff.
#[test]
fn the_confirm_policy_is_one_rule_for_the_suite() {
    assert!(!Action::Done.confirms());
    assert!(!Action::Edit.confirms());
    assert!(!Action::Add.confirms());
    assert!(Action::Remove.confirms());
    assert!(Action::Rename.confirms());
    assert!(Action::Move.confirms());
    assert!(Action::DoneAll.confirms());
    assert!(Action::RemoveAll.confirms());
}

/// **Every relay carries `-y`.**
///
/// A relay's child writes down a pipe, where a mutation without `-y` exits `5` (§7.3).
/// So the acknowledgement has to be the TUI's modal rather than the CLI's prompt — and
/// a relay that lost its `-y` would not fail loudly, it would sit at exit 5 and look
/// like the write silently did nothing.
#[test]
fn a_relay_always_carries_yes() {
    let invocation = Invocation::new("pen", ["edit", "buy_milk", "--done"]);
    let committed = invocation.committed(None);
    assert!(committed.args.contains(&"-y".to_string()));
    assert_eq!(committed.display(), "pen edit buy_milk --done -y");
}

/// The plan token rides along where the overlay computed one, so a change that moved
/// underneath between the review and the commit is refused rather than applied (§7.3).
#[test]
fn a_confirmed_relay_carries_its_plan_token() {
    let invocation = Invocation::new("pen", ["rm", "buy_milk"]);
    assert_eq!(
        invocation.committed(Some("abc123")).display(),
        "pen rm buy_milk -y --plan abc123"
    );
}

/// The overlay is filled from a `--dry-run` of the *same* invocation (P§7) — never a
/// second, hand-built one that could drift from what actually commits.
#[test]
fn the_dry_run_is_the_same_invocation() {
    let invocation = Invocation::new("alb", ["rm", "alex"]);
    assert_eq!(invocation.dry_run().display(), "alb rm alex --dry-run");
}

/// The status line says what the *core* said, since a core says why better than
/// Porticus can guess (§7.3, P§4).
#[test]
fn a_failed_relay_reports_the_cores_own_reason() {
    let refused = Relayed {
        code: 3,
        stdout: String::new(),
        stderr: r#"{"error":{"code":3,"msg":"a record already holds that slug"}}"#.into(),
    };
    assert_eq!(refused.message(), "a record already holds that slug");

    // And falls back to the exit code's meaning where a core said nothing.
    let bare = Relayed {
        code: 4,
        stdout: String::new(),
        stderr: String::new(),
    };
    assert_eq!(bare.message(), "not found");
}

/// The header word is tracked inscriptionally (P§8) — the Trajan-column feel, free in
/// a fixed terminal font.
#[test]
fn the_name_is_tracked() {
    let ident = Ident {
        name: "pensum",
        short: "pen",
        tagline: "intention · tasks",
        symbol: '♂',
        accent: porticus::ident::accent::MINIUM,
    };
    assert_eq!(ident.tracked(), "P E N S U M");
}
