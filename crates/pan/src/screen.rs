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
//! The node cascade (§10.1) is built, so `r` renames the selected node's label and `x`
//! removes it (refused by the spine if it is not empty). `m` (move) stays **dark** —
//! Porticus has no destination prompt for it yet, and a tree browser that offered a move
//! it could not complete would be worse than one that says so (P§7's graceful
//! degradation). `a` (add a child) is likewise not offered here; a child is minted with
//! `pan new`. The bulk repairs (`rename-prefix`, `rename-pattern`, `mv-file`) are CLI
//! verbs with no key of their own.

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
            // ── the validate tab: `d` applies a finding's fix ──
            // The finding's fix rides in the row target as (code, normalized-label), so
            // this relays it verbatim (§10.2). A finding with no fix carries a node
            // target this arm does not match, so `d` there is a no-op.
            (Action::Done, Target::Row(RecordRef { home, key })) => Some(Invocation::new(
                "pan",
                ["rename", home.as_str(), "--label", key.as_str()],
            )),
            // `m` (move) has no destination prompt yet, and every other key is unoffered
            // by the active view — dark, not faked (§10.1, P§7).
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
        // `e` annotates, `r` renames the label, `x` removes an empty node (§10.1, §10.3).
        // `m` (move) and `a` (add a child) are not offered — the chrome has no
        // destination/child prompt for them yet, so their keys stay dark.
        &[Action::Edit, Action::Rename, Action::Remove]
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

/// `pan validate`'s findings, browsable — and now **applicable** (§10.2).
///
/// A [`Finding`] carries its single legal correction as a `pan` command
/// ([`Finding::fix`]); this tab shows it in the row and, where a finding *has* one,
/// `d` applies it. The fix's `(code, normalized-label)` rides in the row's target, so
/// `on_action` relays it verbatim (`pan rename <code> --label <norm>`). A finding with no
/// single correction — a genuine choice — stays inert: its candidates are shown, never
/// picked (§10.2). The shown command is the preview a hand reads before pressing `d`.
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
        Some(findings.iter().map(row_of).collect())
    }

    fn actions(&self) -> &[Action] {
        // `d` applies a finding's single legal correction (§10.2). Distinct from the tree
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

/// One finding as a row. Where it carries a single legal fix, its target holds the fix's
/// `(code, normalized-label)` so `d` can relay it (§10.2); otherwise it targets a bare
/// node the apply arm does not match, leaving `d` a no-op there.
fn row_of(finding: &Finding) -> Row {
    let mark = match finding.severity {
        Severity::Error => "error  ",
        Severity::Warning => "warning",
    };
    let base = format!("{mark}  {}  {}", finding.rel_path.display(), finding.msg);
    // Where the correction is unambiguous, show the command a hand reads before pressing
    // `d` to apply it (§10.2).
    let label = match &finding.fix {
        Some(fix) => format!("{base}  →  {fix}"),
        None => base,
    };
    Row {
        label,
        target: fix_target(finding),
        when: None,
    }
}

/// The row target for a finding: for the one auto-fix shape `pan validate` emits today —
/// `pan rename <code> --label <normalized>` (a non-normalized node label, §10.2) — a
/// `Target::Row` carrying `(code, normalized-label)` so `d` relays it. Anything else (no
/// fix, or a shape this does not recognise) targets a bare node the apply arm ignores.
fn fix_target(finding: &Finding) -> Target {
    let placeholder = Target::Node {
        node: Code::parse("a").unwrap_or_else(|_| unreachable!("`a` is a legal code")),
        at: None,
    };
    let Some(fix) = &finding.fix else {
        return placeholder;
    };
    // Match exactly `pan rename <code> --label <normalized>`; a future fix shape stays
    // display-only until it is taught here.
    match fix.split_whitespace().collect::<Vec<_>>().as_slice() {
        ["pan", "rename", code, "--label", label] => match Code::parse(code) {
            Ok(home) => Target::Row(RecordRef {
                home,
                key: (*label).to_owned(),
            }),
            Err(_) => placeholder,
        },
        _ => placeholder,
    }
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
    use super::{PanApp, TreeTab};
    use pantheon::{NewSpec, plan_new, read_annotations};
    use porticus::view::View;
    use porticus::{Action, App};

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
    /// renames, `x` removes (§10.1, §10.3). `m` (move) and `a` (add) stay **dark** —
    /// Porticus has no destination/child prompt for them yet, so `on_action` returns
    /// `None` and the keys are greyed (P§7). The validate tab's `d` applies a fix carried
    /// in a `Row` target (§10.2).
    #[test]
    fn the_node_actions_are_wired_and_move_stays_dark() {
        let root = fresh_root("dark");
        let mut app = PanApp { root: root.clone() };
        let node = pantheon::Code::parse("a").unwrap();
        let target = porticus::Target::Node { node, at: None };

        for action in [Action::Edit, Action::Rename, Action::Remove] {
            assert!(
                app.on_action(action, &target).is_some(),
                "{action:?} is a built node action (§10.1, §10.3)"
            );
        }
        for action in [Action::Move, Action::Add] {
            assert!(
                app.on_action(action, &target).is_none(),
                "{action:?} has no prompt yet — dark, not faked (P§7)"
            );
        }

        // The validate tab's `d` applies a finding's fix, carried as (code, label).
        let fix = porticus::Target::Row(porticus::RecordRef {
            home: pantheon::Code::parse("ax").unwrap(),
            key: "bad_label".into(),
        });
        assert!(
            app.on_action(Action::Done, &fix).is_some(),
            "`d` applies a finding fix from the validate tab (§10.2)"
        );
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
