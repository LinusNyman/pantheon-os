//! The keymap (P§5) — three tiers, hardcoded (§18).
//!
//! Tier 1 is Porticus's and never varies. Tier 2 is Porticus's canonical set that a
//! view opts into, so a shared verb keeps a shared key. Tier 3 is the view's own.
//!
//! **P-I — chrome keys are reserved and universal.** `?` is help and `+` is title in
//! all twelve instruments, with one carve-out: while a text-entry overlay holds focus,
//! every printable key is a literal character and only `Esc` and `Enter` stay chrome.
//! Chrome and typing cannot both own the keyboard, and the reservation is against
//! *views* — which never take raw input — not against a prompt Porticus itself runs.

use crate::action::Action;

/// A Tier-1 chrome key (P§5).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Chrome {
    Help,
    Title,
    Search,
    /// `.` — collapse the tree to nodes this instrument files at (P§6).
    RecordsOnly,
    /// `f` — follow the reference the view's cursor is on, within this core (P§3, G5).
    Follow,
    /// `1`–`9` — switch to view *n*, zero-indexed here.
    Switch(usize),
    /// `Tab` — cycle pane on a Rail view; inert on a Full view, which has one pane.
    CyclePane,
    /// Descend a tree node · activate a content row into its detail view (P§3).
    Enter,
    /// Pop overlay · un-pin a drill · back.
    Escape,
    Quit,
}

/// Tier 1: chrome, reserved and universal. A view may not rebind these.
#[must_use]
pub fn chrome(key: char) -> Option<Chrome> {
    match key {
        '?' => Some(Chrome::Help),
        '+' => Some(Chrome::Title),
        '/' => Some(Chrome::Search),
        '.' => Some(Chrome::RecordsOnly),
        'f' => Some(Chrome::Follow),
        'q' => Some(Chrome::Quit),
        '1'..='9' => Some(Chrome::Switch(key as usize - '1' as usize)),
        _ => None,
    }
}

/// Tier 2: the standard actions, bound once (P§5).
///
/// **Lowercase acts on the focused row; Shift escalates scope** — to the whole node
/// (`D`, `X`) or to any node by address (`A`).
///
/// These keys are **reserved suite-wide, not merely opt-in**: a view that does not
/// offer an action leaves its key *dark* — a no-op, greyed out of Help — and never
/// repurposes it. That is what keeps a shared verb on a shared key.
#[must_use]
pub fn action(key: char) -> Option<Action> {
    match key {
        'a' => Some(Action::Add),
        'e' => Some(Action::Edit),
        'd' => Some(Action::Done),
        'x' => Some(Action::Remove),
        'r' => Some(Action::Rename),
        'm' => Some(Action::Move),
        'A' => Some(Action::QuickAdd),
        'D' => Some(Action::DoneAll),
        'X' => Some(Action::RemoveAll),
        _ => None,
    }
}

/// The key an action is bound to — for Help, which is generated from the live bindings
/// so it can never drift from them (P§4).
#[must_use]
pub fn key_for(action: Action) -> char {
    match action {
        Action::Add => 'a',
        Action::Edit => 'e',
        Action::Done => 'd',
        Action::Remove => 'x',
        Action::Rename => 'r',
        Action::Move => 'm',
        Action::QuickAdd => 'A',
        Action::DoneAll => 'D',
        Action::RemoveAll => 'X',
    }
}

/// The Tier-2 actions in **binding order**, for Help (P§4).
///
/// Written here beside [`action`] and [`key_for`] rather than derived from `Action`'s
/// declaration, so the order a hand reads is the order the keys are bound in and the two
/// cannot drift. The list is what makes Help complete: an action the active view does not
/// offer is still listed — greyed, since the key stays reserved suite-wide (P§5) — and a
/// reservation a hand cannot see is one they will try to rebind.
pub const TIER_2: &[Action] = &[
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

/// Whether a key is claimed by Tier 1 or Tier 2.
///
/// A Tier-3 key may collide with **neither** — a reserved key stays reserved even in a
/// view that does not offer its action, which is the whole reason the reservation is
/// suite-wide rather than per-view.
#[must_use]
pub fn is_reserved(key: char) -> bool {
    chrome(key).is_some() || action(key).is_some() || matches!(key, 'h' | 'j' | 'k' | 'l')
}

/// The Tier-1 rows Help lists (P§4). Held here beside the bindings they describe, so
/// the two move together.
pub const CHROME_HELP: &[(&str, &str)] = &[
    ("?", "help"),
    ("+", "title"),
    ("/", "search"),
    (".", "records-only tree"),
    ("f", "follow ref"),
    ("1-9", "switch view"),
    ("Tab", "cycle pane"),
    ("←↑↓→ · hjkl", "navigate"),
    ("Enter", "descend · activate"),
    ("Esc", "pop overlay · back"),
    ("q", "quit"),
];
