//! `pan`'s structural TUI (§10). Rides the `tui` feature (§14).
//!
//! **`pan` never touches `data`.** It works one layer down, on the tree itself: codes,
//! files, refs, node annotations (§10). So its two tabs are bespoke rather than catalog
//! views — the catalog renders *records*, and `pan` has none to render (P§3).
//!
//! Two tabs, as §10 names them: the tree browser and the validate findings. Annotate is
//! an action on the selected node (§10.3), not a third tab.
//!
//! # Keys and what stays dark
//!
//! The node cascade (§10.1) is built, so `r` renames the selected node's label, `x`
//! removes it (refused by the spine if it is not empty), and `m` re-homes it — its
//! destination picked off Porticus's tree modal (P§4) rather than typed as a code, since
//! a move names a *node* and the tree is where nodes are legible. `a` (add a child) is
//! not offered here; a child is minted with `pan new`. The bulk repairs
//! (`rename-prefix`, `rename-pattern`, `mv-file`) are CLI verbs with no key of their own.

use std::ffi::OsString;

use clap::Parser;
use pantheon::validate::{Finding, Severity};
use pantheon::{
    Annotations, Code, CoreRegistry, FileClass, build_tree, classify, read_annotations,
    resolve_code, validate,
};
use porticus::action::{Invocation, Relayed};
use porticus::view::{Layout, Row, View, ViewId};
use porticus::{Action, App, Handled, Ident, Nav, RecordRef, Target, Theme, Writer};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget, Wrap};

use crate::cli::{Cli, RunOk};

/// Open `pan`'s screen.
///
/// # Errors
/// If the tree cannot be walked or the terminal cannot be taken.
pub fn open(root: &std::path::Path) -> anyhow::Result<()> {
    porticus::run(&mut PanApp::new(root), root)
}

/// `pan`'s screen, as an `App` (P§2).
///
/// Public so a test can build the **real** one and drive it — the same object `open`
/// runs, with the same two tabs and the same in-process relay.
pub struct PanApp {
    root: std::path::PathBuf,
}

impl PanApp {
    #[must_use]
    pub fn new(root: &std::path::Path) -> Self {
        Self {
            root: root.to_path_buf(),
        }
    }
}

impl App for PanApp {
    fn ident(&self) -> Ident {
        Ident {
            name: "pantheon",
            short: "pan",
            tagline: "the frame",
            symbol: '✶',
            accent: porticus::ident::accent::STONE,
        }
    }

    fn lineup(&mut self) -> Vec<Box<dyn View>> {
        vec![
            Box::new(TreeTab {
                root: self.root.clone(),
            }),
            Box::new(ValidateTab {
                root: self.root.clone(),
            }),
        ]
    }

    fn count_at(&mut self, node: &Code) -> usize {
        // `pan`'s items at a node are its **record files** (§10.1's record count) — the
        // layer it works on. Counted on the frame it is shown, kept nowhere (I1).
        record_files(&self.root, node).len()
    }

    fn writer(&self) -> Writer {
        Writer::InProcess
    }

    fn execute(&mut self, invocation: &Invocation) -> std::io::Result<Relayed> {
        Ok(in_process(invocation))
    }

    fn on_action(&mut self, action: Action, target: &Target) -> Option<Invocation> {
        match (action, target) {
            // ── the tree tab: actions on the selected node ──
            // `annotate` is a node-level write (§5.5, §10.3); the typed `key=value` is
            // appended by Porticus after the prompt.
            (Action::Edit, Target::Node { node, .. }) => {
                Some(Invocation::new("pan", ["annotate", node.as_str(), "--set"]))
            }
            // `r` renames the node's label (Porticus appends the typed label after its
            // rename prompt); a definition-prefix node needs `--def` and says so (§10.1).
            (Action::Rename, Target::Node { node, .. }) => {
                Some(Invocation::new("pan", ["rename", node.as_str(), "--label"]))
            }
            // `x` removes the node — refused by the spine if it is not empty (§10.1).
            (Action::Remove, Target::Node { node, .. }) => {
                Some(Invocation::new("pan", ["rm", node.as_str()]))
            }
            // `m` re-homes the node under a new parent (§10.1). The destination is the
            // node picked in Porticus's tree modal, appended after `--to` — a move names
            // a *node*, and picking one off the tree is how you name a node without
            // spelling its code (P§4).
            (Action::Move, Target::Node { node, .. }) => {
                Some(Invocation::new("pan", ["mv", node.as_str(), "--to"]))
            }
            // ── the validate tab: `d` applies the fix on the focused row ──
            // The row carries the whole `pan …` command as its key, so this relays it
            // verbatim (§10.2) and stays blind to which finding shape produced it. A
            // finding offering no fix carries a node target this arm does not match, so
            // `d` there is a no-op.
            (Action::Done, Target::Row(RecordRef { key, .. })) => fix_invocation(key),
            // Every other key is unoffered by the active view — dark, not faked (P§7).
            _ => None,
        }
    }
}

