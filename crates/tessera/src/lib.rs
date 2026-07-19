//! Tessera — the tiles (§11.2).
//!
//! Small droppable widgets that **fold core JSON into a present and draw it small** —
//! a counter, a GPA, net worth, the local time where a friend lives. A tile reads
//! whatever cores it finds on `PATH` (I4, §12), stores nothing, and recomputes from
//! the latest readings on each data refresh (I1, §5.0), drawing into the caller's
//! `Buffer`.
//!
//! # A peer, never a dependant
//!
//! Tessera links `pantheon` and `ratatui-core` and **nothing else** — in particular
//! not `porticus` (§11, I5). A tile draws into a caller-supplied [`Buffer`] and takes
//! its colours the same way, so it needs no chrome at all. Upstream's own rule is the
//! same one: widget libraries take `ratatui-core`, applications take `ratatui`. Both
//! resolve to one `ratatui-core`, so a tile's `Buffer` *is* the chrome's.
//!
//! # The one package that may fold across cores
//!
//! Porticus fetches nothing, taking everything as JSON already read, and no core reads
//! another (I5, §8.4) — so a friend's clock (Album → an open Fasti residence span →
//! Mappa's `timezone`) and any present the three lenses share are folded **here**,
//! once, rather than three times. Lenses arrange tiles into a **mosaic**, the lens's
//! own draw-view, since a grid of tiles is nothing Porticus can host without importing
//! Tessera (§12).
//!
//! A core may drop in a tile over its **own** readings, linking `tessera` beside
//! `porticus` — never one that reaches another core, which is a lens's alone to do
//! (I5, §12).

use std::process::Command;

use ratatui_core::buffer::Buffer;
use ratatui_core::layout::Rect;
use ratatui_core::style::{Color, Style};
use ratatui_core::text::Line;
use ratatui_core::widgets::Widget;
use serde_json::Value;

/// What a tile draws: a caption, a present, and an optional gloss beneath it.
///
/// Deliberately not a widget tree. A tile is *small* — the whole point of the mosaic
/// is that many fit — so the shape is fixed and the tile only chooses the strings.
#[derive(Clone, Debug, Default)]
pub struct Face {
    /// What this is — "open tasks", "net worth".
    pub caption: String,
    /// The present, folded. The one line the eye goes to.
    pub value: String,
    /// A quieter second line — a trend, a unit, a count behind the count.
    pub gloss: Option<String>,
}

/// A tile: something that folds a present and draws it small.
///
/// `fold` is called on each data refresh and its result is **never kept** (I1) — a
/// tile that cached its value would be a stored present, which is the one thing the
/// whole system is built to avoid.
pub trait Tile {
    /// Fold the present from whatever readings this tile reads.
    fn fold(&mut self) -> Face;
}

/// Draw a face into the caller's buffer.
///
/// The caller owns the layout and the colours: Tessera is handed a [`Rect`] and two
/// [`Color`]s and paints inside them, which is what lets one tile look native in a
/// lens's mosaic and in a core's own screen without knowing either (§11.2).
pub fn draw(face: &Face, area: Rect, buf: &mut Buffer, accent: Color, dim: Color) {
    if area.height == 0 || area.width == 0 {
        return;
    }
    let mut lines = vec![
        Line::styled(face.caption.clone(), Style::default().fg(dim)),
        Line::styled(face.value.clone(), Style::default().fg(accent)),
    ];
    if let Some(gloss) = &face.gloss {
        lines.push(Line::styled(gloss.clone(), Style::default().fg(dim)));
    }
    for (offset, line) in lines.into_iter().enumerate() {
        let Ok(offset) = u16::try_from(offset) else {
            break;
        };
        if offset >= area.height {
            break;
        }
        let row = Rect {
            y: area.y + offset,
            height: 1,
            ..area
        };
        line.render(row, buf);
    }
}

// ── reading the cores (I4, §12) ──────────────────────────────────────────────

/// Run a core's read verb and parse its JSON.
///
/// This is the *whole* of how a tile reaches a core: a subprocess on `PATH` and the
/// contract's JSON coming back (I4). Tessera links no core (I5), so there is nothing
/// else it could do — and that is the property that lets a tile fold across cores
/// where a core itself may not.
///
/// `None` where the core is absent, failed, or answered with something that is not
/// JSON. A tile whose core isn't installed is simply **absent**, not broken (§12) —
/// which is what makes installing one app real (§15.5).
///
/// **`root` is named, never inherited.** A tile reads the tree its caller is showing,
/// and `$PANTHEON_ROOT` is the caller's ambient state (§6.2) — a lens opened with `-C`
/// would otherwise fold one tree while drawing another. `-C` is universal (§7.3), so
/// every core takes it.
#[must_use]
pub fn read(root: &std::path::Path, short: &str, args: &[&str]) -> Option<Value> {
    let out = Command::new(short)
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    serde_json::from_slice(&out.stdout).ok()
}

/// Whether a core is on `PATH` — the probe behind a tile's absence (§12).
#[must_use]
pub fn installed(short: &str) -> bool {
    Command::new(short)
        .arg("version")
        .output()
        .is_ok_and(|out| out.status.success())
}

// ── the tiles the day needs ──────────────────────────────────────────────────

/// A count of rows a core's fold returned — open tasks, people at a node, documents.
///
/// The simplest possible tile, and the one that shows what all of them are: it holds
/// the *question* (which core, which verb) and never the answer.
pub struct Count {
    caption: String,
    short: &'static str,
    args: Vec<String>,
    gloss: Option<String>,
    root: std::path::PathBuf,
}

impl Count {
    #[must_use]
    pub fn of(
        root: impl Into<std::path::PathBuf>,
        caption: impl Into<String>,
        short: &'static str,
        args: &[&str],
    ) -> Self {
        Self {
            caption: caption.into(),
            short,
            args: args.iter().map(|a| (*a).to_string()).collect(),
            gloss: None,
            root: root.into(),
        }
    }

    #[must_use]
    pub fn glossed(mut self, gloss: impl Into<String>) -> Self {
        self.gloss = Some(gloss.into());
        self
    }
}

impl Tile for Count {
    fn fold(&mut self) -> Face {
        let args: Vec<&str> = self.args.iter().map(String::as_str).collect();
        // An absent core is an absent figure, never a zero — a zero is a fold that ran
        // and found nothing, and the two must not read the same (§12). A non-array
        // answer is the same absence: the verb did not return a fold.
        let value = match read(&self.root, self.short, &args) {
            Some(Value::Array(rows)) => rows.len().to_string(),
            _ => "—".to_string(),
        };
        Face {
            caption: self.caption.clone(),
            value,
            gloss: self.gloss.clone(),
        }
    }
}
