//! Fasti's TUI — a thin Porticus provider (P§2). Rides the `tui` feature (§14).
//!
//! The lineup is where Fasti's two shapes show as two surfaces: a `TreeFile` of what is
//! placed at a node, a `Calendar` of every occurrence on a month grid — **the calendar
//! §8.4 calls derived, derived at render and stored nowhere** (I1) — a `Timeline` of
//! every span as a bar, and an `EntityCard` whose strip is the one thing a span
//! natively is, a period.
//!
//! This is P§3's own lineup for Fasti, plus the `EntityCard`: a lineup carries at most
//! one detail view, and without it `Enter` on a row would be inert.

use std::ffi::OsString;

use clap::Parser;
use fasti::{Fasti, FastiRecord};
use pantheon::{Code, EntityRef, Response, SeriesRef, Store};
use porticus::action::{Invocation, Relayed};
use porticus::view::Row;
use porticus::views::{
    Calendar, Card, CardSpan, Chart, Chip, EntityCard, Insights, Panel, Timeline, TreeFile,
};
use porticus::{Action, App, Ident, RecordRef, Target, View, Writer};

use crate::{Cli, with_default_verb};

/// Open Fasti's screen.
///
/// # Errors
/// If the tree cannot be walked or the terminal cannot be taken.
pub fn open(root: &std::path::Path) -> anyhow::Result<()> {
    let mut app = FastiApp {
        root: root.to_path_buf(),
    };
    porticus::run(&mut app, root)
}

struct FastiApp {
    root: std::path::PathBuf,
}

impl App for FastiApp {
    fn ident(&self) -> Ident {
        Ident {
            name: "fasti",
            short: "fas",
            tagline: "actio · placement",
            symbol: '☾',
            accent: porticus::ident::accent::SOL_GOLD,
        }
    }

    fn lineup(&mut self) -> Vec<Box<dyn View>> {
        let for_rows = self.root.clone();
        let for_calendar = self.root.clone();
        let for_timeline = self.root.clone();
        let for_card = self.root.clone();
        let for_insights = self.root.clone();
        vec![
            Box::new(
                TreeFile::of(move |node: &Code| rows_at(&for_rows, node))
                    .offering(&[
                        Action::Add,
                        Action::Edit,
                        Action::Remove,
                        Action::Rename,
                        Action::Move,
                        Action::QuickAdd,
                    ])
                    .empty("nothing placed here"),
            ),
            // The calendar §8.4 calls derived: every occurrence in the tree, on a month
            // grid, folded fresh each frame and stored nowhere (I1). Each row carries
            // its own home, so it spans nodes without a cursor (P§3), and `a` on a cell
            // dates the add from the day you pointed at (§7.3, P§7).
            Box::new(
                Calendar::of(move || agenda(&for_calendar))
                    .offering(&[Action::Add, Action::Edit, Action::Remove])
                    .empty("nothing this day"),
            ),
            // Spans as bars. A period is exactly what a Timeline draws, and each bar
            // carries its own home, so this is cross-node like the calendar (P§3).
            Box::new(
                Timeline::of(move || bars(&for_timeline))
                    .offering(&[Action::Edit, Action::Rename])
                    .empty("no periods yet"),
            ),
            // The span card — a period drawn as one (P§3).
            Box::new(
                EntityCard::of(move |node: &Code, pinned: Option<&RecordRef>| {
                    card_at(&for_card, node, pinned)
                })
                .offering(&[Action::Edit, Action::Rename]),
            ),
            Box::new(Insights::of(move || panels(&for_insights))),
        ]
    }

    fn count_at(&mut self, node: &Code) -> usize {
        spans(&self.root, Some(node)).len() + occurrences(&self.root, Some(node)).len()
    }

    fn writer(&self) -> Writer {
        Writer::InProcess
    }

    fn execute(&mut self, invocation: &Invocation) -> std::io::Result<Relayed> {
        Ok(in_process(invocation))
    }

    fn on_action(&mut self, action: Action, target: &Target) -> Option<Invocation> {
        match (action, target) {
            (Action::Add | Action::QuickAdd, Target::Node { node, .. }) => {
                Some(Invocation::new("fas", ["add", "-H", node.as_str()]))
            }
            (Action::Edit, Target::Row(RecordRef { home, key })) => {
                Some(Invocation::new("fas", ["edit", "-H", home.as_str(), key]))
            }
            (Action::Remove, Target::Row(RecordRef { home, key })) => {
                Some(Invocation::new("fas", ["rm", "-H", home.as_str(), key]))
            }
            (Action::Rename, Target::Row(RecordRef { home, key })) => {
                Some(Invocation::new("fas", ["rename", "-H", home.as_str(), key]))
            }
            (Action::Move, Target::Row(RecordRef { home, key })) => Some(Invocation::new(
                "fas",
                ["move", "-H", home.as_str(), key, "--to"],
            )),
            // A span is a period and an occurrence a point; neither is a thing you mark
            // off, so `d`/`D` stay dark — the key is a no-op rather than repurposed
            // (P§5). Closing a span is `edit --to`, which is a correction, not a toggle.
            _ => None,
        }
    }
}

