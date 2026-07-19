//! The shared ground (P§8): one palette under every instrument, so a node reads the
//! same in all twelve (I3).
//!
//! Hardcoded, with no theme file and no setting (§18, P§11). Ink-dark, parchment-light
//! — the classical feel *inverted* for the terminal: parchment-toned text on an ink
//! ground, not black on white.

use ratatui::style::{Color, Modifier, Style};

use crate::ident::Ident;

/// Warm near-black — the ink.
pub const GROUND: Color = Color::Rgb(0x14, 0x11, 0x0C);
/// Bone / parchment — the reading colour.
pub const TEXT: Color = Color::Rgb(0xCF, 0xC7, 0xB8);
/// Muted taupe — empty nodes, secondary text.
pub const DIM: Color = Color::Rgb(0x6B, 0x65, 0x58);
/// Box borders, the calendar grid.
pub const CHROME: Color = Color::Rgb(0x3A, 0x36, 0x2E);
/// The selection block behind a focused row.
pub const FOCUS_BG: Color = Color::Rgb(0x2A, 0x26, 0x20);

/// The per-sphere tree colours — a *second* shared set, one colour per top-level
/// sphere, so a node reads the same in every instrument (I3).
///
/// **Not keyed to sphere names.** The ontology is derived from disk and personal
/// (§5.0, §2), so there is no fixed set of names to key on: Porticus holds this small
/// fixed palette and assigns it to the top-level spheres in **code order** (§5.1),
/// cycling if a tree has more tops than colours. Stable, name-independent, identical
/// everywhere — and a tree of any shape gets an answer.
const SPHERES: &[Color] = &[
    Color::Rgb(0xC2, 0x8A, 0x5E), // amber
    Color::Rgb(0x6F, 0x9E, 0x8A), // sage
    Color::Rgb(0x8A, 0x8F, 0xC0), // periwinkle
    Color::Rgb(0xBC, 0x7F, 0x8E), // rose
    Color::Rgb(0x9A, 0xA8, 0x62), // olive
    Color::Rgb(0x7E, 0x9C, 0xB8), // slate blue
];

/// The palette plus this instrument's accent. Held by value and passed to a view each
/// frame — a view reads it, never edits it (P-II).
#[derive(Clone, Copy, Debug)]
pub struct Theme {
    pub accent: Color,
}

impl Theme {
    #[must_use]
    pub fn of(ident: &Ident) -> Self {
        Self {
            accent: ident.accent,
        }
    }

    /// The reading style: parchment on ink.
    #[must_use]
    pub fn text(self) -> Style {
        Style::default().fg(TEXT).bg(GROUND)
    }

    /// Secondary text — an empty node, a hint, an unfocused tab.
    #[must_use]
    pub fn dim(self) -> Style {
        Style::default().fg(DIM).bg(GROUND)
    }

    /// Borders and grids.
    #[must_use]
    pub fn chrome(self) -> Style {
        Style::default().fg(CHROME).bg(GROUND)
    }

    /// The name, wherever it appears (P§8) — accented and bold.
    #[must_use]
    pub fn name(self) -> Style {
        Style::default()
            .fg(self.accent)
            .bg(GROUND)
            .add_modifier(Modifier::BOLD)
    }

    /// The focused row or the active tab: the accent over the selection block. The
    /// only two things the accent touches besides the name (P§8).
    #[must_use]
    pub fn focus(self) -> Style {
        Style::default()
            .fg(self.accent)
            .bg(FOCUS_BG)
            .add_modifier(Modifier::BOLD)
    }

    /// An error on the status line — the one place the palette raises its voice.
    #[must_use]
    pub fn error(self) -> Style {
        Style::default()
            .fg(crate::ident::accent::CINNABAR)
            .bg(GROUND)
    }

    /// A top-level sphere's colour, by its position in code order (§5.1).
    #[must_use]
    pub fn sphere(self, index: usize) -> Color {
        SPHERES[index % SPHERES.len()]
    }
}
