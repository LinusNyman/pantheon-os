//! Album's TUI — a thin Porticus provider (P§2). Rides the `tui` feature (§14).

use std::ffi::OsString;

use album::Album;
use clap::Parser;
use pantheon::{Code, EntityRef, Response, Store};
use porticus::action::{Invocation, Relayed};
use porticus::view::Row;
use porticus::views::{Card, Chart, Chip, EntityCard, Insights, Panel, TreeFile};
use porticus::{Action, App, Ident, RecordRef, Target, View, Writer};

use crate::{Cli, with_default_verb};

/// Open Album's screen.
///
/// # Errors
/// If the tree cannot be walked or the terminal cannot be taken.
pub fn open(root: &std::path::Path) -> anyhow::Result<()> {
    let mut app = AlbumApp {
        root: root.to_path_buf(),
    };
    porticus::run(&mut app, root)
}

struct AlbumApp {
    root: std::path::PathBuf,
}

impl App for AlbumApp {
    fn ident(&self) -> Ident {
        Ident {
            name: "album",
            short: "alb",
            tagline: "societas · who",
            symbol: '♀',
            accent: porticus::ident::accent::VERDIGRIS,
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
                    .empty("nobody filed here"),
            ),
            // The contact card — Album's detail view (P§3).
            Box::new(
                EntityCard::of(move |node: &Code| card_at(&for_card, node))
                    .offering(&[Action::Edit, Action::Rename]),
            ),
            Box::new(Insights::of(move || panels(&for_insights))),
        ]
    }

    fn count_at(&mut self, node: &Code) -> usize {
        agents(&self.root, Some(node)).len()
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
                Some(Invocation::new("alb", ["add", "-H", node.as_str()]))
            }
            (Action::Edit, Target::Row(RecordRef { home, key })) => {
                Some(Invocation::new("alb", ["edit", "-H", home.as_str(), key]))
            }
            (Action::Remove, Target::Row(RecordRef { home, key })) => {
                Some(Invocation::new("alb", ["rm", "-H", home.as_str(), key]))
            }
            (Action::Rename, Target::Row(RecordRef { home, key })) => {
                Some(Invocation::new("alb", ["rename", "-H", home.as_str(), key]))
            }
            (Action::Move, Target::Row(RecordRef { home, key })) => Some(Invocation::new(
                "alb",
                ["move", "-H", home.as_str(), key, "--to"],
            )),
            // Album keeps no series and has nothing to toggle, so `d`/`D` stay dark —
            // the key is a no-op rather than repurposed (P§5).
            _ => None,
        }
    }
}

/// Run the invocation in-process, through the very code the CLI runs (P§7).
fn in_process(invocation: &Invocation) -> Relayed {
    let argv =
        std::iter::once(OsString::from("alb")).chain(invocation.args.iter().map(OsString::from));
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

/// Album's own entities — a core folds its own readings and reaches for no other (I5).
fn agents(root: &std::path::Path, at: Option<&Code>) -> Vec<(EntityRef, album::Agent)> {
    Store::<Album>::new(root.to_path_buf())
        .fold_entities(at, None)
        .unwrap_or_default()
        .into_iter()
        .map(|(eref, entity)| (eref, entity.data))
        .collect()
}

fn rows_at(root: &std::path::Path, node: &Code) -> Vec<Row> {
    agents(root, Some(node))
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

/// The node's one agent as a card.
///
/// Where a node holds several, the card shows the **empty "pick a record"** state
/// rather than guessing among them (P§3) — a detail view never picks for you.
fn card_at(root: &std::path::Path, node: &Code) -> Option<Card> {
    let store = Store::<Album>::new(root.to_path_buf());
    let mut found = store.fold_entities(Some(node), None).ok()?;
    if found.len() != 1 {
        return None;
    }
    let (eref, entity) = found.remove(0);
    let mut fields = vec![("kind".into(), eref.kind.clone())];
    for (label, value) in [
        ("gender", entity.data.gender.as_ref()),
        ("closeness", entity.data.closeness.as_ref()),
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

/// Album's own figures — its agents, never another core's (I5, P§3).
fn panels(root: &std::path::Path) -> Vec<Panel> {
    let all = agents(root, None);
    let mut by_kind: Vec<(String, f64)> = Vec::new();
    for (eref, _) in &all {
        match by_kind.iter_mut().find(|(k, _)| *k == eref.kind) {
            Some((_, count)) => *count += 1.0,
            None => by_kind.push((eref.kind.clone(), 1.0)),
        }
    }
    vec![
        Panel {
            title: "filed".into(),
            chart: Chart::Stat("agents".into(), all.len().to_string()),
        },
        Panel {
            title: "by kind".into(),
            chart: Chart::Bars(by_kind),
        },
    ]
}
