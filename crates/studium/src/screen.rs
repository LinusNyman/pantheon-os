//! The screen: Studium as a Porticus app (P§2, §19).
//!
//! Everything here rides the `tui` feature — a headless lens keeps the folds and drops the
//! chrome (§12, §14), so nothing in this file may be reachable without it.
//!
//! The lineup leads with the **mosaic** (P§3), then a browsable **courses** list and the
//! day's **tasks**. The two relays §19.8 asks for the MVP to carry — mark a task done and
//! record/re-mark a grade — are the only writes, each the same verb a hand would type
//! (I2), each carrying `-C`/`-y` from Porticus's own confirm (P-II), never authored here.

use pantheon::Code;
use porticus::view::{Layout, Row, View, ViewId};
use porticus::views::Agenda;
use porticus::{Action, App, FieldSpec, Ident, Invocation, RecordRef, Target, Writer};
use serde_json::Value;

use crate::mosaic::Mosaic;

/// The cores Studium reaches (§19.6). Discovered, never required: a figure whose core is
/// off `PATH` is absent, and so is the relay that would have written to it (§12).
pub(crate) const PENSUM: &str = "pen";
pub(crate) const ANNALES: &str = "ann";
const FASTI: &str = "fas";

/// Open the mosaic.
///
/// # Errors
/// If the tree cannot be walked or the terminal cannot be taken.
pub fn open(root: &std::path::Path) -> anyhow::Result<()> {
    porticus::run(&mut Studium::new(root), root)
}

/// The root the screen is drawing.
///
/// Held rather than left to `$PANTHEON_ROOT`: a lens opened with `-C` must fold the tree
/// it was pointed at, not the caller's ambient one (§6.2, §7.3).
pub struct Studium {
    root: std::path::PathBuf,
}

impl Studium {
    /// Public so a test can build the **real** lens and drive it — the same object `open`
    /// runs, with the same folds and the same **subprocess** relay, so a driven write
    /// crosses the JSON boundary exactly as it does in a hand's terminal (I4, I5, §12).
    #[must_use]
    pub fn new(root: &std::path::Path) -> Self {
        Self {
            root: root.to_path_buf(),
        }
    }
}

impl App for Studium {
    fn ident(&self) -> Ident {
        Ident {
            name: "studium",
            short: "stu",
            tagline: "the studies",
            symbol: '✎',
            accent: porticus::ident::accent::LAPIS,
        }
    }

    fn lineup(&mut self) -> Vec<Box<dyn View>> {
        let for_tasks = self.root.clone();
        vec![
            // A lens leads with its mosaic — the dashboard, not the tree (P§3).
            Box::new(Mosaic::of(&self.root)),
            // The enrolments in scope, each with its grade folded from the paired log
            // (§19.2). Read-only for the MVP: a course's writes (close it, grade it) are
            // the relays below, reached with `a`.
            Box::new(Courses::of(&self.root)),
            // The day's tasks across the tree, each row carrying its own home so the list
            // spans nodes and each `d` relays to the right one (§19.6, P§7).
            Box::new(
                Agenda::of(move || tasks(&for_tasks))
                    .offering(&[Action::Done, Action::Remove])
                    .empty("nothing open"),
            ),
        ]
    }

    fn count_at(&mut self, node: &Code) -> usize {
        // Studium's items at a node are the enrolment spans there — folded, never
        // stored (I1).
        tessera::read(
            &self.root,
            FASTI,
            &["list", "-k", "span", "-H", node.as_str()],
        )
        .and_then(|v| v.as_array().map(Vec::len))
        .unwrap_or(0)
    }

    fn writer(&self) -> Writer {
        // A lens shells out to the core binary on `PATH` (§12): it links no core (I5),
        // and the write crosses the JSON boundary like every other (I4).
        Writer::Subprocess
    }

    fn relays_to(&self) -> Vec<String> {
        vec![PENSUM.to_string(), ANNALES.to_string()]
    }

