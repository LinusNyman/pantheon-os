//! The loop and the frame (P§2, P§4, P§6, P§7).
//!
//! Under every overlay sits one screen Porticus owns and no instrument may restyle
//! (P-II) — three bands, the same in all twelve (I3), laid out from the terminal each
//! frame and stored nowhere (I1).

use pantheon::Code;
use ratatui::Frame;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::layout::{Constraint, Direction, Layout as Cut, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Widget, Wrap};

use crate::action::{Action, Invocation, RecordRef, Relayed, Target, Writer};
use crate::app::App;
use crate::keymap::{self, Chrome};
use crate::overlay::{Overlay, Pending, Prompt};
use crate::rail::Rail;
use crate::term::Screen;
use crate::theme::Theme;
use crate::view::{Grid, Handled, Layout, Nav, Row, View};

/// Which pane owns the arrows and the `/` scope (P§6).
#[derive(Clone, Copy, PartialEq, Eq)]
enum Focus {
    Rail,
    Content,
}

/// What the status line says. One precedence, suite-wide — **error > notice > hint**
/// — an error sticky until the next keystroke clears it, a hint only when the line is
/// otherwise idle, so the two never contend (P§4).
enum Status {
    Idle,
    Notice(String),
    Error(String),
}

/// Everything the loop holds. Note what is *not* here: no folded rows, no tree counts,
/// no rendered value of any kind. Those are recomputed each frame (I1).
struct State {
    rail: Rail,
    views: Vec<Box<dyn View>>,
    active: usize,
    focus: Focus,
    /// Index into the current view's rows. Cursor state, not a fold (P§6).
    row: usize,
    /// The filter `/` left behind on a row-view.
    filter: String,
    /// The record an `Enter`-drill pinned, and the view it drilled *from* — so `Esc`
    /// returns to the row you came from rather than to lineup `[0]` (P§3).
    ///
    /// An address, never a value (I1): the detail view re-folds it every frame.
    pinned: Option<(RecordRef, usize)>,
    overlays: Vec<Overlay>,
    status: Status,
    /// Cores this app relays to that are **not** on `PATH` — probed once at launch, so
    /// an action is dimmed before the key is pressed rather than failing when tried
    /// (§12, P§7).
    missing: Vec<String>,
    root: std::path::PathBuf,
    quit: bool,
}

/// Run an instrument (P§2). Sets up and tears down the terminal, runs the loop, and
/// owns the chrome and the overlays.
///
/// # Errors
/// If the lineup is empty or longer than nine (P§3), if the tree cannot be walked, or
/// if the terminal cannot be taken.
pub fn run(app: &mut impl App, root: &std::path::Path) -> anyhow::Result<()> {
    let views = app.lineup();
    check_lineup(&views)?;

    // Porticus knows which core a `Writer` targets, so it **probes `PATH` and dims the
    // action** before the key is pressed (P§7, §12). Only a lens needs this: a core's
    // own TUI writes in-process and cannot be missing from its own binary.
    let missing: Vec<String> = match app.writer() {
        Writer::Subprocess => app
            .relays_to()
            .into_iter()
            .filter(|short| !crate::action::on_path(short))
            .collect(),
        Writer::InProcess => Vec::new(),
    };

    let mut state = State {
        rail: Rail::new(root)?,
        views,
        active: 0,
        focus: Focus::Rail,
        row: 0,
        filter: String::new(),
        pinned: None,
        overlays: Vec::new(),
        status: Status::Idle,
        missing,
        root: root.to_path_buf(),
        quit: false,
    };

    let ident = app.ident();
    let theme = Theme::of(&ident);
    let mut screen = Screen::enter()?;

    while !state.quit {
        screen
            .terminal()
            .draw(|frame| draw(frame, app, &mut state, theme, &ident))?;
        // A frame is drawn only on an input event, never on a clock — there is no
        // daemon and no watcher (§18, P§6).
        let Event::Key(key) = event::read()? else {
            continue;
        };
        if key.kind != KeyEventKind::Press {
            continue;
        }
        handle(Some(&mut screen), app, &mut state, key)?;
    }
    Ok(())
}

/// The lineup rules (P§3), checked before a terminal is ever taken.
///
/// Called by [`run`] **and** by [`render_once`]: a rule enforced on only one of them
/// would let a test pass a lineup the real loop refuses, or — worse, and what happened
/// here first — let an invalid lineup panic on an index instead of erroring.
fn check_lineup(views: &[Box<dyn View>]) -> anyhow::Result<()> {
    anyhow::ensure!(
        !views.is_empty(),
        "a lineup needs at least one view — `[0]` is what launch opens (P§3)"
    );
    anyhow::ensure!(
        views.len() <= 9,
        "a lineup holds at most nine views — a tenth has no number key (P§3)"
    );
    anyhow::ensure!(
        views.iter().filter(|v| v.is_detail()).count() <= 1,
        "a lineup holds at most one detail view — an instrument has one primitive, so \
         one detail, which is what lets `Enter` route with no shape tag (P§3)"
    );
    let mut seen = std::collections::HashSet::new();
    for view in views {
        anyhow::ensure!(
            seen.insert(view.id()),
            "view ids are unique within a lineup — the switcher and Help key off them (P§3)"
        );
        for (key, _) in view.nav_keys() {
            anyhow::ensure!(
                !keymap::is_reserved(*key),
                "view `{}` declares Tier-3 key `{key}`, which Tier 1 or 2 already \
                 reserves — a reserved key stays reserved even where its action is \
                 not offered (P§5)",
                view.id()
            );
        }
    }
    Ok(())
}

/// The rail's two questions, answered by the instrument (P§6).
///
/// An adapter rather than two closures, because both answers come from the same `App`
/// and two closures would each want a mutable borrow of it.
struct Asking<'a, A>(&'a mut A);

impl<A: App> crate::rail::Presence for Asking<'_, A> {
    fn any(&mut self, node: &Code) -> bool {
        self.0.any_at(node)
    }

    fn count(&mut self, node: &Code) -> usize {
        self.0.count_at(node)
    }
}

// ── the frame: header · body · status (P§4) ──────────────────────────────────

