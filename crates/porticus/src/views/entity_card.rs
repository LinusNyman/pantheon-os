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
/// **Followable within one core** (G5, P§3). `h`/`l` move between chips and `f` follows
/// the focused one: Porticus resolves the token, and where it names *this* instrument's
/// own core it pins that record and takes the tree to its node.
///
/// A **cross-core** chip never could be followed: the running instrument links only its
/// own core, so it cannot render another core's card in place. You open `album:mara` by
/// leaving to `alb` (I4, I5), never by a hop inside `fas` — and pressing `f` on one says
/// exactly that rather than doing nothing.
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
    /// Which ref chip `f` would follow. A cursor over a list the *fold* produces, so it
    /// is clamped against the live chips each frame rather than trusted (I1); a card that
    /// re-folds with fewer refs must not point past them.
    chip: usize,
    /// The chips the last frame drew, kept only so `focused_ref` can answer between
    /// frames — the addresses, never the records (I1).
    chip_refs: Vec<String>,
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
            chip: 0,
            chip_refs: Vec::new(),
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

    fn nav_keys(&self) -> &[(char, &'static str)] {
        // `h`/`l` are Porticus's own motion and reach a content-focused view without
        // being declared; only the follow is this view's to name (P§5).
        &[]
    }

    fn navigate(&mut self, nav: Nav) -> Handled {
        // The chip strip is the card's one internal cursor, so `h`/`l` walk it while the
        // card has content focus. Up/Down are left to Porticus — a card has no rows.
        if self.chip_refs.len() < 2 {
            return Handled::No;
        }
        match nav {
            Nav::Right => {
                self.chip = (self.chip + 1) % self.chip_refs.len();
                Handled::Yes
            }
            Nav::Left => {
                self.chip = (self.chip + self.chip_refs.len() - 1) % self.chip_refs.len();
                Handled::Yes
            }
            _ => Handled::No,
        }
    }

    fn focused_ref(&self) -> Option<String> {
        self.chip_refs.get(self.chip).cloned()
    }

    fn is_detail(&self) -> bool {
        true
    }

    fn pin(&mut self, record: Option<RecordRef>) {
        // Nothing else to reset: a card holds no scroll of its own, and everything it
        // draws is folded from the pin each frame (I1). The chip cursor is the one thing
        // that is *about the pin* rather than about the record, so it starts over: a
        // followed chip lands on the new card's first ref, not the old card's third.
        self.pinned = record;
        self.chip = 0;
        self.chip_refs.clear();
    }

    fn empty_line(&self) -> &'static str {
        self.empty
    }

    fn draw(&mut self, node: &Code, area: Rect, buf: &mut Buffer, theme: Theme) {
        self.chip_refs.clear();
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
            // The strip is the cursor's list, so it is recorded as it is drawn — one
            // fold, read two ways, never a second walk of the record (I1).
            self.chip_refs = card.chips.iter().map(|c| c.reference.clone()).collect();
            self.chip = self.chip.min(self.chip_refs.len() - 1);
            lines.push(Line::from(String::new()));
            let mut spans = Vec::new();
            for (index, chip) in card.chips.iter().enumerate() {
                // The focused chip carries the accent, which P§8 reserves for the name
                // and the focus — a chip cursor is a focus.
                let style = if index == self.chip {
                    theme.name()
                } else {
                    theme.focus()
                };
                spans.push(Span::styled(format!(" {} ", chip.label), style));
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