/// Run the invocation in-process, through the very code the CLI runs (P§7).
fn in_process(invocation: &Invocation) -> Relayed {
    let argv =
        std::iter::once(OsString::from("fas")).chain(invocation.args.iter().map(OsString::from));
    let cli = match Cli::try_parse_from(with_default_verb(argv)) {
        Ok(cli) => cli,
        Err(e) => {
            return Relayed {
                code: 2,
                stdout: String::new(),
                stderr: e.to_string(),
            };
        }
    };
    match crate::run(&cli, true) {
        Ok(Response::Json(value)) => Relayed {
            code: 0,
            stdout: value.to_string(),
            stderr: String::new(),
        },
        Ok(Response::JsonExit(value, code)) => Relayed {
            code: i32::from(code),
            stdout: value.to_string(),
            stderr: String::new(),
        },
        Ok(Response::Raw(text)) => Relayed {
            code: 0,
            stdout: text,
            stderr: String::new(),
        },
        Err(e) => Relayed {
            code: i32::from(e.exit_code().as_u8()),
            stdout: String::new(),
            stderr: e.to_error_json().to_string(),
        },
    }
}

fn store(root: &std::path::Path) -> Store<Fasti> {
    Store::<Fasti>::new(root.to_path_buf())
}

/// Fasti's own spans — a core folds its own records and reaches for no other (I5).
///
/// A file whose bytes disagree with its token is dropped rather than drawn wrong: a
/// screen is not the place to report a parse failure, and the CLI verb over the same
/// file says so plainly (exit `3`, §13).
fn spans(root: &std::path::Path, at: Option<&Code>) -> Vec<(EntityRef, fasti::Span)> {
    store(root)
        .fold_entities(at, Some(Fasti::SPAN))
        .unwrap_or_default()
        .into_iter()
        .filter_map(|(eref, entity)| match entity.data {
            FastiRecord::Span(span) => Some((eref, span)),
            FastiRecord::Event(_) => None,
        })
        .collect()
}

/// Every occurrence in scope — **every line, not each series' present**.
///
/// The calendar is a fold over events by span and date (§8.4), so it wants the
/// occurrences themselves; a present-fold would draw one row per timeline and call it a
/// calendar. This is the same reason `fas list --unspanned` widens past the present.
fn occurrences(
    root: &std::path::Path,
    at: Option<&Code>,
) -> Vec<(SeriesRef, pantheon::envelope::Line<FastiRecord>)> {
    let store = store(root);
    let mut out = Vec::new();
    for sref in store
        .find_series(at, Some(Fasti::EVENT), None)
        .unwrap_or_default()
    {
        for line in store.read_series(&sref).unwrap_or_default() {
            out.push((sref.clone(), line));
        }
    }
    out
}

fn event_label(sref: &SeriesRef, line: &pantheon::envelope::Line<FastiRecord>) -> String {
    let FastiRecord::Event(event) = &line.data else {
        return sref.label().to_string();
    };
    let what = event.values.join(" ");
    let until = event
        .until
        .as_ref()
        .map(|u| format!("–{u}"))
        .unwrap_or_default();
    format!("{}{until}   {what}", sref.label())
}

fn rows_at(root: &std::path::Path, node: &Code) -> Vec<Row> {
    let mut rows: Vec<Row> = spans(root, Some(node))
        .into_iter()
        .map(|(eref, span)| Row {
            label: format!(
                "{}   {}–{}",
                eref.slug,
                span.from,
                span.to.as_deref().unwrap_or("")
            ),
            target: Target::Row(RecordRef {
                home: eref.home,
                key: eref.slug,
            }),
            // A span is a period, not a dated item: it has no one date to sort by, and
            // claiming its `from` would file an open span among the day's occurrences.
            when: None,
        })
        .collect();
    rows.extend(
        occurrences(root, Some(node))
            .into_iter()
            .map(|(sref, line)| {
                let key = line.key.as_str().to_owned();
                Row {
                    label: event_label(&sref, &line),
                    target: Target::Row(RecordRef {
                        home: sref.home.clone(),
                        key: key.clone(),
                    }),
                    when: Some(key),
                }
            }),
    );
    rows
}

/// The whole tree's spans as bars, for the Timeline (P§3).
///
/// Each bar carries its own home, so the view is cross-node without a cursor — a career
/// at one node and a residence at another sit on one range (P§7).
fn bars(root: &std::path::Path) -> Vec<CardSpan> {
    let mut bars: Vec<CardSpan> = spans(root, None)
        .into_iter()
        .map(|(eref, span)| CardSpan {
            label: eref.slug.clone(),
            from: span.from,
            to: span.to,
            home: RecordRef {
                home: eref.home,
                key: eref.slug,
            },
        })
        .collect();
    // Earliest first, then by name — a stable order, so a refold does not shuffle bars
    // under the cursor.
    bars.sort_by(|a, b| a.from.cmp(&b.from).then_with(|| a.label.cmp(&b.label)));
    bars
}

