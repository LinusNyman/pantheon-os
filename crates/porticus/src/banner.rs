//! The Title splash's block-caps face (P§8, C7).
//!
//! P§8 wants the summoned Title (`+`) rendered big, in a **classical** face — tall serifed
//! Roman capitals, the inscriptional (Trajan) letter the whole look leans on. The deferral
//! was never about the code but about a font: a third-party figlet `.flf` that `cargo deny`
//! cannot vet (the licence lives in a data file, not a crate's metadata), and no clean one
//! is on hand. So this is a small **embedded** face authored here — eight rows of solid
//! blocks with serif feet and heads, uppercase only (a name is tracked in caps, P§8) —
//! which is public-domain by construction, adds no dependency, and keeps §18's "hardcoded,
//! nothing loaded at runtime": the glyphs compile into the binary like every other constant.
//!
//! An app still hands over only a string (`ident.name`); Porticus renders it, so the look
//! is identical across the twelve (P§11 — no bespoke banners).

/// Every glyph is this many rows tall; a word's banner is exactly this many lines.
pub(crate) const ROWS: usize = 8;

/// One uppercase letter as [`ROWS`] newline-separated rows of equal width — solid and
/// half-blocks (`█▟▙▛▜▘▝▖▗`) forming serifed Roman caps, space for empty. Stored as one
/// string per glyph (rather than an array) so the table stays compact; [`render`] splits it.
/// Unknown characters (a name is `A`–`Z`) render as a blank cell, so a stray byte leaves a
/// gap rather than breaking the row alignment.
fn glyph(c: char) -> &'static str {
    match c {
        'A' => "  ▟██▙  \n ▟█▛▜█▙ \n██▘  ▝██\n██    ██\n████████\n██    ██\n██    ██\n▀▀    ▀▀",
        'B' => "██████▙ \n██   ▜█▖\n██   ▟█▘\n██████▖ \n██   ▜█▖\n██   ▟█▘\n██████▛ \n▀▀▀▀▀▘  ",
        'C' => " ▟█████▙\n██▘   ▝▀\n██      \n██      \n██      \n██      \n▜█▙   ▟▖\n ▀█████▛",
        'D' => "██████▙ \n██   ▜█▙\n██    ██\n██    ██\n██    ██\n██    ██\n██   ▟█▛\n▀█████▀ ",
        'E' => "████████\n██      \n██      \n██████  \n██      \n██      \n██      \n████████",
        'F' => "████████\n██      \n██      \n██████  \n██      \n██      \n██      \n▀▀      ",
        'G' => " ▟█████▙\n██▘   ▝▀\n██      \n██  ████\n██    ██\n██    ██\n▜█▙  ▟██\n ▀████▛▘",
        'H' => "██    ██\n██    ██\n██    ██\n████████\n██    ██\n██    ██\n██    ██\n▀▀    ▀▀",
        'I' => "██████\n  ██  \n  ██  \n  ██  \n  ██  \n  ██  \n  ██  \n██████",
        'J' => "    ████\n      ██\n      ██\n      ██\n      ██\n██    ██\n▜█▙  ▟█▛\n ▀████▀ ",
        'K' => "██   ▟█▛\n██  ▟█▛ \n██ ▟█▛  \n█████▛  \n██  ▜█▙ \n██   ▜█▙\n██    ▜█\n▀▀    ▀▀",
        'L' => "██      \n██      \n██      \n██      \n██      \n██      \n██      \n████████",
        'M' => "▟█▙  ▟█▙\n███▙▟███\n██▝██▘██\n██ ▝▘ ██\n██    ██\n██    ██\n██    ██\n▀▀    ▀▀",
        'N' => "██▙   ██\n███▙  ██\n██▜█▙ ██\n██ ▜█▙██\n██  ▜███\n██   ▜██\n██    ██\n▀▀    ▀▀",
        'O' => " ▟████▙ \n██▘  ▝██\n██    ██\n██    ██\n██    ██\n██    ██\n▜█▙  ▟█▛\n ▀████▀ ",
        'P' => "██████▙ \n██   ▜█▖\n██   ▟█▘\n██████▛ \n██      \n██      \n██      \n▀▀      ",
        'Q' => " ▟████▙ \n██▘  ▝██\n██    ██\n██    ██\n██  ▖ ██\n██ ▜▙▟█▛\n▜█▙ ▜██▖\n ▀███▙▝▙",
        'R' => "██████▙ \n██   ▜█▖\n██   ▟█▘\n██████▛ \n██ ▜█▙  \n██  ▜█▙ \n██   ▜█▙\n▀▀    ▀▀",
        'S' => " ▟█████▙\n██▘   ▝▀\n▜█▙     \n ▀███▙▖ \n    ▝▜██\n      ██\n▟▖   ▟█▛\n ▀████▛ ",
        'T' => "████████\n   ██   \n   ██   \n   ██   \n   ██   \n   ██   \n   ██   \n  ▀██▀  ",
        'U' => "██    ██\n██    ██\n██    ██\n██    ██\n██    ██\n██    ██\n▜█▙  ▟█▛\n ▀████▀ ",
        'V' => "██    ██\n██    ██\n██    ██\n▜█▙  ▟█▛\n ██▖▗██ \n  ████  \n  ▝██▘  \n   ▀▀   ",
        'W' => "██    ██\n██    ██\n██    ██\n██ ▟▙ ██\n██▟██▙██\n███▘▜███\n▜█▛  ▜█▛\n▀▀    ▀▀",
        'X' => "██▙  ▟██\n ▜█▙▟█▛ \n  ▜██▛  \n   ██   \n  ▟██▙  \n ▟█▛▜█▙ \n██▘  ▝██\n▀▀    ▀▀",
        'Y' => "██    ██\n ▜█▙▟█▛ \n  ▜██▛  \n   ██   \n   ██   \n   ██   \n   ██   \n  ▀██▀  ",
        'Z' => "████████\n    ▟█▛ \n   ▟█▛  \n  ▟█▛   \n ▟█▛    \n▟█▛     \n████████\n▀▀▀▀▀▀▀▀",
        _ => "    \n    \n    \n    \n    \n    \n    \n    ",
    }
}

