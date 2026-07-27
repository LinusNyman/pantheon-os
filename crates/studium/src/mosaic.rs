//! The Mosaic — Studium's lead view (P§3, P§9, §12, §19.9).
//!
//! **A lens writes its own mosaic.** Porticus ships no such view and could not: a grid of
//! tiles is nothing but Tessera, and Porticus links no Tessera (§11, I5). So this — the
//! arrangement of the figures behind the studies dashboard — lives here.
//!
//! It is a **draw-view**: it returns no rows, paints itself, and opts out of `/` search by
//! construction (P§3). The whole `Figures` object is folded **once per frame** and dropped
//! — never stored (I1) — and each face is derived from it, so the GPA a hand sees and the
//! GPA an LLM pipes are one derivation (I8, §19.9).

use pantheon::Code;
use porticus::view::{Layout, Row, View, ViewId};
use porticus::{Handled, Nav, Theme};
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Direction, Layout as Cut, Rect};
use serde_json::Value;
use tessera::Face;

/// The figures behind the dashboard, folded fresh every frame (§19.9).
pub struct Mosaic {
    root: std::path::PathBuf,
    /// The programme the figures are folded within — shared with every other view, read
    /// on the frame it draws and never copied out of (I1, §19.4, N2).
    scope: crate::scope::Scope,
}

impl Mosaic {
    #[must_use]
    pub fn of(root: &std::path::Path, scope: &crate::scope::Scope) -> Self {
        Self {
            root: root.to_path_buf(),
            scope: std::rc::Rc::clone(scope),
        }
    }

    /// The faces the mosaic paints, derived from one fold (§19.9). The GPA leads — it is
    /// the figure that names the lens (§19.4).
    ///
    /// The fold is scoped to the active programme: a GPA across two degrees is a figure
    /// nobody has, and `-H` was always the lever (§19.4) — the screen simply has a hand
    /// on it now.
    fn faces(&self) -> Vec<Face> {
        let scope = self.scope.borrow();
        let f = crate::fold::figures(&self.root, scope.home().map(Code::as_str));
        vec![
            face("GPA", number(&f["gpa"], ""), "grade point average"),
            face(
                "completed",
                number(&f["credits_completed"], " hp"),
                "credits earned",
            ),
            face(
                "in progress",
                number(&f["credits_in_progress"], " hp"),
                "credits open",
            ),
            face(
                "open courses",
                number(&f["open_courses"], ""),
                "enrolments open",
            ),
            face("study hours", number(&f["study_hours"], " h"), "logged"),
            face("next exam", next_exam(&f["next_exam"]), "ahead"),
            // Where the study life *is*, absolutely (§19.5) — a dash on all-the-studies,
            // where there is no single programme to count years from.
            face("period", period(&f["period"]), "now"),
        ]
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
        let faces = self.faces();
        // Three across, as many rows as it takes — a fixed shape, because a mosaic that
        // reflowed per figure count would read as a different screen each launch.
        let across = 3usize;
        let down = faces.len().div_ceil(across);
        let Ok(down_u16) = u16::try_from(down) else {
            return;
        };

        let rows = Cut::default()
            .direction(Direction::Vertical)
            .constraints(vec![Constraint::Ratio(1, u32::from(down_u16)); down])
            .split(area);

        for (index, face) in faces.iter().enumerate() {
            let cells = Cut::default()
                .direction(Direction::Horizontal)
                .constraints(vec![Constraint::Ratio(1, 3); across])
                .split(rows[index / across]);
            tessera::draw(
                face,
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
        // The scope prefixes this through `scope::Switch`, so the header reads
        // `disciplina · asd · a study life` (P§4).
        Some("a study life".into())
    }
}

fn face(caption: &str, value: String, gloss: &str) -> Face {
    Face {
        caption: caption.to_string(),
        value,
        gloss: Some(gloss.to_string()),
    }
}

/// A figure as its one line — `null` is the dash the fold emits for an absent core or an
/// empty numerator (§12, §19.9), never a zero.
fn number(value: &Value, unit: &str) -> String {
    match value {
        Value::Number(n) => format!("{n}{unit}"),
        _ => "—".to_string(),
    }
}

fn next_exam(value: &Value) -> String {
    match (value["date"].as_str(), value["course"].as_str()) {
        (Some(date), Some(course)) => format!("{date}   {course}"),
        _ => "—".to_string(),
    }
}

/// The absolute period, with the term beside it where the calendar names one (§19.5).
fn period(value: &Value) -> String {
    let Some(label) = value["label"].as_str() else {
        return "—".to_string();
    };
    match value["terms"].as_array().and_then(|t| t.first()) {
        Some(Value::String(term)) => format!("{label}   {term}"),
        _ => label.to_owned(),
    }
}
