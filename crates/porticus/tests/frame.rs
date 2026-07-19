//! The frame, rendered (P§4).
//!
//! The contract's snapshots freeze what a core *emits*; these freeze what a hand
//! *sees*. Both halves of I8 are then pinned, and neither can drift silently.

use std::sync::atomic::{AtomicU32, Ordering};

use pantheon::{Code, NewSpec, plan_new};
use porticus::view::{Layout, Row, View, ViewId};
use porticus::views::{Agenda, TreeFile};
use porticus::{Action, App, Ident, Invocation, RecordRef, Target, Writer};

static COUNTER: AtomicU32 = AtomicU32::new(0);

/// A real tree on disk, minted through the spine the same way the contract tests do.
fn fresh_root() -> std::path::PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::SeqCst);
    let root = std::env::temp_dir().join(format!("porticus-frame-{}-{n}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    for (parent, ch, label) in [
        ("root", "a", "actio"),
        ("a", "c", "cura"),
        ("root", "c", "contextus"),
        ("c", "s", "societas"),
    ] {
        let (plan, _) = plan_new(&root, parent, NewSpec::Triple { ch, label }).unwrap();
        plan.apply(&root).unwrap();
    }
    root
}

/// An instrument with two tasks at `ac`, folded from memory rather than from a core —
/// Porticus never reaches for one (I5), so a test does not need one either.
struct Fake;

impl App for Fake {
    fn ident(&self) -> Ident {
        Ident {
            name: "pensum",
            short: "pen",
            tagline: "intention · tasks",
            symbol: '♂',
            accent: porticus::ident::accent::MINIUM,
        }
    }

    fn lineup(&mut self) -> Vec<Box<dyn View>> {
        vec![
            Box::new(
                TreeFile::of(|node: &Code| {
                    if node.as_str() == "ac" {
                        vec![row("buy_milk", "ac"), row("call_the_dentist", "ac")]
                    } else {
                        Vec::new()
                    }
                })
                .offering(&[Action::Done, Action::Edit])
                .empty("no todos here"),
            ),
            Box::new(Agenda::of(|| vec![row("buy_milk", "ac")])),
        ]
    }

    fn count_at(&mut self, node: &Code) -> usize {
        usize::from(node.as_str() == "ac") * 2
    }

    fn writer(&self) -> Writer {
        Writer::InProcess
    }

    fn on_action(&mut self, _action: Action, _target: &Target) -> Option<Invocation> {
        None
    }
}

fn row(key: &str, home: &str) -> Row {
    Row {
        label: key.to_string(),
        target: Target::Row(RecordRef {
            home: Code::parse(home).unwrap(),
            key: key.to_string(),
        }),
        when: None,
    }
}

/// The three bands, with the tree rail beside the content.
///
/// What this pins: the tracked name-word, the path bar with `+` at its tail, the tab
/// strip in lineup order, the outline with its count badge, and the status line.
#[test]
fn the_frame_has_three_bands() {
    let root = fresh_root();
    let buffer = porticus::render_once(&mut Fake, &root, 72, 12).unwrap();
    insta::assert_snapshot!("frame_rail", porticus::as_text(&buffer));
}

/// Absence is calm, never an error (I7): the chrome stands in full and one dim line
/// says so in the content (P§4).
#[test]
fn an_empty_node_keeps_its_chrome() {
    let root = fresh_root();
    // The cursor opens on the first sphere, which holds no todos.
    let buffer = porticus::render_once(&mut Fake, &root, 72, 10).unwrap();
    let text = porticus::as_text(&buffer);
    assert!(text.contains("no todos here"), "{text}");
    // The header and the tab strip are still there — the chrome never collapses.
    assert!(text.contains("P E N S U M"), "{text}");
    assert!(text.contains("records"), "{text}");
}

/// Below a hard floor the chrome collapses to nothing but a notice — the one place it
/// does (P§4).
#[test]
fn a_tiny_terminal_says_so_and_nothing_else() {
    let root = fresh_root();
    let buffer = porticus::render_once(&mut Fake, &root, 20, 4).unwrap();
    let text = porticus::as_text(&buffer);
    assert!(text.starts_with("terminal too small"), "{text}");
    assert!(!text.contains("P E N S U M"), "{text}");
}

/// A lineup must have a `[0]` to open on, and no more than nine views to switch
/// between (P§3). Both are rejected at `run`, before a terminal is taken.
#[test]
fn a_lineup_is_one_to_nine_views() {
    struct Empty;
    impl App for Empty {
        fn ident(&self) -> Ident {
            Fake.ident()
        }
        fn lineup(&mut self) -> Vec<Box<dyn View>> {
            Vec::new()
        }
        fn count_at(&mut self, _node: &Code) -> usize {
            0
        }
        fn writer(&self) -> Writer {
            Writer::InProcess
        }
        fn on_action(&mut self, _a: Action, _t: &Target) -> Option<Invocation> {
            None
        }
    }
    let root = fresh_root();
    let err = porticus::render_once(&mut Empty, &root, 40, 10).unwrap_err();
    assert!(
        err.to_string().contains("at least one view"),
        "an empty lineup must be refused, not indexed into: {err}"
    );
}

/// A Full view owns the whole width and names its own locator in the header, where a
/// Rail view shows the path bar (P§4, P§6).
#[test]
fn a_full_view_has_no_rail() {
    struct FullOnly;
    impl App for FullOnly {
        fn ident(&self) -> Ident {
            Fake.ident()
        }
        fn lineup(&mut self) -> Vec<Box<dyn View>> {
            vec![Box::new(Agenda::of(|| {
                vec![row("buy_milk", "ac"), row("call_the_dentist", "ac")]
            }))]
        }
        fn count_at(&mut self, _node: &Code) -> usize {
            0
        }
        fn writer(&self) -> Writer {
            Writer::InProcess
        }
        fn on_action(&mut self, _a: Action, _t: &Target) -> Option<Invocation> {
            None
        }
    }
    let root = fresh_root();
    let buffer = porticus::render_once(&mut FullOnly, &root, 72, 10).unwrap();
    let text = porticus::as_text(&buffer);
    // No tree codes in the body — the rail is not drawn at all.
    assert!(text.contains("buy_milk"), "{text}");
    assert!(text.contains("by date"), "{text}");
    assert_eq!(
        Agenda::of(Vec::new).layout(),
        Layout::Full,
        "an Agenda is a Full view (P§3)"
    );
}

/// An Agenda's rows sort by date, undated last — a stable order, so a refold does not
/// shuffle rows under the cursor (P§3).
#[test]
fn an_agenda_sorts_dated_first() {
    let mut agenda = Agenda::of(|| {
        vec![
            Row {
                when: None,
                ..row("undated", "ac")
            },
            Row {
                when: Some("260719".into()),
                ..row("later", "ac")
            },
            Row {
                when: Some("260701".into()),
                ..row("earlier", "ac")
            },
        ]
    });
    let node = Code::parse("ac").unwrap();
    let rows = agenda.rows(&node).unwrap();
    let labels: Vec<&str> = rows.iter().map(|r| r.label.as_str()).collect();
    assert_eq!(labels, ["earlier", "later", "undated"]);
}

/// A view's id is what the switcher and Help key off (P§3).
#[test]
fn catalog_views_name_themselves() {
    assert_eq!(
        TreeFile::of(|_: &Code| Vec::new()).id() as ViewId,
        "records"
    );
    assert_eq!(Agenda::of(Vec::new).id() as ViewId, "agenda");
}
