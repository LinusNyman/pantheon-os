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
        target: Target::Row(RecordRef::new(Code::parse(home).unwrap(), key.to_string())),
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

/// A passive overlay (Title/Help) **yields** to a navigation or chrome key: it dismisses
/// and the key reaches the base in one press, rather than being swallowed until `Esc`
/// (P§4). `Esc` still just closes.
#[test]
fn a_passive_overlay_yields_to_a_navigation_key() {
    let root = fresh_root();
    let up = |script: &str| {
        let mut app = Fake;
        porticus::drive(&mut app, &root, &porticus::keys(script), 72, 12).unwrap()
    };

    // `+` raises the Title overlay — its version line is the tell it is up.
    assert!(up("+").contains("format 2"), "title should be up");
    // `Esc` still just closes it — the one unwind that is unchanged.
    assert!(!up("+<esc>").contains("format 2"), "esc closes the title");
    // A non-Esc key yields: `+` then `?` closes the title and opens Help in one press.
    // Without the yield, `?` was swallowed and the title stayed up.
    let swapped = up("+?");
    assert!(
        !swapped.contains("format 2"),
        "title yielded to `?`: {swapped}"
    );
    assert!(
        swapped.contains("switch view"),
        "help came up in its place: {swapped}"
    );
}

/// **The Title splash is a full-page block-caps banner, no tagline** (P§8, C7, C6).
///
/// It was a small three-line box showing the tracked word and a tagline; `+` now paints
/// the name big in the embedded block face over the whole screen, and the tagline is gone
/// — the name is the signature. The version line stays, so a build is still identifiable.
#[test]
fn the_title_is_a_full_page_banner_without_a_tagline() {
    let root = fresh_root();
    let text = porticus::drive(&mut Fake, &root, &porticus::keys("+"), 72, 16).unwrap();
    assert!(
        text.contains('█'),
        "the name is drawn in the block face: {text}"
    );
    assert!(
        !text.contains("intention"),
        "the tagline no longer rides beside the name (C6): {text}"
    );
    assert!(
        text.contains("format 2"),
        "the splash still carries the version line: {text}"
    );
}

/// Narrow: the rail stacks above the content and the frame still renders in full — the
/// content is not squeezed to nothing (P§6). The split decision itself is unit-tested in
/// `runtime`; this pins that the stacked path actually draws.
#[test]
fn a_narrow_terminal_stacks_the_rail_above_the_content() {
    let root = fresh_root();
    let text = porticus::as_text(&porticus::render_once(&mut Fake, &root, 50, 16).unwrap());
    assert!(!text.starts_with("terminal too small"), "{text}");
    assert!(text.contains("actio"), "the rail drew: {text}");
    assert!(
        text.contains("no todos here"),
        "the content drew below it: {text}"
    );
}

