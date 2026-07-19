//! Pensum's TUI — a thin Porticus provider (P§2).
//!
//! Everything here rides the `tui` feature; drop it and the core is headless (§14).
//!
//! The bin stays thin, in keeping with §14's "~30-line clap shell" ethos: the whole of
//! what Pensum contributes is its identity, its lineup, its per-node count, its writer,
//! and the action→invocation mapping. Everything about how the screen *behaves* is
//! Porticus's (P-II).

use std::ffi::OsString;

use crate::Pensum;
use clap::Parser;
use pantheon::{Code, Response, Store};
use porticus::action::{Invocation, Relayed};
use porticus::view::Row;
use porticus::views::{Agenda, Chart, Insights, Panel, TreeFile};
use porticus::{Action, App, Ident, RecordRef, Target, View, Writer};

use crate::cli::{Cli, with_default_verb};

/// Open Pensum's screen.
///
/// # Errors
/// If the tree cannot be walked or the terminal cannot be taken.
pub fn open(root: &std::path::Path) -> anyhow::Result<()> {
    porticus::run(&mut PensumApp::new(root), root)
}

/// Pensum's screen, as an `App` (P§2).
///
/// Public so a test can build the **real** one and drive it — the same object `open`
/// runs, with the same lineup and the same in-process relay. It carries a root and
/// nothing else: everything drawn is folded from readings each frame (I1).
pub struct PensumApp {
    root: std::path::PathBuf,
}

impl PensumApp {
    #[must_use]
    pub fn new(root: &std::path::Path) -> Self {
        Self {
            root: root.to_path_buf(),
        }
    }
}

impl App for PensumApp {
    fn ident(&self) -> Ident {
        Ident {
            name: "pensum",
            short: "pen",
            tagline: "actio · intention",
            symbol: '♂',
            accent: porticus::ident::accent::MINIUM,
        }
    }

    fn lineup(&mut self) -> Vec<Box<dyn View>> {
        let for_node = self.root.clone();
        let for_agenda = self.root.clone();
        let for_insights = self.root.clone();
        vec![
            Box::new(
                TreeFile::of(move |node: &Code| rows_at(&for_node, node))
                    .offering(&[
                        Action::Add,
                        Action::Edit,
                        Action::Done,
                        Action::Remove,
                        Action::Rename,
                        Action::Move,
                        Action::QuickAdd,
                        Action::DoneAll,
                        Action::RemoveAll,
                    ])
                    .empty("no todos here"),
            ),
            Box::new(
                Agenda::of(move || all_rows(&for_agenda))
                    .offering(&[Action::Edit, Action::Done, Action::Remove])
                    .empty("nothing open"),
            ),
            Box::new(Insights::of(move || panels(&for_insights))),
        ]
    }

    fn count_at(&mut self, node: &Code) -> usize {
        // Folded on the frame it is shown, kept nowhere (I1).
        open_tasks(&self.root, Some(node)).len()
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
                // Porticus appends the typed name; a fresh add runs free (§7.3).
                Some(Invocation::new("pen", ["add", "-H", node.as_str()]))
            }
            (Action::Done, Target::Row(RecordRef { home, key })) => Some(Invocation::new(
                "pen",
                ["edit", "-H", home.as_str(), key, "--done"],
            )),
            // The editor form: no value inline, so the hand's own editor opens and the
            // session *is* the confirm (§7.3). Porticus suspends around it (P§10).
            (Action::Edit, Target::Row(RecordRef { home, key })) => {
                Some(Invocation::new("pen", ["edit", "-H", home.as_str(), key]))
            }
            (Action::Remove, Target::Row(RecordRef { home, key })) => {
                Some(Invocation::new("pen", ["rm", "-H", home.as_str(), key]))
            }
            (Action::Rename, Target::Row(RecordRef { home, key })) => {
                Some(Invocation::new("pen", ["rename", "-H", home.as_str(), key]))
            }
            (Action::Move, Target::Row(RecordRef { home, key })) => Some(Invocation::new(
                "pen",
                ["move", "-H", home.as_str(), key, "--to"],
            )),
            // `D` and `X` need no mapping of their own: Porticus enumerates the view's
            // rows and asks for the *single-row* action per item, which is exactly what
            // P§7 means by "n single relays under one acknowledgement" — and why a core
            // grows no bulk verb for them (§18, no thirteenth verb).
            _ => None,
        }
    }
}

/// Run the invocation **in-process**, through the very code the CLI runs (P§7).
///
/// This is the point of `Writer::InProcess`: the argv goes back through the same clap
/// parse and the same verb bodies, so validation, the record lock, and the plan token
/// are one implementation rather than a re-do. The verb's emitted record is **captured,
/// never let onto the alternate screen** — a core prints its result to stdout, and a
/// screen Porticus is drawing has no room for it.
fn in_process(invocation: &Invocation) -> Relayed {
    let argv =
        std::iter::once(OsString::from("pen")).chain(invocation.args.iter().map(OsString::from));
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
    // `as_json` is forced true: the answer is parsed here, not read by an eye, and a
    // table would have to be un-rendered to get the plan token back out.
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

/// The open tasks at a node, or across the whole tree.
///
/// Folded from Pensum's own store in-process — a core folds its own readings and
/// reaches for no other core (I5).
fn open_tasks(root: &std::path::Path, at: Option<&Code>) -> Vec<(Code, String, bool)> {
    let store = Store::<Pensum>::new(root.to_path_buf());
    store
        .fold(at, None)
        .unwrap_or_default()
        .into_iter()
        .map(|present| {
            let done = present.line.data.done.is_some();
            (present.home, present.line.key.as_str().to_owned(), done)
        })
        .collect()
}

fn rows_at(root: &std::path::Path, node: &Code) -> Vec<Row> {
    open_tasks(root, Some(node))
        .into_iter()
        .filter(|(_, _, done)| !done)
        .map(|(home, key, _)| row(home, key))
        .collect()
}

fn all_rows(root: &std::path::Path) -> Vec<Row> {
    open_tasks(root, None)
        .into_iter()
        .filter(|(_, _, done)| !done)
        .map(|(home, key, _)| row(home, key))
        .collect()
}

fn row(home: Code, key: String) -> Row {
    Row {
        label: format!("{key}   {}", home.as_str()),
        target: Target::Row(RecordRef { home, key }),
        when: None,
    }
}

/// Pensum's own figures — its tasks, never another core's (I5, P§3).
fn panels(root: &std::path::Path) -> Vec<Panel> {
    let all = open_tasks(root, None);
    let done = all.iter().filter(|(_, _, d)| *d).count();
    let open = all.len() - done;

    let mut by_node: Vec<(String, f64)> = Vec::new();
    for (home, _, is_done) in &all {
        if *is_done {
            continue;
        }
        let code = home.as_str().to_owned();
        #[allow(clippy::cast_precision_loss)]
        match by_node.iter_mut().find(|(c, _)| *c == code) {
            Some((_, count)) => *count += 1.0,
            None => by_node.push((code, 1.0)),
        }
    }

    vec![
        Panel {
            title: "open".into(),
            chart: Chart::Stat("tasks".into(), open.to_string()),
        },
        Panel {
            title: "done".into(),
            chart: Chart::Stat("tasks".into(), done.to_string()),
        },
        Panel {
            title: "open by node".into(),
            chart: Chart::Bars(by_node),
        },
    ]
}
