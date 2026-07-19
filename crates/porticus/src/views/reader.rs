//! `Reader` (draw · Rail) — a document rendered: frontmatter over a Markdown body
//! (P§3). Tabella's read view.
//!
//! **Reading only.** Editing suspends to the hand's own editor (P§10, §7.3) — there is
//! no in-TUI text editor here or anywhere, and twelve instruments do not each grow a
//! worse vim (P§11).

use pantheon::Code;
use pulldown_cmark::{Event, HeadingLevel, Parser, Tag, TagEnd};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget, Wrap};

use crate::action::{Action, Target};
use crate::view::{Layout, Row, View, ViewId};
use crate::{Handled, Nav, Theme};

/// A document as the reader needs it: the two frontmatter fields the fold reads
/// (§6.1), plus the prose.
pub struct Document {
    pub slug: String,
    pub r#type: Option<String>,
    pub tags: Vec<String>,
    pub body: String,
}

/// The app's document at the cursor, folded fresh each frame.
pub struct Reader<F> {
    fold: F,
    actions: Vec<Action>,
    /// How far down the body the eye is — cursor state, not a fold (P§6).
    scroll: u16,
}

impl<F> Reader<F>
where
    F: FnMut(&Code) -> Option<Document>,
{
    pub fn of(fold: F) -> Self {
        Self {
            fold,
            actions: Vec::new(),
            scroll: 0,
        }
    }

    #[must_use]
    pub fn offering(mut self, actions: &[Action]) -> Self {
        self.actions = actions.to_vec();
        self
    }
}

impl<F> View for Reader<F>
where
    F: FnMut(&Code) -> Option<Document>,
{
    fn id(&self) -> ViewId {
        "read"
    }

    fn layout(&self) -> Layout {
        Layout::Rail
    }

    fn rows(&mut self, _node: &Code) -> Option<Vec<Row>> {
        None
    }

    fn actions(&self) -> &[Action] {
        &self.actions
    }

    fn target(&self) -> Option<Target> {
        None
    }

    fn navigate(&mut self, nav: Nav) -> Handled {
        // Scrolling prose is the one motion a Reader owns; everything else is the
        // rail's (P§3, P§6).
        match nav {
            Nav::Down => {
                self.scroll = self.scroll.saturating_add(1);
                Handled::Yes
            }
            Nav::Up => {
                self.scroll = self.scroll.saturating_sub(1);
                Handled::Yes
            }
            _ => Handled::No,
        }
    }

    fn empty_line(&self) -> &'static str {
        "no document here"
    }

    fn draw(&mut self, node: &Code, area: Rect, buf: &mut Buffer, theme: Theme) {
        let Some(document) = (self.fold)(node) else {
            let middle = Rect {
                y: area.y + area.height / 2,
                height: 1,
                ..area
            };
            Paragraph::new(self.empty_line())
                .style(theme.dim())
                .alignment(ratatui::layout::Alignment::Center)
                .render(middle, buf);
            return;
        };

        let mut lines = vec![Line::from(Span::styled(
            document.slug.clone(),
            theme.name(),
        ))];
        let mut meta = Vec::new();
        if let Some(kind) = &document.r#type {
            meta.push(Span::styled(kind.clone(), theme.text()));
        }
        if !document.tags.is_empty() {
            meta.push(Span::styled(
                format!("  {}", document.tags.join(", ")),
                theme.dim(),
            ));
        }
        if !meta.is_empty() {
            lines.push(Line::from(meta));
        }
        lines.push(Line::from(String::new()));
        lines.extend(markdown(&document.body, theme));

        Paragraph::new(lines)
            .style(theme.text())
            .wrap(Wrap { trim: false })
            .scroll((self.scroll, 0))
            .render(area, buf);
    }
}

/// Markdown, rendered as far as a terminal usefully can (P§3).
///
/// `pulldown-cmark` parses; the styling is deliberately thin — headings bold and
/// accented, emphasis and strong carried, code and quotes dimmed, list items bulleted.
/// A terminal is not a browser, and a renderer that chased every construct would be
/// building the in-TUI viewer P§11 says not to.
fn markdown(body: &str, theme: Theme) -> Vec<Line<'static>> {
    fn flush(current: &mut Vec<Span<'static>>, lines: &mut Vec<Line<'static>>) {
        if current.is_empty() {
            lines.push(Line::from(String::new()));
        } else {
            lines.push(Line::from(std::mem::take(current)));
        }
    }

    let mut lines: Vec<Line> = Vec::new();
    let mut current: Vec<Span> = Vec::new();
    let mut style = theme.text();
    let mut in_code = false;
    let mut list_depth = 0usize;

    for event in Parser::new(body) {
        match event {
            Event::Start(Tag::Heading { level, .. }) => {
                if !current.is_empty() {
                    flush(&mut current, &mut lines);
                }
                lines.push(Line::from(String::new()));
                style = if level == HeadingLevel::H1 {
                    theme.name()
                } else {
                    theme.text().add_modifier(Modifier::BOLD)
                };
            }
            Event::End(TagEnd::Heading(_)) => {
                flush(&mut current, &mut lines);
                style = theme.text();
            }
            Event::Start(Tag::Emphasis) => style = style.add_modifier(Modifier::ITALIC),
            Event::End(TagEnd::Emphasis) => style = style.remove_modifier(Modifier::ITALIC),
            Event::Start(Tag::Strong) => style = style.add_modifier(Modifier::BOLD),
            Event::End(TagEnd::Strong) => style = style.remove_modifier(Modifier::BOLD),
            Event::Start(Tag::BlockQuote(_) | Tag::CodeBlock(_)) => {
                if !current.is_empty() {
                    flush(&mut current, &mut lines);
                }
                in_code = true;
                style = theme.dim();
            }
            Event::End(TagEnd::BlockQuote(_) | TagEnd::CodeBlock) => {
                in_code = false;
                style = theme.text();
            }
            Event::Start(Tag::List(_)) => list_depth += 1,
            Event::End(TagEnd::List(_)) => list_depth = list_depth.saturating_sub(1),
            Event::Start(Tag::Item) => {
                current.push(Span::styled(
                    format!("{}• ", "  ".repeat(list_depth.saturating_sub(1))),
                    theme.dim(),
                ));
            }
            Event::Text(text) | Event::Code(text) => {
                // A code block arrives line by line; keep its breaks.
                if in_code && text.contains('\n') {
                    for piece in text.split('\n') {
                        current.push(Span::styled(piece.to_string(), style));
                        flush(&mut current, &mut lines);
                    }
                } else {
                    current.push(Span::styled(text.to_string(), style));
                }
            }
            Event::SoftBreak => current.push(Span::styled(" ".to_string(), style)),
            // A finished item, a finished paragraph, and an explicit break all end
            // the line being built — the same act, so one arm.
            Event::End(TagEnd::Item | TagEnd::Paragraph) | Event::HardBreak => {
                flush(&mut current, &mut lines);
            }
            Event::Rule => {
                flush(&mut current, &mut lines);
                lines.push(Line::from(Span::styled("─".repeat(24), theme.chrome())));
            }
            _ => {}
        }
    }
    if !current.is_empty() {
        lines.push(Line::from(current));
    }
    lines
}