/// Run the invocation in-process, through the very code the CLI runs (P§7).
fn in_process(invocation: &Invocation) -> Relayed {
    let argv =
        std::iter::once(OsString::from("pan")).chain(invocation.args.iter().map(OsString::from));
    let cli = match Cli::try_parse_from(crate::cli::with_lookup_verb(argv)) {
        Ok(cli) => cli,
        Err(e) => {
            return Relayed {
                code: 2,
                stdout: String::new(),
                stderr: e.to_string(),
            };
        }
    };
    match crate::cli::run(&cli) {
        Ok(RunOk::Json(value)) => Relayed {
            code: 0,
            stdout: value.to_string(),
            stderr: String::new(),
        },
        Ok(RunOk::JsonExit(value, code)) => Relayed {
            code: i32::from(code),
            stdout: value.to_string(),
            stderr: String::new(),
        },
        Ok(RunOk::Raw(text)) => Relayed {
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

// ── the tree tab (§10.1) ─────────────────────────────────────────────────────

/// Browse the ontology. Left pane: the tree, which Porticus draws (P§6). Right pane:
/// the selected node's meta, and which cores have files there.
struct TreeTab {
    root: std::path::PathBuf,
}

impl View for TreeTab {
    fn id(&self) -> ViewId {
        "tree"
    }

    fn layout(&self) -> Layout {
        Layout::Rail
    }

    fn rows(&mut self, _node: &Code) -> Option<Vec<Row>> {
        // A draw-view: the right pane is a node's description, not a list of records —
        // `pan` has no records (§10).
        None
    }

    fn actions(&self) -> &[Action] {
        // `e` annotates, `r` renames the label, `x` removes an empty node, `m` re-homes
        // it against the tree modal (§10.1, §10.3, P§4). `a` (add a child) is still not
        // offered — the chrome has no child prompt, so that key stays dark.
        &[Action::Edit, Action::Rename, Action::Remove, Action::Move]
    }

    fn prompts_for(&self, action: Action) -> Option<&'static str> {
        // `pan annotate` says nothing until a `key=value` is typed (§5.5). `rename` gets
        // Porticus's own rename prompt, so it needs none here.
        (action == Action::Edit).then_some("annotate key=value")
    }

    fn empty_line(&self) -> &'static str {
        "no tree here — mint one with `pan new`"
    }

    fn draw(&mut self, node: &Code, area: Rect, buf: &mut Buffer, theme: Theme) {
        let mut lines = vec![Line::from(Span::styled(
            node.as_str().to_owned(),
            theme.name(),
        ))];

        // Position: children and records, the two counts §10.1 asks for.
        let children = child_count(&self.root, node);
        let records = record_files(&self.root, node);
        lines.push(Line::from(vec![
            Span::styled("children  ", theme.dim()),
            Span::styled(children.to_string(), theme.text()),
            Span::styled("   records  ", theme.dim()),
            Span::styled(records.len().to_string(), theme.text()),
        ]));
        lines.push(Line::from(String::new()));

        // The node's annotations — the one hand-written surface left in the system
        // (§6.6), read here and edited by `e`.
        let ann = read_annotations(&self.root, node).unwrap_or_else(|_| Annotations::default());
        for (label, value) in [
            ("symbol", ann.symbol.clone()),
            ("deity", ann.deity.clone()),
            ("explanation", ann.explanation.clone()),
        ] {
            if let Some(value) = value {
                lines.push(Line::from(vec![
                    Span::styled(format!("{label:<12}"), theme.dim()),
                    Span::styled(value, theme.text()),
                ]));
            }
        }
        if !ann.keywords.is_empty() {
            lines.push(Line::from(vec![
                Span::styled(format!("{:<12}", "keywords"), theme.dim()),
                Span::styled(ann.keywords.join(", "), theme.text()),
            ]));
        }

        // Which cores have files here (§10.1). Read off the **filenames** — a token
        // names its owning core, so this needs no core linked and none imported (I5,
        // §5.0). A token no installed core claims is named as such rather than hidden.
        let registry = CoreRegistry::discover();
        let mut cores: Vec<String> = Vec::new();
        for class in &records {
            let owner = match class {
                FileClass::Partitioned { kind, .. }
                | FileClass::EntityNode { kind, .. }
                | FileClass::NamedSeries { kind, .. }
                | FileClass::DeterminedSeries { kind, .. } => registry
                    .core_of_kind(kind)
                    .map_or_else(|| format!("{kind} (no installed core)"), |c| c.name.clone()),
                FileClass::Document { .. } => "tabella".to_owned(),
                _ => continue,
            };
            if !cores.contains(&owner) {
                cores.push(owner);
            }
        }
        if !cores.is_empty() {
            lines.push(Line::from(String::new()));
            lines.push(Line::from(vec![
                Span::styled(format!("{:<12}", "cores"), theme.dim()),
                Span::styled(cores.join(", "), theme.text()),
            ]));
        }

        Paragraph::new(lines)
            .style(theme.text())
            .wrap(Wrap { trim: false })
            .render(area, buf);
    }
}

// ── the validate tab (§10.2) ─────────────────────────────────────────────────

/// `pan validate`'s findings, browsable and **applicable** (§10.2).
///
/// A [`Finding`] carries either a single legal correction ([`Finding::fix`]) or, where the
/// choice is genuinely a hand's, several ([`Finding::candidates`]). Both become rows: a
/// fix rides on the finding's own row, and each candidate gets **a row of its own**
/// beneath it. So `d` never picks between choices — it applies the command on the line the
/// cursor is on, which is the whole of how a genuine choice is offered without the tools
/// making it (§10.2).
///
/// The command rides in the row's target as the record key, and `on_action` relays it
/// verbatim — so this tab teaches `pan` no fix shapes and a new one in the spine works
/// here the day it lands.
struct ValidateTab {
    root: std::path::PathBuf,
}

impl View for ValidateTab {
    fn id(&self) -> ViewId {
        "validate"
    }

    fn layout(&self) -> Layout {
        // Findings span the whole tree, so the cursor node means nothing to them (P§3).
        Layout::Full
    }

    fn rows(&mut self, _node: &Code) -> Option<Vec<Row>> {
        // Re-run on every refresh: validate is on-demand by design — nothing watches
        // the tree (§18, §5.5) — and a screen event is exactly a hand demanding it.
        let registry = CoreRegistry::discover();
        let findings = validate(&self.root, &registry).unwrap_or_default();
        Some(findings.iter().flat_map(rows_of).collect())
    }

    fn actions(&self) -> &[Action] {
        // `d` applies the correction on the focused row (§10.2). Distinct from the tree
        // tab's keys, so `on_action` can tell the two apart.
        &[Action::Done]
    }

    fn navigate(&mut self, _nav: Nav) -> Handled {
        Handled::No
    }

    fn locator(&self) -> Option<String> {
        Some("findings".into())
    }

    fn empty_line(&self) -> &'static str {
        // Clean is the good answer, and absence is calm (I7).
        "the tree is consistent"
    }
}

