//! The frame, rendered (P§4).
//!
//! The contract's snapshots freeze what a core *emits*; these freeze what a hand
//! *sees*. Both halves of I8 are then pinned, and neither can drift silently.

use std::sync::atomic::{AtomicU32, Ordering};

use pantheon::{Code, NewSpec, plan_new};
use porticus::view::{Handled, Layout, Nav, Row, View, ViewId};
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

/// **Every relay names the tree it is acting on** (§7.3's `-C`).
///
/// Porticus adds this rather than trusting each instrument to, because the failure is
/// silent and severe: `run` is *given* a root, but a relay without `-C` resolves
/// `$PANTHEON_ROOT` instead — so a TUI opened with `-C /some/tree` would read one tree
/// and write to another, and nothing on screen would say so.
///
/// Found by driving `pan`'s annotate through the scripted-key harness against a temp
/// tree with no `PANTHEON_ROOT` set: the write went nowhere. Every core had it.
#[test]
fn every_relay_carries_the_root_it_is_drawing() {
    use std::sync::{Arc, Mutex};

    #[derive(Clone)]
    struct Spy(Arc<Mutex<Vec<String>>>);
    impl App for Spy {
        fn ident(&self) -> Ident {
            Fake.ident()
        }
        fn lineup(&mut self) -> Vec<Box<dyn View>> {
            vec![Box::new(
                TreeFile::of(|_: &Code| vec![row("buy_milk", "ac")]).offering(&[Action::Done]),
            )]
        }
        fn count_at(&mut self, _node: &Code) -> usize {
            1
        }
        fn writer(&self) -> Writer {
            Writer::InProcess
        }
        fn on_action(&mut self, _a: Action, t: &Target) -> Option<Invocation> {
            let Target::Row(record) = t else { return None };
            Some(Invocation::new("pen", ["edit", &record.key, "--done"]))
        }
        fn execute(&mut self, i: &Invocation) -> std::io::Result<porticus::Relayed> {
            self.0.lock().unwrap().push(i.display());
            Ok(porticus::Relayed {
                code: 0,
                stdout: "{}".into(),
                stderr: String::new(),
            })
        }
    }

    let root = fresh_root();
    let seen = Arc::new(Mutex::new(Vec::new()));
    // `<tab>` moves focus to the content, then `d` relays on the focused row.
    porticus::drive(
        &mut Spy(Arc::clone(&seen)),
        &root,
        &porticus::keys("<tab>d"),
        60,
        10,
    )
    .unwrap();

    let calls = seen.lock().unwrap();
    let relayed = calls.first().expect("`d` must relay");
    assert!(
        relayed.contains("-C") && relayed.contains(&root.display().to_string()),
        "a relay must name the tree the screen is drawing: {relayed}"
    );
    assert!(
        relayed.contains("-y"),
        "and still carry -y (§7.3): {relayed}"
    );
}

/// **The dim asks `any_at`; the badge asks `count_at`** (P§6).
///
/// The split is the point: an instrument whose count is costly overrides `any_at` to
/// answer the dim without a full fold, so the badge stays exact where it shows and the
/// dim stays cheap everywhere. The rail had been asking `count_at` for both, which made
/// that override unreachable — a declared escape hatch that nothing could reach.
///
/// This asserts the cheap question is actually asked, and that `count_at` is spared
/// where `any_at` already said no.
#[test]
fn the_dim_asks_any_and_the_badge_asks_count() {
    use std::sync::{Arc, Mutex};

    #[derive(Default)]
    struct Asked {
        any: Vec<String>,
        count: Vec<String>,
    }

    struct Counting(Arc<Mutex<Asked>>);
    impl App for Counting {
        fn ident(&self) -> Ident {
            Fake.ident()
        }
        fn lineup(&mut self) -> Vec<Box<dyn View>> {
            vec![Box::new(TreeFile::of(|_: &Code| Vec::new()))]
        }
        fn count_at(&mut self, node: &Code) -> usize {
            self.0.lock().unwrap().count.push(node.as_str().to_owned());
            7
        }
        fn any_at(&mut self, node: &Code) -> bool {
            self.0.lock().unwrap().any.push(node.as_str().to_owned());
            // Only `ac` holds anything — so only `ac` may be counted.
            node.as_str() == "ac"
        }
        fn writer(&self) -> Writer {
            Writer::InProcess
        }
        fn on_action(&mut self, _a: Action, _t: &Target) -> Option<Invocation> {
            None
        }
    }

    let root = fresh_root();
    let asked = Arc::new(Mutex::new(Asked::default()));
    let text = porticus::as_text(
        &porticus::render_once(&mut Counting(Arc::clone(&asked)), &root, 72, 12).unwrap(),
    );

    let asked = asked.lock().unwrap();
    assert!(
        asked.any.iter().any(|c| c == "a"),
        "every visible node is asked the cheap question: {:?}",
        asked.any
    );
    assert!(
        asked.count.iter().all(|c| c == "ac"),
        "`count_at` must be spared where `any_at` said no: {:?}",
        asked.count
    );
    // And the badge that did show carries the exact count.
    assert!(text.contains("ac cura  7"), "{text}");
}

// ── Calendar (row · Full) and Timeline (draw · Full) — P§3 ────────────────────

/// A `Calendar` is a row-view that *also* paints a grid: the grid is the locator, the
/// rows beneath it are the focused day (P§3, P§6).
#[test]
fn a_calendar_is_a_row_view_with_a_grid() {
    use porticus::views::Calendar;

    let mut calendar = Calendar::of(Vec::new);
    assert_eq!(calendar.layout(), Layout::Full);
    assert_eq!(calendar.id() as ViewId, "calendar");
    assert!(
        calendar.rows(&Code::parse("ac").unwrap()).is_some(),
        "a Calendar is a row-view — `None` would make it a draw-view and forfeit \
         search, filter and scroll (P§3, P§6)"
    );

    let grid = calendar.grid().expect("a Calendar declares a grid");
    assert_eq!(grid.columns.len(), 7, "a week is seven days");
    assert_eq!(
        grid.cells.len() % 7,
        0,
        "the month is padded to whole weeks either side"
    );
    assert!(
        grid.cells[grid.focused].is_some(),
        "the focused cell is a real day, never one of the pad cells"
    );
}

