//! Mappa's TUI — a thin Porticus provider (P§2). Rides the `tui` feature (§14).

use std::ffi::OsString;

use clap::Parser;
use mappa::{Mappa, Place};
use pantheon::{Code, EntityRef, Response, Store};
use porticus::action::{Invocation, Relayed};
use porticus::view::Row;
use porticus::views::{Card, Chart, Chip, EntityCard, Insights, Panel, TreeFile};
use porticus::{Action, App, Ident, RecordRef, Target, View, Writer};

use crate::{Cli, with_default_verb};

/// Open Mappa's screen.
///
/// # Errors
/// If the tree cannot be walked or the terminal cannot be taken.
pub fn open(root: &std::path::Path) -> anyhow::Result<()> {
    let mut app = MappaApp {
        root: root.to_path_buf(),
    };
    porticus::run(&mut app, root)
}

struct MappaApp {
    root: std::path::PathBuf,
}

impl App for MappaApp {
    fn ident(&self) -> Ident {
        Ident {
            name: "mappa",
            short: "map",
            tagline: "locus · where",
            symbol: '♁',
            accent: porticus::ident::accent::TERRACOTTA,
        }
    }

    fn lineup(&mut self) -> Vec<Box<dyn View>> {
        let for_rows = self.root.clone();
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
                    .empty("nowhere filed here"),
            ),
            // The place card — Mappa's detail view (P§3).
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
        places(&self.root, Some(node)).len()
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
                Some(Invocation::new("map", ["add", "-H", node.as_str()]))
            }
            (Action::Edit, Target::Row(RecordRef { home, key })) => {
                Some(Invocation::new("map", ["edit", "-H", home.as_str(), key]))
            }
            (Action::Remove, Target::Row(RecordRef { home, key })) => {
                Some(Invocation::new("map", ["rm", "-H", home.as_str(), key]))
            }
            (Action::Rename, Target::Row(RecordRef { home, key })) => {
                Some(Invocation::new("map", ["rename", "-H", home.as_str(), key]))
            }
            (Action::Move, Target::Row(RecordRef { home, key })) => Some(Invocation::new(
                "map",
                ["move", "-H", home.as_str(), key, "--to"],
            )),
            // Mappa keeps no series and has nothing to toggle, so `d`/`D` stay dark —
            // the key is a no-op rather than repurposed (P§5).
            _ => None,
        }
    }
}

/// Run the invocation in-process, through the very code the CLI runs (P§7).
fn in_process(invocation: &Invocation) -> Relayed {
    let argv =
        std::iter::once(OsString::from("map")).chain(invocation.args.iter().map(OsString::from));
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

/// Mappa's own entities — a core folds its own records and reaches for no other (I5).
fn places(root: &std::path::Path, at: Option<&Code>) -> Vec<(EntityRef, Place)> {
    Store::<Mappa>::new(root.to_path_buf())
        .fold_entities(at, None)
        .unwrap_or_default()
        .into_iter()
        .map(|(eref, entity)| (eref, entity.data))
        .collect()
}

fn rows_at(root: &std::path::Path, node: &Code) -> Vec<Row> {
    places(root, Some(node))
        .into_iter()
        .map(|(eref, _)| Row {
            label: format!("{}   {}", eref.slug, eref.kind),
            target: Target::Row(RecordRef {
                home: eref.home,
                key: eref.slug,
            }),
            when: None,
        })
        .collect()
}

/// Where a place *is*, as one line — the coordinate for a point, the extent for an
/// area (§8.2). A record carrying neither shows nothing rather than a placeholder:
/// the card states what is stored, never what is missing.
fn sited(place: &Place) -> Option<String> {
    if let Some(at) = &place.coordinates {
        return Some(format!("{}, {}", at.lat, at.lon));
    }
    let extent = place.bounds.as_ref()?;
    Some(format!(
        "{}, {} → {}, {}",
        extent.south, extent.west, extent.north, extent.east
    ))
}

/// The pinned place as a card, else the node's one place.
///
/// An `Enter`-drill pins a row and this resolves **that** record (P§3). With nothing
/// pinned it falls back to the node's single place — the entity-as-node case (§5.1) —
/// and where a node holds several it shows the empty "pick a record" state rather than
/// guessing among them: a detail view never picks for you.
///
/// A pinned record gone underneath is simply not found, which lands on that same empty
/// state rather than a stale card (I8, §6.4).
fn card_at(root: &std::path::Path, node: &Code, pinned: Option<&RecordRef>) -> Option<Card> {
    let store = Store::<Mappa>::new(root.to_path_buf());
    let (eref, entity) = if let Some(record) = pinned {
        let mut at = store.fold_entities(Some(&record.home), None).ok()?;
        let index = at.iter().position(|(eref, _)| eref.slug == record.key)?;
        at.remove(index)
    } else {
        let mut found = store.fold_entities(Some(node), None).ok()?;
        if found.len() != 1 {
            return None;
        }
        found.remove(0)
    };
    let mut fields = vec![("kind".into(), eref.kind.clone())];
    if let Some(where_it_is) = sited(&entity.data) {
        fields.push(("at".into(), where_it_is));
    }
    for (label, value) in [
        ("address", entity.data.address.as_ref()),
        ("timezone", entity.data.timezone.as_ref()),
    ] {
        if let Some(value) = value {
            fields.push(((*label).to_string(), value.clone()));
        }
    }
    Some(Card {
        title: eref.slug.clone(),
        fields,
        // Ref chips are display-only in v1 (P§3): legible, not a link.
        chips: entity
            .refs
            .iter()
            .map(|r| Chip {
                label: r.to_token(),
                reference: r.to_token(),
            })
            .collect(),
        strip: Vec::new(),
    })
}

/// Mappa's own figures — its places, never another core's (I5, P§3).
///
/// "sited" counts the places that carry a datum a machine can put on a map — a
/// derived figure, read off the records each frame and stored nowhere (I1).
fn panels(root: &std::path::Path) -> Vec<Panel> {
    let all = places(root, None);
    let mut by_kind: Vec<(String, f64)> = Vec::new();
    for (eref, _) in &all {
        match by_kind.iter_mut().find(|(k, _)| *k == eref.kind) {
            Some((_, count)) => *count += 1.0,
            None => by_kind.push((eref.kind.clone(), 1.0)),
        }
    }
    let sited_count = all
        .iter()
        .filter(|(_, place)| sited(place).is_some())
        .count();
    vec![
        Panel {
            title: "filed".into(),
            chart: Chart::Stat("places".into(), all.len().to_string()),
        },
        Panel {
            title: "by kind".into(),
            chart: Chart::Bars(by_kind),
        },
        Panel {
            title: "sited".into(),
            chart: Chart::Stat("with a datum".into(), sited_count.to_string()),
        },
    ]
}
