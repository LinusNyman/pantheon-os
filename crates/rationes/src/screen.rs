//! Rationes' TUI — a thin Porticus provider (P§2). Rides the `tui` feature (§14).
//!
//! Three views, and the middle one is why Rationes has a card at all: a holding is an
//! entity, so it is a thing to pin and look at, while its balance is a *trend* — the
//! `Trend` panel on Insights is the same figures folded the other way.
//!
//! **Net worth is drawn, never stored** (I1, §8.3): the panels fold fresh each frame
//! from Rationes' own records, and reach for no other core (I5).

use std::ffi::OsString;

use crate::{Holding, Rationes, Record};
use clap::Parser;
use pantheon::{Code, EntityRef, PresentLine, Response, Store};
use porticus::action::{Invocation, Relayed};
use porticus::view::Row;
use porticus::views::{Card, Chart, Chip, EntityCard, Insights, Panel, TreeFile};
use porticus::{Action, App, Ident, RecordRef, Target, View, Writer};

use crate::cli::{Cli, with_default_verb};

/// Open Rationes' screen.
///
/// # Errors
/// If the tree cannot be walked or the terminal cannot be taken.
pub fn open(root: &std::path::Path) -> anyhow::Result<()> {
    porticus::run(&mut RationesApp::new(root), root)
}

/// Rationes's screen, as an `App` (P§2).
///
/// Public so a test can build the **real** one and drive it — the same object `open`
/// runs, with the same lineup and the same in-process relay. It carries a root and
/// nothing else: everything drawn is folded from readings each frame (I1).
pub struct RationesApp {
    root: std::path::PathBuf,
}

impl RationesApp {
    #[must_use]
    pub fn new(root: &std::path::Path) -> Self {
        Self {
            root: root.to_path_buf(),
        }
    }
}

impl App for RationesApp {
    fn ident(&self) -> Ident {
        Ident {
            name: "rationes",
            short: "rat",
            tagline: "res · what",
            symbol: '♃',
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
                    .empty("nothing held here"),
            ),
            // The holding card — Rationes' detail view (P§3).
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
        holdings(&self.root, Some(node)).len()
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
                Some(Invocation::new("rat", ["add", "-H", node.as_str()]))
            }
            (Action::Edit, Target::Row(RecordRef { home, key })) => {
                Some(Invocation::new("rat", ["edit", "-H", home.as_str(), key]))
            }
            (Action::Remove, Target::Row(RecordRef { home, key })) => {
                Some(Invocation::new("rat", ["rm", "-H", home.as_str(), key]))
            }
            (Action::Rename, Target::Row(RecordRef { home, key })) => {
                Some(Invocation::new("rat", ["rename", "-H", home.as_str(), key]))
            }
            (Action::Move, Target::Row(RecordRef { home, key })) => Some(Invocation::new(
                "rat",
                ["move", "-H", home.as_str(), key, "--to"],
            )),
            // A holding has nothing to mark done, so `d`/`D` stay dark — the key is a
            // no-op rather than repurposed (P§5).
            _ => None,
        }
    }
}

