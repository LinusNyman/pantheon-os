//! The screen: Atrium as a Porticus app (P§2).
//!
//! Everything here rides the `tui` feature — a headless lens keeps the folds and drops
//! the chrome (§12, §14), so nothing in this file may be reachable without it.

use pantheon::Code;
use porticus::view::Row;
use porticus::views::{Agenda, TreeFile};
use porticus::{Action, App, Ident, Invocation, RecordRef, Target, View, Writer};
use serde_json::Value;

use crate::cli::{ALBUM, PENSUM, TABELLA};
use crate::mosaic::Mosaic;

/// The actions a lens offers on a record it folded (N1, §12).
///
/// §12 permits a lens to **relay** a human-initiated write, and that permission was
/// never narrow — what made Atrium's reach narrow was this list, three verbs on one
/// core. A relay is the same command a hand would type, so the standard set is the
/// standard set wherever the row came from: only `Done` is Pensum's alone, because only
/// a task has something to toggle (§8.5).
const ON_A_RECORD: &[Action] = &[
    Action::Edit,
    Action::Remove,
    Action::Rename,
    Action::Move,
    Action::Add,
    Action::QuickAdd,
];

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
        let for_people = self.root.clone();
        let for_documents = self.root.clone();
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
            // and each `d` relays to the right one (P§3, P§7). A Full view has no
            // visible tree cursor, so a new task is added through `A`'s pick-a-home
            // modal rather than `a`'s invisible one (P§4).
            Box::new(
                Agenda::of(move || tasks(&for_agenda))
                    .offering(&[
                        Action::Done,
                        Action::Edit,
                        Action::Remove,
                        Action::Rename,
                        Action::Move,
                        Action::QuickAdd,
                    ])
                    .in_core(PENSUM)
                    .empty("nothing open today"),
            ),
            // The people and the documents at the node the rail is on — the other two
            // cores the hearth already counted, now browsable and writable through the
            // same standard actions (N1, §12). Each declares its core, so `a` here mints
            // through `alb` and `a` there through `tab` (P§7).
            Box::new(
                TreeFile::of(move |node: &Code| entities(&for_people, ALBUM, node))
                    .called("people")
                    .in_core(ALBUM)
                    .offering(ON_A_RECORD)
                    .empty("nobody filed here"),
            ),
            Box::new(
                TreeFile::of(move |node: &Code| entities(&for_documents, TABELLA, node))
                    .called("documents")
                    .in_core(TABELLA)
                    .offering(ON_A_RECORD)
                    .empty("nothing written here"),
            ),
        ]
    }

    fn count_at(&mut self, node: &Code) -> usize {
        // Atrium's items at a node are the open tasks there — folded, never stored (I1),
        // and **node-local** (`--here`), because the rail asks this of every visible node
        // and a subtree fold would re-read a branch once per ancestor (P§6).
        //
        // The day is what the hearth counts, so this stays one core's question even
        // though the lineup now browses three: a badge summing three cores would spawn
        // three children per visible node, per frame.
        tessera::read(&self.root, PENSUM, &["list", "-H", node.as_str(), "--here"])
            .and_then(|v| v.as_array().map(Vec::len))
            .unwrap_or(0)
    }

    fn writer(&self) -> Writer {
        // A lens shells out to the core binary on `PATH` (§12): it links no core (I5),
        // and the write crosses the JSON boundary like every other (I4).
        Writer::Subprocess
    }

    fn relays_to(&self) -> Vec<String> {
        // Every core the lineup writes to, so an absent one dims its actions before the
        // key is pressed rather than failing when tried (§12, P§7).
        vec![PENSUM.to_string(), ALBUM.to_string(), TABELLA.to_string()]
    }

    fn on_action(&mut self, action: Action, target: &Target) -> Option<Invocation> {
        // Only the app knows its verb grammar, because only the app authors the write
        // (I2). Porticus owns the confirm and the relay and knows none of this.
        //
        // **The grammar is the shared one** (§7.2): `<short> <verb> -H <home> <key>` is
        // every core's, which is what lets one mapping serve three. What the lens must
        // still know is *which* core — a row says so itself, a new record's view says it
        // for it (P§7), and nothing here guesses.
        match target {
            Target::Row(RecordRef { home, key, core }) => {
                let short = core.as_deref()?;
                let home = home.as_str();
                match action {
                    // Only a task has something to toggle; the key is dark elsewhere by
                    // the view's own offering, and refused here too (§8.5, P§5).
                    Action::Done => (short == PENSUM)
                        .then(|| Invocation::new(PENSUM, ["edit", "-H", home, key, "--done"])),
                    // No value inline is the editor form (§7.3): the record opens in the
                    // hand's own editor and the session is the confirm.
                    Action::Edit => Some(Invocation::new(short, ["edit", "-H", home, key])),
                    Action::Remove => Some(Invocation::new(short, ["rm", "-H", home, key])),
                    // Porticus appends the typed name after its own prompt (P§5).
                    Action::Rename => Some(Invocation::new(short, ["rename", "-H", home, key])),
                    Action::Move => Some(Invocation::new(short, ["move", "-H", home, key, "--to"])),
                    _ => None,
                }
            }
            // A **new** record is still the core's to create — the lens only carries the
            // hand's ask to it (I2, §12). The form's fields are appended by Porticus.
            Target::Node { node, core, .. } => matches!(action, Action::Add | Action::QuickAdd)
                .then(|| {
                    core.as_deref()
                        .map(|short| Invocation::new(short, ["add", "-H", node.as_str()]))
                })
                .flatten(),
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
    let labels = porticus::node_labels(root);
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
            // Node-first, the task de-underscored — the same row shape Pensum shows, so
            // the agenda reads identically wherever it appears (P1, I3).
            let node = labels
                .get(home.as_str())
                .map_or_else(|| home.as_str().to_owned(), Clone::clone);
            Some(Row {
                label: format!("{node}   {}{refs}", porticus::prettify(key)),
                // Stamped with the core it was folded from, so `d` routes by provenance
                // even on a list the hearth now shares with two other cores (P§7).
                target: Target::Row(RecordRef::in_core(PENSUM, home, key.to_string())),
                when: row["data"]["done"].as_str().map(str::to_owned),
            })
        })
        .collect()
}

/// The records one core holds **at a node**, as rows (N1, §12).
///
/// Read off that core's `list --here` and nothing else — the contract is the only thing
/// that crosses (I4), and one fold serves every core because `list`'s envelope is the
/// shared one (§7.2). Node-local, because a Rail view is *about the selected node* and
/// the tree beside it is how you reach the rest (P§6).
///
/// Each row is stamped with the core it came from, so a relay on it routes by provenance
/// rather than by whichever view happened to be active (P§7).
fn entities(root: &std::path::Path, short: &'static str, node: &Code) -> Vec<Row> {
    let Some(Value::Array(rows)) =
        tessera::read(root, short, &["list", "-H", node.as_str(), "--here"])
    else {
        return Vec::new();
    };
    rows.iter()
        .filter_map(|row| {
            // A partitioned entity is named by `slug`, a series line by `key` (§5.4,
            // §7.2) — the lens folds both shapes, so it asks for either.
            let key = row["slug"].as_str().or_else(|| row["key"].as_str())?;
            let home = Code::parse(row["home"].as_str()?).ok()?;
            let kind = row["kind"].as_str().unwrap_or("");
            Some(Row {
                // De-underscored for reading, keyed on the real slug for the write (P1).
                label: format!("{}   {kind}", porticus::prettify(key)),
                target: Target::Row(RecordRef::in_core(short, home, key.to_string())),
                when: None,
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
