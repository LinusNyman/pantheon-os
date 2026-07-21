//! The folds that name the lens (§19.4, §19.9).
//!
//! Studium mints nothing (I1): a grade, a credit, an hour given, an exam are each some
//! core's record already, reached over `PATH` as JSON (I4, I5). The whole substance here
//! is the reduction that reads them together — above all the **GPA**, the credit-weighted
//! mean of graded enrolments, derived on sight and stored nowhere (§8.3, I1).
//!
//! Every figure obeys the **count-versus-null discipline** (§12): a core off `PATH` is
//! `null`, never `0` — an absent Fasti is not a GPA of zero, and no graded course yet is a
//! `gpa` of `null`, the honest dash that the fold ran and found nothing to weigh.

use std::collections::HashSet;
use std::path::Path;

use serde_json::{Value, json};

use crate::curriculum::{self, Curriculum};

/// The cores a study life folds from (§19.6). Discovered, never required: a figure whose
/// core is absent is `null`, and the fold degrades to what it finds (§12).
const FASTI: &str = "fas";
const ANNALES: &str = "ann";

/// The §19.9 surface: the figures behind the mosaic, as one object.
///
/// `home` scopes the fold as every fold is scoped — `-H` narrows the enrolments read, the
/// same lever net worth takes (§6.3, §19.4). The GPA is keyed to the grade fact, not the
/// node, so a course anywhere in scope is weighed.
#[must_use]
pub fn figures(root: &Path, home: Option<&str>) -> Value {
    let fasti = spans(root, home);
    let annales_present = read(root, ANNALES, &["list"]).is_some();
    let curricula = curriculum::discover(root);

    // Fasti absent → every enrolment figure is null; it is not an absence of courses (§12).
    let Some(spans) = fasti else {
        return json!({
            "gpa": Value::Null,
            "credits_completed": Value::Null,
            "credits_in_progress": Value::Null,
            "open_courses": Value::Null,
            "study_hours": study_hours(root, annales_present, &HashSet::new()),
            "next_exam": Value::Null,
        });
    };

    let programmes = programmes(&spans);
    let courses: Vec<&Value> = spans
        .iter()
        .filter(|s| slug(s).is_some_and(|slug| !programmes.contains(slug)))
        .collect();

    let mut open_courses = 0usize;
    // The GPA sums; `any` distinguishes "no graded course" (null) from a mean of zero.
    let mut num = 0.0f64;
    let mut den = 0.0f64;
    let mut credits_completed = 0.0f64;
    let mut credits_in_progress = 0.0f64;

    for course in &courses {
        let Some(slug) = slug(course) else { continue };
        let closed = course["data"]["to"].as_str().is_some();

        // The grade is a fact paired to the span by its slug (§19.2): `fasti:<slug>` and
        // `annales:<slug>` at the same node, read one from the other. No log → no grade.
        let Some(readings) = read(root, ANNALES, &["series", slug]) else {
            if !closed {
                open_courses += 1;
            }
            continue;
        };
        let Some(readings) = readings.as_array() else {
            continue;
        };

        let graded: Vec<Graded> = readings
            .iter()
            .filter_map(|line| evaluate(line, &curricula))
            .collect();

        if closed {
            // Completed = a closed span with a passing grade (§19.4). Best passing wins on
            // a retake — the highest that passed, recomputed on sight, never stored.
            if let Some(best) = best_passing(&graded) {
                credits_completed += best.credits;
                // The GPA takes the best passing grade whose scale counts (§19.4); a
                // pass/fail credit is completed but stays out of the mean.
                if let Some(g) = best_passing_counting(&graded) {
                    num += g.value * g.credits;
                    den += g.credits;
                }
            }
        } else {
            open_courses += 1;
            // An open enrolment is out of the mean (§19.4). Its credits are in progress if
            // the fact records them yet — §19.2 leaves them off until earned, so a course
            // with no fact simply adds nothing here.
            if let Some(latest) = graded.last() {
                credits_in_progress += latest.credits;
            }
        }
    }

    let gpa = if den > 0.0 {
        json!(round2(num / den))
    } else {
        Value::Null
    };

    json!({
        "gpa": gpa,
        "credits_completed": present(annales_present, credits_completed),
        "credits_in_progress": present(annales_present, credits_in_progress),
        "open_courses": open_courses,
        "study_hours": study_hours(root, annales_present, &course_slugs(&spans)),
        "next_exam": next_exam(root, home, &today_yymmdd()),
    })
}

