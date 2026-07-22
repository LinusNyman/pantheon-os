//! `TreeFile` (row · Rail) — tree nav on the left, the node's records as rows on the
//! right. **The core default lead** (P§3, P§9).

use pantheon::Code;

use crate::action::Action;
use crate::view::{Layout, Row, View, ViewId};

/// The node's own records, folded fresh each frame.
pub struct TreeFile<F> {
    fold: F,
    id: ViewId,
    actions: Vec<Action>,
    empty: &'static str,
    core: Option<&'static str>,
}

impl<F> TreeFile<F>
where
    F: FnMut(&Code) -> Vec<Row>,
{
    /// Capture the instrument's fold (P§3).
    ///
    /// `fold` is called with the tree cursor on every frame this view is drawn — it is
    /// *the* derivation, and holding its result would be a stored present (I1). The
    /// instrument folds its own store; Porticus never reaches for a core (I5).
    pub fn of(fold: F) -> Self {
        Self {
            fold,
            id: "records",
            actions: Vec::new(),
            empty: "nothing here",
            core: None,
        }
    }

    /// This list's own name in the switcher (P§3).
    ///
    /// A core's own tree tab is simply "records" — it has one kind and the tab strip
    /// says which instrument you are in. A **lens** stacks several such lists in one
    /// lineup and must name each ("people", "documents"), and a lineup's ids have to be
    /// unique for the switcher to key off them (P§3).
    #[must_use]
    pub fn called(mut self, id: ViewId) -> Self {
        self.id = id;
        self
    }

    /// The core a **new** record on this list belongs to (P§3, §12).
    ///
    /// A lens folds several cores into one lineup, and `a` has no record to ask — so the
    /// view says which binary an add here reaches and Porticus stamps the target. A
    /// core's own TUI needs none: it has one core and names it in `on_action` (I5).
    #[must_use]
    pub fn in_core(mut self, short: &'static str) -> Self {
        self.core = Some(short);
        self
    }

    /// Which standard actions this lineup offers (P§5). Anything not named leaves its
    /// key dark rather than repurposed.
    #[must_use]
    pub fn offering(mut self, actions: &[Action]) -> Self {
        self.actions = actions.to_vec();
        self
    }

    /// The instrument's word for "nothing here" — "no todos here" at a bare node.
    ///
    /// Porticus owns the *wording pattern* and one voice across the catalog (P-II);
    /// what a core calls its own records is the one part it must supply.
    #[must_use]
    pub fn empty(mut self, line: &'static str) -> Self {
        self.empty = line;
        self
    }
}

impl<F> View for TreeFile<F>
where
    F: FnMut(&Code) -> Vec<Row>,
{
    fn id(&self) -> ViewId {
        self.id
    }

    fn layout(&self) -> Layout {
        Layout::Rail
    }

    fn rows(&mut self, node: &Code) -> Option<Vec<Row>> {
        // `Some(vec![])` where the node holds nothing: a real empty result, which is
        // what draws the calm empty line rather than a draw-view's own paint (P§3).
        Some((self.fold)(node))
    }

    fn actions(&self) -> &[Action] {
        &self.actions
    }

    fn core(&self) -> Option<&str> {
        self.core
    }

    fn empty_line(&self) -> &'static str {
        self.empty
    }
}
