//! The screen: Studium as a Porticus app (P§2, §19).
//!
//! Everything here rides the `tui` feature — a headless lens keeps the folds and drops the
//! chrome (§12, §14), so nothing in this file may be reachable without it.
//!
//! The lineup is §19.6's: the **mosaic**, then **courses**, **tasks**, **deadlines**,
//! **study** time, **people**, and **reflections** — seven folds over records six cores
//! already own, and Studium mints none of them (I1).
//!
//! **All five of §19.8's relays are here, and one `on_action` carries them**: mark a task
//! done, record a grade, close an enrolment, log study time, place an occurrence. That is
//! possible because the verb grammar is the shared one (§7.2) — only the *core* differs,
//! and nothing guesses it. A row was stamped by the fold that built it and a new record
//! takes its view's declaration (G8), so each arm reads the core off the address. Every
//! write is a hand's, relayed to the command a hand would type (I2), with `-C`/`-y` from
//! Porticus's own confirm (P-II) and never authored here.
//!
//! **People and reflections stay read-only on purpose.** §19.8 lists no relay for either,
//! and a person is Album's to edit — you open `alb` (I4, I5). A lens that grew an `e`
//! there would be inventing a write the spec does not have.

use pantheon::Code;
use porticus::view::{Layout, Row, View, ViewId};
use porticus::views::Agenda;
use porticus::{Action, App, FieldSpec, Ident, Invocation, RecordRef, Target, Writer};
use serde_json::Value;

use crate::mosaic::Mosaic;
use crate::scope::{Scope, Studies, Switch};