/// One finding as its rows: the finding itself, then one row per candidate where the
/// correction is a genuine choice (§10.2).
///
/// A row that carries a command holds it whole in its target, so `d` relays it without
/// this tab knowing a single fix shape. A row with no command targets a bare node the
/// apply arm does not match, which is what leaves `d` a no-op there.
fn rows_of(finding: &Finding) -> Vec<Row> {
    let mark = match finding.severity {
        Severity::Error => "error  ",
        Severity::Warning => "warning",
    };
    let base = format!("{mark}  {}  {}", finding.rel_path.display(), finding.msg);
    // Where the correction is unambiguous, show the command a hand reads before pressing
    // `d` to apply it (§10.2).
    let mut rows = vec![match &finding.fix {
        Some(fix) => Row {
            label: format!("{base}  →  {fix}"),
            target: fix_target(Some(fix)),
            when: None,
        },
        None => Row {
            label: base,
            target: fix_target(None),
            when: None,
        },
    }];
    // Each candidate is its own row: the cursor is how a hand picks, so nothing here
    // chooses and nothing is hidden behind a chooser Porticus does not have (P-II).
    for candidate in &finding.candidates {
        rows.push(Row {
            label: format!("          ·  {candidate}"),
            target: fix_target(Some(candidate)),
            when: None,
        });
    }
    rows
}

