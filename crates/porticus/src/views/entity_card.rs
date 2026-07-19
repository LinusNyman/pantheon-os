//! `EntityCard` (draw · Rail) — one focused entity in detail (P§3).
//!
//! A **detail view**: it renders *one* record, the pinned one. A lineup holds at most
//! one detail view, which is what lets `Enter` route with no shape tag on the record.
//!
//! The card is a Porticus view-model the app fills — **title · labeled fields · ref
//! chips · a timeline strip** — so Album's contact card, Mappa's place, and Rationes'
//! holding are one implementation with three fillings (I3).

use pantheon::Code;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget, Wrap};

use crate::action::{Action, RecordRef, Target};
use crate::view::{Layout, Row, View, ViewId};
use crate::{Handled, Nav, Theme};

/// A `core:slug` ref (§5.4), rendered as a chip.
///
/// **Display-only in v1.** Porticus resolves the reference through *Pantheon* for a
/// legible label — `album:alex (csa)` — because the hub resolves and no core reads
/// another (I5). Following a chip is not offered yet, and a **cross-core** chip never
/// could be: the running instrument links only its own core, so it cannot render
/// another core's card in place. You open `album:mara` by leaving to `alb` (I4), never
/// by a hop inside `fas`.
pub struct Chip {
    pub label: String,
    pub reference: String,
}

/// A period. `Timeline`'s bar and `Card`'s strip share this one type, so a span is
/// drawn once (I3).
pub struct Span_ {
    pub label: String,
    pub from: String,
    /// `None` = open, drawn to the range's right edge.
    pub to: Option<String>,
    /// Its own home and key. A `Timeline` is cross-node, so a bar resolves an action
    /// exactly as a row does (P§3, P§7) — the address rides with the period rather
    /// than leaning on a cursor a Full view does not have.
    pub home: RecordRef,
}

/// One record in detail — the view-model the app fills each frame.
#[derive(Default)]
pub struct Card {
    /// The entity's name — carries the accent (P§8).
    pub title: String,
    /// Labeled fields, folded to display strings, in order.
    pub fields: Vec<(String, String)>,
    /// Its `core:slug` refs.
    pub chips: Vec<Chip>,
    /// A strip of the entity's own spans; empty → none drawn.
    pub strip: Vec<Span_>,
}

/// The app's card for the node's entity, folded fresh each frame.
pub struct EntityCard<F> {
    fold: F,
    /// The record an `Enter`-drill pinned — an address, re-folded each frame (I1).
    pinned: Option<RecordRef>,
    actions: Vec<Action>,
    empty: &'static str,
}

impl<F> EntityCard<F>
where
    F: FnMut(&Code, Option<&RecordRef>) -> Option<Card>,
{
    /// Capture the instrument's fold (P§3).
    ///
    /// `None` → the **empty "pick a record" state**. Reached by its number key with
    /// nothing pinned, a detail view shows the node's one record where the node holds
    /// exactly one of its shape (the entity-as-node, §5.1) and otherwise says so — it
    /// never guesses among several.
    pub fn of(fold: F) -> Self {
        Self {
            fold,
            pinned: None,
            actions: Vec::new(),
            empty: "pick a record",
        }
    }

    #[must_use]
    pub fn offering(mut self, actions: &[Action]) -> Self {
        self.actions = actions.to_vec();
        self
    }
}

impl<F> View for EntityCard<F>
where
    F: FnMut(&Code, Option<&RecordRef>) -> Option<Card>,
{
    fn id(&self) -> ViewId {
        "card"
    }

    fn layout(&self) -> Layout {
        Layout::Rail
    }

    fn rows(&mut self, _node: &Code) -> Option<Vec<Row>> {
        // A draw-view: it paints itself, including its own empty (P§3).
        None
    }

    fn actions(&self) -> &[Action] {
        &self.actions
    }

    fn target(&self) -> Option<Target> {
        // A detail view's selection is the record the *rail* is on, and the app builds
        // its invocation from that node — so there is no separate address to name here
        // (P§7). Returning None lets Porticus fall through to the node target.
        None
    }

    fn navigate(&mut self, _nav: Nav) -> Handled {
        Handled::No
    }

    fn is_detail(&self) -> bool {
        true
    }

    fn pin(&mut self, record: Option<RecordRef>) {
        // Nothing else to reset: a card holds no scroll of its own, and everything it
        // draws is folded from the pin each frame (I1).
        self.pinned = record;
    }

    fn empty_line(&self) -> &'static str {
        self.empty
    }

    fn draw(&mut self, node: &Code, area: Rect, buf: &mut Buffer, theme: Theme) {
        let Some(card) = (self.fold)(node, self.pinned.as_ref()) else {
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
        };

        let mut lines = vec![
            Line::from(Span::styled(card.title.clone(), theme.name())),
            Line::from(String::new()),
        ];

        let width = card
            .fields
            .iter()
            .map(|(label, _)| label.chars().count())
            .max()
            .unwrap_or(0);
        for (label, value) in &card.fields {
            lines.push(Line::from(vec![
                Span::styled(format!("{label:<width$}  "), theme.dim()),
                Span::styled(value.clone(), theme.text()),
            ]));
        }

        if !card.chips.is_empty() {
            lines.push(Line::from(String::new()));
            let mut spans = Vec::new();
            for chip in &card.chips {
                spans.push(Span::styled(format!(" {} ", chip.label), theme.focus()));
                spans.push(Span::styled(" ", theme.text()));
            }
            lines.push(Line::from(spans));
        }

        if !card.strip.is_empty() {
            lines.push(Line::from(String::new()));
            for span in &card.strip {
                let to = span.to.as_deref().unwrap_or("—");
                lines.push(Line::from(vec![
                    Span::styled(format!("{}  ", span.label), theme.text()),
                    Span::styled(format!("{}–{to}", span.from), theme.dim()),
                ]));
            }
        }

        Paragraph::new(lines)
            .style(theme.text())
            .wrap(Wrap { trim: false })
            .render(area, buf);
    }
}
