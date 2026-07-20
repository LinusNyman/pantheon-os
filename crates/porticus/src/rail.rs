//! The tree pane (P§6) — the left rail of a Rail-layout view.
//!
//! Full ontology by default: no sphere-locking (§18). Two annotations and one toggle,
//! all driven by the app's own count at a node — a count badge, empty nodes dimmed,
//! and a records-only collapse.
//!
//! The tree itself is **re-walked**, never cached (§5.0): [`Rail::refold`] runs on the
//! four refresh events and on nothing else. What persists across a refold is the
//! *cursor* — which node is selected, which are expanded — and P§6 says outright that
//! this is cursor state Porticus holds, not a derived value (I1).

use std::collections::HashSet;

use pantheon::tree::{Node, TreeRoot};
use pantheon::{Code, build_tree};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget};

use crate::theme::{self, Theme};

/// The two questions the rail asks about a node (P§6).
///
/// A trait rather than two closures because both answers come from the *same*
/// instrument, and two closures would each want a mutable borrow of it.
///
/// They are separate on purpose. **`any` asks only *any?*** and drives the dim and the
/// `.` collapse; **`count` asks how many** and drives the badge. An instrument whose
/// count is costly overrides `any_at` to answer the dim without a full fold — collapse
/// the two and that override becomes unreachable, and every instrument pays a full
/// count for a yes/no.
pub trait Presence {
    fn any(&mut self, node: &Code) -> bool;
    fn count(&mut self, node: &Code) -> usize;
}

/// One visible line of the outline.
pub struct Visible<'a> {
    pub node: &'a Node,
    pub depth: usize,
    /// Which top-level sphere this node descends from, in code order (§5.1) — the
    /// index the shared sphere palette is keyed on (P§8).
    pub sphere: usize,
    pub expandable: bool,
    pub expanded: bool,
}

/// The rail's own state.
pub struct Rail {
    root: TreeRoot,
    /// Codes whose children are shown. Cursor state (P§6), not a fold.
    expanded: HashSet<String>,
    /// Index into [`Rail::visible`].
    cursor: usize,
    /// `.` — collapse to nodes this instrument actually files at (P§5, P§6).
    records_only: bool,
}

impl Rail {
    /// Walk the tree and open the spheres, so a fresh launch shows the shape rather
    /// than a row of closed roots.
    ///
    /// # Errors
    /// If the root cannot be walked.
    pub fn new(root: &std::path::Path) -> pantheon::Result<Self> {
        let tree = build_tree(root, None)?;
        let mut expanded = HashSet::new();
        for node in tops(&tree) {
            expanded.insert(node.code.as_str().to_owned());
        }
        Ok(Self {
            root: tree,
            expanded,
            cursor: 0,
            records_only: false,
        })
    }

    /// Re-walk the tree, keeping the cursor where it was by **code** rather than by
    /// index (§5.0, P§6).
    ///
    /// By code, because a refold can add or drop nodes underneath — another hand's
    /// `pan new`, a hook (I8, §6.4) — and an index would then point at a different
    /// node than the one the eye is on. If the node is gone the cursor clamps rather
    /// than following whatever slid into its slot.
    ///
    /// # Errors
    /// If the root cannot be walked.
    pub fn refold(&mut self, root: &std::path::Path) -> pantheon::Result<()> {
        let was = self.selected_code();
        self.root = build_tree(root, None)?;
        if let Some(code) = was {
            let found = self
                .visible()
                .iter()
                .position(|v| v.node.code.as_str() == code);
            self.cursor = found.unwrap_or_else(|| self.cursor.min(self.last()));
        }
        Ok(())
    }

