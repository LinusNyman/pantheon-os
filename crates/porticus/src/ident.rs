//! Identity (P§8): the one signature each instrument carries over the shared ground.
//!
//! Every field is a compile-time constant, not a setting (§18, P§11). The accent is
//! the *only* per-tool knob in the whole layer, and it is a field on this struct
//! rather than anything a hand could edit.

use ratatui::style::Color;

/// What an instrument is, rendered.
///
/// `name` feeds the header word and the title banner; Porticus renders it — an
/// instrument hands over a string, never its own art (P§11).
#[derive(Clone, Copy, Debug)]
pub struct Ident {
    /// `"pensum"` — rendered as the tracked header word and the title banner.
    pub name: &'static str,
    /// `"pen"` — the three-char short (§7.3).
    pub short: &'static str,
    /// `"intention · tasks"`.
    pub tagline: &'static str,
    /// The instrument's mark — the cosmology of P§9.
    pub symbol: char,
    /// The one per-instrument colour, threaded into the name and the focused item
    /// and nothing structural (P§8).
    pub accent: Color,
}

/// The locked accent palette (P§9), hardcoded here so the twelve are chosen once.
///
/// Cores are source light and saturated; lenses are reflected light, muted and
/// cooler; `pan` is the neutral frame; `aus` the one hot alarm. Planetary metal ties
/// the hue where it can.
pub mod accent {
    use ratatui::style::Color;

    /// Album ♀ — Venus, copper→green.
    pub const VERDIGRIS: Color = Color::Rgb(0x4E, 0x9A, 0x6B);
    /// Mappa ♁ — Earth, fired clay.
    pub const TERRACOTTA: Color = Color::Rgb(0xB4, 0x63, 0x3C);
    /// Rationes ♃ — Jupiter, regal blue.
    pub const LAPIS: Color = Color::Rgb(0x3B, 0x6E, 0xA5);
    /// Fasti ♄ — Saturn, lead.
    pub const LEAD_VIOLET: Color = Color::Rgb(0x6E, 0x5A, 0x8C);
    /// Pensum ♂ — Mars, the reserved red. The anchor of the set.
    pub const MINIUM: Color = Color::Rgb(0xD6, 0x5A, 0x8E);
    /// Annales ☉ — Sun, gold.
    pub const SOL_GOLD: Color = Color::Rgb(0xD4, 0xB0, 0x2A);
    /// Tabella ☿ — Mercury, silver-teal.
    pub const QUICKSILVER: Color = Color::Rgb(0x4F, 0xB0, 0xB8);
    /// Speculum ☽ — Moon, reflected, pale.
    pub const MOON_SILVER: Color = Color::Rgb(0xA7, 0xB0, 0xC0);
    /// Atrium ☊ — the welcoming room, softened.
    pub const HEARTH: Color = Color::Rgb(0xC9, 0xA0, 0x6E);
    /// Studium ☋ — the scholar's violet.
    pub const LAVENDER: Color = Color::Rgb(0x8E, 0x86, 0xC9);
    /// Pantheon ✶ — the frame, un-coloured.
    pub const STONE: Color = Color::Rgb(0xB8, 0xAE, 0x93);
    /// Auspex ☄ — the comet, the omen.
    pub const CINNABAR: Color = Color::Rgb(0xE2, 0x4A, 0x2B);
}

impl Ident {
    /// The header word, inscriptionally tracked (P§8): `P E N S U M`.
    ///
    /// The spacing *is* the Trajan-column feel, and it is free in a fixed terminal
    /// font — so it is done here rather than asked of a font.
    #[must_use]
    pub fn tracked(&self) -> String {
        self.name
            .to_uppercase()
            .chars()
            .map(|c| c.to_string())
            .collect::<Vec<_>>()
            .join(" ")
    }
}