/// One grade reading, weighed against its governing scale (§19.4).
struct Graded {
    credits: f64,
    /// The GPA value of the symbol, `0.0` where its scale does not count it.
    value: f64,
    passing: bool,
    counts_in_gpa: bool,
}

/// Read one Annales grade line as a weighed grade (§19.2): `values: [grade, credits]`,
/// scale resolved by the curriculum governing the fact's node (§19.4).
///
/// A grade whose symbol no governing scale holds cannot be valued, so it is dropped — the
/// same calm absence a missing curriculum yields, never a guessed number.
fn evaluate(line: &Value, curricula: &[(pantheon::Code, Curriculum)]) -> Option<Graded> {
    let values = line["data"]["values"].as_array()?;
    let grade = values.first()?.as_str()?;
    let credits = values.get(1)?.as_str()?.parse::<f64>().ok()?;
    let home = line["home"].as_str()?;

    let scale = curriculum::governing(curricula, home)?.scale_holding(grade)?;
    Some(Graded {
        credits,
        value: scale.value(grade).unwrap_or(0.0),
        passing: scale.is_passing(grade),
        counts_in_gpa: scale.counts_in_gpa,
    })
}

/// The best passing grade among a course's attempts (§19.4) — highest value, order-blind,
/// so "best" is a fold over every attempt and not the latest sitting.
fn best_passing(graded: &[Graded]) -> Option<&Graded> {
    graded
        .iter()
        .filter(|g| g.passing)
        .max_by(|a, b| a.value.total_cmp(&b.value))
}

/// The best passing grade whose scale counts toward the mean (§19.4).
fn best_passing_counting(graded: &[Graded]) -> Option<&Graded> {
    graded
        .iter()
        .filter(|g| g.passing && g.counts_in_gpa)
        .max_by(|a, b| a.value.total_cmp(&b.value))
}

/// Study time (§19.6): a `log` of hours given, one dated line per session. The grade logs
/// are named for their courses (§19.2), so **every other log in scope is study time** —
/// its first value, summed where it reads as a number.
///
/// `null` where Annales is off `PATH`; a sum (possibly `0.0`) where it answers (§12).
fn study_hours(root: &Path, annales_present: bool, course_slugs: &HashSet<String>) -> Value {
    if !annales_present {
        return Value::Null;
    }
    let Some(logs) = read(root, ANNALES, &["list"]).and_then(|v| array(&v)) else {
        return json!(0.0);
    };
    let mut hours = 0.0f64;
    for log in &logs {
        let Some(name) = log["series"].as_str() else {
            continue;
        };
        if course_slugs.contains(name) {
            continue; // a grade log, not a study-time log (§19.2)
        }
        if let Some(lines) = read(root, ANNALES, &["series", name]).and_then(|v| array(&v)) {
            for line in &lines {
                if let Some(h) = line["data"]["values"]
                    .as_array()
                    .and_then(|v| v.first())
                    .and_then(Value::as_str)
                    .and_then(|s| s.parse::<f64>().ok())
                {
                    hours += h;
                }
            }
        }
    }
    json!(round2(hours))
}

/// The next exam (§19.6, §19.9): the earliest upcoming Fasti `event` referencing a course.
///
/// "Upcoming" is relative to `today`, which is the one place the live fold reads the clock
/// — every other figure is folded from dated records alone (§19.4). `null` where Fasti is
/// absent or nothing is scheduled ahead (§12). Filtering exams from other events (a
/// deadline) is deferred; any course-referencing occurrence is a candidate.
fn next_exam(root: &Path, home: Option<&str>, today: &str) -> Value {
    let mut args = vec!["list", "-k", "event"];
    if let Some(home) = home {
        args.push("-H");
        args.push(home);
    }
    let Some(series_rows) = read(root, FASTI, &args).and_then(|v| array(&v)) else {
        return Value::Null;
    };

    let mut events: Vec<(String, String)> = Vec::new();
    for row in &series_rows {
        let Some(name) = row["series"].as_str() else {
            continue;
        };
        let Some(lines) = read(root, FASTI, &["series", name]).and_then(|v| array(&v)) else {
            continue;
        };
        for line in &lines {
            let Some(course) = course_ref(line) else {
                continue;
            };
            if let Some(date) = line["key"].as_str().map(day) {
                events.push((date, course));
            }
        }
    }

    match pick_next(&mut events, today) {
        Some((date, course)) => json!({ "date": date, "course": course }),
        None => Value::Null,
    }
}

/// The earliest event on or after `today` — a pure pick, so the "upcoming" rule is
/// testable without the wall clock (§19.4).
fn pick_next(events: &mut [(String, String)], today: &str) -> Option<(String, String)> {
    events.sort_by(|a, b| a.0.cmp(&b.0));
    events
        .iter()
        .find(|(date, _)| date.as_str() >= today)
        .cloned()
}

