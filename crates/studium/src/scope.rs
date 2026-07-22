//! The studies scope and the programme switch (§19.4, §19.6, N2).
//!
//! Studium used to fold the **whole tree**: every span, every task, every hour, whatever
//! node they sat at. That is right for a CLI given `-H` and wrong for a screen, where a
//! study life is lived one programme at a time — a GPA across two degrees is not a
//! figure anybody has.
//!
//! So the screen carries a scope. What it may *not* be is a setting: §18 admits no
//! config, and §19.3 admits exactly one hand-written file, the curriculum. Two things
//! follow, and they are the whole design here.
//!
//! - **The programmes are discovered, never declared.** A programme is a node with a
//!   `[code]_curriculum.toml`, which §19.3 already puts there and §19.4 already reads to
//!   weigh a grade: the scale that governs a course *is* the boundary of the programme
//!   it belongs to. Nothing new is written to the tree, and nothing is added to the file.
//! - **The choice is view state, not storage.** §19.4 says outright that nothing is
//!   stored to remember it; it lives as long as the screen does, like Speculum's horizon
//!   (§12), and a fresh launch opens on the whole of the studies again.
//!
//! One [`Studies`] is shared by every view through an `Rc<RefCell<_>>` — the mosaic that
//! switches it, and the folds that read it. That is a *cursor*, not a cache: nothing
//! derived is kept in it (I1).

use std::cell::RefCell;
use std::rc::Rc;

use pantheon::Code;

/// One discovered programme — the node a curriculum governs (§19.3, §6.3).
pub(crate) struct Programme {
    pub(crate) code: Code,
    /// The node's own label, so the switch reads `disciplina` and not `asd` (P1, §5.1).
    pub(crate) label: String,
}

/// The studies scope: which programmes exist, and which one the screen is folding.
pub(crate) struct Studies {
    programmes: Vec<Programme>,
    /// `None` is **all the studies** — the whole tree, the scope the lens opened on
    /// before there was a switch, and the honest answer where no curriculum exists.
    active: Option<usize>,
}

impl Studies {
    /// Walk the tree for curricula and take each one's node as a programme (§19.3).
    ///
    /// Walked, never cached (§18): the same walk `curriculum::discover` does to weigh a
    /// grade, so a curriculum added by hand while the screen is open is found on the
    /// next launch exactly as a grade governed by it is.
    pub(crate) fn discover(root: &std::path::Path) -> Self {
        let labels = porticus::node_labels(root);
        let mut programmes: Vec<Programme> = crate::curriculum::discover(root)
            .into_iter()
            .map(|(code, _)| Programme {
                label: labels
                    .get(code.as_str())
                    .cloned()
                    .unwrap_or_else(|| code.as_str().to_owned()),
                code,
            })
            .collect();
        // By code, so the order is the tree's and does not shuffle between launches.
        programmes.sort_by(|a, b| a.code.as_str().cmp(b.code.as_str()));
        Self {
            programmes,
            active: None,
        }
    }

    /// The node every fold is scoped to, or `None` for all the studies (§6.3).
    pub(crate) fn home(&self) -> Option<&Code> {
        self.active
            .and_then(|i| self.programmes.get(i))
            .map(|p| &p.code)
    }

    /// What the header calls the current scope (P§4).
    pub(crate) fn name(&self) -> String {
        match self.active.and_then(|i| self.programmes.get(i)) {
            Some(p) => format!("{} · {}", p.label, p.code.as_str()),
            None => "all studies".to_owned(),
        }
    }

    /// Step to the next programme, **through** "all the studies" at the wrap.
    ///
    /// The whole-tree scope is a position in the cycle rather than a mode you leave the
    /// cycle to reach, so `]` alone gets you everywhere a study life has.
    pub(crate) fn next(&mut self) {
        if self.programmes.is_empty() {
            return;
        }
        self.active = match self.active {
            None => Some(0),
            Some(i) if i + 1 < self.programmes.len() => Some(i + 1),
            Some(_) => None,
        };
    }

    pub(crate) fn previous(&mut self) {
        if self.programmes.is_empty() {
            return;
        }
        self.active = match self.active {
            None => Some(self.programmes.len() - 1),
            Some(0) => None,
            Some(i) => Some(i - 1),
        };
    }

    /// Back to all the studies — the `t`-shaped return of Speculum's horizon (§12).
    pub(crate) fn all(&mut self) {
        self.active = None;
    }
}

/// The shared handle every view holds.
pub(crate) type Scope = Rc<RefCell<Studies>>;

/// The scope's own Tier-3 keys, declared so Porticus routes them, keeps them off the
/// other tiers, and lists them in Help (P§5). Mirrors the horizon's `[`/`]`/`t`.
pub(crate) const KEYS: &[(char, &str)] = &[
    ('[', "previous programme"),
    (']', "next programme"),
    ('p', "all programmes"),
];