fn draw(
    frame: &mut Frame,
    app: &mut impl App,
    state: &mut State,
    theme: Theme,
    ident: &crate::Ident,
) {
    let area = frame.area();
    // Below a hard floor Porticus draws a single dim line — the one place the chrome
    // collapses, to nothing but that notice (P§4).
    if area.height < 6 || area.width < 30 {
        frame.render_widget(
            Paragraph::new("terminal too small").style(theme.dim()),
            area,
        );
        return;
    }

    let bands = Cut::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .split(area);

    // **The body first, though it is drawn second.** A Full view names its own locator
    // in the header (P§4), and a locator is *derived* — a Timeline's range is its bars'
    // extent, which it only knows once it has folded them. Asking the header first
    // reports the fold before last, so the range lags a frame and is simply absent on
    // the first. The bands are separate rects, so the order of the two calls is free.
    draw_body(frame, app, state, theme, bands[1]);
    draw_header(frame, state, theme, ident, bands[0]);
    draw_status(frame, state, theme, bands[2]);

    if let Some(top) = state.overlays.last() {
        draw_overlay(frame, top, theme, ident, area);
    }
}

fn draw_header(frame: &mut Frame, state: &State, theme: Theme, ident: &crate::Ident, area: Rect) {
    // A Full view has no cursor, so it names its own locator where a Rail view shows
    // the path bar (P§4).
    let view = &state.views[state.active];
    let middle = match view.layout() {
        Layout::Full => view.locator().unwrap_or_else(|| view.id().to_owned()),
        Layout::Rail => state.rail.breadcrumb(),
    };

    let mut spans = vec![
        Span::styled(ident.tracked(), theme.name()),
        Span::styled("   ", theme.text()),
        Span::styled(middle, theme.text()),
        // `+` parked quietly at the path bar's tail, as the title hint (P§4).
        Span::styled("  +", theme.dim()),
        Span::styled("   ", theme.text()),
    ];
    for (index, view) in state.views.iter().enumerate() {
        let style = if index == state.active {
            theme.focus()
        } else {
            theme.dim()
        };
        spans.push(Span::styled(format!(" {} ", view.id()), style));
    }
    frame.render_widget(Paragraph::new(Line::from(spans)).style(theme.text()), area);
}

/// Below this width the rail stacks above the content rather than sitting beside it — a
/// narrow terminal has no room for two columns (P§6).
const STACK_BELOW: u16 = 60;
/// At or above this width the rail takes a fixed narrow column instead of a third of the
/// screen, so it does not sprawl on a wide terminal (P§6).
const WIDE_AT: u16 = 120;
/// The rail's capped width once the terminal is [`WIDE_AT`] or wider.
const WIDE_RAIL: u16 = 30;

/// How to split a Rail view's body for a given terminal width (P§6, §18 — derived from
/// the terminal, never a knob): stacked above the content when narrow, a fixed narrow
/// column when wide, a third of the width in between.
fn rail_cut(width: u16) -> (Direction, [Constraint; 2]) {
    if width < STACK_BELOW {
        (
            Direction::Vertical,
            [Constraint::Percentage(40), Constraint::Percentage(60)],
        )
    } else if width >= WIDE_AT {
        (
            Direction::Horizontal,
            [Constraint::Length(WIDE_RAIL), Constraint::Min(1)],
        )
    } else {
        (
            Direction::Horizontal,
            [Constraint::Percentage(34), Constraint::Percentage(66)],
        )
    }
}

fn draw_body(frame: &mut Frame, app: &mut impl App, state: &mut State, theme: Theme, area: Rect) {
    let Some(node) = state.rail.selected() else {
        // An empty tree is not an error (I7): the chrome stands, the content says so.
        draw_empty(frame, "no tree here — mint one with `pan new`", theme, area);
        return;
    };
    let layout = state.views[state.active].layout();

    let (rail_area, content) = match layout {
        // The rail's shape follows the terminal, not a knob (§18): stacked above the
        // content when there is no width for two columns, a fixed narrow column when
        // there is width to spare, and a third of the screen in between (P§6).
        Layout::Rail => {
            let (direction, constraints) = rail_cut(area.width);
            let cut = Cut::default()
                .direction(direction)
                .constraints(constraints)
                .split(area);
            (Some(cut[0]), cut[1])
        }
        Layout::Full => (None, area),
    };

    if let Some(rail_area) = rail_area {
        let focused = state.focus == Focus::Rail;
        // One borrow of the instrument, two questions (P§6).
        state.rail.draw(
            rail_area,
            frame.buffer_mut(),
            theme,
            focused,
            &mut Asking(app),
        );
    }

    // The view re-folds here, every frame it is drawn — derived-out, nothing cached
    // back (I1).
    let rows = state.views[state.active].rows(&node);
    if let Some(rows) = rows {
        // A dated Full row-view paints a month grid above its rows: the grid is the
        // locator, the rows beneath it are the focused day (P§3). It stays a row-view
        // throughout, so search, filter and scroll are unchanged (P§6).
        let content = match state.views[state.active].grid() {
            Some(grid) => draw_grid(frame, &grid, theme, content),
            None => content,
        };
        // `Some(vec![])` is a real empty result; `None` is a draw-view, which paints
        // its own empty (P§3).
        let rows = filtered(&rows, &state.filter);
        if rows.is_empty() {
            draw_empty(
                frame,
                state.views[state.active].empty_line(),
                theme,
                content,
            );
        } else {
            draw_rows(frame, &rows, state, theme, content);
        }
    } else {
        state.views[state.active].draw(&node, content, frame.buffer_mut(), theme);
    }
}

/// Draw the month grid and return what is left of the area for the day's rows (P§3).
///
/// Two lines per week: the day numbers, and a marker line under them — a cell says
/// *that* something falls there, never how many, which is a number the eye would have
/// to read (P§8). The focused cell carries the accent, so the day and its rows below
/// are visibly the same selection.
fn draw_grid(frame: &mut Frame, grid: &Grid, theme: Theme, area: Rect) -> Rect {
    const CELL: usize = 4;
    let columns = grid.columns.len().max(1);
    let weeks = grid.cells.len().div_ceil(columns);
    // Header + two lines a week + one blank before the rows.
    let wanted = u16::try_from(1 + weeks * 2 + 1).unwrap_or(u16::MAX);
    if area.height <= wanted {
        return area; // too short to be worth halving — let the rows have it
    }

    let mut header = String::with_capacity(grid.columns.len() * CELL);
    for column in &grid.columns {
        for _ in 0..CELL.saturating_sub(column.chars().count()) {
            header.push(' ');
        }
        header.push_str(column);
    }
    let mut lines = vec![Line::from(Span::styled(header, theme.dim()))];
    for week in 0..weeks {
        let mut days = Vec::new();
        let mut marks = Vec::new();
        for column in 0..columns {
            let index = week * columns + column;
            let (label, mark, style) = match grid.cells.get(index).and_then(Option::as_ref) {
                Some(cell) => (
                    cell.label.clone(),
                    if cell.items == 0 { " " } else { "·" },
                    if index == grid.focused {
                        theme.focus()
                    } else {
                        theme.text()
                    },
                ),
                // A pad cell: the weeks either side of the month are blank, not absent.
                None => (String::new(), " ", theme.dim()),
            };
            days.push(Span::styled(format!("{label:>CELL$}"), style));
            marks.push(Span::styled(format!("{mark:>CELL$}"), theme.dim()));
        }
        lines.push(Line::from(days));
        lines.push(Line::from(marks));
    }

    let head = Rect {
        height: wanted,
        ..area
    };
    frame.render_widget(Paragraph::new(lines).style(theme.text()), head);
    Rect {
        y: area.y + wanted,
        height: area.height - wanted,
        ..area
    }
}