// ── reading the cores (I4, §12) ──────────────────────────────────────────────

/// The enrolment spans in scope (§19.1), or `None` where Fasti is off `PATH`.
fn spans(root: &Path, home: Option<&str>) -> Option<Vec<Value>> {
    let mut args = vec!["list", "-k", "span"];
    if let Some(home) = home {
        args.push("-H");
        args.push(home);
    }
    read(root, FASTI, &args).and_then(|v| array(&v))
}

/// The programme spans — the ones a course points *at* (§19.1). A programme is itself a
/// span, so a course is a span **no other span references**; distinguishing the two by the
/// edge, never by a directory (I3).
fn programmes(spans: &[Value]) -> HashSet<String> {
    let mut out = HashSet::new();
    for span in spans {
        if let Some(refs) = span["refs"].as_array() {
            for r in refs {
                if let Some(slug) = r.as_str().and_then(|t| t.strip_prefix("fasti:")) {
                    out.insert(slug.to_owned());
                }
            }
        }
    }
    out
}

/// Every span's slug — the grade-log names (§19.2), so study time can tell its own logs
/// from the courses'.
fn course_slugs(spans: &[Value]) -> HashSet<String> {
    spans
        .iter()
        .filter_map(|s| slug(s).map(str::to_owned))
        .collect()
}

fn slug(span: &Value) -> Option<&str> {
    span["slug"].as_str()
}

/// The course an event references (§8.4): the slug of its first `fasti:` ref.
fn course_ref(line: &Value) -> Option<String> {
    line["refs"]
        .as_array()?
        .iter()
        .filter_map(Value::as_str)
        .find_map(|t| t.strip_prefix("fasti:").map(str::to_owned))
}

/// The day part of a series key — `260315` from `260315` or `260315T0900` (§5.4).
fn day(key: &str) -> String {
    key.split('T').next().unwrap_or(key).to_owned()
}

fn read(root: &Path, short: &str, args: &[&str]) -> Option<Value> {
    tessera::read(root, short, args)
}

fn array(value: &Value) -> Option<Vec<Value>> {
    value.as_array().cloned()
}

/// A figure the fold computed, or `null` where its core is absent (§12).
fn present(core_present: bool, value: f64) -> Value {
    if core_present {
        json!(round2(value))
    } else {
        Value::Null
    }
}

/// Two decimals, the GPA's shape (§19.9 shows `4.09`). No cast: arithmetic on `f64`.
fn round2(x: f64) -> f64 {
    (x * 100.0).round() / 100.0
}

/// Today as `YYMMDD` (§5.4) — the one clock read, for "next exam" alone (§19.4).
fn today_yymmdd() -> String {
    jiff::Zoned::now().strftime("%y%m%d").to_string()
}

#[cfg(test)]
mod tests {
    // Grade values are exact small constants; comparing them is the assertion.
    #![allow(clippy::float_cmp)]
    use super::*;

    #[test]
    fn best_passing_is_the_highest_attempt_not_the_latest() {
        // A retake: A at the first sitting, C at the re-sit. Best passing is A (§19.4).
        let graded = vec![
            Graded {
                credits: 6.0,
                value: 5.0,
                passing: true,
                counts_in_gpa: true,
            },
            Graded {
                credits: 6.0,
                value: 3.0,
                passing: true,
                counts_in_gpa: true,
            },
        ];
        assert_eq!(best_passing(&graded).unwrap().value, 5.0);
        assert_eq!(best_passing_counting(&graded).unwrap().value, 5.0);
    }

    #[test]
    fn a_failing_grade_is_no_best() {
        let graded = vec![Graded {
            credits: 7.5,
            value: 0.0,
            passing: false,
            counts_in_gpa: true,
        }];
        assert!(best_passing(&graded).is_none());
    }

    #[test]
    fn next_is_the_earliest_on_or_after_today() {
        let mut events = vec![
            ("260315".to_owned(), "sf1624".to_owned()),
            ("260901".to_owned(), "sf1626".to_owned()),
            ("260110".to_owned(), "past".to_owned()),
        ];
        let picked = pick_next(&mut events, "260401").unwrap();
        assert_eq!(picked, ("260901".to_owned(), "sf1626".to_owned()));
        // Nothing ahead → nothing (§12).
        let mut only_past = vec![("250101".to_owned(), "old".to_owned())];
        assert!(pick_next(&mut only_past, "260401").is_none());
    }
}
