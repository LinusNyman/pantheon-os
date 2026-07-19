//! `Insights` (draw · Full) — typed chart panels over the app's **own** readings
//! (P§3).
//!
//! The app hands up `Vec<Panel>`; Porticus draws them and owns the frame, so a graph
//! is written once for twelve rather than twelve times (I3). The mix differs per
//! instrument — a pie here, a trend there — while the rendering is one implementation.
//!
//! **Every panel draws over figures the app folds from its own readings.** A core
//! reads no other core (I5), so an insight needing another core's data is not the
//! core's at all but a **lens's**, folded across `PATH` into a mosaic tile (§11.2).
//! *Where you've been* — an Annales log referencing `mappa:<place>` — is therefore
//! Annales' insight or a lens's, never Mappa's, because aboutness homes the fact where
//! it lives (I3, §2). Mappa charts its own places; Fasti its own span durations.

use pantheon::Code;
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Direction, Layout as Cut, Rect};
use ratatui::style::Style;
use ratatui::symbols::Marker;
use ratatui::text::{Line, Span};
use ratatui::widgets::canvas::{Canvas, Context, Rectangle};
// `Chart` is the spec's name for the vocabulary below (P§3), so `ratatui`'s widget of
// that name is aliased rather than the other way round: the spec wins the name.
use ratatui::widgets::{
    Axis, Bar, BarChart, BarGroup, Block, Borders, Chart as LineChart, Dataset, GraphType,
    Paragraph, Sparkline, Widget,
};

use crate::view::{Layout, Row, View, ViewId};
use crate::{Nav, Theme};

/// A caption — a field name, a chart category, a stat title.
pub type Label = String;
/// A dated trend: what `Trend` and `Spark` read. The date is Pantheon's reading key
/// (§6.1) — referenced, never coined here (I1).
pub type Series = Vec<(String, f64)>;

/// One titled chart.
pub struct Panel {
    pub title: String,
    pub chart: Chart,
}

/// The chart vocabulary (P§3).
pub enum Chart {
    /// A line over time — weight, net worth, throughput.
    Trend(Series),
    /// Counts by category — demographics, tasks by concern.
    Bars(Vec<(Label, f64)>),
    /// Proportions — documents by type / tag.
    Pie(Vec<(Label, f64)>),
    /// A compact inline trend.
    Spark(Series),
    /// A contribution grid — logging consistency.
    Heatmap(Vec<(String, f64)>),
    /// A single numeric — open vs done, a streak count.
    Stat(Label, String),
}

/// The app's panels, folded fresh each frame.
pub struct Insights<F> {
    fold: F,
}

impl<F> Insights<F>
where
    F: FnMut() -> Vec<Panel>,
{
    /// Capture the instrument's fold (P§3). Called every frame — a panel held from
    /// construction would be a stored present (I1).
    pub fn of(fold: F) -> Self {
        Self { fold }
    }
}

impl<F> View for Insights<F>
where
    F: FnMut() -> Vec<Panel>,
{
    fn id(&self) -> ViewId {
        "insights"
    }

    fn layout(&self) -> Layout {
        Layout::Full
    }

    fn rows(&mut self, _node: &Code) -> Option<Vec<Row>> {
        // A draw-view: it paints itself, and so opts out of `/` search by construction
        // (P§3).
        None
    }

    fn navigate(&mut self, _nav: Nav) -> crate::Handled {
        crate::Handled::No
    }

    fn locator(&self) -> Option<String> {
        Some("insights".into())
    }

    fn draw(&mut self, _node: &Code, area: Rect, buf: &mut Buffer, theme: Theme) {
        let panels = (self.fold)();
        if panels.is_empty() {
            centred(buf, area, "no data yet", theme.dim());
            return;
        }
        // Two across, as many rows as it takes — the same fixed shape a mosaic keeps,
        // and for the same reason: a layout that reflowed per panel count would read as
        // a different screen each time.
        let across = 2usize;
        let down = panels.len().div_ceil(across);
        let Ok(down_u32) = u32::try_from(down) else {
            return;
        };
        let rows = Cut::default()
            .direction(Direction::Vertical)
            .constraints(vec![Constraint::Ratio(1, down_u32); down])
            .split(area);

        for (index, panel) in panels.iter().enumerate() {
            let cells = Cut::default()
                .direction(Direction::Horizontal)
                .constraints(vec![Constraint::Ratio(1, 2); across])
                .split(rows[index / across]);
            draw_panel(panel, cells[index % across], buf, theme);
        }
    }
}