fn draw_rows(frame: &mut Frame, rows: &[Row], state: &State, theme: Theme, area: Rect) {
    let height = area.height as usize;
    let cursor = state.row.min(rows.len().saturating_sub(1));
    let first = cursor.saturating_sub(height.saturating_sub(1));
    let focused = state.focus == Focus::Content;

    let lines: Vec<Line> = rows
        .iter()
        .enumerate()
        .skip(first)
        .take(height)
        .map(|(index, row)| {
            let style = if index == cursor && focused {
                theme.focus()
            } else if index == cursor {
                theme.text().bg(crate::theme::FOCUS_BG)
            } else {
                theme.text()
            };
            let when = row
                .when
                .as_ref()
                .map_or(String::new(), |w| format!("{w}  "));
            Line::from(Span::styled(format!("{when}{}", row.label), style))
        })
        .collect();

    frame.render_widget(Paragraph::new(lines).style(theme.text()), area);
}

/// Absence is calm, never an error (I7): one dim, centred line in the content, with
/// the chrome standing in full around it (P§4).
fn draw_empty(frame: &mut Frame, line: &str, theme: Theme, area: Rect) {
    let middle = Rect {
        y: area.y + area.height / 2,
        height: 1,
        ..area
    };
    frame.render_widget(
        Paragraph::new(line)
            .style(theme.dim())
            .alignment(ratatui::layout::Alignment::Center),
        middle,
    );
}

fn draw_status(frame: &mut Frame, state: &State, theme: Theme, area: Rect) {
    let (text, style) = match &state.status {
        // A hint shows only when the line is otherwise idle, so the two never contend
        // (P§4). It is derived from the view's declared Tier-3 keys each frame rather
        // than stored — the same rule as everything else on screen (I1).
        Status::Idle => (hint(state), theme.dim()),
        Status::Notice(text) => (text.clone(), theme.text()),
        Status::Error(text) => (text.clone(), theme.error()),
    };
    frame.render_widget(Paragraph::new(text).style(style), area);
}

/// The active view's Tier-3 keys, which is the only thing a view can put here — it
/// declared them precisely so Porticus could route them *and* list them (P§3, P§5).
fn hint(state: &State) -> String {
    state.views[state.active]
        .nav_keys()
        .iter()
        .map(|(key, what)| format!("{key} {what}"))
        .collect::<Vec<_>>()
        .join("   ")
}

fn draw_overlay(
    frame: &mut Frame,
    overlay: &Overlay,
    theme: Theme,
    ident: &crate::Ident,
    area: Rect,
) {
    let body: Vec<Line> = match overlay {
        Overlay::Title => vec![
            Line::from(Span::styled(ident.tracked(), theme.name())),
            Line::from(Span::styled(
                format!("{}  {}", ident.symbol, ident.tagline),
                theme.text(),
            )),
            Line::from(Span::styled(
                format!("crate {}  ·  format 1", env!("CARGO_PKG_VERSION")),
                theme.dim(),
            )),
        ],
        Overlay::Help => help_lines(theme),
        Overlay::Search { buffer } => {
            vec![Line::from(Span::styled(format!("/{buffer}"), theme.text()))]
        }
        Overlay::Line { buffer, .. } => {
            vec![Line::from(Span::styled(
                format!("> {buffer}"),
                theme.text(),
            ))]
        }
        Overlay::Confirm { pending, heavy, .. } => {
            let mut lines = Vec::new();
            // The overlay **names the full set and its count** (P§7). One item shows
            // its computed change; many show what will be run, because n changes would
            // not fit and a truncated list reads as the whole set.
            if let [only] = pending.as_slice() {
                lines.push(Line::from(Span::styled(
                    only.invocation.display(),
                    theme.name(),
                )));
                lines.push(Line::from(String::new()));
                for line in only.change.lines().take(area.height as usize / 2) {
                    lines.push(Line::from(Span::styled(line.to_owned(), theme.text())));
                }
                if let Some(token) = &only.token {
                    lines.push(Line::from(Span::styled(
                        format!("plan {token}"),
                        theme.dim(),
                    )));
                }
            } else {
                lines.push(Line::from(Span::styled(
                    format!("{} records", pending.len()),
                    theme.name(),
                )));
                lines.push(Line::from(String::new()));
                let room = (area.height as usize / 2).max(1);
                for item in pending.iter().take(room) {
                    lines.push(Line::from(Span::styled(
                        item.invocation.display(),
                        theme.text(),
                    )));
                }
                if pending.len() > room {
                    // Say what is not shown. A list that silently stopped would let the
                    // count and the visible rows disagree.
                    lines.push(Line::from(Span::styled(
                        format!("… and {} more", pending.len() - room),
                        theme.dim(),
                    )));
                }
            }
            lines.push(Line::from(Span::styled(
                match heavy {
                    // The one that demands a distinct, heavier keystroke: the count
                    // named and an explicit key, never a stray `y` (P§5).
                    Some(count) => {
                        format!("remove all {count} — press X again to commit, Esc to refuse")
                    }
                    None => "y / Enter to commit · n / Esc to refuse".into(),
                },
                theme.dim(),
            )));
            lines
        }
    };

    let box_area = centred(area, 72, u16::try_from(body.len() + 2).unwrap_or(8));
    frame.render_widget(Clear, box_area);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme.chrome())
        .title(overlay.heading())
        .style(theme.text());
    let inner = block.inner(box_area);
    block.render(box_area, frame.buffer_mut());
    frame.render_widget(
        Paragraph::new(body)
            .style(theme.text())
            .wrap(Wrap { trim: false }),
        inner,
    );
}