/// Render `text` (any case) as [`ROWS`] lines of serifed block caps, one blank column
/// between glyphs. Every returned line is the same width, so the caller can centre the
/// block as one rectangle.
pub(crate) fn render(text: &str) -> Vec<String> {
    let mut lines = vec![String::new(); ROWS];
    for (i, c) in text.to_uppercase().chars().enumerate() {
        if i > 0 {
            for line in &mut lines {
                line.push(' ');
            }
        }
        for (row, seg) in glyph(c).split('\n').take(ROWS).enumerate() {
            lines[row].push_str(seg);
        }
    }
    lines
}

/// The rendered block's width in columns — the same for every row, so the first is
/// enough.
pub(crate) fn width(lines: &[String]) -> usize {
    lines.first().map_or(0, |l| l.chars().count())
}

#[cfg(test)]
mod tests {
    use super::{ROWS, render, width};

    #[test]
    fn a_word_renders_as_equal_rows_of_serifed_blocks() {
        let banner = render("pensum");
        assert_eq!(banner.len(), ROWS, "a banner is exactly {ROWS} rows");
        // Every row is the same width, or the block would not centre as one rectangle.
        let w = width(&banner);
        assert!(w > 0, "the banner has width");
        assert!(
            banner.iter().all(|l| l.chars().count() == w),
            "every row is the same width: {banner:?}"
        );
        // It is the block face, not the bare letters.
        assert!(
            banner.iter().any(|l| l.contains('█')),
            "the banner is drawn in blocks: {banner:?}"
        );
        // The serif feet sit on the last row — the classical base (P§8).
        assert!(
            banner[ROWS - 1].contains('▀'),
            "the caps have serif feet: {banner:?}"
        );
    }

    #[test]
    fn case_does_not_matter() {
        assert_eq!(render("Pen"), render("PEN"));
    }

    /// Every letter is a well-formed glyph — exactly [`ROWS`] rows, all the same width — so
    /// no transcription slip leaves one letter a row short or a column ragged, which would
    /// misalign the whole banner (the rows concatenate, P§8).
    #[test]
    fn every_letter_is_a_rectangle() {
        for c in 'A'..='Z' {
            let g = render(&c.to_string());
            assert_eq!(g.len(), ROWS, "`{c}` is {} rows, not {ROWS}", g.len());
            let w = width(&g);
            assert!(
                g.iter().all(|l| l.chars().count() == w),
                "`{c}` has ragged rows: {g:?}"
            );
        }
    }
}