/// The row target for a correction: the whole `pan …` command, carried as the record key
/// so `on_action` can relay it verbatim (§10.2).
///
/// The home slot is unused by the relay and holds a legal placeholder — a `RecordRef`
/// always names a node, and a fix names a command. `None` (or a command not addressed to
/// `pan`, which this screen relays in-process and could not run) targets a bare node the
/// apply arm ignores.
fn fix_target(fix: Option<&String>) -> Target {
    let placeholder =
        Target::node(Code::parse("a").unwrap_or_else(|_| unreachable!("`a` is a legal code")));
    let Some(fix) = fix.filter(|f| f.starts_with("pan ")) else {
        return placeholder;
    };
    Target::Row(RecordRef::new(
        Code::parse("a").unwrap_or_else(|_| unreachable!("`a` is a legal code")),
        fix.clone(),
    ))
}

/// A carried fix command as the invocation that runs it: the words after `pan`.
///
/// Verbatim, because the spine computed it and `pan` is the tool that owns every fix
/// shape it emits (§10.2, §5.5). A row carrying no command never reaches here.
fn fix_invocation(command: &str) -> Option<Invocation> {
    let mut words = command.split_whitespace();
    if words.next() != Some("pan") {
        return None;
    }
    let args: Vec<String> = words.map(str::to_owned).collect();
    (!args.is_empty()).then(|| Invocation::new("pan", args))
}

// ── reading the layer `pan` works on ─────────────────────────────────────────

/// The record files at a node — the meta dir's records plus its loose documents.
///
/// Classified by **filename alone** (§5.2): the walker needs the extension and the
/// `__` split, never a token's meaning, so this asks no core anything (I5).
fn record_files(root: &std::path::Path, node: &Code) -> Vec<FileClass> {
    let Ok(dir) = resolve_code(root, node) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let meta = dir.join(format!("{}__", node.as_str()));
    for base in [meta, dir] {
        let Ok(entries) = std::fs::read_dir(&base) else {
            continue;
        };
        for entry in entries.flatten() {
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            let name = entry.file_name().to_string_lossy().into_owned();
            match classify(&name, kind.is_dir(), node) {
                FileClass::Partitioned { .. }
                | FileClass::EntityNode { .. }
                | FileClass::NamedSeries { .. }
                | FileClass::DeterminedSeries { .. }
                | FileClass::Document { .. } => {
                    out.push(classify(&name, kind.is_dir(), node));
                }
                _ => {}
            }
        }
    }
    out
}