/// Help is generated from the live keymap (P§4), so it cannot drift from the bindings.
fn help_lines(theme: Theme) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    for (key, what) in keymap::CHROME_HELP {
        lines.push(Line::from(vec![
            Span::styled(format!("{key:<14}"), theme.name()),
            Span::styled((*what).to_string(), theme.text()),
        ]));
    }
    lines
}

fn centred(area: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(area.width.saturating_sub(2));
    let height = height.min(area.height.saturating_sub(2));
    Rect {
        x: area.x + (area.width.saturating_sub(width)) / 2,
        y: area.y + (area.height.saturating_sub(height)) / 2,
        width,
        height,
    }
}

/// Incremental match over a view's labels — the whole of what a view exposes for
/// search, which is why search is written once for all of them (P§6).
fn filtered(rows: &[Row], filter: &str) -> Vec<Row> {
    if filter.is_empty() {
        return rows.to_vec();
    }
    let needle = filter.to_lowercase();
    rows.iter()
        .filter(|row| row.label.to_lowercase().contains(&needle))
        .cloned()
        .collect()
}

// ── input (P§5) ──────────────────────────────────────────────────────────────

fn handle(
    mut screen: Option<&mut Screen>,
    app: &mut impl App,
    state: &mut State,
    key: KeyEvent,
) -> anyhow::Result<()> {
    // An error is sticky until the next keystroke clears it (P§4).
    if matches!(state.status, Status::Error(_)) {
        state.status = Status::Idle;
    }

    // **A chord is not its key.** Raw mode delivers `Ctrl-D` as `Char('d')` with a
    // CONTROL modifier (P§10 — `Ctrl-C` arrives the same way, as a key event rather
    // than a signal). Without this check every control chord fired the Tier-2 action
    // of its bare letter, so `Ctrl-D` silently marked a record done and `Ctrl-X`
    // removed one. The keymap is three closed tiers of *unmodified* keys (P§5); SHIFT
    // is the one modifier that carries meaning, because that is how `A`/`D`/`X` reach
    // us at all.
    if key
        .modifiers
        .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SUPER)
    {
        return Ok(());
    }

    if state.overlays.last().is_some() {
        return handle_overlay(screen.as_deref_mut(), app, state, key);
    }

    match key.code {
        // `Esc` unwinds in order: it pops an overlay (handled above), else un-pins an
        // `Enter`-drill back to the row it came from, and only with nothing to unwind
        // does it fall through at the base (P§4).
        KeyCode::Esc => undrill(state),
        KeyCode::Enter => {
            if state.focus == Focus::Rail {
                state.rail.descend();
                refresh(state)?;
            } else {
                // On a content row `Enter` **activates**: pin that row's record and
                // switch to the lineup's detail view (P§3, P§5).
                drill(state);
            }
        }
        KeyCode::Tab => {
            // Inert on a Full view — one pane, nothing to cycle (P§6).
            if state.views[state.active].layout() == Layout::Rail {
                state.focus = if state.focus == Focus::Rail {
                    Focus::Content
                } else {
                    Focus::Rail
                };
            }
        }
        KeyCode::Up => motion(state, Nav::Up)?,
        KeyCode::Down => motion(state, Nav::Down)?,
        KeyCode::Left => motion(state, Nav::Left)?,
        KeyCode::Right => motion(state, Nav::Right)?,
        KeyCode::Char(c) => return handle_char(screen, app, state, c),
        _ => {}
    }
    Ok(())
}

fn handle_char(
    screen: Option<&mut Screen>,
    app: &mut impl App,
    state: &mut State,
    c: char,
) -> anyhow::Result<()> {
    match c {
        'h' => return motion(state, Nav::Left),
        'j' => return motion(state, Nav::Down),
        'k' => return motion(state, Nav::Up),
        'l' => return motion(state, Nav::Right),
        _ => {}
    }

    if let Some(chrome) = keymap::chrome(c) {
        return handle_chrome(state, chrome);
    }
    if let Some(action) = keymap::action(c) {
        return begin(screen, app, state, action);
    }

    // Tier 3: only the keys this view declared, delivered as `Nav` (P§5).
    let view = &mut state.views[state.active];
    if view.nav_keys().iter().any(|(key, _)| *key == c) {
        view.navigate(Nav::Key(c));
        refresh(state)?;
    }
    Ok(())
}

fn handle_chrome(state: &mut State, chrome: Chrome) -> anyhow::Result<()> {
    match chrome {
        Chrome::Quit => state.quit = true,
        Chrome::Help => state.overlays.push(Overlay::Help),
        Chrome::Title => state.overlays.push(Overlay::Title),
        Chrome::Search => state.overlays.push(Overlay::Search {
            buffer: String::new(),
        }),
        Chrome::RecordsOnly => {
            if state.views[state.active].layout() == Layout::Rail {
                state.rail.toggle_records_only();
            }
        }
        Chrome::Switch(index) => {
            if index < state.views.len() {
                // Reached by its number key with nothing pinned, a detail view shows
                // the node's one record or its empty state — it never inherits a drill
                // (P§3). A switch keeps no history, so the pin goes.
                if state.pinned.take().is_some() {
                    if let Some(detail) = state.views.iter().position(|v| v.is_detail()) {
                        state.views[detail].pin(None);
                    }
                }
                state.active = index;
                state.row = 0;
                state.filter.clear();
                // Focus starts on the rail on a plain view-switch — you orient by node
                // first, so a fresh `/` finds a node (P§6).
                state.focus = Focus::Rail;
                refresh(state)?;
            }
        }
        Chrome::CyclePane | Chrome::Enter | Chrome::Escape => {}
    }
    Ok(())
}

/// `Enter` on a content row — **activate** (P§3, P§5).
///
/// Pins the focused row's `RecordRef` and switches to the lineup's detail view, which
/// folds that record each frame (I1). A lineup with no detail view leaves `Enter` inert
/// on a leaf and says so on the status line.
fn drill(state: &mut State) {
    let Some(detail) = state.views.iter().position(|v| v.is_detail()) else {
        state.status = Status::Notice("nothing to open here".into());
        return;
    };
    let Some(Target::Row(record)) = current_target(state) else {
        return;
    };
    state.pinned = Some((record.clone(), state.active));
    state.views[detail].pin(Some(record));
    state.active = detail;
    // An `Enter`-drill lands on the **content** you just opened — the one case where
    // focus does not start on the rail (P§6).
    state.focus = Focus::Content;
}

