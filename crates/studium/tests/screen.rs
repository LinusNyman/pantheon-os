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

/// The KTH scales (§19.3): `af` counts toward the GPA, `pf` does not.
const CURRICULUM: &str = r#"
university    = "kth"
default_scale = "af"
periods_per_year = 5

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
            "240801",
            "-r",
            "album:kth",
        ],
    );

    // Enrolments — courses group under the programme by ref, not nesting (I3, §19.1).
    for (slug, from, to) in [
        ("mekanik", "250110", Some("250601")),
        ("elektromagnetism", "250110", Some("250826")),
        ("projektkurs", "250110", Some("250601")),
        ("kvantfysik", "250115", None),
        ("flervariabel", "250115", None),
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
            "250601",
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
            "250310",
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
            "250825",
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
            "250601",
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
            "250815",
            "-r",
            "fasti:flervariabel",
        ],
    );
    // A study-time log — named for no course, so it is time, not a grade (§19.2, §19.6).
    run(
        root,
        "ann",
        &["-H", "asd", "add", "studytime", "3.5", "-c", "-a", "250601"],
    );
    run(
        root,
        "ann",
        &["-H", "asd", "add", "studytime", "2.0", "-a", "250602"],
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