/// The cores Studium reaches (§19.6). Discovered, never required: a figure whose core is
/// off `PATH` is absent, and so is the relay that would have written to it (§12).
pub(crate) const PENSUM: &str = "pen";
pub(crate) const ANNALES: &str = "ann";
const FASTI: &str = "fas";
const ALBUM: &str = "alb";
const TABELLA: &str = "tab";

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
        // The scope is discovered once per launch and shared by every view (N2, §19.4).
        // Nothing stores the choice: a fresh launch opens on all the studies again.
        let scope: Scope = std::rc::Rc::new(std::cell::RefCell::new(Studies::discover(&self.root)));
        let folds = |root: &std::path::PathBuf| (root.clone(), Scope::clone(&scope));
        let (task_root, task_scope) = folds(&self.root);
        let (due_root, due_scope) = folds(&self.root);
        let (study_root, study_scope) = folds(&self.root);
        vec![
            // A lens leads with its mosaic — the dashboard, not the tree (P§3).
            Box::new(Switch::of(Mosaic::of(&self.root, &scope), &scope)),
            // The enrolments in scope, each with its grade and its placement folded from
            // the records beside it (§19.2, §19.5). `a` records a grade, `e` closes the
            // enrolment (§19.8).
            Box::new(Switch::of(Courses::of(&self.root), &scope)),
            // The day's tasks **within the active programme**, each row carrying its own
            // home so the list spans nodes and each `d` relays to the right one (§19.6,
            // P§7). Tree-wide before N2, which is why a study screen showed the shopping.
            Box::new(Switch::of(
                Agenda::of(move || tasks(&task_root, &task_scope))
                    .offering(&[Action::Done, Action::Remove])
                    .empty("nothing open")
                    .in_core(PENSUM),
                &scope,
            )),
            // **Deadlines & exams** (§19.6): the Fasti occurrences in the next four weeks.
            // `a` places a new one — the §19.8 relay that was never wired.
            Box::new(Switch::of(
                Agenda::of(move || upcoming(&due_root, &due_scope))
                    .offering(&[Action::Add])
                    .empty("nothing in the next four weeks")
                    .in_core(FASTI)
                    .called("deadlines")
                    .with_form(occurrence_form()),
                &scope,
            )),
            // **Study time** (§19.6): the hours given, one dated line per session. `a`
            // logs another — the second §19.8 relay that was never wired.
            Box::new(Switch::of(
                Agenda::of(move || sessions(&study_root, &study_scope))
                    .offering(&[Action::Add])
                    .empty("no hours logged")
                    .in_core(ANNALES)
                    .called("study")
                    .with_form(session_form()),
                &scope,
            )),
            // **People** and **Reflections** (§19.6) — folds over what a study life's
            // records point at and what it wrote about itself. Read-only: §19.8 lists no
            // relay for either, and a person is Album's to edit, in `alb` (I5).
            Box::new(Switch::of(People::of(&self.root, &scope), &scope)),
            Box::new(Switch::of(Reflections::of(&self.root, &scope), &scope)),
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
        vec![PENSUM.to_string(), ANNALES.to_string(), FASTI.to_string()]
    }

    /// The grade-recording form (§19.8): `ann <course> <grade> <credits> --at <date>`.
    ///
    /// Porticus renders the fields and assembles the invocation from the base
    /// [`on_action`](App::on_action) gives it, so Annales still authors the write (I2) and
    /// owns which fields exist (I5).
    ///
    /// The **first** grade on a course needs the log minted first — `add` fills a
    /// container and never mints one (§7.3) — which is why recording one was deferred to
    /// a typed command. The `new course log` switch is that mint, made explicit: the hand
    /// says so, rather than the lens inferring it by checking whether the log exists and
    /// silently creating one on a typo (I2, §18 keeps no undo).
    fn add_form(&self) -> Vec<FieldSpec> {
        vec![
            FieldSpec::positional("course"),
            FieldSpec::positional("grade"),
            FieldSpec::positional("credits"),
            FieldSpec::field("date", "--at"),
            FieldSpec::switch("new course log", "-c"),
        ]
    }

    fn on_action(&mut self, action: Action, target: &Target) -> Option<Invocation> {
        // Only the app knows its verb grammar, because only the app authors the write
        // (I2). Porticus owns the confirm and the relay and knows none of this.
        //
        // **Which core is the target's to say, not this function's to guess** (G8). A row
        // was stamped by the fold that built it; a new record takes the view's own
        // declaration. So each arm reads the core off the address and the grammar is the
        // shared one (§7.2) — which is what lets one `on_action` serve five tabs.
        match (action, target) {
            // Record a grade (`ann`), log study time (`ann`), place an occurrence (`fas`)
            // — three records, one verb, and the tab's own form fills the rest (§19.8).
            (Action::Add, Target::Node { node, core, .. }) => Some(Invocation::new(
                core.clone().unwrap_or_else(|| ANNALES.to_owned()),
                ["add", "-H", node.as_str()],
            )),
            // Close an enrolment — `fas edit <span> --to <date>`, the date typed into
            // Porticus's line prompt and appended after the flag (§19.8, P§5).
            (
                Action::Edit,
                Target::Row(RecordRef {
                    home,
                    key,
                    core: Some(core),
                }),
            ) if core == FASTI => Some(Invocation::new(
                FASTI,
                ["edit", "-H", home.as_str(), key, "--to"],
            )),
            // Mark a task done — the Atrium relay unchanged (§19.8, §7.2).
            (Action::Done, Target::Row(RecordRef { home, key, .. })) => Some(Invocation::new(
                PENSUM,
                ["edit", "-H", home.as_str(), key, "--done"],
            )),
            (Action::Remove, Target::Row(RecordRef { home, key, core })) => Some(Invocation::new(
                core.clone().unwrap_or_else(|| PENSUM.to_owned()),
                ["rm", "-H", home.as_str(), key],
            )),
            _ => None,
        }
    }
}

/// Placing an occurrence — `fas add <series> <what> -a <date> -r fasti:<course>` (§19.8,
/// §8.4).
///
/// The **`concerns`** field is what makes an exam an exam: §8.4 gives an event no kind
/// beyond `event`, so what ties a sitting to its enrolment is the reference, and a hand
/// writes it rather than the lens inferring one (I5). The `new series` switch is the same
/// discipline as the grade form's: a minting write is the hand's word, never a lens's
/// guess at whether a container exists (§7.3, §18 keeps no undo).
fn occurrence_form() -> Vec<FieldSpec> {
    vec![
        FieldSpec::positional("series"),
        FieldSpec::positional("what"),
        FieldSpec::field("date", "-a"),
        FieldSpec::field("concerns", "-r"),
        FieldSpec::switch("new event series", "-c"),
    ]
}