    /// The grade-recording form (§19.8): `ann <course> <grade> <credits> --at <date>`.
    ///
    /// Porticus renders the fields and assembles the invocation from the base
    /// [`on_action`](App::on_action) gives it, so Annales still authors the write (I2) and
    /// owns which fields exist (I5). The course names an **existing** log — a first grade's
    /// `-c` mint is deferred — so this records a retake or corrects a mark.
    fn add_form(&self) -> Vec<FieldSpec> {
        vec![
            FieldSpec {
                label: "course",
                flag: None,
                required: true,
            },
            FieldSpec {
                label: "grade",
                flag: None,
                required: true,
            },
            FieldSpec {
                label: "credits",
                flag: None,
                required: true,
            },
            FieldSpec::field("date", "--at"),
        ]
    }

    fn on_action(&mut self, action: Action, target: &Target) -> Option<Invocation> {
        // Only the app knows its verb grammar, because only the app authors the write
        // (I2). Porticus owns the confirm and the relay and knows none of this.
        match (action, target) {
            // Record or re-mark a grade — the add form's base, filled by the fields above
            // and homed at the node the form picked (§19.8).
            (Action::Add, Target::Node { node, .. }) => {
                Some(Invocation::new(ANNALES, ["add", "-H", node.as_str()]))
            }
            // Mark a task done — the Atrium relay unchanged (§19.8, §7.2).
            (Action::Done, Target::Row(RecordRef { home, key })) => Some(Invocation::new(
                PENSUM,
                ["edit", "-H", home.as_str(), key, "--done"],
            )),
            (Action::Remove, Target::Row(RecordRef { home, key })) => {
                Some(Invocation::new(PENSUM, ["rm", "-H", home.as_str(), key]))
            }
            _ => None,
        }
    }
}

/// The enrolments at a node, each with its grade folded from the paired Annales log
/// (§19.1, §19.2). A **Rail row-view**: about the selected node, browsable, read-only.
struct Courses {
    root: std::path::PathBuf,
}

impl Courses {
    fn of(root: &std::path::Path) -> Self {
        Self {
            root: root.to_path_buf(),
        }
    }
}

impl View for Courses {
    fn id(&self) -> ViewId {
        "courses"
    }

    fn layout(&self) -> Layout {
        Layout::Rail
    }

    fn rows(&mut self, node: &Code) -> Option<Vec<Row>> {
        let spans = tessera::read(
            &self.root,
            FASTI,
            &["list", "-k", "span", "-H", node.as_str()],
        )
        .and_then(|v| v.as_array().cloned())
        .unwrap_or_default();

        Some(
            spans
                .iter()
                .filter_map(|span| self.course_row(span))
                .collect(),
        )
    }

    fn empty_line(&self) -> &'static str {
        "no enrolments here"
    }
}

impl Courses {
    fn course_row(&self, span: &Value) -> Option<Row> {
        let slug = span["slug"].as_str()?;
        let home = Code::parse(span["home"].as_str()?).ok()?;
        let from = span["data"]["from"].as_str().unwrap_or("");
        let period = match span["data"]["to"].as_str() {
            Some(to) => format!("{from}–{to}"),
            None => format!("{from}–   open"),
        };
        // The grade is a fact paired by slug (§19.2): the log's present, folded on sight.
        let grade = tessera::read(&self.root, ANNALES, &["get", slug])
            .and_then(|line| {
                line["data"]["values"]
                    .as_array()
                    .and_then(|v| v.first())
                    .and_then(Value::as_str)
                    .map(str::to_owned)
            })
            .unwrap_or_else(|| "—".to_string());

        Some(Row {
            label: format!("{slug:<24}  {grade:<4}  {period}"),
            target: Target::Row(RecordRef {
                home,
                key: slug.to_string(),
            }),
            when: None,
        })
    }
}

/// The open tasks across the whole tree, as rows (§19.6).
///
/// Read off `pen list`'s JSON and nothing else — the contract is the only thing that
/// crosses (I4). Each row keeps the home the core reported, which is what lets a
/// cross-node list relay each `d` to its own node (P§7).
fn tasks(root: &std::path::Path) -> Vec<Row> {
    let Some(Value::Array(rows)) = tessera::read(root, PENSUM, &["list"]) else {
        return Vec::new();
    };
    rows.iter()
        .filter_map(|row| {
            let key = row["key"].as_str()?;
            let home = Code::parse(row["home"].as_str()?).ok()?;
            Some(Row {
                label: format!("{key}   {}", home.as_str()),
                target: Target::Row(RecordRef {
                    home,
                    key: key.to_string(),
                }),
                when: row["data"]["done"].as_str().map(str::to_owned),
            })
        })
        .collect()
}
