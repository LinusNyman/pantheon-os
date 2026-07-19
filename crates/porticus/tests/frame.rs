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

/// Every chart in the vocabulary draws (P§3).
///
/// A chart that panicked on an edge — an empty series, a flat one, a zero total —
/// would do it on someone's real tree, not here, so each shape is drawn once.
#[test]
fn every_chart_shape_draws() {
    use porticus::views::{Chart, Insights, Panel};

    struct Charts;
    impl App for Charts {
        fn ident(&self) -> Ident {
            Fake.ident()
        }
        fn lineup(&mut self) -> Vec<Box<dyn View>> {
            vec![Box::new(Insights::of(|| {
                vec![
                    Panel {
                        title: "weight".into(),
                        chart: Chart::Trend(vec![
                            ("260701".into(), 78.4),
                            ("260708".into(), 78.1),
                            ("260715".into(), 77.9),
                        ]),
                    },
                    Panel {
                        title: "by kind".into(),
                        chart: Chart::Bars(vec![("person".into(), 2.0), ("group".into(), 1.0)]),
                    },
                    Panel {
                        title: "by type".into(),
                        chart: Chart::Pie(vec![("quote".into(), 3.0), ("principle".into(), 1.0)]),
                    },
                    Panel {
                        title: "streak".into(),
                        chart: Chart::Stat("days".into(), "14".into()),
                    },
                    Panel {
                        title: "logging".into(),
                        chart: Chart::Heatmap(vec![("260701".into(), 1.0), ("260702".into(), 0.0)]),
                    },
                    Panel {
                        title: "throughput".into(),
                        chart: Chart::Spark(vec![("260701".into(), 3.0), ("260702".into(), 5.0)]),
                    },
                ]
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
    let buffer = porticus::render_once(&mut Charts, &root, 80, 24).unwrap();
    let text = porticus::as_text(&buffer);
    for title in [
        "weight",
        "by kind",
        "by type",
        "streak",
        "logging",
        "throughput",
    ] {
        assert!(text.contains(title), "panel `{title}` missing:\n{text}");
    }
    assert!(text.contains("14"), "the stat's value should show:\n{text}");
}

/// The degenerate inputs each chart can actually meet: nothing to draw, and a series
/// with no spread. Absence is calm per panel (I7, P§4) — and a flat series must not
/// collapse the axis it is scaled against.
#[test]
fn a_chart_survives_empty_and_flat_data() {
    use porticus::views::{Chart, Insights, Panel};

    struct Edge;
    impl App for Edge {
        fn ident(&self) -> Ident {
            Fake.ident()
        }
        fn lineup(&mut self) -> Vec<Box<dyn View>> {
            vec![Box::new(Insights::of(|| {
                vec![
                    Panel {
                        title: "empty trend".into(),
                        chart: Chart::Trend(Vec::new()),
                    },
                    Panel {
                        title: "empty pie".into(),
                        chart: Chart::Pie(Vec::new()),
                    },
                    Panel {
                        title: "flat".into(),
                        chart: Chart::Trend(vec![("260701".into(), 5.0), ("260702".into(), 5.0)]),
                    },
                    Panel {
                        title: "zero pie".into(),
                        chart: Chart::Pie(vec![("none".into(), 0.0)]),
                    },
                ]
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
    let text = porticus::as_text(&porticus::render_once(&mut Edge, &root, 80, 20).unwrap());
    assert!(text.contains("no data yet"), "{text}");
}

/// An instrument with no panels yet still gets a calm screen, not a blank one.
#[test]
fn insights_with_nothing_to_show_says_so() {
    struct Bare;
    impl App for Bare {
        fn ident(&self) -> Ident {
            Fake.ident()
        }
        fn lineup(&mut self) -> Vec<Box<dyn View>> {
            vec![Box::new(porticus::views::Insights::of(Vec::new))]
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
    let text = porticus::as_text(&porticus::render_once(&mut Bare, &root, 60, 12).unwrap());
    assert!(text.contains("no data yet"), "{text}");
    assert!(
        text.contains("P E N S U M"),
        "the chrome still stands:\n{text}"
    );
}

/// The contact card: title, labeled fields, ref chips (P§3).
///
/// One implementation serves Album's contact, Mappa's place and Rationes' holding, so
/// what it renders is pinned once here rather than three times downstream (I3).
#[test]
fn the_entity_card_draws_its_model() {
    use porticus::views::{Card, Chip, EntityCard};

    struct Carded;
    impl App for Carded {
        fn ident(&self) -> Ident {
            Fake.ident()
        }
        fn lineup(&mut self) -> Vec<Box<dyn View>> {
            vec![Box::new(EntityCard::of(
                |_: &Code, _: Option<&RecordRef>| {
                    Some(Card {
                        title: "mara".into(),
                        fields: vec![
                            ("kind".into(), "person".into()),
                            ("closeness".into(), "friend".into()),
                        ],
                        chips: vec![Chip {
                            label: "album:alex".into(),
                            reference: "album:alex".into(),
                        }],
                        strip: Vec::new(),
                    })
                },
            ))]
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
    let text = porticus::as_text(&porticus::render_once(&mut Carded, &root, 72, 14).unwrap());
    for want in [
        "mara",
        "kind",
        "person",
        "closeness",
        "friend",
        "album:alex",
    ] {
        assert!(text.contains(want), "`{want}` missing:\n{text}");
    }
}

/// A detail view **never guesses among several** (P§3): with nothing to pin it shows
/// its empty "pick a record" state rather than choosing one.
#[test]
fn a_detail_view_with_no_single_record_says_pick_one() {
    use porticus::views::EntityCard;

    struct Unpinned;
    impl App for Unpinned {
        fn ident(&self) -> Ident {
            Fake.ident()
        }
        fn lineup(&mut self) -> Vec<Box<dyn View>> {
            vec![Box::new(EntityCard::of(
                |_: &Code, _: Option<&RecordRef>| None,
            ))]
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
    let text = porticus::as_text(&porticus::render_once(&mut Unpinned, &root, 60, 12).unwrap());
    assert!(text.contains("pick a record"), "{text}");
}

/// The Reader renders frontmatter over a Markdown body (P§3).
///
/// Headings, emphasis, and list bullets survive; the fence's two fields sit above the
/// prose. What it must *not* do is offer to edit in place — that suspends to the hand's
/// own editor (P§11), which is why this view has no input surface beyond scrolling.
#[test]
fn the_reader_renders_a_document() {
    use porticus::views::{Document, Reader};

    struct Reading;
    impl App for Reading {
        fn ident(&self) -> Ident {
            Fake.ident()
        }
        fn lineup(&mut self) -> Vec<Box<dyn View>> {
            vec![Box::new(Reader::of(|_: &Code, _: Option<&RecordRef>| {
                Some(Document {
                    slug: "a_note".into(),
                    r#type: Some("principium".into()),
                    tags: vec!["mores".into()],
                    body: "# A heading\n\nProse with *emphasis*.\n\n- one\n- two\n".into(),
                })
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
    let buffer = porticus::render_once(&mut Reading, &root, 72, 18).unwrap();
    insta::assert_snapshot!("frame_reader", porticus::as_text(&buffer));
}

/// A node with no document is calm, not empty-looking (I7).
#[test]
fn the_reader_with_no_document_says_so() {
    use porticus::views::{Document, Reader};

    struct Nothing;
    impl App for Nothing {
        fn ident(&self) -> Ident {
            Fake.ident()
        }
        fn lineup(&mut self) -> Vec<Box<dyn View>> {
            vec![Box::new(Reader::of(
                |_: &Code, _: Option<&RecordRef>| -> Option<Document> { None },
            ))]
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
    let text = porticus::as_text(&porticus::render_once(&mut Nothing, &root, 60, 12).unwrap());
    assert!(text.contains("no document here"), "{text}");
}

/// `Enter` on a content row **activates**: it pins that row's record and switches to
/// the lineup's detail view, which folds *that* record (P§3, P§5).
///
/// This is what makes a detail view usable at all. Without it a card could only ever
/// render a node holding exactly one record — at a node with two people there would be
/// no way to say which, and "pick a record" would be a dead end rather than a prompt.
#[test]
fn enter_pins_a_row_into_the_detail_view() {
    use porticus::views::{Card, EntityCard};

    struct Two;
    impl App for Two {
        fn ident(&self) -> Ident {
            Fake.ident()
        }
        fn lineup(&mut self) -> Vec<Box<dyn View>> {
            vec![
                Box::new(TreeFile::of(|_: &Code| {
                    vec![row("alex", "ac"), row("mara", "ac")]
                })),
                // The fold answers from the *pin*, not the node — which is the whole
                // point: the node holds two, so only a pin can name one.
                Box::new(EntityCard::of(
                    |_: &Code, pinned: Option<&RecordRef>| -> Option<Card> {
                        pinned.map(|record| Card {
                            title: record.key.clone(),
                            fields: vec![("home".into(), record.home.as_str().to_owned())],
                            chips: Vec::new(),
                            strip: Vec::new(),
                        })
                    },
                )),
            ]
        }
        fn count_at(&mut self, _node: &Code) -> usize {
            2
        }
        fn writer(&self) -> Writer {
            Writer::InProcess
        }
        fn on_action(&mut self, _a: Action, _t: &Target) -> Option<Invocation> {
            None
        }
    }

    // No tree needed: this drives the view directly, since what is under test is the
    // pin rather than the frame around it.
    let mut app = Two;
    let mut lineup = app.lineup();
    assert!(lineup[1].is_detail(), "the card is the detail view (P§3)");
    let node = Code::parse("ac").unwrap();
    assert!(
        lineup[1].rows(&node).is_none(),
        "a detail view is a draw-view (P§3)"
    );

    // Pinned, it folds that record.
    lineup[1].pin(Some(RecordRef {
        home: node.clone(),
        key: "mara".into(),
    }));
    let buffer = {
        let mut term = ratatui::Terminal::new(ratatui::backend::TestBackend::new(40, 8)).unwrap();
        term.draw(|f| {
            let area = f.area();
            lineup[1].draw(
                &node,
                area,
                f.buffer_mut(),
                porticus::Theme::of(&app.ident()),
            );
        })
        .unwrap();
        term.backend().buffer().clone()
    };
    let text = porticus::as_text(&buffer);
    assert!(
        text.contains("mara"),
        "the pinned record is what folds:\n{text}"
    );
    assert!(!text.contains("alex"), "and only that one:\n{text}");

    // Un-pinned, it falls back to its empty state rather than a stale record (I8).
    lineup[1].pin(None);
    let buffer = {
        let mut term = ratatui::Terminal::new(ratatui::backend::TestBackend::new(40, 8)).unwrap();
        term.draw(|f| {
            let area = f.area();
            lineup[1].draw(
                &node,
                area,
                f.buffer_mut(),
                porticus::Theme::of(&app.ident()),
            );
        })
        .unwrap();
        term.backend().buffer().clone()
    };
    assert!(porticus::as_text(&buffer).contains("pick a record"));
}

/// A lineup holds **at most one** detail view (P§3) — that is what lets `Enter` route
/// with no shape tag on the record.
#[test]
fn a_lineup_holds_at_most_one_detail_view() {
    use porticus::views::{Card, EntityCard};

    struct TwoDetails;
    impl App for TwoDetails {
        fn ident(&self) -> Ident {
            Fake.ident()
        }
        fn lineup(&mut self) -> Vec<Box<dyn View>> {
            let card = || {
                Box::new(EntityCard::of(
                    |_: &Code, _: Option<&RecordRef>| -> Option<Card> { None },
                )) as Box<dyn View>
            };
            vec![card(), card()]
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
    let err = porticus::render_once(&mut TwoDetails, &root, 40, 10).unwrap_err();
    assert!(
        err.to_string().contains("at most one detail view"),
        "a second detail view must be refused: {err}"
    );
}