/// `Esc` at the base — un-pin an `Enter`-drill back to the row it came from (P§4).
///
/// A one-level drill-back, distinct from a number-key switch, which keeps no history.
fn undrill(state: &mut State) {
    let Some((_, from)) = state.pinned.take() else {
        return;
    };
    if let Some(detail) = state.views.iter().position(|v| v.is_detail()) {
        state.views[detail].pin(None);
    }
    state.active = from.min(state.views.len().saturating_sub(1));
    state.focus = Focus::Content;
}

fn motion(state: &mut State, nav: Nav) -> anyhow::Result<()> {
    let full = state.views[state.active].layout() == Layout::Full;
    if full || state.focus == Focus::Content {
        // A view gets first refusal on its own internal motion.
        if state.views[state.active].navigate(nav) == Handled::Yes {
            refresh(state)?;
            return Ok(());
        }
        match nav {
            Nav::Down => state.row = state.row.saturating_add(1),
            Nav::Up => state.row = state.row.saturating_sub(1),
            _ => {}
        }
        return Ok(());
    }
    match nav {
        Nav::Up => state.rail.up(),
        Nav::Down => state.rail.down(),
        Nav::Left => state.rail.left(),
        Nav::Right => state.rail.right(),
        Nav::Key(_) => {}
    }
    // Navigation is a refresh event (P§6).
    refresh(state)
}

/// Re-walk the tree. The view itself re-folds in `draw`, so this is the whole of what
/// a refresh has to do eagerly (P§6).
fn refresh(state: &mut State) -> anyhow::Result<()> {
    state.rail.refold(&state.root)?;
    Ok(())
}

// ── the write flow (P§7) ─────────────────────────────────────────────────────

/// Resolve the target, ask the app for the invocation, then run Porticus's own confirm
/// policy over it. The app never decides whether something confirms (P§5, P-II).
fn begin(
    mut screen: Option<&mut Screen>,
    app: &mut impl App,
    state: &mut State,
    action: Action,
) -> anyhow::Result<()> {
    if !state.views[state.active].actions().contains(&action) {
        // An unoffered action's key is dark — a no-op, not a rebind (P§5).
        return Ok(());
    }

    // A **scoped** action presupposes a row source, so it is a row-view's alone: a
    // draw-view returning `None` from `rows` has no set to enumerate (P§7).
    if matches!(action, Action::DoneAll | Action::RemoveAll) {
        return begin_scoped(screen.as_deref_mut(), app, state, action);
    }

    let Some(target) = target_for(state, action) else {
        state.status = Status::Notice("pick a row first".into());
        return Ok(());
    };

    // A view may declare that an action needs a line first (P§5) — `annotate` has
    // nothing to say until a `key=value` is typed.
    if let Some(label) = state.views[state.active].prompts_for(action) {
        state.overlays.push(Overlay::Line {
            prompt: Prompt::Field(action, target),
            label: label.to_owned(),
            buffer: String::new(),
        });
        return Ok(());
    }

    // `r` and `m` take a line prompt before there is anything to confirm (P§5).
    if action == Action::Rename {
        state.overlays.push(Overlay::Line {
            prompt: Prompt::Rename(target),
            label: "rename to".into(),
            buffer: String::new(),
        });
        return Ok(());
    }
    if action == Action::QuickAdd {
        state.overlays.push(Overlay::Line {
            prompt: Prompt::QuickAddCode,
            label: "add at code".into(),
            buffer: String::new(),
        });
        return Ok(());
    }

    let Some(invocation) = app.on_action(action, &target) else {
        // None → the action does not apply to this target (P§2).
        state.status = Status::Notice(format!("{} does not apply here", action.label()));
        return Ok(());
    };

    if let Some(short) = state.missing.iter().find(|s| **s == invocation.short) {
        state.status = Status::Notice(format!("{short} is not on PATH"));
        return Ok(());
    }

    commit_or_confirm(screen, app, state, action, invocation)
}

fn commit_or_confirm(
    mut screen: Option<&mut Screen>,
    app: &mut impl App,
    state: &mut State,
    action: Action,
    invocation: Invocation,
) -> anyhow::Result<()> {
    if action.confirms() {
        let pending = compute(
            screen.as_deref_mut(),
            app,
            &state.root.clone(),
            String::new(),
            invocation,
        )?;
        state.overlays.push(Overlay::Confirm {
            action,
            pending: vec![pending],
            // A focused-row confirm has no membership to re-scan: it names one record
            // and the per-item token already guards that one (P§7).
            snapshot: Vec::new(),
            heavy: None,
        });
        return Ok(());
    }
    let out = relay_one(screen, app, &state.root.clone(), &invocation, None)?;
    report(state, &invocation, &out);
    refresh(state)
}

/// `D` / `X` — every item at the current node, as *n* single relays under one
/// acknowledgement (P§7).
///
/// The set is the view's own rows, so the action reaches exactly what is on screen
/// (filter included) rather than a second, differently-derived list. Each item gets its
/// own `--dry-run`, and therefore its own plan token: an item that *shifted* underneath
/// is refused at commit rather than acted on stale (§7.3).
fn begin_scoped(
    mut screen: Option<&mut Screen>,
    app: &mut impl App,
    state: &mut State,
    action: Action,
) -> anyhow::Result<()> {
    let Some(single) = action.escalates_from() else {
        return Ok(());
    };
    let targets = row_targets(state);
    if targets.is_empty() {
        state.status = Status::Notice("nothing here to act on".into());
        return Ok(());
    }

    let root = state.root.clone();
    let mut pending = Vec::new();
    for (key, target) in targets {
        let Some(invocation) = app.on_action(single, &target) else {
            // One item the action does not apply to is not a reason to refuse the rest,
            // but it *is* a reason not to claim the set is what it looked like.
            state.status = Status::Notice(format!("{} does not apply here", action.label()));
            return Ok(());
        };
        pending.push(compute(screen.as_deref_mut(), app, &root, key, invocation)?);
    }

    let snapshot: Vec<String> = pending.iter().map(|p| p.key.clone()).collect();
    let count = pending.len();
    state.overlays.push(Overlay::Confirm {
        action,
        pending,
        snapshot,
        // Remove-all is irreversible and keeps no undo (§18), so it takes the heaviest
        // friction: the count named and an explicit, distinct key (P§5).
        heavy: action.is_heavy().then_some(count),
    });
    Ok(())
}