fn draw_panel(panel: &Panel, area: Rect, buf: &mut Buffer, theme: Theme) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme.chrome())
        .title(Span::styled(panel.title.clone(), theme.dim()))
        .style(theme.text());
    let inner = block.inner(area);
    block.render(area, buf);
    if inner.width == 0 || inner.height == 0 {
        return;
    }

    match &panel.chart {
        // Absence is calm per panel too (I7, P§4).
        Chart::Trend(s) | Chart::Spark(s) if s.is_empty() => {
            centred(buf, inner, "no data yet", theme.dim());
        }
        Chart::Bars(v) | Chart::Pie(v) if v.is_empty() => {
            centred(buf, inner, "no data yet", theme.dim());
        }
        Chart::Heatmap(v) if v.is_empty() => centred(buf, inner, "no data yet", theme.dim()),

        Chart::Stat(label, value) => draw_stat(label, value, inner, buf, theme),
        Chart::Spark(series) => {
            let values: Vec<u64> = series.iter().map(|(_, v)| magnitude(*v)).collect();
            Sparkline::default()
                .data(values)
                .style(Style::default().fg(theme.accent))
                .render(inner, buf);
        }
        Chart::Bars(values) => draw_bars(values, inner, buf, theme),
        Chart::Trend(series) => draw_trend(series, inner, buf, theme),
        // `ratatui` ships no pie, so Porticus paints these on its `Canvas` (P§3).
        Chart::Pie(values) => draw_pie(values, inner, buf, theme),
        Chart::Heatmap(values) => draw_heatmap(values, inner, buf, theme),
    }
}

/// A chart magnitude as `ratatui`'s bar widgets want it.
///
/// The cast is deliberate and bounded here rather than allowed across the module: a
/// negative reading has no bar to draw and a non-finite one is a fold that went wrong,
/// so both floor at zero, and a value past `u64` is a data error rather than a render
/// one — it saturates instead of wrapping.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss
)]
fn magnitude(value: f64) -> u64 {
    if !value.is_finite() || value <= 0.0 {
        return 0;
    }
    if value >= u64::MAX as f64 {
        return u64::MAX;
    }
    value.round() as u64
}

/// A series index as an x-coordinate.
///
/// Precision is lost past 2^53 points, which is more readings than a personal corpus
/// will ever hold (§5.0) — and a chart that wide has no pixels to spend on them
/// anyway.
#[allow(clippy::cast_precision_loss)]
fn position(index: usize) -> f64 {
    index as f64
}

fn draw_stat(label: &str, value: &str, area: Rect, buf: &mut Buffer, theme: Theme) {
    Paragraph::new(vec![
        Line::from(Span::styled(value.to_owned(), theme.name())),
        Line::from(Span::styled(label.to_owned(), theme.dim())),
    ])
    .style(theme.text())
    .render(area, buf);
}

fn draw_bars(values: &[(Label, f64)], area: Rect, buf: &mut Buffer, theme: Theme) {
    let bars: Vec<Bar> = values
        .iter()
        .map(|(label, value)| {
            Bar::default()
                .label(Line::from(label.clone()))
                .value(magnitude(*value))
                .style(Style::default().fg(theme.accent))
        })
        .collect();
    BarChart::default()
        .data(BarGroup::default().bars(&bars))
        .bar_width(bar_width(area.width, values.len()))
        .bar_gap(1)
        .style(theme.text())
        .render(area, buf);
}

/// Wide enough to read, narrow enough that every bar fits.
fn bar_width(width: u16, count: usize) -> u16 {
    let Ok(count) = u16::try_from(count) else {
        return 1;
    };
    if count == 0 {
        return 1;
    }
    (width / count).saturating_sub(1).clamp(1, 8)
}

