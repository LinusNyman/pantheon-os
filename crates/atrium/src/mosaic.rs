//! The Mosaic — Atrium's lead view (P§3, P§9, §12).
//!
//! **A lens writes its own mosaic.** Porticus ships no such view and could not: a grid
//! of tiles is nothing but Tessera, and Porticus links no Tessera (§11, I5). So the
//! catalog holds what Porticus can draw with Pantheon and `ratatui` alone, and this —
//! the arrangement of tiles a lens links itself — lives here.
//!
//! It is a **draw-view**: it returns no rows, paints itself, and opts out of `/` search
//! by construction (P§3).

use pantheon::Code;
use porticus::view::{Layout, Row, View, ViewId};
use porticus::{Handled, Nav, Theme};
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Direction, Layout as Cut, Rect};
use tessera::Tile;

/// A grid of tiles, folded fresh every frame.
pub struct Mosaic {
    tiles: Vec<Box<dyn Tile>>,
}

impl Mosaic {
    #[must_use]
    pub fn of(tiles: Vec<Box<dyn Tile>>) -> Self {
        Self { tiles }
    }
}

impl View for Mosaic {
    fn id(&self) -> ViewId {
        "mosaic"
    }

    fn layout(&self) -> Layout {
        // The dashboard, not the tree, opens first for a lens (P§3).
        Layout::Full
    }

    fn rows(&mut self, _node: &Code) -> Option<Vec<Row>> {
        // A draw-view: `None` means "I paint my own", including my own empty (P§3).
        None
    }

    fn draw(&mut self, _node: &Code, area: Rect, buf: &mut Buffer, theme: Theme) {
        if self.tiles.is_empty() {
            return;
        }
        // Three across, as many rows as it takes — a fixed shape, because a mosaic that
        // reflowed per tile count would read as a different screen each launch.
        let across = 3usize;
        let down = self.tiles.len().div_ceil(across);
        let Ok(down_u16) = u16::try_from(down) else {
            return;
        };

        let rows = Cut::default()
            .direction(Direction::Vertical)
            .constraints(vec![Constraint::Ratio(1, u32::from(down_u16)); down])
            .split(area);

        for (index, tile) in self.tiles.iter_mut().enumerate() {
            let cells = Cut::default()
                .direction(Direction::Horizontal)
                .constraints(vec![Constraint::Ratio(1, 3); across])
                .split(rows[index / across]);
            // Folded here, drawn, and dropped — a tile never stores its value (I1).
            let face = tile.fold();
            tessera::draw(
                &face,
                cells[index % across],
                buf,
                theme.accent,
                porticus::theme::DIM,
            );
        }
    }

    fn navigate(&mut self, _nav: Nav) -> Handled {
        Handled::No
    }

    fn locator(&self) -> Option<String> {
        Some("the day".into())
    }
}