/// Logging study time — `ann <log> <hours> --at <date>` (§19.8, §8.6).
fn session_form() -> Vec<FieldSpec> {
    vec![
        FieldSpec::positional("log"),
        FieldSpec::positional("hours"),
        FieldSpec::field("date", "--at"),
        FieldSpec::switch("new log", "-c"),
    ]
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

    fn actions(&self) -> &[Action] {
        // `a` records or re-marks a grade against the enrolment at the cursor's node; `e`
        // closes the enrolment under the cursor — the day the course ended (§19.8).
        &[Action::Add, Action::Edit]
    }

    fn prompts_for(&self, action: Action) -> Option<&'static str> {
        // Closing an enrolment has nothing to say until the day is typed (P§5). The line
        // is appended after `--to`, so the relay is the command a hand would write.
        (action == Action::Edit).then_some("closed on (yyyymmdd)")
    }

    fn rows(&mut self, node: &Code) -> Option<Vec<Row>> {
        let spans = tessera::read(
            &self.root,
            FASTI,
            &["list", "-k", "span", "-H", node.as_str()],
        )
        .and_then(|v| v.as_array().cloned())
        .unwrap_or_default();

        // The calendar a placement is read against, walked once for the whole fold
        // (§19.3, §19.5) — never per row, and never cached past this frame (I1).
        let curricula = crate::curriculum::discover(&self.root);
        let mut programme_starts = std::collections::HashMap::new();
        Some(
            spans
                .iter()
                .filter_map(|span| self.course_row(span, &curricula, &mut programme_starts))
                .collect(),
        )
    }

    fn empty_line(&self) -> &'static str {
        "no enrolments here"
    }
}

