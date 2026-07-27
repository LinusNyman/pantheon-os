//! The shared ground (P§8): one palette under every instrument, so a node reads the
//! same in all twelve (I3).
//!
//! Hardcoded, with no theme file and no setting (§18, P§11). Ink-dark, parchment-light
//! — the classical feel *inverted* for the terminal: parchment-toned text on an ink
//! ground, not black on white.

use ratatui::style::{Color, Modifier, Style};

use crate::ident::Ident;

// The classical pass (C5): a measured warm/legibility lift over the same ink-on-vellum
// model — deeper ink, a warmer vellum that reads brighter, a secondary that no longer
// sinks into the ground, and an inscribed rule with a touch more presence. Values only;
// the accent's restraint (name + focus alone, P§8) is unchanged.

/// Warm near-black — the ink.
pub const GROUND: Color = Color::Rgb(0x16, 0x12, 0x0B);
/// Bone / vellum — the reading colour.
pub const TEXT: Color = Color::Rgb(0xD8, 0xCF, 0xBC);
/// Muted taupe — empty nodes, secondary text; lifted just clear of the ground.
pub const DIM: Color = Color::Rgb(0x7A, 0x72, 0x63);
/// Box borders, the calendar grid — an inscribed rule, present but held back.
pub const CHROME: Color = Color::Rgb(0x4A, 0x44, 0x37);
/// The selection block behind a focused row.
pub const FOCUS_BG: Color = Color::Rgb(0x33, 0x2D, 0x22);

/// The per-sphere tree colours — a *second* shared set, one colour per top-level
/// sphere, so a node reads the same in every instrument (I3).
///
/// **Not keyed to sphere names.** The ontology is derived from disk and personal
/// (§5.0, §2), so there is no fixed set of names to key on: Porticus holds this small
/// fixed palette and assigns it to the top-level spheres in **code order** (§5.1),
/// cycling if a tree has more tops than colours. Stable, name-independent, identical
/// everywhere — and a tree of any shape gets an answer.
const SPHERES: &[Color] = &[
    Color::Rgb(0xCE, 0x95, 0x60), // amber
    Color::Rgb(0x74, 0xA8, 0x8E), // sage
    Color::Rgb(0x90, 0x96, 0xCC), // periwinkle
    Color::Rgb(0xC9, 0x85, 0x94), // rose
    Color::Rgb(0xA6, 0xB3, 0x68), // olive
    Color::Rgb(0x86, 0xA6, 0xC4), // slate blue
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
