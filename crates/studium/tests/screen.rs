//! The GPA fold and Studium's real screen, driven over the live cores (§19.4, §19.9, §12).
//!
//! The figure that names the lens is folded across three cores at once — Fasti the
//! enrolment's period, Annales its result, a `curriculum.toml` the scale to weigh it — and
//! nothing about that crossing (the `PATH` discovery, the `-C <root>`, the JSON coming
//! back) was until now exercised by anything but a hand. So: seed a studies subtree with
//! the real `pan`/`fas`/`ann`, put the built binaries on `PATH`, and read the figures back
//! through `stu` itself and through its real screen.
//!
//! **One test, alone in its own test binary, on purpose.** It mutates `PATH`, which is
//! process-global; Cargo gives each integration-test file its own process, so a lone test
//! here cannot race anything (the same reason Atrium isolates its relay).

#![cfg(feature = "tui")]

use std::path::{Path, PathBuf};
use std::process::Command;

use pantheon::Code;
use serde_json::Value;
use studium::Studium;

/// Where Cargo put the workspace's binaries — the directory `stu` itself is in, so the
/// sibling cores sit beside it. Found from `stu` rather than a core's `CARGO_BIN_EXE_*`,
/// because **Studium depends on no core** and could not name one (I5).
fn bin_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_stu"))
        .parent()
        .expect("a binary has a directory")
        .to_path_buf()
}

