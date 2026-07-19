//! Views (P§3) — the only place instruments diverge on-screen.
//!
//! **A view declares intent; Porticus runs the flow (P-II).** The trait deliberately
//! has no raw key handler: a view says which [`Action`]s it offers and Porticus owns
//! the key→action binding, the decision to confirm, and every prompt. The only
//! keystrokes a view sees are the [`Nav`] events for Tier-3 keys it *declared*.
//!
//! Because a view never receives a mutating or chrome keystroke, it *cannot* author a
//! confirm, a search, or a prompt. What routing cannot reach is a view's own body —
//! `draw` and `navigate` are ordinary Rust — so "a view originates no write" is a
//! contract on trusted first-party code (it holds no [`Writer`](crate::action::Writer)
//! and reaches for no core), not a thing the type system forbids. A view that shells
//! out to a core itself is violating I2 and is a bug, not a supported path.

use pantheon::Code;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use crate::action::{Action, RecordRef, Target};
use crate::theme::Theme;

/// The switcher label and the Help key. Unique within a lineup; the number key is
/// positional, never declared.
pub type ViewId = &'static str;

/// Display only: a tree rail beside the view, or the whole width (P§6).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Layout {
    /// Tree left, view fills right. About the selected node.
    Rail,
    /// The view owns the width and carries its own selection — a date, a span-bar. The
    /// tree cursor means nothing to it.
    Full,
}

/// One line a row-view offers.
///
/// `label` is what Porticus renders *and* searches — a view exposes a label per item
/// and nothing else, which is what lets search, filter, and scroll be written once
/// (P§6). `target` is the record an action hits. One `Row` type across every list is
/// the point: an Agenda and a `TreeFile` scroll identically because they are the same
/// rows.
#[derive(Clone, Debug)]
pub struct Row {
    pub label: String,
    pub target: Target,
    /// Places the row on a dated Full view (an Agenda date, a Calendar cell); `None`
    /// on a plain list.
    pub when: Option<String>,
}

/// Side-effect-free motion delivered to a view (P§3).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Nav {
    Left,
    Right,
    Up,
    Down,
    /// A Tier-3 key the view declared in [`View::nav_keys`].
    Key(char),
}

/// Whether a view consumed a [`Nav`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Handled {
    Yes,
    No,
}

/// What a view contributes: *what* is shown and *which* actions exist — never *how* an
/// interaction feels (P-II).
pub trait View {
    /// The switcher label (P§3).
    fn id(&self) -> ViewId;

    /// Rail (beside the tree) or Full (the whole width).
    fn layout(&self) -> Layout {
        Layout::Rail
    }

    /// `Some` → Porticus renders the list and search, filter, and scroll come free and
    /// identical everywhere (P§6). `None` → a draw-view, which paints itself.
    ///
    /// `node` is the tree cursor; a Full view folds from its own app and ignores it.
    ///
    /// Empty is signalled with `Some(vec![])` — a real empty result. `None` means
    /// "I paint my own", not "I have nothing".
    fn rows(&mut self, node: &Code) -> Option<Vec<Row>>;

    /// For row-less views. Draws into the caller's buffer, like everything else here.
    fn draw(&mut self, node: &Code, area: Rect, buf: &mut Buffer, theme: Theme) {
        let _ = (node, area, buf, theme);
    }

    /// Which standard actions this view offers (P§5). A key whose action is not
    /// offered stays **dark** — a no-op, greyed out of Help — and is never repurposed.
    fn actions(&self) -> &[Action] {
        &[]
    }

    /// Tier-3 keys and their labels, **declared** so Porticus can route them, keep
    /// them off Tiers 1 and 2, and list them in Help (P§4).
    fn nav_keys(&self) -> &[(char, &'static str)] {
        &[]
    }

    /// A draw/Full view's current selection, as an *address* (P§7) — never a value
    /// (I1). Row-views return `None`: Porticus uses the focused `Row`'s target.
    fn target(&self) -> Option<Target> {
        None
    }

    /// View-internal motion only — no side effects (P§3).
    fn navigate(&mut self, nav: Nav) -> Handled {
        let _ = nav;
        Handled::No
    }

    /// What a Full view names in the header where a Rail view shows the path bar
    /// (P§4): a Calendar's month, a Timeline's range. Defaults to the view's id.
    fn locator(&self) -> Option<String> {
        None
    }

    /// Whether this is a **detail view** — one that renders a single *pinned* record
    /// (P§3).
    ///
    /// A lineup holds **at most one**, which is what lets `Enter` route with no shape
    /// tag on the record: an instrument has one primitive, so it has one detail.
    /// [`run`](crate::run) rejects a second.
    fn is_detail(&self) -> bool {
        false
    }

    /// Porticus hands the pinned record down (P§3).
    ///
    /// `Enter` on a content row pins that row's [`RecordRef`] and switches here; `Esc`
    /// un-pins with `None` and returns to the row it drilled from. A detail view folds
    /// the pinned record **each frame** (I1) — this only says *which*, never carries
    /// the record itself.
    ///
    /// A pinned record gone underneath (another hand, a hook — I8, §6.4) falls to the
    /// view's empty state, never a stale record: the fold simply finds nothing, which
    /// is the same answer as never having pinned.
    fn pin(&mut self, record: Option<RecordRef>) {
        let _ = record;
    }

    /// The one dim, centred line shown when there is nothing to draw (P§4).
    ///
    /// Absence is calm, never an error (I7): the chrome stands in full and only the
    /// content says so. Porticus owns the wording for its catalog views — one voice,
    /// not the app's (P-II).
    fn empty_line(&self) -> &'static str {
        "nothing here"
    }
}
