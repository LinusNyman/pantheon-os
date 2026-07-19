//! Tabella's TUI — a thin Porticus provider (P§2). Rides the `tui` feature (§14).

use std::ffi::OsString;

use clap::Parser;
use pantheon::{Code, Response, Store};
use porticus::action::{Invocation, Relayed};
use porticus::view::Row;
use porticus::views::{Chart, Document, Insights, Panel, Reader, TreeFile};
use porticus::{Action, App, Ident, RecordRef, Target, View, Writer};
use tabella::Tabella;

use crate::{Cli, with_default_verb};

/// Open Tabella's screen.
///
/// # Errors
/// If the tree cannot be walked or the terminal cannot be taken.
pub fn open(root: &std::path::Path) -> anyhow::Result<()> {
    let mut app = TabellaApp {
        root: root.to_path_buf(),
    };
    porticus::run(&mut app, root)
}

struct TabellaApp {
    root: std::path::PathBuf,
}

impl App for TabellaApp {
    fn ident(&self) -> Ident {
        Ident {
            name: "tabella",
            short: "tab",
            tagline: "prose · meaning",
            symbol: '☿',
            accent: porticus::ident::accent::QUICKSILVER,
        }
    }

    fn lineup(&mut self) -> Vec<Box<dyn View>> {
        let for_rows = self.root.clone();
        let for_read = self.root.clone();
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
                    .empty("no documents here"),
            ),
            // The document rendered — Tabella's detail view (P§3). Reading only:
            // editing suspends to the hand's own editor (§7.3, P§10).
            Box::new(
                Reader::of(move |node: &Code, pinned: Option<&RecordRef>| {
                    document_at(&for_read, node, pinned)
                })
                .offering(&[Action::Edit, Action::Rename, Action::Remove]),
            ),
            Box::new(Insights::of(move || panels(&for_insights))),
        ]
    }

    fn count_at(&mut self, node: &Code) -> usize {
        documents(&self.root, Some(node)).len()
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
                Some(Invocation::new("tab", ["add", "-H", node.as_str()]))
            }
            // `edit` with no value inline is the **editor form** (§7.3): a document is
            // opened in place, because it already *is* the text (§8.7). Porticus
            // suspends around the session, and the session is itself the confirm.
            (Action::Edit, Target::Row(RecordRef { home, key })) => {
                Some(Invocation::new("tab", ["edit", "-H", home.as_str(), key]))
            }
            (Action::Remove, Target::Row(RecordRef { home, key })) => {
                Some(Invocation::new("tab", ["rm", "-H", home.as_str(), key]))
            }
            (Action::Rename, Target::Row(RecordRef { home, key })) => {
                Some(Invocation::new("tab", ["rename", "-H", home.as_str(), key]))
            }
            (Action::Move, Target::Row(RecordRef { home, key })) => Some(Invocation::new(
                "tab",
                ["move", "-H", home.as_str(), key, "--to"],
            )),
            _ => None,
        }
    }
}

fn in_process(invocation: &Invocation) -> Relayed {
    let argv =
        std::iter::once(OsString::from("tab")).chain(invocation.args.iter().map(OsString::from));
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

/// The node's documents — **frontmatter only**.
///
/// A fold never reads bodies (§6.1, §7.1, §7.2, §8.7 — the spec says it four times),
/// and `fold_documents` stops at the closing fence. The Reader below is the one place
/// a body is read, and it reads exactly one.
fn documents(
    root: &std::path::Path,
    at: Option<&Code>,
) -> Vec<(pantheon::DocumentRef, pantheon::Frontmatter)> {
    Store::<Tabella>::new(root.to_path_buf())
        .fold_documents(at)
        .unwrap_or_default()
}

fn rows_at(root: &std::path::Path, node: &Code) -> Vec<Row> {
    documents(root, Some(node))
        .into_iter()
        .map(|(dref, front)| Row {
            label: match &front.r#type {
                Some(kind) => format!("{}   {kind}", dref.slug),
                None => dref.slug.clone(),
            },
            target: Target::Row(RecordRef {
                home: dref.home,
                key: dref.slug,
            }),
            when: None,
        })
        .collect()
}

/// The pinned document, body and all — else the node's one document.
///
/// This is the one read that *does* open a body, which is exactly what a Reader is for:
/// a fold never reads bodies (§7.1), and this is not a fold but one record opened
/// deliberately. An `Enter`-drill names which; with nothing pinned it falls back to the
/// node's single document, and where a node holds several it shows the empty state
/// rather than guessing (P§3).
fn document_at(
    root: &std::path::Path,
    node: &Code,
    pinned: Option<&RecordRef>,
) -> Option<Document> {
    let store = Store::<Tabella>::new(root.to_path_buf());
    let dref = if let Some(record) = pinned {
        let at = store.fold_documents(Some(&record.home)).ok()?;
        at.into_iter()
            .find(|(dref, _)| dref.slug == record.key)
            .map(|(dref, _)| dref)?
    } else {
        let mut found = store.fold_documents(Some(node)).ok()?;
        if found.len() != 1 {
            return None;
        }
        found.remove(0).0
    };
    let whole = store.read_document(&dref).ok()?;
    Some(Document {
        slug: dref.slug,
        r#type: whole.frontmatter.r#type,
        tags: whole.frontmatter.tags,
        body: whole.body,
    })
}

/// Tabella's own figures — usage by frontmatter (P§3).
fn panels(root: &std::path::Path) -> Vec<Panel> {
    let all = documents(root, None);
    let mut by_type: Vec<(String, f64)> = Vec::new();
    let mut by_tag: Vec<(String, f64)> = Vec::new();
    for (_, front) in &all {
        let kind = front.r#type.clone().unwrap_or_else(|| "untyped".into());
        match by_type.iter_mut().find(|(k, _)| *k == kind) {
            Some((_, count)) => *count += 1.0,
            None => by_type.push((kind, 1.0)),
        }
        for tag in &front.tags {
            match by_tag.iter_mut().find(|(t, _)| t == tag) {
                Some((_, count)) => *count += 1.0,
                None => by_tag.push((tag.clone(), 1.0)),
            }
        }
    }
    vec![
        Panel {
            title: "documents".into(),
            chart: Chart::Stat("filed".into(), all.len().to_string()),
        },
        Panel {
            title: "by type".into(),
            chart: Chart::Pie(by_type),
        },
        Panel {
            title: "by tag".into(),
            chart: Chart::Bars(by_tag),
        },
    ]
}
