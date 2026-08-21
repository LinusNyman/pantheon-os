//! The Horizon — Speculum's review view (P§3, §12).
//!
//! Speculum reviews **across any horizon (day → week → month → year), across every
//! core**. This is the view that gives that its shape: a **row-view** (so search,
//! filter and scroll come free and identical — P§6) over the dated points every core
//! holds, folded fresh each frame ([`crate::screen`] supplies the fold) and **filtered
//! to a window** the hand widens, narrows, and steps through.
//!
//! **The horizon is a Tier-3 concept, declared through the view (P§5).** `w` widens and
//! `n` narrows the span; `[` / `]` step it back and forward; `t` returns to now — the
//! same shape Atrium's Calendar declares its `[` / `]` / `t` in, non-mutating motion
//! Porticus routes because the view *named* it. A view declares intent; Porticus runs
//! the flow (P-II).
//!
//! It carries its own anchor and span — cursor state Porticus never holds and nothing
//! stores (I1); the window is *derived* from them each frame. A `Full` view, so it owns
//! the whole width and names its window in the header where a Rail view shows the path
//! bar (P§4).

use jiff::civil::{Date, date};
use pantheon::Code;

use porticus::Handled;
use porticus::action::Action;
use porticus::action::Target;
use porticus::view::{Layout, Nav, Row, View, ViewId};

/// The width of the review window (§12). Widening climbs day → week → month → year and
/// stops; narrowing descends and stops. There is no fifth span — the four are the
/// horizons §12 names, hardcoded like every other key and layout (§18).
#[derive(Clone, Copy, PartialEq, Eq)]
enum Span {
    Day,
    Week,
    Month,
    Year,
}

impl Span {
    fn widen(self) -> Self {
        match self {
            Span::Day => Span::Week,
            Span::Week => Span::Month,
            Span::Month | Span::Year => Span::Year,
        }
    }

    fn narrow(self) -> Self {
        match self {
            Span::Year => Span::Month,
            Span::Month => Span::Week,
            Span::Week | Span::Day => Span::Day,
        }
    }

    fn word(self) -> &'static str {
        match self {
            Span::Day => "day",
            Span::Week => "week",
            Span::Month => "month",
            Span::Year => "year",
        }
    }
}

/// The reading key a dated record wears (§6.1) — `YYYYMMDD`. Referenced, never coined
/// here (I1); the same format Porticus's Calendar speaks, so a window boundary and a
/// row's `when` compare as plain strings — and with a four-digit year that lexical
/// order is chronological outright, not merely within one century (§5.4).
fn key_of(day: Date) -> String {
    format!("{:04}{:02}{:02}", day.year(), day.month(), day.day())
}

/// The dated points across every core, on a window the hand controls.
pub struct Horizon<F> {
    fold: F,
    span: Span,
    /// The core a **new** reading here belongs to, and the last node the rail was on —
    /// the two things an add needs that a row already carries (P§7, N3).
    core: Option<&'static str>,
    last_node: Option<Code>,
    /// The day the window is anchored on — a cursor Porticus never holds (I1). Opens on
    /// today, the one wall-clock read on this screen; `t` returns to it.
    anchor: Date,
    actions: Vec<Action>,
}

impl<F> Horizon<F>
where
    F: FnMut() -> Vec<Row>,
{
    /// Capture the instrument's fold (P§3). It takes no node: a Full view has no cursor
    /// and folds from its own app. Opens on this week — the review's natural default.
    pub fn of(fold: F) -> Self {
        Self {
            fold,
            span: Span::Week,
            core: None,
            last_node: None,
            anchor: jiff::Zoned::now().date(),
            actions: Vec::new(),
        }
    }

    /// The core a reading logged from here is written to (§8.6, N3).
    ///
    /// A row on the horizon says which core it came from; a *new* reading has no row
    /// yet, so the view answers for it and Porticus stamps the target (P§7).
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

    /// The window's inclusive bounds, derived from the anchor and span (I1).
    fn window(&self) -> (Date, Date) {
        match self.span {
            Span::Day => (self.anchor, self.anchor),
            Span::Week => {
                // Monday-first, as the week is here (P§3's Calendar keeps the same).
                let off = i64::from(self.anchor.weekday().to_monday_zero_offset());
                let start = self
                    .anchor
                    .checked_add(jiff::Span::new().days(-off))
                    .unwrap_or(self.anchor);
                let end = start
                    .checked_add(jiff::Span::new().days(6))
                    .unwrap_or(start);
                (start, end)
            }
            Span::Month => (
                date(self.anchor.year(), self.anchor.month(), 1),
                self.anchor.last_of_month(),
            ),
            Span::Year => (
                date(self.anchor.year(), 1, 1),
                date(self.anchor.year(), 12, 31),
            ),
        }
    }

    /// Step the anchor by whole spans — a week at a time on a week horizon, a year on a
    /// year horizon — so `[` / `]` page the review at whatever width it is set to.
    fn step(&mut self, forward: bool) {
        let n = if forward { 1 } else { -1 };
        let delta = match self.span {
            Span::Day => jiff::Span::new().days(n),
            Span::Week => jiff::Span::new().weeks(n),
            Span::Month => jiff::Span::new().months(n),
            Span::Year => jiff::Span::new().years(n),
        };
        if let Ok(moved) = self.anchor.checked_add(delta) {
            self.anchor = moved;
        }
    }
}

