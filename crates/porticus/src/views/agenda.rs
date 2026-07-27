//! `Agenda` (row · Full) — dated items as a linear list by date.
//!
//! **Each row carries its own home**, so it spans nodes: Pensum's tasks from all over,
//! and the list Atrium's day is made of (P§3, §12). That is exactly why
//! [`Row::target`](crate::Row) holds a [`RecordRef`](crate::RecordRef) rather than
//! leaning on the tree cursor — an agenda's rows each relay to their own node, no
//! cursor involved (P§7).

use pantheon::Code;

use crate::action::{Action, FieldSpec};
use crate::view::{Layout, Nav, Row, View, ViewId};

/// The instrument's dated items, folded fresh each frame and sorted by date.
pub struct Agenda<F> {
    fold: F,
    actions: Vec<Action>,
    empty: &'static str,
    core: Option<&'static str>,
    id: ViewId,
    form: Option<Vec<FieldSpec>>,
}

impl<F> Agenda<F>
where
    F: FnMut() -> Vec<Row>,
{
    /// Capture the instrument's fold (P§3).
    ///
    /// It takes no node: a Full view has no cursor and folds from its own app, so the
    /// tree cursor means nothing to it (P§3, P§6).
    pub fn of(fold: F) -> Self {
        Self {
            fold,
            actions: Vec::new(),
            empty: "nothing scheduled",
            core: None,
            id: "agenda",
            form: None,
        }
    }

    /// Name this list, because a lineup's view ids must be unique (P§3) and a lens may
    /// stack several dated lists — tasks, deadlines, hours — in one lineup.
    #[must_use]
    pub fn called(mut self, id: ViewId) -> Self {
        self.id = id;
        self
    }

    /// The fields `a` collects here, where they differ from the app's (§7.3, P§7).
    ///
    /// A lens's `a` means a different record on each tab; the tab is what knows which.
    #[must_use]
    pub fn with_form(mut self, fields: Vec<FieldSpec>) -> Self {
        self.form = Some(fields);
        self
    }

    /// The core a **new** record on this list belongs to (P§3, §12).
    ///
    /// A row says its own core; an add has no record yet, so the view answers for it and
    /// Porticus stamps the target it builds. Only a lens needs this (I5).
    #[must_use]
    pub fn in_core(mut self, short: &'static str) -> Self {
        self.core = Some(short);
        self
    }

    #[must_use]
    pub fn offering(mut self, actions: &[Action]) -> Self {
        self.actions = actions.to_vec();
        self
    }

    #[must_use]
    pub fn empty(mut self, line: &'static str) -> Self {
        self.empty = line;
        self
    }
}

impl<F> View for Agenda<F>
where
    F: FnMut() -> Vec<Row>,
{
    fn id(&self) -> ViewId {
        self.id
    }

    fn layout(&self) -> Layout {
        Layout::Full
    }

    fn rows(&mut self, _node: &Code) -> Option<Vec<Row>> {
        let mut rows = (self.fold)();
        // By date, then by label — a stable order, so a refold does not shuffle rows
        // under the cursor. Undated rows sort last: they are not *scheduled*, and
        // putting them first would bury the day.
        rows.sort_by(|a, b| match (&a.when, &b.when) {
            (Some(x), Some(y)) => x.cmp(y).then_with(|| a.label.cmp(&b.label)),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => a.label.cmp(&b.label),
        });
        Some(rows)
    }

    fn actions(&self) -> &[Action] {
        &self.actions
    }

    fn core(&self) -> Option<&str> {
        self.core
    }

    fn add_form(&self) -> Option<Vec<FieldSpec>> {
        self.form.clone()
    }

    fn navigate(&mut self, _nav: Nav) -> crate::Handled {
        // Row motion is Porticus's — an Agenda has no internal geometry of its own, so
        // it declines and lets the shared scroll handle it (P§3, P§6).
        crate::Handled::No
    }

    fn locator(&self) -> Option<String> {
        // A Full view names its own locator in the header, where a Rail view shows the
        // path bar (P§4).
        Some("by date".into())
    }

    fn empty_line(&self) -> &'static str {
        self.empty
    }
}