/// The current view's rows as targets, keyed for the membership re-scan.
///
/// `None` from `rows` means a draw-view, which has no set — the scoped action is simply
/// unavailable there (P§7).
fn row_targets(state: &mut State) -> Vec<(String, Target)> {
    let Some(node) = state.rail.selected() else {
        return Vec::new();
    };
    let filter = state.filter.clone();
    let Some(rows) = state.views[state.active].rows(&node) else {
        return Vec::new();
    };
    filtered(&rows, &filter)
        .into_iter()
        .map(|row| (row_key(&row.target), row.target))
        .collect()
}

/// A record's identity within the set. `Target::Node` has none — it names no record —
/// which is why only row targets can join a scoped set.
fn row_key(target: &Target) -> String {
    match target {
        Target::Row(record) => format!("{}:{}", record.home.as_str(), record.key),
        Target::Node { node, .. } => node.as_str().to_owned(),
    }
}

/// Run one item's `--dry-run` and keep what it computed (P§7).
fn compute(
    screen: Option<&mut Screen>,
    app: &mut impl App,
    root: &std::path::Path,
    key: String,
    invocation: Invocation,
) -> anyhow::Result<Pending> {
    let dry = execute(screen, app, &rooted(&invocation.dry_run(), root))?;
    let value = dry.json();
    let token = value
        .as_ref()
        .and_then(|v| v["token"].as_str().map(str::to_owned));
    let change = value
        .as_ref()
        .map_or_else(|| dry.stdout.clone(), |v| pretty(&v["change"]));
    Ok(Pending {
        key,
        invocation,
        token,
        change,
    })
}

/// Run one write.
fn relay_one(
    screen: Option<&mut Screen>,
    app: &mut impl App,
    root: &std::path::Path,
    invocation: &Invocation,
    token: Option<&str>,
) -> anyhow::Result<Relayed> {
    execute(screen, app, &rooted(&invocation.committed(token), root))
}

/// Put whatever came back on the status line (P§4).
fn report(state: &mut State, invocation: &Invocation, out: &Relayed) {
    if out.ok() {
        state.status = Status::Notice(invocation.display());
    } else {
        state.status = Status::Error(out.message());
    }
}

/// Run one invocation, suspending the screen around it.
///
/// The suspension is unconditional because an `edit` given no value inline is the
/// **editor form** (§7.3): the terminal belongs to the hand's own editor for the
/// length of the session, and Porticus cannot know from the invocation alone whether
/// this one will take it. Suspending around a child that does not need it costs a
/// redraw; not suspending around one that does corrupts the screen.
fn execute(
    screen: Option<&mut Screen>,
    app: &mut impl App,
    invocation: &Invocation,
) -> anyhow::Result<Relayed> {
    Ok(Screen::suspend_maybe(screen, || app.execute(invocation))??)
}

/// Every relay names the tree it is acting on (§7.3's `-C`).
///
/// Porticus adds this rather than trusting each instrument to, because the failure is
/// silent and severe: `run` is *given* a root, but a relay without `-C` resolves
/// `$PANTHEON_ROOT` instead — so a TUI opened with `-C /some/tree` would read one tree
/// and write to another. The env var is the caller's ambient state (§6.2); the root the
/// screen is drawing is the fact, and the relay must carry it.
///
/// `-C` is universal (§7.3), so this is safe for every instrument.
fn rooted(invocation: &Invocation, root: &std::path::Path) -> Invocation {
    let mut args = vec!["-C".to_string(), root.display().to_string()];
    args.extend(invocation.args.iter().cloned());
    Invocation {
        short: invocation.short.clone(),
        args,
    }
}

fn target_for(state: &mut State, action: Action) -> Option<Target> {
    // A scoped action presupposes a row source, so it is a row-view's alone (P§7).
    let node = state.rail.selected()?;
    match action {
        Action::Add => {
            // A dated Full view fills the `at` from its own cell, so `a` on a calendar
            // keeps the day you pointed at rather than defaulting to today (§7.3, P§7).
            let at = match state.views[state.active].target() {
                Some(Target::Node { at, .. }) => at,
                _ => None,
            };
            Some(Target::Node { node, at })
        }
        Action::DoneAll | Action::RemoveAll | Action::QuickAdd => {
            Some(Target::Node { node, at: None })
        }
        _ => current_target(state),
    }
}

/// The focused row's target — bound to the **record key captured at render**, never
/// the row's live index (P§5).
///
/// That is what makes a direct action safe: a refresh that reindexes the list (another
/// hand or a hook writing underneath — I8, §6.4) cannot land the keystroke on a
/// different record, because the key travelled with the row.
fn current_target(state: &mut State) -> Option<Target> {
    let node = state.rail.selected()?;
    let view = &mut state.views[state.active];
    let Some(rows) = view.rows(&node) else {
        // **`None` is a draw-view, not an empty one** (P§3). A draw/Full view carries
        // its own selection and names it as an address — a Timeline's focused bar. One
        // that names none is *about the selected node* — `pan`'s tree tab is the case —
        // so the node is the subject.
        return view.target().or(Some(Target::Node { node, at: None }));
    };
    // A **row-view's** focused row wins over any address the view also names. A dated
    // Full view names its *cell* so `a` can date the add (P§7, `target_for`), and that
    // cell must not stand in for the event the cursor is actually on. An empty row-view
    // returns `Some(vec![])` and falls through below with nothing, which is the honest
    // answer there: it has a set, and the set is empty.
    let rows = filtered(&rows, &state.filter);
    let index = state.row.min(rows.len().saturating_sub(1));
    rows.get(index).map(|row| row.target.clone())
}

fn pretty(value: &serde_json::Value) -> String {
    serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string())
}

// ── overlay input (P§4) ──────────────────────────────────────────────────────