fn draw_trend(series: &[(String, f64)], area: Rect, buf: &mut Buffer, theme: Theme) {
    let points: Vec<(f64, f64)> = series
        .iter()
        .enumerate()
        .map(|(i, (_, v))| (position(i), *v))
        .collect();
    let (low, high) = bounds(series);
    let last = position(points.len().saturating_sub(1));
    let datasets = vec![
        Dataset::default()
            .marker(Marker::Braille)
            .graph_type(GraphType::Line)
            .style(Style::default().fg(theme.accent))
            .data(&points),
    ];
    let first_label = series.first().map(|(d, _)| d.clone()).unwrap_or_default();
    let last_label = series.last().map(|(d, _)| d.clone()).unwrap_or_default();
    LineChart::new(datasets)
        .x_axis(
            Axis::default()
                .bounds([0.0, last.max(1.0)])
                .labels([first_label, last_label])
                .style(theme.chrome()),
        )
        .y_axis(
            Axis::default()
                .bounds([low, high])
                .labels([format!("{low:.1}"), format!("{high:.1}")])
                .style(theme.chrome()),
        )
        .style(theme.text())
        .render(area, buf);
}

/// A little headroom, and never a zero-height axis — a flat series would otherwise
/// collapse to a line the chart cannot scale.
fn bounds(series: &[(String, f64)]) -> (f64, f64) {
    let mut low = f64::INFINITY;
    let mut high = f64::NEG_INFINITY;
    for (_, value) in series {
        low = low.min(*value);
        high = high.max(*value);
    }
    if !low.is_finite() || !high.is_finite() {
        return (0.0, 1.0);
    }
    if (high - low).abs() < f64::EPSILON {
        return (low - 1.0, high + 1.0);
    }
    let pad = (high - low) * 0.1;
    (low - pad, high + pad)
}

/// A pie, painted on the canvas `ratatui` gives us instead of a pie widget (P§3).
///
/// Drawn as a ring of filled wedges rather than an outline: at terminal resolution an
/// outlined pie reads as noise, and the proportion is the whole of what a pie says.
fn draw_pie(values: &[(Label, f64)], area: Rect, buf: &mut Buffer, theme: Theme) {
    let total: f64 = values.iter().map(|(_, v)| v.max(0.0)).sum();
    if total <= 0.0 {
        centred(buf, area, "no data yet", theme.dim());
        return;
    }
    let slices: Vec<(f64, f64)> = values
        .iter()
        .scan(0.0, |from, (_, value)| {
            let start = *from;
            *from += value.max(0.0) / total * std::f64::consts::TAU;
            Some((start, *from))
        })
        .collect();

    Canvas::default()
        .x_bounds([-1.0, 1.0])
        .y_bounds([-1.0, 1.0])
        .paint(|ctx: &mut Context| {
            for (index, (from, to)) in slices.iter().enumerate() {
                let colour = theme.sphere(index);
                let mut angle = *from;
                while angle < *to {
                    // Fill by drawing radii — the canvas has no arc fill, and a dense
                    // enough fan is indistinguishable from one at this resolution.
                    let mut radius = 0.0;
                    while radius < 1.0 {
                        ctx.draw(&Rectangle {
                            x: radius * angle.cos(),
                            y: radius * angle.sin(),
                            width: 0.0,
                            height: 0.0,
                            color: colour,
                        });
                        radius += 0.05;
                    }
                    angle += 0.02;
                }
            }
        })
        .render(area, buf);
}

/// A contribution grid — one cell per dated reading, brightest where the value is
/// highest (P§3).
fn draw_heatmap(values: &[(String, f64)], area: Rect, buf: &mut Buffer, theme: Theme) {
    let high = values.iter().map(|(_, v)| *v).fold(0.0_f64, f64::max);
    let width = area.width.max(1) as usize;
    let mut lines: Vec<Line> = Vec::new();
    for chunk in values.chunks(width) {
        let spans: Vec<Span> = chunk
            .iter()
            .map(|(_, value)| {
                let level = if high > 0.0 { value / high } else { 0.0 };
                let glyph = match level {
                    l if l <= 0.0 => '·',
                    l if l < 0.34 => '░',
                    l if l < 0.67 => '▒',
                    _ => '▓',
                };
                Span::styled(
                    glyph.to_string(),
                    if level <= 0.0 {
                        theme.dim()
                    } else {
                        Style::default().fg(theme.accent)
                    },
                )
            })
            .collect();
        lines.push(Line::from(spans));
    }
    Paragraph::new(lines).style(theme.text()).render(area, buf);
}

fn centred(buf: &mut Buffer, area: Rect, text: &str, style: Style) {
    let middle = Rect {
        y: area.y + area.height / 2,
        height: 1,
        ..area
    };
    Paragraph::new(text.to_owned())
        .style(style)
        .alignment(ratatui::layout::Alignment::Center)
        .render(middle, buf);
}
