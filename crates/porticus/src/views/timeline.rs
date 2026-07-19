//! `Timeline` (draw · Full) — periods as bars across a range (P§3).
//!
//! Fasti's spans: a life, a career, an enrolment, a residence. The bar and
//! [`Card`](super::Card)'s strip are the one [`Span_`](super::CardSpan) type, so a
//! period is drawn once (I3).
//!
//! **Bar-rows carry their home.** A timeline is cross-node — a career at one node, a
//! residence at another — so each bar names its own record and an action on it resolves
//! exactly as a row's would (P§7). That is what lets a draw-view offer `Edit` at all:
//! it has an address, not a cursor into someone else's list.
//!
//! An **open** period is drawn to the right edge of the range and labelled as open, not
//! as ending today: the absent `to` is a state, not a missing value (§8.4).

use jiff::civil::Date;
use pantheon::Code;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget};

use crate::action::{Action, Target};
use crate::view::{Layout, Nav, Row, View, ViewId};
use crate::views::entity_card::Span_;
use crate::{Handled, Theme};

/// Parse a `YYMMDD` reading key (§6.1) into a date. A key that does not parse is a
/// `pan validate` finding, not a drawing failure — the bar is simply dropped.
fn parse_key(key: &str) -> Option<Date> {
    if key.len() != 6 {
        return None;
    }
    let year: i16 = key.get(0..2)?.parse().ok()?;
    let month: i8 = key.get(2..4)?.parse().ok()?;
    let day: i8 = key.get(4..6)?.parse().ok()?;
    // A two-digit year is this century — the same reading every core writes.
    Date::new(2000 + year, month, day).ok()
}

/// Where a date sits along the track, in columns from its left edge.
///
/// Saturating throughout: a span reaching outside the drawn range clamps to an edge
/// rather than wrapping, which is what a range derived from the bars themselves makes
/// unreachable anyway — but a bar is drawn from a record a hand can edit (I8).
fn place(offset: jiff::Span, track: usize, total: i64) -> usize {
    let days = i64::from(offset.get_days()).max(0);
    let track = i64::try_from(track).unwrap_or(i64::MAX);
    usize::try_from(days.saturating_mul(track) / total.max(1)).unwrap_or(0)
}

/// The instrument's periods as bars, folded fresh each frame.
pub struct Timeline<F> {
    fold: F,
    actions: Vec<Action>,
    empty: &'static str,
    /// Which bar is focused — a cursor, never derived (I1).
    cursor: usize,
    /// How many bars the last fold produced, so motion can be bounded without folding
    /// again. Refreshed every draw.
    count: usize,
    /// The focused bar's address, captured at draw so an action resolves against what
    /// was on screen (P§5).
    focused: Option<Target>,
    /// The drawn range, for the header locator (P§4).
    range: Option<(Date, Date)>,
}