/// How many child nodes a node has (§10.1).
fn child_count(root: &std::path::Path, node: &Code) -> usize {
    match build_tree(root, Some(node)) {
        Ok(pantheon::TreeRoot::Subtree(n)) => n.children.len(),
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::{PanApp, TreeTab, rows_of};
    use pantheon::validate::{Finding, FindingCode, Severity};
    use pantheon::{NewSpec, plan_new, read_annotations};
    use porticus::view::View;
    use porticus::{Action, App, Target};

    /// A real tree, minted through the spine.
    fn fresh_root(tag: &str) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!("pan-screen-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        for (parent, ch, label) in [("root", "a", "actio"), ("a", "c", "cura")] {
            let (plan, _) = plan_new(&root, parent, NewSpec::Triple { ch, label }).unwrap();
            plan.apply(&root).unwrap();
        }
        root
    }

    /// `e` on the tree tab annotates the selected node (§10.3, §5.5).
    ///
    /// Driven through the **same loop the terminal drives** — keys in, write really
    /// performed. A pty cannot check this: it has no size, so it draws no cells, and it
    /// echoes scripted input in cooked mode before the app takes raw mode.
    #[test]
    fn e_annotates_the_selected_node() {
        let root = fresh_root("annotate");
        let mut app = PanApp { root: root.clone() };
        porticus::drive(
            &mut app,
            &root,
            &porticus::keys("edeity=Prometheus<enter>"),
            70,
            12,
        )
        .unwrap();

        let code = pantheon::Code::parse("a").unwrap();
        let ann = read_annotations(&root, &code).unwrap();
        assert_eq!(
            ann.deity.as_deref(),
            Some("Prometheus"),
            "`e` then a typed key=value must reach `pan annotate --set`"
        );
    }

    /// `pan`'s node actions are wired now that the cascade is built: `e` annotates, `r`
    /// renames, `x` removes, `m` re-homes (§10.1, §10.3, P§4). `a` (add a child) stays
    /// **dark** — Porticus has no child prompt, so `on_action` returns `None` and the key
    /// is greyed (P§7). The validate tab's `d` relays the command carried in a `Row`
    /// target (§10.2).
    #[test]
    fn the_node_actions_are_wired_and_add_stays_dark() {
        let root = fresh_root("dark");
        let mut app = PanApp { root: root.clone() };
        let node = pantheon::Code::parse("a").unwrap();
        let target = porticus::Target::node(node);

        for action in [Action::Edit, Action::Rename, Action::Remove, Action::Move] {
            assert!(
                app.on_action(action, &target).is_some(),
                "{action:?} is a built node action (§10.1, §10.3, P§4)"
            );
        }
        assert!(
            app.on_action(Action::Add, &target).is_none(),
            "a child has no prompt yet — dark, not faked (P§7)"
        );

        // The validate tab's `d` relays whatever command the row carries — the tab
        // teaches `pan` no fix shapes (§10.2).
        let fix = porticus::Target::Row(porticus::RecordRef::new(
            pantheon::Code::parse("a").unwrap(),
            "pan rename ax --label bad_label",
        ));
        assert!(
            app.on_action(Action::Done, &fix).is_some(),
            "`d` applies a finding fix from the validate tab (§10.2)"
        );
        let not_pan = porticus::Target::Row(porticus::RecordRef::new(
            pantheon::Code::parse("a").unwrap(),
            "alb rename alex alex_csa",
        ));
        assert!(
            app.on_action(Action::Done, &not_pan).is_none(),
            "a command `pan` cannot run in-process is not relayed (I4)"
        );
    }

    /// **A genuine choice becomes a row per candidate** (§10.2, G7).
    ///
    /// The cursor is how a hand picks between corrections, so each candidate gets its own
    /// line carrying its own command — there is no chooser and nothing here decides. The
    /// finding's own row carries no command, so `d` on it is a no-op.
    #[test]
    fn a_multi_candidate_finding_offers_a_row_each() {
        let finding = Finding {
            code: FindingCode::DuplicateSlug,
            severity: Severity::Warning,
            rel_path: std::path::PathBuf::from("c_contextus/csa__person__alex.json"),
            msg: "album:alex also names a record at cso".into(),
            fix: None,
            candidates: vec![
                "pan rename-pattern alex alex_csa csa".to_string(),
                "pan rename-pattern alex alex_cso cso".to_string(),
            ],
        };

        let rows = rows_of(&finding);
        assert_eq!(rows.len(), 3, "the finding, then a row per candidate");
        assert!(
            matches!(rows[0].target, Target::Node { .. }),
            "the finding itself carries no command, so `d` there is dark (P§7)"
        );
        for (row, command) in rows[1..].iter().zip(&finding.candidates) {
            assert!(row.label.contains(command.as_str()), "{}", row.label);
            let Target::Row(record) = &row.target else {
                panic!("a candidate row carries its command: {}", row.label)
            };
            assert_eq!(&record.key, command);
        }
    }

    /// `pan`'s two tabs (§10), and the tree tab as a draw-view about the selected node.
    #[test]
    fn pan_leads_with_its_tree_browser() {
        let root = fresh_root("tabs");
        let mut app = PanApp { root: root.clone() };
        let lineup = app.lineup();
        let ids: Vec<&str> = lineup.iter().map(|v| v.id()).collect();
        assert_eq!(ids, ["tree", "validate"], "two tabs, tree first (§10, P§9)");

        let frame =
            porticus::drive(&mut PanApp { root: root.clone() }, &root, &[], 70, 12).unwrap();
        assert!(frame.contains("P A N T H E O N"), "{frame}");
        assert!(
            frame.contains("children"),
            "the node's counts (§10.1):\n{frame}"
        );
    }

    /// A clean tree says so rather than showing an empty list (I7).
    #[test]
    fn the_validate_tab_reports_a_clean_tree() {
        let root = fresh_root("validate");
        let frame = porticus::drive(
            &mut PanApp { root: root.clone() },
            &root,
            &porticus::keys("2"),
            70,
            12,
        )
        .unwrap();
        assert!(frame.contains("the tree is consistent"), "{frame}");
    }

    /// A draw-view that names no target of its own is about the **selected node** — the
    /// distinction P§3 draws between `None` (a draw-view) and `Some(vec![])` (a row-view
    /// with nothing in it). Without this `e` had no target and did nothing at all.
    #[test]
    fn a_draw_view_targets_the_selected_node() {
        let mut tab = TreeTab {
            root: std::path::PathBuf::new(),
        };
        let node = pantheon::Code::parse("a").unwrap();
        assert!(tab.rows(&node).is_none(), "the tree tab is a draw-view");
        assert!(tab.target().is_none(), "it names no target of its own");
        assert_eq!(tab.prompts_for(Action::Edit), Some("annotate key=value"));
    }
}