/// The whole tree's occurrences, for the Calendar (P§3).
fn agenda(root: &std::path::Path) -> Vec<Row> {
    occurrences(root, None)
        .into_iter()
        .map(|(sref, line)| {
            let key = line.key.as_str().to_owned();
            Row {
                label: event_label(&sref, &line),
                target: Target::Row(RecordRef {
                    home: sref.home.clone(),
                    key: key.clone(),
                }),
                when: Some(key),
            }
        })
        .collect()
}

/// The pinned span as a card, else the node's one span.
///
/// An `Enter`-drill pins a row and this resolves **that** record (P§3). With nothing
/// pinned it falls back to the node's single span — the entity-as-node case (§5.1) — and
/// where a node holds several it shows the empty "pick a record" state rather than
/// guessing among them: a detail view never picks for you.
fn card_at(root: &std::path::Path, node: &Code, pinned: Option<&RecordRef>) -> Option<Card> {
    let (eref, span, refs) = {
        let store = store(root);
        let at = pinned.map_or(node.clone(), |record| record.home.clone());
        let mut found = store.fold_entities(Some(&at), Some(Fasti::SPAN)).ok()?;
        let index = match pinned {
            Some(record) => found.iter().position(|(eref, _)| eref.slug == record.key)?,
            None if found.len() == 1 => 0,
            None => return None,
        };
        let (eref, entity) = found.remove(index);
        let FastiRecord::Span(span) = entity.data else {
            return None;
        };
        (eref, span, entity.refs)
    };
    let mut fields = vec![("from".into(), span.from.clone())];
    fields.push((
        "to".into(),
        // An absent `to` is an open span, which is a state and not a missing value
        // (§8.4) — so it is named rather than left blank.
        span.to.clone().unwrap_or_else(|| "open".to_string()),
    ));
    if let Some(note) = &span.note {
        fields.push(("note".into(), note.clone()));
    }
    Some(Card {
        title: eref.slug.clone(),
        fields,
        // Ref chips are display-only in v1 (P§3): legible, not a link.
        chips: refs
            .iter()
            .map(|r| Chip {
                label: r.to_token(),
                reference: r.to_token(),
            })
            .collect(),
        // The one view-model a span fills natively: `Card`'s strip and `Timeline`'s bar
        // are one type, so the period is drawn once (P§3).
        strip: vec![CardSpan {
            label: eref.slug.clone(),
            from: span.from,
            to: span.to,
            home: RecordRef {
                home: eref.home.clone(),
                key: eref.slug.clone(),
            },
        }],
    })
}

/// Fasti's own figures — its spans and occurrences, never another core's (I5, P§3).
///
/// "How many events sit outside a span" is the same derived set `fas list --unspanned`
/// answers, shown rather than enforced: §8.4 keeps it off the validator on purpose, so
/// the screen reports it and nothing nags.
fn panels(root: &std::path::Path) -> Vec<Panel> {
    let all_spans = spans(root, None);
    let all_events = occurrences(root, None);
    let known: std::collections::HashSet<&str> = all_spans
        .iter()
        .map(|(eref, _)| eref.slug.as_str())
        .collect();

    let open = all_spans.iter().filter(|(_, s)| s.to.is_none()).count();
    let unspanned = all_events
        .iter()
        .filter(|(_, line)| {
            !line
                .refs
                .iter()
                .any(|r| r.core == "fasti" && known.contains(r.slug.as_str()))
        })
        .count();

    let mut by_series: Vec<(String, f64)> = Vec::new();
    for (sref, _) in &all_events {
        let name = sref.label().to_string();
        match by_series.iter_mut().find(|(s, _)| *s == name) {
            Some((_, count)) => *count += 1.0,
            None => by_series.push((name, 1.0)),
        }
    }
    let mut placed: Vec<(String, f64)> = all_events
        .iter()
        .map(|(_, line)| (line.key.as_str().to_owned(), 1.0))
        .collect();
    placed.sort_by(|a, b| a.0.cmp(&b.0));

    vec![
        Panel {
            title: "spans".into(),
            chart: Chart::Stat(format!("{open} open"), all_spans.len().to_string()),
        },
        Panel {
            title: "unspanned".into(),
            chart: Chart::Stat("events with no period".into(), unspanned.to_string()),
        },
        Panel {
            title: "by timeline".into(),
            chart: Chart::Bars(by_series),
        },
        Panel {
            title: "placement".into(),
            chart: Chart::Heatmap(placed),
        },
    ]
}
