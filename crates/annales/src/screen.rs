//! Annales' TUI — a thin Porticus provider (P§2). Rides the `tui` feature (§14).
//!
//! The lineup is the shortest of the four: `TreeFile` and `Insights`. Annales' records
//! are dated readings, and a reading is a *sample*, never a ref target (I1, §5.4) — so
//! there is no detail view to pin one to.

use std::ffi::OsString;

use annales::Annales;
use clap::Parser;
use pantheon::{Code, Response, Store};
use porticus::action::{Invocation, Relayed};
use porticus::view::Row;
use porticus::views::{Chart, Insights, Panel, TreeFile};
use porticus::{Action, App, Ident, RecordRef, Target, View, Writer};

use crate::{Cli, with_default_verb};

/// Open Annales' screen.
///
/// # Errors
/// If the tree cannot be walked or the terminal cannot be taken.
pub fn open(root: &std::path::Path) -> anyhow::Result<()> {
    let mut app = AnnalesApp {
        root: root.to_path_buf(),
    };
    porticus::run(&mut app, root)
}

struct AnnalesApp {
    root: std::path::PathBuf,
}

impl App for AnnalesApp {
    fn ident(&self) -> Ident {
        Ident {
            name: "annales",
            short: "ann",
            tagline: "actio · fact",
            symbol: '☉',
            accent: porticus::ident::accent::SOL_GOLD,
        }
    }

    fn lineup(&mut self) -> Vec<Box<dyn View>> {
        let for_rows = self.root.clone();
        let for_insights = self.root.clone();
        vec![
            Box::new(
                TreeFile::of(move |node: &Code| rows_at(&for_rows, node))
                    .offering(&[Action::Add, Action::Edit, Action::Remove, Action::QuickAdd])
                    .empty("nothing logged here"),
            ),
            Box::new(Insights::of(move || panels(&for_insights))),
        ]
    }

    fn count_at(&mut self, node: &Code) -> usize {
        readings(&self.root, Some(node)).len()
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
                Some(Invocation::new("ann", ["add", "-H", node.as_str()]))
            }
            (Action::Edit, Target::Row(RecordRef { home, key })) => {
                Some(Invocation::new("ann", ["edit", "-H", home.as_str(), key]))
            }
            (Action::Remove, Target::Row(RecordRef { home, key })) => {
                Some(Invocation::new("ann", ["rm", "-H", home.as_str(), key]))
            }
            // A reading has no name of its own to rename and no home but the one it was
            // taken at, so `r` and `m` stay dark (P§5, §5.4).
            _ => None,
        }
    }
}

fn in_process(invocation: &Invocation) -> Relayed {
    let argv =
        std::iter::once(OsString::from("ann")).chain(invocation.args.iter().map(OsString::from));
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

/// Annales' own readings, folded to the present per key (I1).
fn readings(root: &std::path::Path, at: Option<&Code>) -> Vec<(Code, String, String, String)> {
    Store::<Annales>::new(root.to_path_buf())
        .fold(at, None)
        .unwrap_or_default()
        .into_iter()
        .map(|present| {
            let series = present.name.clone().unwrap_or_default();
            let values = present
                .line
                .data
                .values
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(" ");
            (
                present.home,
                series,
                present.line.key.as_str().to_owned(),
                values,
            )
        })
        .collect()
}

fn rows_at(root: &std::path::Path, node: &Code) -> Vec<Row> {
    readings(root, Some(node))
        .into_iter()
        .map(|(home, series, key, values)| Row {
            label: format!("{series}   {values}"),
            target: Target::Row(RecordRef {
                home,
                key: key.clone(),
            }),
            when: Some(key),
        })
        .collect()
}

/// Annales' own figures. *Where you've been* and *time spent* are Annales' insights
/// precisely because aboutness homes the fact where it lives (I3, §2, P§3) — Mappa
/// could not draw them, having no logs to read.
fn panels(root: &std::path::Path) -> Vec<Panel> {
    let all = readings(root, None);
    let mut by_series: Vec<(String, f64)> = Vec::new();
    for (_, series, _, _) in &all {
        match by_series.iter_mut().find(|(s, _)| s == series) {
            Some((_, count)) => *count += 1.0,
            None => by_series.push((series.clone(), 1.0)),
        }
    }
    let mut logged: Vec<(String, f64)> = all
        .iter()
        .map(|(_, _, key, _)| (key.clone(), 1.0))
        .collect();
    logged.sort_by(|a, b| a.0.cmp(&b.0));

    vec![
        Panel {
            title: "readings".into(),
            chart: Chart::Stat("logged".into(), all.len().to_string()),
        },
        Panel {
            title: "by log".into(),
            chart: Chart::Bars(by_series),
        },
        Panel {
            title: "consistency".into(),
            chart: Chart::Heatmap(logged),
        },
    ]
}