/// Run a built binary by absolute path — the fixture writes need no `PATH`.
fn run(root: &Path, short: &str, args: &[&str]) {
    let bin = bin_dir().join(short);
    assert!(
        bin.exists(),
        "`{short}` is not built. A lens's fold test drives other tools' binaries, so \
         `cargo build --workspace --bins` has to run first."
    );
    let out = Command::new(bin)
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .unwrap_or_else(|e| panic!("running {short}: {e}"));
    assert!(
        out.status.success(),
        "{short} {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// The KTH scales (§19.3) and its academic calendar (§19.5): `af` counts toward the GPA,
/// `pf` does not; the five periods are the year-less anchors a placement is read against.
const CURRICULUM: &str = r#"
university    = "kth"
default_scale = "af"
periods_per_year = 5

terms = [
  { slug = "ht", periods = ["P1","P2"] },
  { slug = "vt", periods = ["P3","P4"] },
]
periods = [
  { n = 1, slug = "P1", term = "ht", start = "0826", end = "1025" },
  { n = 2, slug = "P2", term = "ht", start = "1026", end = "0114" },
  { n = 3, slug = "P3", term = "vt", start = "0115", end = "0315" },
  { n = 4, slug = "P4", term = "vt", start = "0316", end = "0602" },
  { n = 5, slug = "P5", term = "summer", start = "0603", end = "0825" },
]

[scale.af]
counts_in_gpa = true
grades  = { A = 5, B = 4, C = 3, D = 2, E = 1, Fx = 0, F = 0 }
passing = ["A", "B", "C", "D", "E"]

[scale.pf]
counts_in_gpa = false
grades  = { P = 0, F = 0 }
passing = ["P"]
"#;

/// A studies subtree under Disciplina (`asd`), with enough shape to exercise every rule of
/// the GPA fold: a closed graded course, a **retake** whose best grade is not its latest, a
/// **pass/fail** course, an **open** course with no grade, and an **open** course whose
/// credits are in progress. Plus a study-time log and a governing curriculum.
#[allow(clippy::too_many_lines)]
fn seed(root: &Path) {
    run(root, "pan", &["new", "root", "a", "actio", "-y"]);
    run(root, "pan", &["new", "a", "s", "scientia", "-y"]);
    run(root, "pan", &["new", "as", "d", "disciplina", "-y"]);

    // The curriculum governs `asd` and everything under it (§6.3, §19.3).
    let asd = pantheon::resolve_code(root, &Code::parse("asd").unwrap()).unwrap();
    std::fs::write(asd.join("asd_curriculum.toml"), CURRICULUM).unwrap();

    // A programme is a span other spans point at (§19.1) — open until the degree ends.
    run(
        root,
        "fas",
        &[
            "-H",
            "asd",
            "add",
            "teknisk_fysik",
            "--from",
            "20240801",
            "-r",
            "album:kth",
        ],
    );

    // Enrolments — courses group under the programme by ref, not nesting (I3, §19.1).
    for (slug, from, to) in [
        ("mekanik", "20250110", Some("20250601")),
        ("elektromagnetism", "20250110", Some("20250826")),
        ("projektkurs", "20250110", Some("20250601")),
        ("kvantfysik", "20250115", None),
        ("flervariabel", "20250115", None),
    ] {
        let mut args = vec!["-H", "asd", "add", slug, "--from", from];
        if let Some(to) = to {
            args.push("--to");
            args.push(to);
        }
        args.push("-r");
        args.push("fasti:teknisk_fysik");
        run(root, "fas", &args);
    }

    // Grades are facts paired to their span by slug (§19.2): `values: [grade, credits]`.
    // mekanik: a plain B (counts, 7.5 hp).
    run(
        root,
        "ann",
        &[
            "-H",
            "asd",
            "add",
            "mekanik",
            "B",
            "7.5",
            "-c",
            "-a",
            "20250601",
            "-r",
            "fasti:mekanik",
        ],
    );
    // elektromagnetism: a retake — A at the first sitting, C at the re-sit. Best is A,
    // even though C is the latest (§19.4).
    run(
        root,
        "ann",
        &[
            "-H",
            "asd",
            "add",
            "elektromagnetism",
            "A",
            "6.0",
            "-c",
            "-a",
            "20250310",
            "-r",
            "fasti:elektromagnetism",
        ],
    );
    run(
        root,
        "ann",
        &[
            "-H",
            "asd",
            "add",
            "elektromagnetism",
            "C",
            "6.0",
            "-a",
            "20250825",
            "-r",
            "fasti:elektromagnetism",
        ],
    );
    // projektkurs: a pass/fail P — out of the mean, its credits still completed (§19.4).
    run(
        root,
        "ann",
        &[
            "-H",
            "asd",
            "add",
            "projektkurs",
            "P",
            "7.5",
            "-c",
            "-a",
            "20250601",
            "-r",
            "fasti:projektkurs",
        ],
    );
    // flervariabel: an open course that already has a grade — out of the mean because the
    // span is open, its credits in progress (§19.4).
    run(
        root,
        "ann",
        &[
            "-H",
            "asd",
            "add",
            "flervariabel",
            "E",
            "7.5",
            "-c",
            "-a",
            "20250815",
            "-r",
            "fasti:flervariabel",
        ],
    );
    // Two tasks: one inside the programme's subtree, one outside it entirely. The
    // programme switch is what tells them apart (N2, §19.6).
    run(root, "pen", &["add", "-H", "asd", "read_chapter", "-y"]);
    run(root, "pen", &["add", "-H", "a", "buy_milk", "-y"]);

    // A professor, referenced from a course's grade fact — "contacts" is the fold over the
    // people a course's records point at (§19.6), never a copy under the course (I3).
    run(
        root,
        "alb",
        &["-H", "asd", "add", "Ada Prof", "-k", "person"],
    );
    run(
        root,
        "ann",
        &[
            "-H",
            "asd",
            "add",
            "mekanik",
            "B",
            "7.5",
            "-a",
            "20250602",
            "-r",
            "album:ada_prof",
        ],
    );

    // A reflection — a Tabella document whose `type` is `reflection` (§19.6, §8.7).
    run(
        root,
        "tab",
        &[
            "-H",
            "asd",
            "add",
            "Mekanik Retrospective",
            "--type",
            "reflection",
        ],
    );

    // A study-time log — named for no course, so it is time, not a grade (§19.2, §19.6).
    run(
        root,
        "ann",
        &[
            "-H",
            "asd",
            "add",
            "studytime",
            "3.5",
            "-c",
            "-a",
            "20250601",
        ],
    );
    run(
        root,
        "ann",
        &["-H", "asd", "add", "studytime", "2.0", "-a", "20250602"],
    );
}

/// Put the cores on `PATH` the way a lens finds them (§12), so `stu` and its screen can
/// spawn `fas`/`ann` by name.
fn with_cores_on_path() {
    // SAFETY: this is the only test in this binary, so nothing else reads the environment
    // concurrently. Cargo gives every integration-test file its own process.
    let path = std::env::var_os("PATH").unwrap_or_default();
    let mut dirs = vec![bin_dir()];
    dirs.extend(std::env::split_paths(&path));
    let joined = std::env::join_paths(dirs).expect("a joinable PATH");
    unsafe { std::env::set_var("PATH", &joined) };
}

fn figures(root: &Path) -> Value {
    let out = Command::new(bin_dir().join("stu"))
        .arg("-C")
        .arg(root)
        .args(["-f", "json"])
        .output()
        .expect("stu runs");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).expect("stu emits JSON")
}

fn about(value: &Value, target: f64) -> bool {
    value.as_f64().is_some_and(|v| (v - target).abs() < 0.001)
}

/// C2: `a` on the courses view opens the grade form and relays `ann add`, recording a mark
/// on the enrolment under the rail cursor (§19.8, §12).
///
/// The form and its relay were always wired; only the Courses view's action offering was
/// missing, so `a` sat dark. Driven end to end here: land on the studies node, fill the
/// form, submit, and read the fact back through `ann` — a keystroke in the lens becoming a
/// write by another process.
fn records_a_grade_through_the_lens(root: &Path) {
    // Land the cursor on the studies node `asd`, where the enrolments live: a fresh launch
    // expands only the top sphere, so `a` (actio) is open and `as` (scientia) is not — step
    // down onto `as`, expand it, then step in to `asd`.
    let on_asd = "2<down><right><down>";
    let at_asd = porticus::drive(
        &mut Studium::new(root),
        root,
        &porticus::keys(on_asd),
        100,
        24,
    )
    .expect("the lens drives");
    assert!(
        at_asd.contains("mekanik"),
        "the rail is on `asd`, its enrolments folded into the courses view: {at_asd}"
    );

    // `a` raises the grade form: its `grade`/`credits` field labels show — words the
    // read-only course rows never print (they carry the grade *value*, not the word).
    let form = porticus::drive(
        &mut Studium::new(root),
        root,
        &porticus::keys(&format!("{on_asd}a")),
        100,
        24,
    )
    .expect("the lens drives");
    assert!(
        form.contains("grade") && form.contains("credits"),
        "`a` on the courses view opens the grade form (it was dark before C2): {form}"
    );

    // Fill it for a mekanik retake and submit. Add does not confirm, so Enter relays at
    // once (P§5); the fields are course · grade · credits · date, Tab between them. This
    // records `ann add -H asd mekanik A 7.5 --at 20250701` — an A after the seeded B.
    porticus::drive(
        &mut Studium::new(root),
        root,
        &porticus::keys(&format!(
            "{on_asd}amekanik<tab>A<tab>7.5<tab>20250701<enter>"
        )),
        100,
        24,
    )
    .expect("the lens drives the add");

    // Read the fact back through the core: the present is the latest reading (I1), so
    // mekanik now grades A, not the seeded B.
    let out = Command::new(bin_dir().join("ann"))
        .arg("-C")
        .arg(root)
        .args(["get", "mekanik", "-f", "json"])
        .output()
        .expect("ann runs");
    assert!(
        out.status.success(),
        "ann get mekanik: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let present: Value = serde_json::from_slice(&out.stdout).expect("ann emits JSON");
    assert_eq!(
        present["data"]["values"][0].as_str(),
        Some("A"),
        "`a` in the lens recorded the retake through `ann` — mekanik's present grade is now \
         the A just entered, not the seeded B: {present}"
    );
}

/// **N2 — the studies are folded one programme at a time** (§19.4, §19.6).
///
/// Studium folded the whole tree: a study screen showed the shopping, because `pen list`
/// ran with no `-H` at all. The scope is now the node a curriculum governs — discovered,
/// never declared (§19.3) — and `]`/`[`/`p` step through the programmes as view state,
/// stored nowhere (§19.4, §18).
fn the_switch_scopes_the_studies(root: &Path) {
    let drive = |script: &str| {
        porticus::drive(
            &mut Studium::new(root),
            root,
            &porticus::keys(script),
            100,
            24,
        )
        .expect("the lens drives")
    };

    // Unscoped, the agenda is every open task — the tree-wide fold this always was.
    let all = drive("3");
    assert!(
        all.contains("read chapter") && all.contains("buy milk"),
        "all the studies is the whole tree: {all}"
    );
    assert!(
        all.contains("all studies"),
        "and the header says which scope that is: {all}"
    );

    // `]` steps onto the one discovered programme — the node `asd_curriculum.toml`
    // governs — and the agenda narrows to its subtree.
    let scoped = drive("3]");
    assert!(
        scoped.contains("read chapter"),
        "the programme's own task stays: {scoped}"
    );
    assert!(
        !scoped.contains("buy milk"),
        "a task outside the programme is out of scope (N2): {scoped}"
    );
    assert!(
        scoped.contains("disciplina") && scoped.contains("asd"),
        "the header names the programme being folded (P§4): {scoped}"
    );

    // The switch works from any view, not only the dashboard that owns the figures: the
    // key is wrapped around every view in the lineup (I3).
    let from_mosaic = drive("]");
    assert!(
        from_mosaic.contains("disciplina"),
        "the mosaic switches too, and names its scope: {from_mosaic}"
    );

    // `p` returns to all the studies; `]` past the last programme wraps through it, so
    // one key reaches every scope a study life has.
    assert!(
        drive("3]p").contains("buy milk"),
        "`p` returns to all the studies"
    );
    assert!(
        drive("3]]").contains("buy milk"),
        "and the cycle wraps through all, not back to the first programme"
    );
}

/// **G2 — the derivations §19.6 names but nothing read** (people, reflections, the
/// occurrences ahead), and **G3 — the relays §19.8 lists but nothing wired** (close an
/// enrolment, log study time, place an exam).
///
/// Six tabs, one `on_action`, and the core each write reaches named by the *address* the
/// row or the view carries (G8) — not guessed from the action.
fn the_rest_of_a_study_life(root: &Path) {
    let drive = |script: &str| {
        porticus::drive(
            &mut Studium::new(root),
            root,
            &porticus::keys(script),
            100,
            24,
        )
        .expect("the lens drives")
    };

    // ── the tabs §19.6 asks for ──────────────────────────────────────────────
    // Reached by their number keys rather than read off the tab strip: seven tabs is more
    // than 100 columns of strip, so the last one is legitimately clipped on this width and
    // the switch is what proves it is in the lineup (P§4).
    for (key, locator) in [
        ("4", "by date"),
        ("5", "by date"),
        ("6", "who this points at"),
        ("7", "what it wrote about itself"),
    ] {
        let frame = drive(key);
        assert!(
            frame.contains(locator),
            "view {key} is in the lineup and names itself: {frame}"
        );
    }

    // People is a fold over the *references* a study life's records make (§19.6).
    let people = drive("6");
    assert!(
        people.contains("ada prof"),
        "the professor a grade fact points at (§19.6): {people}"
    );
    // Reflections is a fold over Tabella documents typed `reflection` (§8.7).
    let reflections = drive("7");
    assert!(
        reflections.contains("mekanik retrospective"),
        "the reflection, from its frontmatter and no further (§6.1): {reflections}"
    );

    // ── G3: log study time (§19.8) ───────────────────────────────────────────
    // `5` is the study tab; `a` opens **its own** form, not the app's grade form — a
    // lens's `a` means a different record on each tab.
    //
    // The `<enter>` before the form is the pick-a-node modal: a study tab is an Agenda, a
    // Full view draws no rail, and Porticus therefore asks which home before opening the
    // form rather than filing at a cursor the hand cannot see (P§4). Taking the node the
    // modal opens on is the home this write used to land at silently.
    let form = drive("5a<enter>");
    assert!(
        form.contains("hours") && !form.contains("credits"),
        "the study tab's `a` opens the study form, not the grade form: {form}"
    );
    drive("5a<enter>lectures<tab>2.5<tab>20250610<tab>y<enter>");
    let logged = Command::new(bin_dir().join("ann"))
        .arg("-C")
        .arg(root)
        .args(["series", "lectures", "-f", "json"])
        .output()
        .expect("ann runs");
    let lines: Value = serde_json::from_slice(&logged.stdout).unwrap_or_default();
    assert_eq!(
        lines[0]["data"]["values"][0].as_str(),
        Some("2.5"),
        "`a` on the study tab logged hours through `ann` (§19.8): {lines}"
    );

    // ── G3: place an exam (§19.8) ────────────────────────────────────────────
    // `4` is the deadlines tab. Its form is Fasti's: series, what, date, what it
    // concerns, and the explicit mint — five fields, none of them a grade's.
    drive("4a<enter>tentor<tab>mekanik tenta<tab>20260315<tab>fasti:mekanik<tab>y<enter>");
    let placed = Command::new(bin_dir().join("fas"))
        .arg("-C")
        .arg(root)
        .args(["series", "tentor", "-f", "json"])
        .output()
        .expect("fas runs");
    let events: Value = serde_json::from_slice(&placed.stdout).unwrap_or_default();
    assert_eq!(
        events[0]["key"].as_str(),
        Some("20260315"),
        "`a` on the deadlines tab placed a Fasti event (§19.8, §8.4): {events}"
    );
    assert_eq!(
        events[0]["refs"][0].as_str(),
        Some("fasti:mekanik"),
        "referencing the enrolment it concerns — what makes a sitting an exam (§8.4)"
    );

    // ── G3: close an enrolment (§19.8) ───────────────────────────────────────
    // `e` on a courses row prompts for the day and relays `fas edit <span> --to <date>`.
    // `kvantfysik` was seeded open; `<down>` past mekanik/elektromagnetism/flervariabel
    // is fragile, so search picks it out — `/` narrows the rows and ranks the match first.
    let on_asd = "2<down><right><down>";
    drive(&format!("{on_asd}<tab>/kvantfysik<enter>e20260610<enter>y"));
    let closed = Command::new(bin_dir().join("fas"))
        .arg("-C")
        .arg(root)
        .args(["get", "kvantfysik", "-f", "json"])
        .output()
        .expect("fas runs");
    let span: Value = serde_json::from_slice(&closed.stdout).unwrap_or_default();
    assert_eq!(
        span["data"]["to"].as_str(),
        Some("20260610"),
        "`e` on a courses row closed the enrolment through `fas` (§19.8): {span}"
    );
}

/// One test on purpose: it mutates `PATH` once (a process-global the harness must not
/// race), then folds a rich tree, an empty one, and the real screen over that one `PATH`.
#[test]
fn the_gpa_folds_across_three_cores_and_the_screen_shows_it() {
    let root = std::env::temp_dir().join(format!("stu-screen-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    seed(&root);
    with_cores_on_path();

    // ── the figures behind the mosaic (§19.9) ────────────────────────────────
    let f = figures(&root);

    // GPA = Σ(value × credits) / Σ(credits) over completed, passing, counting courses:
    // mekanik B(4)×7.5 + em best A(5)×6.0 over 13.5 credits = 60/13.5 = 4.44.
    assert!(
        about(&f["gpa"], 4.44),
        "gpa is the credit-weighted mean: {f}"
    );
    // Excluding the open course proves it: with flervariabel's E folded in it would be
    // 3.21, and taking em's latest C rather than its best A it would be 3.56.

    // Completed credits count the pass/fail course; the mean does not (§19.4):
    // 7.5 + 6.0 + 7.5 = 21.0.
    assert!(
        about(&f["credits_completed"], 21.0),
        "pass/fail credits still complete: {f}"
    );
    // In progress: flervariabel's 7.5; kvantfysik has no fact and adds nothing (§19.2).
    assert!(
        about(&f["credits_in_progress"], 7.5),
        "open credits, from the fact: {f}"
    );
    // The two open enrolments; the programme span is not a course (§19.1).
    assert_eq!(f["open_courses"].as_u64(), Some(2), "open courses: {f}");
    // Study time, summed from the non-course log (§19.6): 3.5 + 2.0.
    assert!(about(&f["study_hours"], 5.5), "study hours fold: {f}");

    // ── the same fold, drawn (§19.9, I8) ─────────────────────────────────────
    let frame = porticus::drive(
        &mut Studium::new(&root),
        &root,
        &porticus::keys(""),
        100,
        24,
    )
    .expect("the lens draws");
    assert!(
        frame.contains("GPA"),
        "the mosaic leads with the GPA: {frame}"
    );
    assert!(
        frame.contains("4.44"),
        "and shows the folded figure: {frame}"
    );

    // The lineup is legal and browsable: the courses and tasks views switch in (P§3).
    let courses = porticus::drive(
        &mut Studium::new(&root),
        &root,
        &porticus::keys("2"),
        100,
        24,
    )
    .expect("the lens drives");
    assert!(
        courses.contains("courses"),
        "the second view is courses: {courses}"
    );

    // ── G1: §19.5's period placement, derived and drawn ──────────────────────
    // Mekanik ran `20250110 → 20250601`, in a programme that began `20240801` — study year one,
    // an interval covering P2, P3 and P4. Nothing stores it (I1): the span carries a `from`
    // and a `to`, the curriculum carries year-less anchors, and the label is the fold.
    let placed = porticus::drive(
        &mut Studium::new(&root),
        &root,
        &porticus::keys("2<down><right><down>"),
        100,
        24,
    )
    .expect("the lens drives");
    assert!(
        placed.contains("P2–P4"),
        "the courses row places the enrolment absolutely (§19.5): {placed}"
    );
    let tasks = porticus::drive(
        &mut Studium::new(&root),
        &root,
        &porticus::keys("3"),
        100,
        24,
    )
    .expect("the lens drives");
    assert!(
        tasks.contains("agenda"),
        "the third view is the tasks agenda: {tasks}"
    );

    // ── C2: `a` on the courses view records a grade (§19.8, §12) ──────────────
    records_a_grade_through_the_lens(&root);

    // ── N2: the programme switch scopes the screen (§19.4, §19.6) ─────────────
    the_switch_scopes_the_studies(&root);

    // ── G2/G3: the rest of §19.6's tabs and §19.8's relays ────────────────────
    the_rest_of_a_study_life(&root);

    // ── an empty scope is no GPA, not a zero (§19.4) ─────────────────────────
    // A studies life with no grade fact yet: the fold ran and found nothing to weigh, so
    // the GPA is `null` — the honest dash, never `0.0` (the count-vs-null discipline).
    // Completed credits, by contrast, are a real `0.0`.
    let empty = std::env::temp_dir().join(format!("stu-empty-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&empty);
    std::fs::create_dir_all(&empty).unwrap();
    run(&empty, "pan", &["new", "root", "a", "actio", "-y"]);
    run(&empty, "pan", &["new", "a", "s", "scientia", "-y"]);

    let e = figures(&empty);
    assert!(e["gpa"].is_null(), "no graded course is no GPA: {e}");
    assert!(
        about(&e["credits_completed"], 0.0),
        "but completed credits are a real zero: {e}"
    );
    assert_eq!(
        e["open_courses"].as_u64(),
        Some(0),
        "and no open courses: {e}"
    );
}