/// Run the invocation in-process, through the very code the CLI runs (P§7).
fn in_process(invocation: &Invocation) -> Relayed {
    let argv =
        std::iter::once(OsString::from("rat")).chain(invocation.args.iter().map(OsString::from));
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
    match crate::cli::run(&cli, true) {
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

/// Rationes' own holdings — a core folds its own records and reaches for no other (I5).
fn holdings(root: &std::path::Path, at: Option<&Code>) -> Vec<(EntityRef, Holding)> {
    Store::<Rationes>::new(root.to_path_buf())
        .fold_entities(at, None)
        .unwrap_or_default()
        .into_iter()
        .filter_map(|(eref, entity)| {
            // A file whose body disagrees with its filename is a `pan validate`
            // finding, not a row: the screen simply does not draw it (§5.2, §10.2).
            let holding = entity.data.as_holding().ok()?.clone();
            Some((eref, holding))
        })
        .collect()
}

/// Every balance series under `at`, each folded to the line at its latest key — the
/// present, derived on the frame it is shown and never stored (I1, P§3).
fn balances(root: &std::path::Path, at: Option<&Code>) -> Vec<PresentLine<Record>> {
    Store::<Rationes>::new(root.to_path_buf())
        .fold(at, Some(Rationes::BALANCE))
        .unwrap_or_default()
}

/// The latest balance of one holding, if it has one.
fn latest(eref: &EntityRef, folded: &[PresentLine<Record>]) -> Option<(String, f64)> {
    let present = folded
        .iter()
        .find(|p| p.home == eref.home && p.name.as_deref() == Some(&eref.slug))?;
    let amount = present.line.data.as_balance().ok()?.amount;
    Some((present.line.key.as_str().to_owned(), amount))
}

fn rows_at(root: &std::path::Path, node: &Code) -> Vec<Row> {
    let folded = balances(root, Some(node));
    holdings(root, Some(node))
        .into_iter()
        .map(|(eref, _)| {
            let figure = latest(&eref, &folded)
                .map_or_else(String::new, |(_, amount)| format!("   {amount}"));
            Row {
                label: format!("{}   {}{figure}", eref.slug, eref.kind),
                target: Target::Row(RecordRef {
                    home: eref.home,
                    key: eref.slug,
                }),
                when: None,
            }
        })
        .collect()
}

/// The pinned holding as a card, else the node's one holding.
///
/// An `Enter`-drill pins a row and this resolves **that** record (P§3). With nothing
/// pinned it falls back to the node's single holding — the entity-as-node case (§5.1)
/// — and where a node holds several it shows the empty "pick a record" state rather
/// than guessing among them: a detail view never picks for you.
fn card_at(root: &std::path::Path, node: &Code, pinned: Option<&RecordRef>) -> Option<Card> {
    let store = Store::<Rationes>::new(root.to_path_buf());
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
    let holding = entity.data.as_holding().ok()?;

    let mut fields = vec![("kind".into(), eref.kind.clone())];
    for (label, value) in [
        ("currency", holding.currency.as_ref()),
        ("expires", holding.expires.as_ref()),
    ] {
        if let Some(value) = value {
            fields.push(((*label).to_string(), value.clone()));
        }
    }
    // The derived present, beside the fields that are stored (I1).
    if let Some((key, amount)) = latest(&eref, &balances(root, Some(&eref.home))) {
        fields.push(("balance".into(), amount.to_string()));
        fields.push(("as of".into(), key));
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

/// Rationes' own figures — its holdings and their balances, never another core's
/// (I5, P§3).
fn panels(root: &std::path::Path) -> Vec<Panel> {
    let all = holdings(root, None);
    let folded = balances(root, None);

    let mut by_kind: Vec<(String, f64)> = Vec::new();
    for (eref, _) in &all {
        match by_kind.iter_mut().find(|(k, _)| *k == eref.kind) {
            Some((_, count)) => *count += 1.0,
            None => by_kind.push((eref.kind.clone(), 1.0)),
        }
    }

    // Net worth, folded by currency (§8.3): dollars are not added to shares, and a
    // `claim` never reaches here because it carries no balance to fold.
    let mut net: Vec<(String, f64)> = Vec::new();
    for (eref, holding) in &all {
        let Some((_, amount)) = latest(eref, &folded) else {
            continue;
        };
        let currency = holding.currency.clone().unwrap_or_else(|| "—".to_string());
        match net.iter_mut().find(|(c, _)| *c == currency) {
            Some((_, total)) => *total += amount,
            None => net.push((currency, amount)),
        }
    }
    net.sort_by(|a, b| a.0.cmp(&b.0));

    vec![
        Panel {
            title: "held".into(),
            chart: Chart::Stat("holdings".into(), all.len().to_string()),
        },
        Panel {
            title: "net worth".into(),
            chart: Chart::Bars(net),
        },
        Panel {
            title: "by kind".into(),
            chart: Chart::Bars(by_kind),
        },
    ]
}