    /// The flattened outline, top-down.
    #[must_use]
    pub fn visible(&self) -> Vec<Visible<'_>> {
        let mut out = Vec::new();
        for (index, node) in tops(&self.root).iter().enumerate() {
            self.walk(node, 0, index, &mut out);
        }
        out
    }

    fn walk<'a>(&'a self, node: &'a Node, depth: usize, sphere: usize, out: &mut Vec<Visible<'a>>) {
        let expanded = self.expanded.contains(node.code.as_str());
        out.push(Visible {
            node,
            depth,
            sphere,
            expandable: !node.children.is_empty(),
            expanded,
        });
        if expanded {
            for child in &node.children {
                self.walk(child, depth + 1, sphere, out);
            }
        }
    }

    fn last(&self) -> usize {
        self.visible().len().saturating_sub(1)
    }

    fn selected_code(&self) -> Option<String> {
        self.visible()
            .get(self.cursor)
            .map(|v| v.node.code.as_str().to_owned())
    }

    /// The node the cursor is on — the home a Rail view's `add` resolves to (P§7).
    #[must_use]
    pub fn selected(&self) -> Option<Code> {
        self.visible().get(self.cursor).map(|v| v.node.code.clone())
    }

    /// The breadcrumb the header's path bar shows (P§4).
    #[must_use]
    pub fn breadcrumb(&self) -> String {
        // The current node's own name, not the trail down to it (P§4). The tree beside
        // the header already draws the ancestry, so repeating `sphere · … · node` in the
        // header was noise; the definition of where the cursor *is* stands alone.
        self.visible()
            .get(self.cursor)
            .map(|current| current.node.label.to_string())
            .unwrap_or_default()
    }

    pub fn down(&mut self) {
        self.cursor = (self.cursor + 1).min(self.last());
    }

    pub fn up(&mut self) {
        self.cursor = self.cursor.saturating_sub(1);
    }

    /// Expand, or step to the first child if already open.
    pub fn right(&mut self) {
        let Some(entry) = self.visible().into_iter().nth(self.cursor) else {
            return;
        };
        if entry.expandable && !entry.expanded {
            self.expanded.insert(entry.node.code.as_str().to_owned());
        } else if entry.expandable {
            self.down();
        }
    }

    /// Collapse, or step out to the parent if already closed.
    pub fn left(&mut self) {
        // The outline borrows the tree, and closing a node writes to `expanded` — so
        // read what is needed, then drop it before mutating.
        let (code, depth, expanded, parent) = {
            let visible = self.visible();
            let Some(entry) = visible.get(self.cursor) else {
                return;
            };
            let parent = visible[..self.cursor]
                .iter()
                .rposition(|v| v.depth + 1 == entry.depth);
            (
                entry.node.code.as_str().to_owned(),
                entry.depth,
                entry.expanded,
                parent,
            )
        };
        if expanded {
            self.expanded.remove(&code);
        } else if depth > 0 {
            if let Some(parent) = parent {
                self.cursor = parent;
            }
        }
    }

    /// `Enter` in the rail: descend (P§5).
    pub fn descend(&mut self) {
        self.right();
    }

    /// `.` — the records-only toggle (P§5).
    pub fn toggle_records_only(&mut self) {
        self.records_only = !self.records_only;
    }

    #[must_use]
    pub fn records_only(&self) -> bool {
        self.records_only
    }

    /// Move the cursor to the first node whose code or label matches — what `/` does
    /// when the rail has focus (P§6).
    pub fn seek(&mut self, needle: &str) {
        let needle = needle.to_lowercase();
        if needle.is_empty() {
            return;
        }
        if let Some(found) = self.visible().iter().position(|v| {
            v.node.code.as_str().contains(&needle) || v.node.label.to_lowercase().contains(&needle)
        }) {
            self.cursor = found;
        }
    }

    /// Draw the rail.
    ///
    /// [`Presence`] is asked only for the lines actually on screen, and `count` only
    /// where a badge will show it (P§6).
    pub fn draw(
        &self,
        area: Rect,
        buf: &mut Buffer,
        theme: Theme,
        focused: bool,
        presence: &mut impl Presence,
    ) {
        let visible = self.visible();
        let height = area.height as usize;
        let first = self.cursor.saturating_sub(height.saturating_sub(1));
        let mut lines = Vec::new();

        for (index, entry) in visible.iter().enumerate().skip(first).take(height) {
            // The cheap question first: it is all the dim and the `.` collapse need.
            let holds = presence.any(&entry.node.code);
            if self.records_only && !holds && entry.node.children.is_empty() {
                continue;
            }
            let marker = match (entry.expandable, entry.expanded) {
                (true, true) => '▾',
                (true, false) => '▸',
                (false, _) => ' ',
            };
            let selected = index == self.cursor;
            let body = Style::default().fg(theme.sphere(entry.sphere));
            // Empty nodes dim (P§6) — and *any?* is the whole of that question.
            let body = if holds { body } else { theme.dim() };
            let style = if selected && focused {
                theme.focus()
            } else if selected {
                body.bg(theme::FOCUS_BG)
            } else {
                body
            };
            // The exact count only where a badge will actually show it.
            let badge = if holds {
                format!("  {}", presence.count(&entry.node.code))
            } else {
                String::new()
            };
            lines.push(Line::from(vec![Span::styled(
                format!(
                    "{}{marker} {} {}{badge}",
                    "  ".repeat(entry.depth),
                    entry.node.code.as_str(),
                    entry.node.label
                ),
                style,
            )]));
        }

        Paragraph::new(lines).style(theme.text()).render(area, buf);
    }
}

/// The top-level nodes, whichever shape the walk returned.
fn tops(root: &TreeRoot) -> Vec<&Node> {
    match root {
        TreeRoot::Forest(nodes) => nodes.iter().collect(),
        TreeRoot::Subtree(node) => vec![node],
    }
}
