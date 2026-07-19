//! The screen: Atrium as a Porticus app (P§2).
//!
//! Everything here rides the `tui` feature — a headless lens keeps the folds and drops
//! the chrome (§12, §14), so nothing in this file may be reachable without it.

use pantheon::Code;
use porticus::view::Row;
use porticus::views::Agenda;
use porticus::{Action, App, Ident, Invocation, RecordRef, Target, View, Writer};
use serde_json::Value;

use crate::mosaic::Mosaic;
use crate::{ALBUM, PENSUM, TABELLA};

/// Open the mosaic.
///
/// # Errors
/// If the tree cannot be walked or the terminal cannot be taken.
pub fn open(root: &std::path::Path) -> anyhow::Result<()> {
    porticus::run(&mut Atrium, root)
}

struct Atrium;

impl App for Atrium {
    fn ident(&self) -> Ident {
        Ident {
            name: "atrium",
            short: "atr",
            tagline: "the hearth",
            symbol: '☊',
            accent: porticus::ident::accent::HEARTH,
        }
    }

    fn lineup(&mut self) -> Vec<Box<dyn View>> {
        vec![
            // A lens leads with its mosaic — the dashboard, not the tree (P§3).
            Box::new(Mosaic::of(vec![
                Box::new(tessera::Count::of("open tasks", PENSUM, &["list"])),
                Box::new(tessera::Count::of("people", ALBUM, &["list"])),
                Box::new(tessera::Count::of("documents", TABELLA, &["list"])),
            ])),
            // The day's tasks, each row carrying its own home so the list spans nodes
            // and each `d` relays to the right one (P§3, P§7).
            Box::new(
                Agenda::of(tasks)
                    .offering(&[Action::Done, Action::Edit, Action::Remove])
                    .empty("nothing open today"),
            ),
        ]
    }

    fn count_at(&mut self, node: &Code) -> usize {
        // Atrium's items at a node are the open tasks there — folded, never stored (I1).
        tessera::read(PENSUM, &["list", "-H", node.as_str()])
            .and_then(|v| v.as_array().map(Vec::len))
            .unwrap_or(0)
    }

    fn writer(&self) -> Writer {
        // A lens shells out to the core binary on `PATH` (§12): it links no core (I5),
        // and the write crosses the JSON boundary like every other (I4).
        Writer::Subprocess
    }

    fn relays_to(&self) -> Vec<String> {
        vec![PENSUM.to_string()]
    }

    fn on_action(&mut self, action: Action, target: &Target) -> Option<Invocation> {
        // Only the app knows its verb grammar, because only the app authors the write
        // (I2). Porticus owns the confirm and the relay and knows none of this.
        let Target::Row(RecordRef { home, key }) = target else {
            // Atrium adds nothing: it owns no primitive, so a new record is a core's to
            // create, not a dashboard's (§12).
            return None;
        };
        let home = home.as_str();
        match action {
            Action::Done => Some(Invocation::new(PENSUM, ["edit", "-H", home, key, "--done"])),
            Action::Edit => Some(Invocation::new(PENSUM, ["edit", "-H", home, key])),
            Action::Remove => Some(Invocation::new(PENSUM, ["rm", "-H", home, key])),
            _ => None,
        }
    }
}

/// The open tasks across the whole tree, as rows.
///
/// Read off `pen list`'s JSON and nothing else — the contract is the only thing that
/// crosses (I4). Each row keeps the home the core reported, which is what lets a
/// cross-node agenda relay each `d` to its own node (P§7).
fn tasks() -> Vec<Row> {
    let Some(Value::Array(rows)) = tessera::read(PENSUM, &["list"]) else {
        return Vec::new();
    };
    rows.iter()
        .filter_map(|row| {
            let key = row["key"].as_str()?;
            let home = Code::parse(row["home"].as_str()?).ok()?;
            let refs = row["refs"].as_array().map_or(String::new(), |refs| {
                let names: Vec<&str> = refs.iter().filter_map(Value::as_str).collect();
                if names.is_empty() {
                    String::new()
                } else {
                    format!("   {}", names.join(", "))
                }
            });
            Some(Row {
                label: format!("{key}   {}{refs}", home.as_str()),
                target: Target::Row(RecordRef {
                    home,
                    key: key.to_string(),
                }),
                when: row["data"]["done"].as_str().map(str::to_owned),
            })
        })
        .collect()
}