/// The grid shows the month; the rows show **one day of it**. A dated item on another
/// day is counted in its own cell and kept out of the list.
#[test]
fn a_calendar_lists_only_the_focused_day() {
    use porticus::views::Calendar;

    // 1 January 1999 is not today, whenever today is — so the row is always elsewhere.
    let mut calendar = Calendar::of(|| {
        vec![Row {
            when: Some("990101".into()),
            ..row("long_ago", "ac")
        }]
    });
    let rows = calendar.rows(&Code::parse("ac").unwrap()).unwrap();
    assert!(
        rows.is_empty(),
        "a row on another day is not this day's: {rows:?}"
    );
}

/// `[` and `]` page the month and `t` returns to today — Tier-3 keys the view declares
/// so Porticus can route them and Help can list them (P§5).
#[test]
fn a_calendar_pages_by_month_and_comes_back() {
    use porticus::views::Calendar;

    let mut calendar = Calendar::of(Vec::new);
    let declared: Vec<char> = calendar.nav_keys().iter().map(|(key, _)| *key).collect();
    assert_eq!(declared, ['t', '[', ']']);

    let opened = calendar.locator();
    assert_eq!(calendar.navigate(Nav::Key(']')), Handled::Yes);
    assert_ne!(calendar.locator(), opened, "`]` moves to the next month");
    assert_eq!(calendar.navigate(Nav::Key('[')), Handled::Yes);
    assert_eq!(calendar.locator(), opened, "`[` comes back");

    // Three months out and `t` returns, however far the cursor wandered.
    for _ in 0..3 {
        calendar.navigate(Nav::Key(']'));
    }
    calendar.navigate(Nav::Key('t'));
    assert_eq!(calendar.locator(), opened, "`t` is today");
}

/// The cell dates the add: `a` on a calendar keeps the day you pointed at rather than
/// defaulting to today (§7.3, P§7).
#[test]
fn a_calendar_cell_dates_the_add() {
    use porticus::views::Calendar;

    let mut calendar = Calendar::of(Vec::new);
    let node = Code::parse("ac").unwrap();
    calendar.rows(&node);

    let Some(Target::Node { at, .. }) = calendar.target() else {
        panic!("a dated Full view names its cell through `target` (P§7)");
    };
    let at = at.expect("the cell carries its date");
    assert_eq!(at.len(), 6, "a reading key is YYMMDD (§6.1): {at}");

    // Move a day and the date the add would carry moves with it.
    calendar.navigate(Nav::Right);
    calendar.rows(&node);
    let Some(Target::Node { at: moved, .. }) = calendar.target() else {
        unreachable!()
    };
    assert_ne!(moved.unwrap(), at, "the cell cursor is what dates the add");
}

/// A `Timeline` is a draw-view whose bars each carry their own home, so it is
/// cross-node and an action on a bar resolves exactly as a row's would (P§3, P§7).
#[test]
fn a_timeline_bar_carries_its_own_home() {
    use porticus::views::{CardSpan, Timeline};

    struct Bars;
    impl App for Bars {
        fn ident(&self) -> Ident {
            Ident {
                name: "fasti",
                short: "fas",
                tagline: "actio · placement",
                symbol: '☾',
                accent: porticus::ident::accent::SOL_GOLD,
            }
        }
        fn lineup(&mut self) -> Vec<Box<dyn View>> {
            vec![Box::new(
                Timeline::of(|| {
                    vec![
                        CardSpan {
                            label: "mvp_phase".into(),
                            from: "260101".into(),
                            to: Some("260630".into()),
                            home: RecordRef {
                                home: Code::parse("ac").unwrap(),
                                key: "mvp_phase".into(),
                            },
                        },
                        CardSpan {
                            label: "residence".into(),
                            from: "260201".into(),
                            // Open: drawn to the range's right edge (§8.4).
                            to: None,
                            home: RecordRef {
                                home: Code::parse("cs").unwrap(),
                                key: "residence".into(),
                            },
                        },
                    ]
                })
                .offering(&[Action::Edit]),
            )]
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
    let text = porticus::as_text(&porticus::render_once(&mut Bars, &root, 80, 14).unwrap());
    assert!(text.contains("mvp_phase"), "{text}");
    assert!(text.contains("residence"), "{text}");
    assert!(text.contains('─'), "a period is drawn as a bar: {text}");
    // A Full view names its own locator where a Rail view shows the path bar (P§4).
    assert!(
        text.contains("2026-01-01"),
        "the range is the header: {text}"
    );
}

/// A Timeline with nothing in it says so calmly and draws no range (I7, P§4).
#[test]
fn an_empty_timeline_is_calm() {
    use porticus::views::{CardSpan, Timeline};

    let mut timeline = Timeline::of(Vec::<CardSpan>::new);
    assert_eq!(timeline.layout(), Layout::Full);
    assert!(
        timeline.rows(&Code::parse("ac").unwrap()).is_none(),
        "a Timeline is a draw-view: it paints itself (P§3)"
    );
    assert_eq!(timeline.locator().as_deref(), Some("no range"));
    assert_eq!(timeline.empty_line(), "no periods yet");
    assert!(
        timeline.target().is_none(),
        "nothing drawn is nothing focused — never a stale address (I1)"
    );
}