impl Courses {
    fn course_row(
        &self,
        span: &Value,
        curricula: &[(Code, crate::curriculum::Curriculum)],
        programme_starts: &mut std::collections::HashMap<String, Option<String>>,
    ) -> Option<Row> {
        let slug = span["slug"].as_str()?;
        let home = Code::parse(span["home"].as_str()?).ok()?;
        let from = span["data"]["from"].as_str().unwrap_or("");
        let to = span["data"]["to"].as_str();
        let dates = match to {
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
        // Where it sat (§19.5) — derived from the interval against the governing
        // curriculum's anchors, never stored on the span (I1, §8.4). A dash where no
        // calendar is declared or the course names no programme to count years from.
        let placement = self
            .placement_of(span, from, to, curricula, programme_starts)
            .map_or_else(|| "—".to_string(), |p| p.label);

        Some(Row {
            label: format!("{slug:<24}  {grade:<4}  {placement:<8}  {dates}"),
            // Stamped with Fasti, because `e` closes the *span* while `a` records an
            // Annales fact — one view, two cores, and the address is what tells them
            // apart (G8, I5).
            target: Target::Row(RecordRef::in_core(FASTI, home, slug.to_string())),
            when: None,
        })
    }

    /// The placement of one enrolment (§19.5).
    ///
    /// Two things it needs beyond the span: the curriculum governing its node (§19.3) and
    /// the **programme span's** start, which is what a study year is counted from. The
    /// programme is the span this one references (§19.1) — read from the edge, never from
    /// a directory — and each is read at most once per fold.
    fn placement_of(
        &self,
        span: &Value,
        from: &str,
        to: Option<&str>,
        curricula: &[(Code, crate::curriculum::Curriculum)],
        programme_starts: &mut std::collections::HashMap<String, Option<String>>,
    ) -> Option<crate::period::Placement> {
        let curriculum = crate::curriculum::governing(curricula, span["home"].as_str()?)?;
        let programme = span["refs"]
            .as_array()?
            .iter()
            .filter_map(Value::as_str)
            .find_map(|t| t.strip_prefix("fasti:"))?
            .to_owned();
        let started = programme_starts
            .entry(programme.clone())
            .or_insert_with(|| {
                tessera::read(&self.root, FASTI, &["get", &programme]).and_then(|s| {
                    s["data"]["from"]
                        .as_str()
                        .map(str::to_owned)
                        .or_else(|| s["from"].as_str().map(str::to_owned))
                })
            })
            .clone()?;
        crate::period::placement(curriculum, &started, from, to)
    }
}

/// The open tasks **in the active programme's subtree**, as rows (§19.6, N2).
///
/// Read off `pen list`'s JSON and nothing else — the contract is the only thing that
/// crosses (I4). Each row keeps the home the core reported, which is what lets a
/// cross-node list relay each `d` to its own node (P§7).
///
/// The scope is `-H`, the same lever the CLI takes (§6.3): with no programme active it
/// is absent and the list is every open task, which is what this always was.
/// The occurrences in the next four weeks (§19.6) — "deadlines & exams", as rows.
///
/// The window is four weeks because the tab answers *what is coming*: a timeline with next
/// year's re-exam on it answers nothing. Each row carries its own home and its core, so
/// the list spans nodes and every relay reaches the right binary (P§7, G8).
fn upcoming(root: &std::path::Path, scope: &Scope) -> Vec<Row> {
    let home = scope.borrow().home().map(|code| code.as_str().to_owned());
    crate::fold::upcoming(root, home.as_deref(), &today(), 28)
        .into_iter()
        .filter_map(|event| {
            let at = Code::parse(&event.home).ok()?;
            let concerns = event
                .course
                .map_or_else(String::new, |c| format!("   {}", porticus::prettify(&c)));
            Some(Row {
                label: format!(
                    "{}   {}{concerns}",
                    porticus::prettify(&event.series),
                    porticus::prettify(&event.label)
                ),
                target: Target::Row(RecordRef::in_core(FASTI, at, event.series)),
                when: Some(event.date),
            })
        })
        .collect()
}

/// The study-time readings in scope (§19.6, §8.6) — the hours given, one row per session.
fn sessions(root: &std::path::Path, scope: &Scope) -> Vec<Row> {
    let home = scope.borrow().home().map(|code| code.as_str().to_owned());
    crate::fold::sessions(root, home.as_deref())
        .into_iter()
        .filter_map(|session| {
            let at = Code::parse(&session.home).ok()?;
            Some(Row {
                label: format!(
                    "{:<24}  {} h",
                    porticus::prettify(&session.log),
                    session.hours
                ),
                target: Target::Row(RecordRef::in_core(ANNALES, at, session.log)),
                when: Some(session.date),
            })
        })
        .collect()
}

/// Today as `YYYYMMDD` (§5.4) — the clock, read where "what is coming" needs it and nowhere
/// else (§19.4).
fn today() -> String {
    jiff::Zoned::now().strftime("%Y%m%d").to_string()
}

fn tasks(root: &std::path::Path, scope: &Scope) -> Vec<Row> {
    let home = scope.borrow().home().map(|code| code.as_str().to_owned());
    let mut args = vec!["list"];
    if let Some(home) = home.as_deref() {
        args.extend_from_slice(&["-H", home]);
    }
    let Some(Value::Array(rows)) = tessera::read(root, PENSUM, &args) else {
        return Vec::new();
    };
    let labels = porticus::node_labels(root);
    rows.iter()
        .filter_map(|row| {
            let key = row["key"].as_str()?;
            let home = Code::parse(row["home"].as_str()?).ok()?;
            // Node-first, the task de-underscored — the same row shape Pensum shows, so
            // the agenda reads identically wherever it appears (P1, I3).
            let node = labels
                .get(home.as_str())
                .map_or_else(|| home.as_str().to_owned(), Clone::clone);
            Some(Row {
                label: format!("{node}   {}", porticus::prettify(key)),
                target: Target::Row(RecordRef::new(home, key.to_string())),
                when: row["data"]["done"].as_str().map(str::to_owned),
            })
        })
        .collect()
}

/// **People** (§19.6, §8.1) — the ones a study life's records point at.
///
/// A fold over *references*, never a directory: §19.6 says contacts are "the people a
/// course's records point at", so a professor is in the studies exactly when something in
/// scope names them, and nothing is copied under a course (I3).
///
/// Read-only. §19.8 lists no relay for a person, and editing one is Album's — you open
/// `alb` (I4, I5). A lens that offered `e` here would be inventing a write the spec does
/// not have.
struct People {
    root: std::path::PathBuf,
    scope: Scope,
}

impl People {
    fn of(root: &std::path::Path, scope: &Scope) -> Self {
        Self {
            root: root.to_path_buf(),
            scope: Scope::clone(scope),
        }
    }
}

impl View for People {
    fn id(&self) -> ViewId {
        "people"
    }

