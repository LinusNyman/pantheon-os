//! `TreeFile` (row · Rail) — tree nav on the left, the node's records as rows on the
//! right. **The core default lead** (P§3, P§9).

use pantheon::Code;

use crate::action::Action;
use crate::view::{Layout, Row, View, ViewId};

/// The node's own records, folded fresh each frame.
pub struct TreeFile<F> {
    fold: F,
    actions: Vec<Action>,
    empty: &'static str,
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
            actions: Vec::new(),
            empty: "nothing here",
        }
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
        "records"
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

    fn empty_line(&self) -> &'static str {
        self.empty
    }
}