/// Wrap a view so the programme switch works wherever you are (P§3, P-II).
///
/// A Tier-3 key belongs to the *view* that declared it, so a switch declared on the
/// mosaic alone would be dead on the courses tab — and a scope you can only change on
/// one screen is a scope you will forget you set. The wrapper delegates everything and
/// adds the three keys, so every view in the lineup answers them identically (I3).
///
/// It holds no state of its own beyond the shared handle: the keys it adds mutate the
/// scope, and the folds behind `rows`/`draw` read it on the next frame (I1).
pub(crate) struct Switch<V> {
    inner: V,
    scope: Scope,
    keys: Vec<(char, &'static str)>,
}

impl<V: porticus::View> Switch<V> {
    pub(crate) fn of(inner: V, scope: &Scope) -> Self {
        let mut keys = inner.nav_keys().to_vec();
        keys.extend_from_slice(KEYS);
        Self {
            inner,
            scope: Rc::clone(scope),
            keys,
        }
    }
}

impl<V: porticus::View> porticus::View for Switch<V> {
    fn id(&self) -> porticus::ViewId {
        self.inner.id()
    }

    fn layout(&self) -> porticus::Layout {
        self.inner.layout()
    }

    fn rows(&mut self, node: &Code) -> Option<Vec<porticus::Row>> {
        self.inner.rows(node)
    }

    fn draw(
        &mut self,
        node: &Code,
        area: ratatui::layout::Rect,
        buf: &mut ratatui::buffer::Buffer,
        theme: porticus::Theme,
    ) {
        self.inner.draw(node, area, buf, theme);
    }

    fn grid(&mut self) -> Option<porticus::Grid> {
        self.inner.grid()
    }

    fn actions(&self) -> &[porticus::Action] {
        self.inner.actions()
    }

    fn core(&self) -> Option<&str> {
        self.inner.core()
    }

    fn nav_keys(&self) -> &[(char, &'static str)] {
        &self.keys
    }

    fn target(&self) -> Option<porticus::Target> {
        self.inner.target()
    }

    fn navigate(&mut self, nav: porticus::Nav) -> porticus::Handled {
        // The inner view sees the key first: a view that declared `[` for its own motion
        // keeps it, and only what it declines reaches the switch.
        if self.inner.navigate(nav) == porticus::Handled::Yes {
            return porticus::Handled::Yes;
        }
        match nav {
            porticus::Nav::Key(']') => self.scope.borrow_mut().next(),
            porticus::Nav::Key('[') => self.scope.borrow_mut().previous(),
            porticus::Nav::Key('p') => self.scope.borrow_mut().all(),
            _ => return porticus::Handled::No,
        }
        porticus::Handled::Yes
    }

    fn locator(&self) -> Option<String> {
        // A Full view names its scope in the header where a Rail view shows the path bar
        // (P§4) — so the programme you are folding is on screen, never guessed at.
        self.inner
            .locator()
            .map(|inner| format!("{} · {inner}", self.scope.borrow().name()))
    }

    fn prompts_for(&self, action: porticus::Action) -> Option<&'static str> {
        self.inner.prompts_for(action)
    }

    fn is_detail(&self) -> bool {
        self.inner.is_detail()
    }

    fn pin(&mut self, record: Option<porticus::RecordRef>) {
        self.inner.pin(record);
    }

    fn empty_line(&self) -> &'static str {
        self.inner.empty_line()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn studies(codes: &[&str]) -> Studies {
        Studies {
            programmes: codes
                .iter()
                .map(|c| Programme {
                    code: Code::parse(c).unwrap(),
                    label: (*c).to_owned(),
                })
                .collect(),
            active: None,
        }
    }

    /// The cycle passes **through** all-the-studies, so one key reaches every scope.
    #[test]
    fn the_switch_cycles_through_every_programme_and_back_to_all() {
        let mut s = studies(&["asd", "ase"]);
        assert!(s.home().is_none(), "opens on all the studies (§19.4)");
        s.next();
        assert_eq!(s.home().map(Code::as_str), Some("asd"));
        s.next();
        assert_eq!(s.home().map(Code::as_str), Some("ase"));
        s.next();
        assert!(
            s.home().is_none(),
            "wraps through all, not back to the first"
        );
        s.previous();
        assert_eq!(s.home().map(Code::as_str), Some("ase"), "and steps back");
        s.all();
        assert!(s.home().is_none());
    }

    /// No curriculum, no programmes — the switch is a no-op and the scope stays whole.
    #[test]
    fn a_tree_with_no_curriculum_has_nothing_to_switch_to() {
        let mut s = studies(&[]);
        s.next();
        s.previous();
        assert!(s.home().is_none());
        assert_eq!(s.name(), "all studies");
    }
}
