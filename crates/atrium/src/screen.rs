//! The screen: Atrium as a Porticus app (P§2).
//!
//! Everything here rides the `tui` feature — a headless lens keeps the folds and drops
//! the chrome (§12, §14), so nothing in this file may be reachable without it.

use pantheon::Code;
use porticus::view::Row;
use porticus::views::Agenda;
use porticus::{Action, App, Ident, Invocation, RecordRef, Target, View, Writer};
use serde_json::Value;

use crate::cli::{ALBUM, PENSUM, TABELLA};
use crate::mosaic::Mosaic;

/// Open the mosaic.
///
/// # Errors
/// If the tree cannot be walked or the terminal cannot be taken.
pub fn open(root: &std::path::Path) -> anyhow::Result<()> {
    porticus::run(&mut Atrium::new(root), root)
}

/// The root the screen is drawing.
///
/// Held rather than left to `$PANTHEON_ROOT`: a lens opened with `-C` must fold the
/// tree it was pointed at, not the caller's ambient one (§6.2, §7.3).
pub struct Atrium {
    root: std::path::PathBuf,
}

impl Atrium {
    /// Public so a test can build the **real** lens and drive it — the same object
    /// `open` runs, with the same tiles and the same **subprocess** relay, so a driven
    /// write crosses the JSON boundary exactly as it does in a hand's terminal
    /// (I4, I5, §12).
    #[must_use]
    pub fn new(root: &std::path::Path) -> Self {
        Self {
            root: root.to_path_buf(),
        }
    }
}

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
        let for_agenda = self.root.clone();
        vec![
            // A lens leads with its mosaic — the dashboard, not the tree (P§3).
            Box::new(Mosaic::of(vec![
                Box::new(tessera::Count::of(
                    &self.root,
                    "open tasks",
                    PENSUM,
                    &["list"],
                )),
                Box::new(tessera::Count::of(&self.root, "people", ALBUM, &["list"])),
                Box::new(tessera::Count::of(
                    &self.root,
                    "documents",
                    TABELLA,
                    &["list"],
                )),
            ])),
            // The day's tasks, each row carrying its own home so the list spans nodes
            // and each `d` relays to the right one (P§3, P§7).
            Box::new(
                Agenda::of(move || tasks(&for_agenda))
                    .offering(&[Action::Done, Action::Edit, Action::Remove])
                    .empty("nothing open today"),
            ),
        ]
    }

    fn count_at(&mut self, node: &Code) -> usize {
        // Atrium's items at a node are the open tasks there — folded, never stored (I1).
        tessera::read(&self.root, PENSUM, &["list", "-H", node.as_str()])
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
fn tasks(root: &std::path::Path) -> Vec<Row> {
    let Some(Value::Array(rows)) = tessera::read(root, PENSUM, &["list"]) else {
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

#[cfg(test)]
mod tests {
    use super::Atrium;
    use std::path::PathBuf;
    use std::process::Command;

    /// Where the freshly built binaries are.
    ///
    /// `CARGO_BIN_EXE_*` is an *integration* test's variable; a unit test inside the bin
    /// has no such thing, so this walks up from the test binary itself
    /// (`target/debug/deps/<test>` → `target/debug`).
    fn bins() -> PathBuf {
        std::env::current_exe()
            .expect("a test binary knows where it is")
            .parent()
            .and_then(|deps| deps.parent())
            .expect("target/debug/deps/<test>")
            .to_path_buf()
    }

    /// A seeded tree, filled by driving the real core binaries — the same hands a user
    /// would use (I8).
    fn fresh_root(tag: &str) -> PathBuf {
        let bins = bins();
        let root = std::env::temp_dir().join(format!("atrium-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let run = |short: &str, args: &[&str]| {
            let out = Command::new(bins.join(short))
                .args(args)
                .env("PANTHEON_ROOT", &root)
                .output()
                .unwrap_or_else(|e| panic!("running {short}: {e}"));
            assert!(
                out.status.success(),
                "{short} {args:?}: {}",
                String::from_utf8_lossy(&out.stderr)
            );
        };
        run("pan", &["new", "root", "a", "actio", "-y"]);
        run("pan", &["new", "a", "c", "cura", "-y"]);
        run("pen", &["ac", "buy_milk", "-y"]);
        run("pen", &["ac", "call_alex", "-y"]);
        root
    }

    fn open_tasks(root: &std::path::Path) -> usize {
        let out = Command::new(bins().join("pen"))
            .args(["list"])
            .env("PANTHEON_ROOT", root)
            .output()
            .unwrap();
        serde_json::from_slice::<serde_json::Value>(&out.stdout)
            .ok()
            .and_then(|v| v.as_array().map(Vec::len))
            .unwrap_or(0)
    }

    /// **The gate** (§16 step 6): a keystroke in the lens relays a write through a
    /// core's own verb, and the core writes it (I2, §12).
    ///
    /// Atrium links no core — the write crosses as JSON over `PATH` (I4, I5). Driven
    /// through the same loop a terminal drives, so what this asserts is what a hand
    /// gets.
    #[test]
    fn d_relays_a_write_through_pensum() {
        let root = fresh_root("relay");
        // The relay shells out by name, so the freshly built cores must be findable.
        let path = format!(
            "{}:{}",
            bins().display(),
            std::env::var("PATH").unwrap_or_default()
        );
        // SAFETY: single-threaded test setup, before any relay reads PATH.
        unsafe { std::env::set_var("PATH", path) };

        assert_eq!(open_tasks(&root), 2, "two tasks to start");
        // `2` switches to the agenda (a Full view, so motion goes to its rows), `d`
        // marks the focused one done.
        porticus::drive(
            &mut Atrium { root: root.clone() },
            &root,
            &porticus::keys("2d"),
            80,
            20,
        )
        .unwrap();
        assert_eq!(
            open_tasks(&root),
            1,
            "`d` must relay `pen edit … --done -y` and the core must write it"
        );
    }
}