fn handle_overlay(
    mut screen: Option<&mut Screen>,
    app: &mut impl App,
    state: &mut State,
    key: KeyEvent,
) -> anyhow::Result<()> {
    let text_entry = state.overlays.last().is_some_and(Overlay::is_text_entry);

    // A passive overlay (Title/Help) yields the keyboard: any key other than `Esc` and
    // `Enter` (which already dismiss) pops it and is **re-dispatched to the base**, so a
    // view-switch or a nav key both closes the overlay and acts, in one press (P§4).
    // Without this the overlay swallowed every such key at the `_ => Ok(())` below, and a
    // hand on Help had to `Esc` before it could go anywhere.
    if state.overlays.last().is_some_and(Overlay::is_passive)
        && !matches!(key.code, KeyCode::Esc | KeyCode::Enter)
    {
        state.overlays.pop();
        return handle(screen, app, state, key);
    }

    match key.code {
        KeyCode::Esc => {
            state.overlays.pop();
            return Ok(());
        }
        KeyCode::Enter => return submit(screen.as_deref_mut(), app, state),
        KeyCode::Backspace if text_entry => {
            if let Some(buffer) = state.overlays.last_mut().and_then(Overlay::buffer_mut) {
                buffer.pop();
                live_search(state);
            }
            return Ok(());
        }
        KeyCode::Char(c) => {
            if text_entry {
                // P-I's carve-out: every printable key is a literal character here.
                if let Some(buffer) = state.overlays.last_mut().and_then(Overlay::buffer_mut) {
                    buffer.push(c);
                    live_search(state);
                }
                return Ok(());
            }
            return handle_confirm_key(screen, app, state, c);
        }
        _ => {}
    }
    Ok(())
}

fn handle_confirm_key(
    mut screen: Option<&mut Screen>,
    app: &mut impl App,
    state: &mut State,
    c: char,
) -> anyhow::Result<()> {
    let heavy = matches!(
        state.overlays.last(),
        Some(Overlay::Confirm { heavy: Some(_), .. })
    );
    match c {
        'y' if !heavy => submit(screen.as_deref_mut(), app, state),
        'X' if heavy => submit(screen, app, state),
        'n' => {
            state.overlays.pop();
            Ok(())
        }
        // Over Help, Title, or Confirm, `q` is inert and `Esc` dismisses (P§4).
        _ => Ok(()),
    }
}

/// Search matches live (P§6) — and *whose* labels it matches follows focus.
fn live_search(state: &mut State) {
    let Some(Overlay::Search { buffer }) = state.overlays.last() else {
        return;
    };
    let needle = buffer.clone();
    if state.focus == Focus::Rail {
        state.rail.seek(&needle);
    } else {
        state.filter = needle;
        state.row = 0;
    }
}

fn submit(
    mut screen: Option<&mut Screen>,
    app: &mut impl App,
    state: &mut State,
) -> anyhow::Result<()> {
    let Some(overlay) = state.overlays.pop() else {
        return Ok(());
    };
    match overlay {
        Overlay::Search { buffer } => {
            if state.focus == Focus::Rail {
                state.rail.seek(&buffer);
            } else {
                state.filter = buffer;
            }
            Ok(())
        }
        Overlay::Confirm {
            action,
            pending,
            snapshot,
            ..
        } => commit(
            screen.as_deref_mut(),
            app,
            state,
            action,
            &pending,
            &snapshot,
        ),
        Overlay::Line { prompt, buffer, .. } => submit_line(screen, app, state, prompt, buffer),
        Overlay::Title | Overlay::Help => Ok(()),
    }
}

/// Commit an acknowledged set (P§7).
///
/// **The membership is re-scanned first.** A per-item plan token catches an item that
/// *shifted* underneath, but it cannot catch one that was *added* — a hook appending a
/// task while the overlay sat open would otherwise be swept into a "remove all" the
/// hand never saw. So the set is derived again and compared against the snapshot the
/// overlay was opened over; a difference re-prompts rather than commits.
fn commit(
    mut screen: Option<&mut Screen>,
    app: &mut impl App,
    state: &mut State,
    action: Action,
    pending: &[Pending],
    snapshot: &[String],
) -> anyhow::Result<()> {
    if !snapshot.is_empty() {
        let now: Vec<String> = row_targets(state).into_iter().map(|(key, _)| key).collect();
        if now != snapshot {
            state.status =
                Status::Error("the set changed underneath — look again before committing".into());
            return begin_scoped(screen.as_deref_mut(), app, state, action);
        }
    }

    // *n* single relays, each carrying its own plan token. The first failure stops the
    // run and says why: a bulk action that half-applied while reporting success would
    // be worse than one that stopped.
    let root = state.root.clone();
    for (done, item) in pending.iter().enumerate() {
        let out = relay_one(
            screen.as_deref_mut(),
            app,
            &root,
            &item.invocation,
            item.token.as_deref(),
        )?;
        if !out.ok() {
            state.status = Status::Error(format!(
                "{} — after {done} of {}",
                out.message(),
                pending.len()
            ));
            return refresh(state);
        }
    }
    state.status = match pending.len() {
        1 => Status::Notice(pending[0].invocation.display()),
        n => Status::Notice(format!("{} — {n} records", action.label())),
    };
    refresh(state)
}

fn submit_line(
    mut screen: Option<&mut Screen>,
    app: &mut impl App,
    state: &mut State,
    prompt: Prompt,
    buffer: String,
) -> anyhow::Result<()> {
    if buffer.trim().is_empty() {
        return Ok(());
    }
    match prompt {
        Prompt::Field(action, target) => {
            let Some(mut invocation) = app.on_action(action, &target) else {
                state.status = Status::Notice(format!("{} does not apply here", action.label()));
                return Ok(());
            };
            invocation.args.push(buffer);
            commit_or_confirm(screen.as_deref_mut(), app, state, action, invocation)
        }
        Prompt::Rename(target) => {
            let Some(invocation) = app.on_action(Action::Rename, &target) else {
                state.status = Status::Notice("rename does not apply here".into());
                return Ok(());
            };
            let mut invocation = invocation;
            invocation.args.push(buffer);
            commit_or_confirm(
                screen.as_deref_mut(),
                app,
                state,
                Action::Rename,
                invocation,
            )
        }
        Prompt::QuickAddCode => {
            state.overlays.push(Overlay::Line {
                prompt: Prompt::QuickAddContent(buffer),
                label: "add what".into(),
                buffer: String::new(),
            });
            Ok(())
        }
        Prompt::QuickAddContent(code) => {
            let Ok(node) = Code::parse(code.trim()) else {
                state.status = Status::Error(format!("no node with code {code}"));
                return Ok(());
            };
            let target = Target::Node { node, at: None };
            let Some(invocation) = app.on_action(Action::Add, &target) else {
                state.status = Status::Notice("add does not apply here".into());
                return Ok(());
            };
            let mut invocation = invocation;
            invocation.args.push(buffer);
            commit_or_confirm(screen.as_deref_mut(), app, state, Action::Add, invocation)
        }
        Prompt::PickHome => {
            // A Full view's `a` has no tree cursor to resolve a home from (P§7).
            let Ok(node) = Code::parse(buffer.trim()) else {
                state.status = Status::Error(format!("no node with code {buffer}"));
                return Ok(());
            };
            let target = Target::Node { node, at: None };
            let Some(invocation) = app.on_action(Action::Add, &target) else {
                state.status = Status::Notice("add does not apply here".into());
                return Ok(());
            };
            commit_or_confirm(screen, app, state, Action::Add, invocation)
        }
    }
}

