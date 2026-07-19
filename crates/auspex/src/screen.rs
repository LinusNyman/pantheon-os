//! Auspex's screen (§9.6): the rules browser — what exists and what is scoped where.
//!
//! **One Full view, and it offers no actions.** A rule is a file: `touch` mints one,
//! `rm` removes one, and re-scoping is a move plus a prefix rewrite — by any hand
//! (I8, §9.1). So there is no write here for the chrome to relay, and every Tier-2 key
//! stays dark rather than being wired to something Auspex does not do. That is `pan`'s
//! validate tab's precedent: showing is the honest half, and it is the half that needs
//! no surface that does not exist yet.
//!
//! Full rather than Rail because §9.6 asks for "what exists and what is scoped where"
//! — the whole picture, not the tree cursor's node. A rule's scope is a *subtree*, so
//! a per-node list would tell a reader least where it matters most.

use std::path::{Path, PathBuf};

use porticus::action::Writer;
use porticus::{
    Action, App, Ident, Invocation, Layout, RecordRef, Relayed, Row, Target, View, ViewId,
};

use pantheon::code::Code;

/// Open the rules browser.
///
/// # Errors
/// If the lineup is illegal, the tree cannot be walked, or the terminal cannot be
/// taken.
pub(crate) fn open(root: &Path) -> anyhow::Result<()> {
    porticus::run(&mut AuspexApp::new(root), root)
}

/// Auspex's app. Carries a root and nothing else: the rules are re-read from the tree
/// each frame, never cached (§9.4, §18 — the engine keeps nothing).
pub struct AuspexApp {
    root: PathBuf,
}

impl AuspexApp {
    #[must_use]
    pub fn new(root: &Path) -> Self {
        Self {
            root: root.to_path_buf(),
        }
    }
}

impl App for AuspexApp {
    fn ident(&self) -> Ident {
        Ident {
            name: "auspex",
            short: "aus",
            tagline: "the omens",
            symbol: '☄',
            // The palette reserves this one for Auspex by name — "the one hot alarm"
            // (P§9). Note `Theme::error` also paints in CINNABAR, so on this screen
            // alone the accent and an error speak in the same colour. Deliberate: the
            // instrument *is* the alarm.
            accent: porticus::ident::accent::CINNABAR,
        }
    }

    fn lineup(&mut self) -> Vec<Box<dyn View>> {
        vec![Box::new(RulesTab {
            root: self.root.clone(),
        })]
    }

    /// Rules scoped **at** this node — not the subtree beneath it. A rule governs
    /// everything under where it sits (§9.1), so a subtree count would report the same
    /// rule at every descendant and tell a reader nothing about where it lives.
    fn count_at(&mut self, node: &Code) -> usize {
        crate::discover(&self.root, Some(node)).map_or(0, |rules| {
            rules
                .iter()
                .filter(|rule| rule.scope.as_str() == node.as_str())
                .count()
        })
    }

    fn writer(&self) -> Writer {
        Writer::InProcess
    }

    fn execute(&mut self, invocation: &Invocation) -> std::io::Result<Relayed> {
        Ok(in_process(invocation))
    }

    /// Nothing yet. The view offers no actions, so Porticus never reaches here — but
    /// a rule *is* a file, and `a`/`x` would mean `touch` and `rm`, which §9.1 leaves
    /// to the hand rather than giving Auspex a verb for.
    fn on_action(&mut self, _action: Action, _target: &Target) -> Option<Invocation> {
        None
    }
}

/// Re-enter `aus` in this process rather than spawning a copy of ourselves.
///
/// Unreachable while the view offers no actions, and kept real rather than stubbed so
/// the day one is offered the path is already the same code the CLI runs — validation
/// and exit codes included, which is the whole reason a core's own TUI writes
/// in-process (P§7).
fn in_process(invocation: &Invocation) -> Relayed {
    use std::ffi::OsString;

    use clap::Parser;

    let argv =
        std::iter::once(OsString::from("aus")).chain(invocation.args.iter().map(OsString::from));
    let cli = match crate::cli::Cli::try_parse_from(argv) {
        Ok(cli) => cli,
        Err(e) => {
            return Relayed {
                code: 2,
                stdout: String::new(),
                stderr: e.to_string(),
            };
        }
    };
    match crate::cli::run(&cli, true) {
        Ok(pantheon::contract::Response::Json(value)) => Relayed {
            code: 0,
            stdout: value.to_string(),
            stderr: String::new(),
        },
        Ok(pantheon::contract::Response::JsonExit(value, code)) => Relayed {
            code: i32::from(code),
            stdout: value.to_string(),
            stderr: String::new(),
        },
        Ok(pantheon::contract::Response::Raw(text)) => Relayed {
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

/// Every rule in the tree, with its scope and what it declares.
struct RulesTab {
    root: PathBuf,
}

impl View for RulesTab {
    fn id(&self) -> ViewId {
        "rules"
    }

    fn layout(&self) -> Layout {
        Layout::Full
    }

    /// The tree cursor means nothing to a Full view, so `node` is ignored: this lists
    /// the whole tree, which is what "what is scoped where" asks for (§9.6).
    fn rows(&mut self, _node: &Code) -> Option<Vec<Row>> {
        let rules = crate::discover(&self.root, None).unwrap_or_default();
        Some(
            rules
                .iter()
                .map(|rule| Row {
                    label: label_for(rule),
                    // The scope is the rule's home, so an action would relay to the
                    // right node the day one is offered (P§7).
                    target: Target::Row(RecordRef {
                        home: rule.scope.clone(),
                        key: rule.name.clone(),
                    }),
                    when: None,
                })
                .collect(),
        )
    }

    fn empty_line(&self) -> &'static str {
        // Absence is calm, never an error (I7). No rules is a tree that simply does
        // not ask anything of itself yet.
        "no rules — nothing watches this tree"
    }
}

/// One rule as a line: where it is scoped, what it is called, and what it may touch.
///
/// The grant is shown because it is the whole guard (§9.2) — a browser that listed
/// rules without their capabilities would hide the one thing worth reading.
fn label_for(rule: &crate::Rule) -> String {
    use std::fmt::Write;

    let mut line = format!("{:<8} {}", rule.scope.as_str(), rule.name);
    if !rule.header.watch.is_empty() {
        let _ = write!(line, "   watch={}", rule.header.watch.join(","));
    }
    if rule.header.writes.is_empty() {
        // Default-deny is the interesting case, not the empty one: such a rule may
        // propose and nothing it proposes lands (§9.2).
        line.push_str("   writes=none");
    } else {
        let _ = write!(line, "   writes={}", rule.header.writes.join(";"));
    }
    if rule.declared.is_some() {
        line.push_str("   [misfiled]");
    }
    if rule.header.error.is_some() {
        line.push_str("   [header error]");
    }
    line
}