/// `a` opens the add form even for a core that declares no fields: the default form is
/// the record's name alone, so `a` mints a record on every core rather than relaying the
/// nameless `add` the spine used to refuse (§7.3, P§7).
#[test]
fn a_opens_the_default_add_form() {
    struct Adder;
    impl App for Adder {
        fn ident(&self) -> Ident {
            Fake.ident()
        }
        fn lineup(&mut self) -> Vec<Box<dyn View>> {
            vec![Box::new(
                TreeFile::of(|_: &Code| Vec::new()).offering(&[Action::Add]),
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
    // `a` raises the form; it shows the required `name` field the default declares.
    let frame = porticus::drive(&mut Adder, &root, &porticus::keys("a"), 72, 12).unwrap();
    assert!(
        frame.contains("name"),
        "the default name field is shown: {frame}"
    );
}

/// A target names its core, so **one lens can act on several** (§12, P§7).
///
/// A row carries the core it was folded from; an add has no record yet, so the *view*
/// declares it and Porticus stamps the [`Target::Node`] it builds — for `a` and for the
/// pick-a-home modal's `A` alike. Without this a cross-core lens had to guess which
/// binary a keystroke meant, which is what Speculum's re-read-every-core hack was.
#[test]
fn a_target_names_the_core_its_view_declared() {
    /// Records what `on_action` was handed, so the test asserts the *target* rather
    /// than a rendered string.
    struct CrossCore {
        seen: Option<(String, Option<String>)>,
    }
    impl App for CrossCore {
        fn ident(&self) -> Ident {
            Fake.ident()
        }
        fn lineup(&mut self) -> Vec<Box<dyn View>> {
            vec![
                Box::new(
                    TreeFile::of(|node: &Code| vec![people_row("mara", node.as_str())])
                        .offering(&[Action::Add, Action::Remove])
                        .called("people")
                        .in_core("alb"),
                ),
                Box::new(
                    TreeFile::of(|_: &Code| Vec::new())
                        .offering(&[Action::Add])
                        .called("documents")
                        .in_core("tab"),
                ),
            ]
        }
        fn count_at(&mut self, _node: &Code) -> usize {
            0
        }
        fn writer(&self) -> Writer {
            Writer::InProcess
        }
        fn on_action(&mut self, action: Action, target: &Target) -> Option<Invocation> {
            let core = match target {
                Target::Row(record) => record.core.clone(),
                Target::Node { core, .. } => core.clone(),
            };
            self.seen = Some((action.label().to_owned(), core));
            None
        }
    }

    let root = fresh_root();

    // `a` on the people list: the add is Album's, because that view said so.
    let mut app = CrossCore { seen: None };
    porticus::drive(&mut app, &root, &porticus::keys("amara<enter>"), 72, 12).unwrap();
    assert_eq!(
        app.seen,
        Some(("add".to_owned(), Some("alb".to_owned()))),
        "the add target carries the view's core"
    );

    // The second view declares another — one lineup, two binaries (§12).
    let mut app = CrossCore { seen: None };
    porticus::drive(&mut app, &root, &porticus::keys("2anote<enter>"), 72, 12).unwrap();
    assert_eq!(
        app.seen,
        Some(("add".to_owned(), Some("tab".to_owned()))),
        "the second view's add reaches its own core"
    );

    // A **row** answers for itself: its core rode with it out of the fold, so a relay
    // on an existing record never consults the view at all.
    let mut app = CrossCore { seen: None };
    porticus::drive(&mut app, &root, &porticus::keys("x"), 72, 12).unwrap();
    assert_eq!(
        app.seen,
        Some(("remove".to_owned(), Some("map".to_owned()))),
        "the row's own core wins over the view's"
    );
}

/// A row whose core is not its view's — what a cross-core fold produces.
fn people_row(key: &str, home: &str) -> Row {
    Row {
        label: key.to_string(),
        target: Target::Row(RecordRef::in_core(
            "map",
            Code::parse(home).unwrap(),
            key.to_string(),
        )),
        when: None,
    }
}

/// `A` (quick add) opens the tree as a modal to pick a home (P§4) — replacing the old
/// type-a-code line prompt — and its box is headed so.
#[test]
fn quick_add_opens_the_tree_modal() {
    struct QuickAdder;
    impl App for QuickAdder {
        fn ident(&self) -> Ident {
            Fake.ident()
        }
        fn lineup(&mut self) -> Vec<Box<dyn View>> {
            vec![Box::new(
                TreeFile::of(|_: &Code| Vec::new()).offering(&[Action::QuickAdd]),
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
    let frame = porticus::drive(&mut QuickAdder, &root, &porticus::keys("A"), 72, 14).unwrap();
    assert!(
        frame.contains("pick a node"),
        "the tree modal opened: {frame}"
    );
}

/// **A Full view's `a` opens the same modal**, because a Full view draws no rail (P§3)
/// and so has no cursor to add at.
///
/// Before this, `a` on an Agenda, a Calendar, a Horizon or a Timeline built its target
/// from the invisible tree cursor — the first node in the tree on a fresh launch — so the
/// record landed at a home the hand never saw and was never shown. Fixed in Porticus
/// rather than per view, so it holds for all twelve at once (P-II).
#[test]
fn a_full_views_add_picks_its_home_off_the_tree() {
    struct FullAdder;
    impl App for FullAdder {
        fn ident(&self) -> Ident {
            Fake.ident()
        }
        fn lineup(&mut self) -> Vec<Box<dyn View>> {
            vec![Box::new(
                Agenda::of(|| vec![row("buy_milk", "ac")]).offering(&[Action::Add]),
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
    let frame = porticus::drive(&mut FullAdder, &root, &porticus::keys("a"), 72, 14).unwrap();
    assert!(
        frame.contains("pick a node"),
        "`a` on a Full view asks which home: {frame}"
    );
    // And taking a node hands off to the ordinary add form, so `a` still ends in one
    // question about the record itself.
    let frame =
        porticus::drive(&mut FullAdder, &root, &porticus::keys("a<enter>"), 72, 14).unwrap();
    assert!(frame.contains("name"), "the add form followed: {frame}");
}

/// The detour through the modal **keeps the date the view named** (P§4, P§7).
///
/// A Calendar's `a` is dated by its cell, and routing `a` through the pick-a-node modal
/// would have thrown that away had `Picking::Home` not re-read `view_at` when the node is
/// taken. So the cell still dates the add; only the home is now asked for out loud.
#[test]
fn a_picked_home_still_carries_the_views_date() {
    use porticus::views::Calendar;

    struct Dated {
        seen: Option<Target>,
    }
    impl App for Dated {
        fn ident(&self) -> Ident {
            Fake.ident()
        }
        fn lineup(&mut self) -> Vec<Box<dyn View>> {
            vec![Box::new(Calendar::of(Vec::new).offering(&[Action::Add]))]
        }
        fn count_at(&mut self, _node: &Code) -> usize {
            0
        }
        fn writer(&self) -> Writer {
            Writer::InProcess
        }
        fn on_action(&mut self, _a: Action, target: &Target) -> Option<Invocation> {
            self.seen = Some(target.clone());
            None
        }
    }

    let root = fresh_root();
    let mut app = Dated { seen: None };
    // `a`, take the node the modal opens on, then the form's name and submit.
    porticus::drive(&mut app, &root, &porticus::keys("a<enter>x<enter>"), 72, 20).unwrap();
    let Some(Target::Node { node, at, .. }) = app.seen else {
        panic!("the add reached `on_action` with a node target");
    };
    assert_eq!(node.as_str(), "a", "the home is the one taken in the modal");
    assert_eq!(
        at.expect("the calendar cell still dates the add").len(),
        8,
        "a reading key is YYYYMMDD (§6.1)"
    );
}

/// **Help lists the standard actions**, which it never did (P§4).
///
/// `?` showed the eleven Tier-1 chrome rows and stopped, so nothing on screen ever said
/// that `a` adds or `d` marks done — the two functions written for this
/// (`Action::label`, `keymap::key_for`) had no caller at all. The greying of an unoffered
/// action is style, which `as_text` strips, so it is pinned by a unit test in `runtime`.
#[test]
fn help_names_the_standard_actions_beside_the_chrome_keys() {
    let root = fresh_root();
    let frame = porticus::drive(&mut Fake, &root, &porticus::keys("?"), 90, 16).unwrap();
    // Tier 1 still there, unchanged.
    assert!(frame.contains("switch view"), "{frame}");
    // Tier 2 now beside it — the view's own (`d`, `e`) and the ones it leaves dark.
    for label in [
        "add",
        "edit",
        "done / toggle",
        "remove",
        "rename",
        "move",
        "quick add by code",
    ] {
        assert!(frame.contains(label), "help names {label}: {frame}");
    }
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

/// A tall list, labelled so its sort order is known: undated rows sort by label,
/// ascending, so `task_00`…`task_19` render in order. A Full row-view, so `motion`
/// drives the `state.row` branch rather than the rail.
struct Long;
impl App for Long {
    fn ident(&self) -> Ident {
        Fake.ident()
    }
    fn lineup(&mut self) -> Vec<Box<dyn View>> {
        vec![Box::new(Agenda::of(|| {
            (0..20)
                .map(|i| row(&format!("task_{i:02}"), "ac"))
                .collect()
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

/// **The cursor cannot scroll off the end of a list** (P§6, C1).
///
/// `motion`'s Down grew `state.row` unbounded; every reader (`draw_rows`,
/// `current_target`) re-clamped, so the *visible* cursor looked pinned while the counter
/// drifted past the end — and walking back up then moved nothing until the drift was
/// spent. Clamped, the counter cannot exceed the last index, so over-scrolling and
/// stepping all the way back returns exactly to the head; drifted, the extra Downs are
/// dead presses the Ups must first undo, so the same key count lands short of the top.
#[test]
fn the_cursor_stops_at_the_last_row() {
    let root = fresh_root();
    // Content height is 8 (10 rows less header and status), so a 20-row list scrolls.
    let downs = "<down>".repeat(25); // well past the 20th (last) row

    // Over-scrolling down parks the window at the foot: the last row shows, the first has
    // scrolled out — proving the viewport really is shorter than the list.
    let bottom = porticus::drive(&mut Long, &root, &porticus::keys(&downs), 72, 10).unwrap();
    assert!(
        bottom.contains("task_19"),
        "the last row sits in the over-scrolled window: {bottom}"
    );
    assert!(
        !bottom.contains("task_00"),
        "the list is genuinely scrolled — the first row is off-screen: {bottom}"
    );

    // 25 Downs then 19 Ups. Clamped, the counter tops out at 19, so 19 Ups return it to
    // row 0 and `task_00` is back at the head. Drifted, the counter reached 25, so 19 Ups
    // leave it at 6 — and `task_00` is still scrolled off. The window sits under scrolloff,
    // so this reads the cursor by which rows are on screen, not by the old bottom-anchor.
    let back = porticus::drive(
        &mut Long,
        &root,
        &porticus::keys(&format!("{downs}{}", "<up>".repeat(19))),
        72,
        10,
    )
    .unwrap();
    assert!(
        back.contains("task_00"),
        "stepping back the exact list length returns to the head — the counter was clamped, \
         not left to drift past the end: {back}"
    );
}

/// **Scrolling begins before the cursor reaches an edge** (P§6, C3).
///
/// The old viewport was bottom-anchored: `first = cursor - (height-1)`, so the list stayed
/// top-pinned until the cursor hit the very last line, then scrolled one-per-step. Scrolloff
/// centres the cursor through the body, so the window moves while rows are still visible
/// *below* the cursor — the scroll starts before the edge, and on a tall pane the cursor
/// rides the middle (the "start at the middle" the ask names).
#[test]
fn scrolling_keeps_the_cursor_off_the_edge() {
    let root = fresh_root();
    // Ten Downs on the 20-row list, content height 8: the cursor sits at row 10, centred,
    // so the window is rows 6–13 — `task_13` shows *below* the cursor and `task_00` above
    // has scrolled off. Bottom-anchored it would be rows 3–10 with the cursor on the last
    // line and nothing beneath it, so `task_13` never drew.
    let mid = porticus::drive(
        &mut Long,
        &root,
        &porticus::keys(&"<down>".repeat(10)),
        72,
        10,
    )
    .unwrap();
    assert!(
        mid.contains("task_10") && mid.contains("task_13"),
        "the cursor is mid-pane with rows still visible below it — the scroll began before \
         the edge: {mid}"
    );
    assert!(
        !mid.contains("task_00"),
        "and the list scrolled to get there — the head is off-screen: {mid}"
    );
}

/// **Search ranks the closest answer to the top** (P§6, C4).
///
/// The old filter kept matches in the list's own order, so a prefix hit could sit below a
/// mid-word one and the "top answer while typing" never appeared. Ranking floats
/// prefix > word-boundary > substring, so the nearest match surfaces first.
#[test]
fn search_ranks_the_closest_match_first() {
    struct Searchable;
    impl App for Searchable {
        fn ident(&self) -> Ident {
            Fake.ident()
        }
        fn lineup(&mut self) -> Vec<Box<dyn View>> {
            // Undated rows sort by label, so unfiltered the order is buy_milk,
            // call_the_dentist, milk_run. Search "milk" must reorder: milk_run (a prefix)
            // above buy_milk (a word-boundary match), and drop call_the_dentist (no match).
            vec![Box::new(Agenda::of(|| {
                vec![
                    row("buy_milk", "ac"),
                    row("call_the_dentist", "ac"),
                    row("milk_run", "ac"),
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
    // Submit the search so its overlay closes and the ranked rows draw unobstructed.
    let text = porticus::drive(
        &mut Searchable,
        &root,
        &porticus::keys("/milk<enter>"),
        72,
        12,
    )
    .unwrap();
    let milk_run = text.find("milk_run");
    let buy_milk = text.find("buy_milk");
    assert!(
        milk_run.is_some() && buy_milk.is_some(),
        "both matches are shown: {text}"
    );
    assert!(
        milk_run < buy_milk,
        "the prefix match `milk_run` ranks above the word-boundary match `buy_milk`: {text}"
    );
    assert!(
        !text.contains("call_the_dentist"),
        "a non-match is filtered out: {text}"
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
                when: Some("20260719".into()),
                ..row("later", "ac")
            },
            Row {
                when: Some("20260701".into()),
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
                            ("20260701".into(), 78.4),
                            ("20260708".into(), 78.1),
                            ("20260715".into(), 77.9),
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
                        chart: Chart::Heatmap(vec![
                            ("20260701".into(), 1.0),
                            ("20260702".into(), 0.0),
                        ]),
                    },
                    Panel {
                        title: "throughput".into(),
                        chart: Chart::Spark(vec![
                            ("20260701".into(), 3.0),
                            ("20260702".into(), 5.0),
                        ]),
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
                        chart: Chart::Trend(vec![
                            ("20260701".into(), 5.0),
                            ("20260702".into(), 5.0),
                        ]),
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
    lineup[1].pin(Some(RecordRef::new(node.clone(), "mara")));
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

/// **The dim and the badge share one memoized fold per node** (P§6).
///
/// The dim is `count_at > 0` and the badge is `count_at` — the *same* node-local fold,
/// which the rail memoizes for the life of the frame. So a node the badge shows is folded
/// once, not once for the dim and again for the count: that second fold, multiplied across
/// every visible node, was the cost that made walking the tree slow.
///
/// This asserts every visible node is folded, and that **none is folded twice in a frame**.
#[test]
fn the_dim_and_badge_share_one_fold_per_node() {
    use std::sync::{Arc, Mutex};

    struct Counting(Arc<Mutex<Vec<String>>>);
    impl App for Counting {
        fn ident(&self) -> Ident {
            Fake.ident()
        }
        fn lineup(&mut self) -> Vec<Box<dyn View>> {
            vec![Box::new(TreeFile::of(|_: &Code| Vec::new()))]
        }
        fn count_at(&mut self, node: &Code) -> usize {
            self.0.lock().unwrap().push(node.as_str().to_owned());
            // Only `ac` holds anything, so only its badge shows a number.
            usize::from(node.as_str() == "ac") * 7
        }
        fn writer(&self) -> Writer {
            Writer::InProcess
        }
        fn on_action(&mut self, _a: Action, _t: &Target) -> Option<Invocation> {
            None
        }
    }

    let root = fresh_root();
    let calls = Arc::new(Mutex::new(Vec::new()));
    let text = porticus::as_text(
        &porticus::render_once(&mut Counting(Arc::clone(&calls)), &root, 72, 12).unwrap(),
    );

    let calls = calls.lock().unwrap();
    // Every visible node is folded — the dim needs the count to know whether to dim…
    assert!(
        calls.iter().any(|c| c == "a") && calls.iter().any(|c| c == "ac"),
        "every visible node is folded once: {calls:?}"
    );
    // …and none is folded twice: the badge of a held node reuses the dim's memoized fold.
    let mut seen = std::collections::HashSet::new();
    for code in calls.iter() {
        assert!(
            seen.insert(code.clone()),
            "`{code}` was folded twice in one frame — the per-frame memo lapsed: {calls:?}"
        );
    }
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
            when: Some("20990101".into()),
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
    assert_eq!(at.len(), 8, "a reading key is YYYYMMDD (§6.1): {at}");

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
                            from: "20260101".into(),
                            to: Some("20260630".into()),
                            home: RecordRef::new(Code::parse("ac").unwrap(), "mvp_phase"),
                        },
                        CardSpan {
                            label: "residence".into(),
                            from: "20260201".into(),
                            // Open: drawn to the range's right edge (§8.4).
                            to: None,
                            home: RecordRef::new(Code::parse("cs").unwrap(), "residence"),
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