impl<F> View for Horizon<F>
where
    F: FnMut() -> Vec<Row>,
{
    fn id(&self) -> ViewId {
        "horizon"
    }

    fn layout(&self) -> Layout {
        Layout::Full
    }

    fn rows(&mut self, node: &Code) -> Option<Vec<Row>> {
        // Kept only so `target` can name a home: a Full view draws no rail, but the
        // cursor behind it is still where an add lands (P§7).
        self.last_node = Some(node.clone());
        let (start, end) = self.window();
        let (lo, hi) = (key_of(start), key_of(end));
        let mut rows: Vec<Row> = (self.fold)()
            .into_iter()
            .filter(|row| {
                // A dated point's `when` is its key; its date prefix decides the window
                // (a timed reading `20260703T1400` falls on `20260703`). An undated row
                // — a task, a place — has no `when` and never lands on the horizon.
                row.when.as_deref().is_some_and(|when| {
                    let day = when.get(..pantheon::DATE_WIDTH).unwrap_or(when);
                    day.len() == pantheon::DATE_WIDTH && day >= lo.as_str() && day <= hi.as_str()
                })
            })
            .collect();
        // By date, then by label — a stable order, so a refold does not shuffle rows
        // under the cursor.
        rows.sort_by(|a, b| a.when.cmp(&b.when).then_with(|| a.label.cmp(&b.label)));
        Some(rows)
    }

    fn actions(&self) -> &[Action] {
        &self.actions
    }

    fn core(&self) -> Option<&str> {
        self.core
    }

    fn target(&self) -> Option<Target> {
        // **The horizon dates the reading.** A review window is a place in time, so a
        // reading logged while looking at last week belongs to last week — the same rule
        // a Calendar cell follows (§7.3, P§7). Porticus resolves the home by layout and
        // overwrites the node; only the `at` survives.
        self.last_node.clone().map(|node| Target::Node {
            node,
            at: Some(key_of(self.anchor)),
            core: None,
        })
    }

    fn nav_keys(&self) -> &[(char, &'static str)] {
        // Tier 3: view-local, non-mutating, declared so Porticus can route them, keep
        // them off the other tiers, and list them in Help (P§5). The horizon control.
        &[
            ('w', "widen"),
            ('n', "narrow"),
            ('[', "previous"),
            (']', "next"),
            ('t', "now"),
        ]
    }

    fn navigate(&mut self, nav: Nav) -> Handled {
        match nav {
            Nav::Key('w') => self.span = self.span.widen(),
            Nav::Key('n') => self.span = self.span.narrow(),
            Nav::Key('[') => self.step(false),
            Nav::Key(']') => self.step(true),
            Nav::Key('t') => self.anchor = jiff::Zoned::now().date(),
            // The arrows scroll the day's rows — Porticus's, not the view's (P§6).
            _ => return Handled::No,
        }
        Handled::Yes
    }

    fn locator(&self) -> Option<String> {
        // A Full view names its own locator in the header (P§4): the span and its window.
        let (start, end) = self.window();
        Some(match self.span {
            Span::Day => format!("day · {}", key_of(start)),
            span => format!("{} · {}–{}", span.word(), key_of(start), key_of(end)),
        })
    }

    fn empty_line(&self) -> &'static str {
        "nothing in this horizon"
    }
}
