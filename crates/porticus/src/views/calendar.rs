//! `Calendar` (row · Full) — a month grid, cells filled by the app's dated items (P§3).
//!
//! **A row-view that also paints a grid.** The grid is the locator — which day you are
//! pointed at, and which days hold anything — and the rows beneath it are that day's
//! items, so search, filter and scroll remain Porticus's and are written once (P§6).
//! The cells carry a marker rather than a count: *that* something falls there is what
//! a month at a glance is for.
//!
//! The same fold feeds both, so a calendar and an [`Agenda`](super::Agenda) over one
//! instrument are two readings of one set — a month, and a line. Nothing is stored
//! either way: the calendar §8.4 calls derived is derived here, each frame (I1).

use jiff::civil::{Date, date};
use pantheon::Code;

use crate::Handled;
use crate::action::{Action, Target};
use crate::view::{Grid, GridCell, Layout, Nav, Row, View, ViewId};

/// Monday-first, which is what the week is here.
const COLUMNS: [&str; 7] = ["Mo", "Tu", "We", "Th", "Fr", "Sa", "Su"];

/// The reading key a dated record wears (§6.1) — `YYMMDD`, and the format every cell,
/// row and relay speaks. Referenced, never coined here (I1).
fn key_of(day: Date) -> String {
    format!(
        "{:02}{:02}{:02}",
        day.year().rem_euclid(100),
        day.month(),
        day.day()
    )
}

/// The instrument's dated items on a month grid, folded fresh each frame.
pub struct Calendar<F> {
    fold: F,
    actions: Vec<Action>,
    empty: &'static str,
    /// The focused day — a cursor Porticus holds, never a derived value (I1).
    cursor: Date,
    /// The last node `rows` was given. A Full view ignores the tree cursor for its own
    /// content (P§3), but [`Target::Node`] needs a home to carry the cell's date on,
    /// and Porticus overwrites the node with the rail's own before relaying (P§7).
    last_node: Option<Code>,
}

impl<F> Calendar<F>
where
    F: FnMut() -> Vec<Row>,
{
    /// Capture the instrument's fold (P§3).
    ///
    /// It takes no node: a Full view has no cursor and folds from its own app. Opens on
    /// today — a calendar with no day in it is not a calendar — which is the one wall
    /// clock read on this screen, and `t` returns to it.
    pub fn of(fold: F) -> Self {
        Self {
            fold,
            actions: Vec::new(),
            empty: "nothing this day",
            cursor: jiff::Zoned::now().date(),
            last_node: None,
        }
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

    /// The first of the focused month.
    fn first(&self) -> Date {
        date(self.cursor.year(), self.cursor.month(), 1)
    }

    /// Move the cursor by whole months, keeping the day where the month is long enough
    /// and clamping where it is not — 31 January stepped forward is the end of
    /// February, never a date that does not exist.
    fn shift_month(&mut self, months: i32) {
        let first = self.first();
        let target = first.checked_add(jiff::Span::new().months(months)).ok();
        let Some(target) = target else { return };
        let last = target.last_of_month().day();
        self.cursor = date(target.year(), target.month(), self.cursor.day().min(last));
    }

    fn shift_days(&mut self, days: i32) {
        if let Ok(moved) = self.cursor.checked_add(jiff::Span::new().days(days)) {
            self.cursor = moved;
        }
    }
}

impl<F> View for Calendar<F>
where
    F: FnMut() -> Vec<Row>,
{
    fn id(&self) -> ViewId {
        "calendar"
    }

    fn layout(&self) -> Layout {
        Layout::Full
    }

    fn rows(&mut self, node: &Code) -> Option<Vec<Row>> {
        // A Full view ignores the tree cursor for its content, but remembers it so the
        // cell's date has something to ride on (see `last_node`).
        self.last_node = Some(node.clone());
        let today = key_of(self.cursor);
        let mut rows: Vec<Row> = (self.fold)()
            .into_iter()
            .filter(|row| row.when.as_deref() == Some(today.as_str()))
            .collect();
        // Within one day the key says nothing more, so the label is the order.
        rows.sort_by(|a, b| a.label.cmp(&b.label));
        Some(rows)
    }

    fn grid(&mut self) -> Option<Grid> {
        let first = self.first();
        let days = self.cursor.last_of_month().day();
        // Monday is column 0; `weekday().to_monday_zero_offset()` counts from Monday.
        let lead = usize::from(first.weekday().to_monday_zero_offset().unsigned_abs());

        // One pass over the fold, bucketed by day-of-month, so a month costs one fold
        // rather than one per cell (P§6).
        let mut counts = [0usize; 32];
        let prefix = format!(
            "{:02}{:02}",
            self.cursor.year().rem_euclid(100),
            self.cursor.month()
        );
        for row in (self.fold)() {
            let Some(when) = row.when.as_deref() else {
                continue;
            };
            if let Some(day) = when.strip_prefix(prefix.as_str())
                && let Ok(day) = day.parse::<usize>()
                && (1..=31).contains(&day)
            {
                counts[day] += 1;
            }
        }

        let mut cells = vec![None; lead];
        for day in 1..=days {
            cells.push(Some(GridCell {
                label: day.to_string(),
                items: counts[day.unsigned_abs() as usize],
            }));
        }
        // Pad the tail so the last week is a whole row of cells.
        while cells.len() % COLUMNS.len() != 0 {
            cells.push(None);
        }
        Some(Grid {
            columns: COLUMNS.to_vec(),
            focused: lead + self.cursor.day().unsigned_abs() as usize - 1,
            cells,
        })
    }

    fn actions(&self) -> &[Action] {
        &self.actions
    }

    fn nav_keys(&self) -> &[(char, &'static str)] {
        // Tier 3: view-local, non-mutating, declared so Porticus can route them, keep
        // them off the other tiers and list them in Help (P§5).
        &[('t', "today"), ('[', "previous month"), (']', "next month")]
    }

    fn target(&self) -> Option<Target> {
        // The cell's date, so `a` on a calendar keeps the day you pointed at rather
        // than defaulting to today (§7.3, P§7). Porticus resolves the *home* by layout
        // and overwrites the node here; only the `at` survives.
        self.last_node.clone().map(|node| Target::Node {
            node,
            at: Some(key_of(self.cursor)),
        })
    }

    fn navigate(&mut self, nav: Nav) -> Handled {
        // The cell cursor is the view's own internal motion, so the arrows are the
        // grid's here and never the row list's (P§3).
        match nav {
            Nav::Left => self.shift_days(-1),
            Nav::Right => self.shift_days(1),
            Nav::Up => self.shift_days(-7),
            Nav::Down => self.shift_days(7),
            Nav::Key('[') => self.shift_month(-1),
            Nav::Key(']') => self.shift_month(1),
            Nav::Key('t') => self.cursor = jiff::Zoned::now().date(),
            Nav::Key(_) => return Handled::No,
        }
        Handled::Yes
    }

    fn locator(&self) -> Option<String> {
        // A Full view names its own locator in the header, where a Rail view shows the
        // path bar (P§4): a Calendar's month.
        Some(format!(
            "{} {}",
            month_name(self.cursor.month()),
            self.cursor.year()
        ))
    }

    fn empty_line(&self) -> &'static str {
        self.empty
    }
}

fn month_name(month: i8) -> &'static str {
    match month {
        1 => "January",
        2 => "February",
        3 => "March",
        4 => "April",
        5 => "May",
        6 => "June",
        7 => "July",
        8 => "August",
        9 => "September",
        10 => "October",
        11 => "November",
        _ => "December",
    }
}