// ── a seam for tests ─────────────────────────────────────────────────────────

/// Draw one frame into an off-screen buffer, with no terminal involved.
///
/// This exists so an instrument's screen can be **snapshotted like its JSON is**
/// (§7.2): the contract's own tests freeze what a core emits, and without this the
/// other half of I8 — what a *human* sees — would be the only surface in the suite
/// that nothing pins.
///
/// It runs the same [`draw`] the loop runs, so a snapshot that passes here is the
/// frame the loop would have put on the terminal.
///
/// # Errors
/// If the lineup is invalid or the tree cannot be walked.
pub fn render_once(
    app: &mut impl App,
    root: &std::path::Path,
    width: u16,
    height: u16,
) -> anyhow::Result<ratatui::buffer::Buffer> {
    let views = app.lineup();
    check_lineup(&views)?;
    let mut state = State {
        rail: Rail::new(root)?,
        views,
        active: 0,
        focus: Focus::Rail,
        row: 0,
        filter: String::new(),
        pinned: None,
        overlays: Vec::new(),
        status: Status::Idle,
        missing: Vec::new(),
        root: root.to_path_buf(),
        quit: false,
    };
    let ident = app.ident();
    let theme = Theme::of(&ident);
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(width, height))?;
    terminal.draw(|frame| draw(frame, app, &mut state, theme, &ident))?;
    Ok(terminal.backend().buffer().clone())
}

/// The buffer's visible text, one line per row, trailing blanks trimmed — what a
/// snapshot should hold, rather than the styled cells behind it.
#[must_use]
pub fn as_text(buffer: &ratatui::buffer::Buffer) -> String {
    let width = buffer.area.width;
    let mut out = String::new();
    for row in 0..buffer.area.height {
        let mut line = String::new();
        for column in 0..width {
            line.push_str(buffer[(column, row)].symbol());
        }
        out.push_str(line.trim_end());
        out.push('\n');
    }
    out
}

/// Drive an instrument with scripted keys, with no terminal involved.
///
/// This runs **the same [`handle`] the loop runs** and the same relay — the only
/// difference is that there is no screen to suspend around a child, which a headless
/// driver has nothing to do about anyway ([`Screen::suspend_maybe`]).
///
/// It exists because three separate defects in this crate passed every unit test and
/// were caught only by driving a real binary in a pty: a control chord firing its bare
/// key's action, a relay that killed the screen after committing, and an action whose
/// target never resolved. A pty proves the lifecycle but has no size, so it draws no
/// cells and cannot be asserted against. This closes that gap: keys in, final frame
/// out, writes really performed.
///
/// Returns the frame after the last key.
///
/// # Errors
/// If the lineup is invalid, the tree cannot be walked, or a relay fails to spawn.
pub fn drive(
    app: &mut impl App,
    root: &std::path::Path,
    keys: &[ratatui::crossterm::event::KeyEvent],
    width: u16,
    height: u16,
) -> anyhow::Result<String> {
    let views = app.lineup();
    check_lineup(&views)?;
    let mut state = State {
        rail: Rail::new(root)?,
        views,
        active: 0,
        focus: Focus::Rail,
        row: 0,
        filter: String::new(),
        pinned: None,
        overlays: Vec::new(),
        status: Status::Idle,
        missing: Vec::new(),
        root: root.to_path_buf(),
        quit: false,
    };
    let ident = app.ident();
    let theme = Theme::of(&ident);
    for key in keys {
        if state.quit {
            break;
        }
        handle(None, app, &mut state, *key)?;
    }
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(width, height))?;
    terminal.draw(|frame| draw(frame, app, &mut state, theme, &ident))?;
    Ok(as_text(terminal.backend().buffer()))
}

/// Spell a key script the way a hand would type it: `"2d"` is two keystrokes, and the
/// named ones are written `<enter>`, `<esc>`, `<tab>`, `<up>`, `<down>`, `<left>`,
/// `<right>`, `<bs>`.
///
/// # Panics
/// On an unknown `<name>`, which is a typo in a test rather than a runtime condition.
#[must_use]
pub fn keys(script: &str) -> Vec<ratatui::crossterm::event::KeyEvent> {
    use ratatui::crossterm::event::{KeyCode, KeyEvent};
    let mut out = Vec::new();
    let mut rest = script;
    while !rest.is_empty() {
        if let Some(end) = rest.strip_prefix('<').and_then(|r| r.find('>')) {
            let name = &rest[1..=end];
            let code = match name {
                "enter" => KeyCode::Enter,
                "esc" => KeyCode::Esc,
                "tab" => KeyCode::Tab,
                "up" => KeyCode::Up,
                "down" => KeyCode::Down,
                "left" => KeyCode::Left,
                "right" => KeyCode::Right,
                "bs" => KeyCode::Backspace,
                other => panic!("unknown key `<{other}>` in a key script"),
            };
            out.push(KeyEvent::from(code));
            rest = &rest[end + 2..];
            continue;
        }
        let mut chars = rest.chars();
        let c = chars.next().expect("rest is non-empty");
        out.push(KeyEvent::from(KeyCode::Char(c)));
        rest = chars.as_str();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{rail_cut, Constraint, Direction, STACK_BELOW, WIDE_AT, WIDE_RAIL};

    #[test]
    fn the_rail_cut_follows_the_terminal_width() {
        // Narrow: the rail stacks above the content — no room for two columns.
        assert_eq!(rail_cut(50).0, Direction::Vertical, "a narrow width stacks");
        // The mid band keeps a third of the width, side by side. Every width the frame
        // tests render at (60–90) lands here, so their layout is unchanged.
        assert_eq!(
            rail_cut(STACK_BELOW).0,
            Direction::Horizontal,
            "the stack floor is exclusive — 60 is not narrow"
        );
        assert!(matches!(rail_cut(90).1[0], Constraint::Percentage(34)));
        // Wide: a fixed narrow rail column rather than a sprawling third.
        assert_eq!(rail_cut(WIDE_AT).0, Direction::Horizontal);
        assert!(matches!(rail_cut(WIDE_AT).1[0], Constraint::Length(WIDE_RAIL)));
        assert!(matches!(rail_cut(200).1[0], Constraint::Length(WIDE_RAIL)));
    }
}