    fn layout(&self) -> Layout {
        // Cross-node, like every other fold here — the scope is the programme, not the
        // tree cursor (§19.4).
        Layout::Full
    }

    fn rows(&mut self, _node: &Code) -> Option<Vec<Row>> {
        let home = self
            .scope
            .borrow()
            .home()
            .map(|code| code.as_str().to_owned());
        Some(
            crate::fold::people(&self.root, home.as_deref())
                .iter()
                .filter_map(|person| {
                    let slug = person["slug"].as_str()?;
                    let at = Code::parse(person["home"].as_str()?).ok()?;
                    let kind = person["kind"].as_str().unwrap_or("");
                    Some(Row {
                        label: format!("{:<28}  {kind}", porticus::prettify(slug)),
                        target: Target::Row(RecordRef::in_core(ALBUM, at, slug.to_string())),
                        when: None,
                    })
                })
                .collect(),
        )
    }

    fn locator(&self) -> Option<String> {
        Some("who this points at".into())
    }

    fn empty_line(&self) -> &'static str {
        "nobody referenced here"
    }
}

/// **Reflections** (§19.6, §8.7) — the Documents a study life wrote about itself.
///
/// Folded from frontmatter and no further: a fold never reads bodies (§6.1, §8.7), so the
/// list shows what the fence declares and opening one is `tab`'s.
struct Reflections {
    root: std::path::PathBuf,
    scope: Scope,
}

impl Reflections {
    fn of(root: &std::path::Path, scope: &Scope) -> Self {
        Self {
            root: root.to_path_buf(),
            scope: Scope::clone(scope),
        }
    }
}

impl View for Reflections {
    fn id(&self) -> ViewId {
        "reflections"
    }

    fn layout(&self) -> Layout {
        Layout::Full
    }

    fn rows(&mut self, _node: &Code) -> Option<Vec<Row>> {
        let home = self
            .scope
            .borrow()
            .home()
            .map(|code| code.as_str().to_owned());
        Some(
            crate::fold::reflections(&self.root, home.as_deref())
                .iter()
                .filter_map(|doc| {
                    let slug = doc["slug"].as_str()?;
                    let at = Code::parse(doc["home"].as_str()?).ok()?;
                    let tags = doc["tags"]
                        .as_array()
                        .map(|t| {
                            t.iter()
                                .filter_map(Value::as_str)
                                .collect::<Vec<_>>()
                                .join(", ")
                        })
                        .unwrap_or_default();
                    Some(Row {
                        label: format!("{:<32}  {tags}", porticus::prettify(slug)),
                        target: Target::Row(RecordRef::in_core(TABELLA, at, slug.to_string())),
                        when: None,
                    })
                })
                .collect(),
        )
    }

    fn locator(&self) -> Option<String> {
        Some("what it wrote about itself".into())
    }

    fn empty_line(&self) -> &'static str {
        "nothing written yet"
    }
}