impl<F> Timeline<F>
where
    F: FnMut() -> Vec<Span_>,
{
    /// Capture the instrument's fold (P§3).
    pub fn of(fold: F) -> Self {
        Self {
            fold,
            actions: Vec::new(),
            empty: "no periods yet",
            cursor: 0,
            count: 0,
            focused: None,
            range: None,
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
}

impl<F> View for Timeline<F>
where
    F: FnMut() -> Vec<Span_>,
{
    fn id(&self) -> ViewId {
        "timeline"
    }

    fn layout(&self) -> Layout {
        Layout::Full
    }

    fn rows(&mut self, _node: &Code) -> Option<Vec<Row>> {
        // A draw-view: it paints itself, including its own empty (P§3).
        None
    }

    fn actions(&self) -> &[Action] {
        &self.actions
    }

    fn target(&self) -> Option<Target> {
        // The focused bar's own home and key (P§7) — captured at draw, so a refold that
        // reorders the bars cannot land the keystroke on a different period.
        self.focused.clone()
    }

    fn navigate(&mut self, nav: Nav) -> Handled {
        match nav {
            Nav::Up => {
                self.cursor = self.cursor.saturating_sub(1);
                Handled::Yes
            }
            Nav::Down => {
                self.cursor = (self.cursor + 1).min(self.count.saturating_sub(1));
                Handled::Yes
            }
            // Left/Right would pan a range this view derives rather than holds; there is
            // nothing to pan to, so they fall through to Porticus (P§3).
            _ => Handled::No,
        }
    }

    fn locator(&self) -> Option<String> {
        Some(match self.range {
            Some((from, to)) => format!("{from}–{to}"),
            None => "no range".into(),
        })
    }

    fn empty_line(&self) -> &'static str {
        self.empty
    }

    fn draw(&mut self, _node: &Code, area: Rect, buf: &mut Buffer, theme: Theme) {
        let spans = (self.fold)();
        self.count = spans.len();
        self.cursor = self.cursor.min(spans.len().saturating_sub(1));
        if spans.is_empty() {
            self.focused = None;
            self.range = None;
            let middle = Rect {
                y: area.y + area.height / 2,
                height: 1,
                ..area
            };
            Paragraph::new(self.empty)
                .style(theme.dim())
                .alignment(ratatui::layout::Alignment::Center)
                .render(middle, buf);
            return;
        }

        // The range is every period's extent, derived from the bars themselves — a
        // timeline shows what it has, and holds no window of its own (I1).
        let dated: Vec<(usize, Date, Option<Date>)> = spans
            .iter()
            .enumerate()
            .filter_map(|(index, span)| {
                let from = parse_key(&span.from)?;
                Some((index, from, span.to.as_deref().and_then(parse_key)))
            })
            .collect();
        let start = dated.iter().map(|(_, from, _)| *from).min();
        // An open period reaches the right edge, so it extends the range to today.
        let today = jiff::Zoned::now().date();
        let end = dated
            .iter()
            .map(|(_, from, to)| match to {
                Some(to) => *to,
                None => today.max(*from),
            })
            .max();
        let (Some(start), Some(end)) = (start, end) else {
            self.focused = None;
            self.range = None;
            return;
        };
        self.range = Some((start, end));
        let total = i64::from((end - start).get_days()).max(1);

        let label_width = spans
            .iter()
            .map(|span| span.label.chars().count())
            .max()
            .unwrap_or(0)
            .min(24);
        let track = usize::from(area.width)
            .saturating_sub(label_width + 2)
            .max(1);

        let mut lines = Vec::new();
        for (position, span) in spans.iter().enumerate() {
            let focused = position == self.cursor;
            let style = if focused { theme.focus() } else { theme.text() };
            let bar = match dated.iter().find(|(index, _, _)| *index == position) {
                Some((_, from, to)) => {
                    let begin = place(*from - start, track, total);
                    let finish = match to {
                        Some(to) => place(*to - start, track, total),
                        // An open period reaches the right edge: the absent `to` is a
                        // state, not a value to invent (§8.4).
                        None => track,
                    };
                    let width = finish.saturating_sub(begin).max(1);
                    format!(
                        "{}{}",
                        " ".repeat(begin.min(track)),
                        "─".repeat(width.min(track.saturating_sub(begin.min(track))).max(1))
                    )
                }
                // A period whose bounds do not parse still names itself — the record is
                // there and the screen says so; the CLI over the same file says why.
                None => String::new(),
            };
            lines.push(Line::from(vec![
                Span::styled(
                    format!(
                        "{:<width$}  ",
                        truncate(&span.label, label_width),
                        width = label_width
                    ),
                    style,
                ),
                Span::styled(bar, if focused { theme.focus() } else { theme.dim() }),
            ]));
        }

        self.focused = spans
            .get(self.cursor)
            .map(|span| Target::Row(span.home.clone()));

        Paragraph::new(lines).style(theme.text()).render(area, buf);
    }
}

fn truncate(label: &str, width: usize) -> String {
    if label.chars().count() <= width {
        return label.to_owned();
    }
    label
        .chars()
        .take(width.saturating_sub(1))
        .collect::<String>()
        + "…"
}
