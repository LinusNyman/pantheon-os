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
/// The last two are the screen's alone — `figures` folds no contacts and no reflections
/// (§19.9), so a headless build reads neither core.
#[cfg(feature = "tui")]
const ALBUM: &str = "alb";
#[cfg(feature = "tui")]
const TABELLA: &str = "tab";

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
            "period": Value::Null,
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
        "next_exam": next_exam(root, home, &today_ymd()),
        "period": period_now(&spans, &programmes, &curricula, &today_ymd()),
    })
}

/// Which period the study life is **in**, absolutely (§19.5).
///
/// Only answerable within one programme: the label counts study years from a programme's
/// start, and across two degrees there is no such count — the same reason the screen folds
/// one programme at a time (§19.4). So this is `null` on all-the-studies, which is the
/// honest dash and not a zero (§12).
fn period_now(
    spans: &[Value],
    programmes: &HashSet<String>,
    curricula: &[(pantheon::Code, Curriculum)],
    today: &str,
) -> Value {
    let mut in_scope = spans
        .iter()
        .filter(|s| slug(s).is_some_and(|slug| programmes.contains(slug)));
    let Some(programme) = in_scope.next() else {
        return Value::Null;
    };
    if in_scope.next().is_some() {
        return Value::Null; // more than one degree in scope: no single count of years
    }
    let placed = programme["home"]
        .as_str()
        .and_then(|home| curriculum::governing(curricula, home))
        .zip(programme["data"]["from"].as_str())
        .and_then(|(curriculum, started)| {
            crate::period::placement(curriculum, started, today, None)
        });
    match placed {
        Some(p) => json!({ "label": p.label, "terms": p.terms }),
        None => Value::Null,
    }
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
/// absent or nothing is scheduled ahead (§12).
fn next_exam(root: &Path, home: Option<&str>, today: &str) -> Value {
    let Some(lines) = event_lines(root, home) else {
        return Value::Null;
    };
    let mut events: Vec<(String, String)> = lines
        .iter()
        .filter_map(|line| {
            let course = course_ref(line)?;
            Some((day(line["key"].as_str()?), course))
        })
        .collect();

    match pick_next(&mut events, today) {
        Some((date, course)) => json!({ "date": date, "course": course }),
        None => Value::Null,
    }
}

/// Every Fasti `event` line in scope (§8.4), or `None` where Fasti is off `PATH` (§12).
///
/// **`list` is the present, not the history** (I1): a core's `list` answers with the
/// *latest* line of each series, so reading it alone would show one occurrence per series
/// and hide every other date in it. The whole timeline is `list` to name the series, then
/// `series <name>` for its lines — which is exactly what a fold over samples must do.
fn event_lines(root: &Path, home: Option<&str>) -> Option<Vec<Value>> {
    series_lines(root, FASTI, home, Some("event"))
}

/// Every line of every series a core holds in scope — the readings, not the present (I1).
///
/// `None` only where the core itself is off `PATH` (§12); a core that answers with no
/// series answers with no lines, which is a real empty and not an absence.
fn series_lines(
    root: &Path,
    short: &str,
    home: Option<&str>,
    kind: Option<&str>,
) -> Option<Vec<Value>> {
    let mut args = vec!["list"];
    if let Some(kind) = kind {
        args.extend_from_slice(&["-k", kind]);
    }
    if let Some(home) = home {
        args.extend_from_slice(&["-H", home]);
    }
    let present = read(root, short, &args).and_then(|v| array(&v))?;
    let mut names: Vec<&str> = present
        .iter()
        .filter_map(|r| r["series"].as_str())
        .collect();
    names.sort_unstable();
    names.dedup();
    let mut out = Vec::new();
    for name in names {
        if let Some(lines) = read(root, short, &["series", name]).and_then(|v| array(&v)) {
            out.extend(lines);
        }
    }
    Some(out)
}

/// One dated occurrence — an exam sitting, a deadline (§8.4, §19.6).
///
/// **Studium does not tell an exam from a deadline, and does not pretend to.** §8.4 gives
/// an event no kind of its own beyond `event`, and inventing one here would be a lens
/// deciding a core's vocabulary (I5). What it *does* distinguish is what a hand needs: an
/// occurrence that names a course, and one that does not.
#[cfg(feature = "tui")]
pub(crate) struct Occurrence {
    pub date: String,
    /// What the line says — its first value, else the series it sits in.
    pub label: String,
    /// The course it concerns, where it references one (§8.4).
    pub course: Option<String>,
    pub home: String,
    pub series: String,
}

/// The occurrences in the next `days` days (§19.6) — "deadlines & exams", folded on sight.
///
/// Bounded rather than endless, because the tab answers *what is coming*, and a timeline
/// with next year's re-exam on it answers nothing. Sorted by date, earliest first.
#[cfg(feature = "tui")]
pub(crate) fn upcoming(root: &Path, home: Option<&str>, today: &str, days: i32) -> Vec<Occurrence> {
    let horizon = plus_days(today, days);
    let Some(lines) = event_lines(root, home) else {
        return Vec::new();
    };
    let mut out: Vec<Occurrence> = lines
        .iter()
        .filter_map(|line| {
            let date = day(line["key"].as_str()?);
            if date.as_str() < today || horizon.as_ref().is_some_and(|end| &date > end) {
                return None;
            }
            let series = line["series"].as_str().unwrap_or_default().to_owned();
            let label = line["data"]["values"]
                .as_array()
                .and_then(|v| v.first())
                .and_then(Value::as_str)
                .map_or_else(|| series.clone(), str::to_owned);
            Some(Occurrence {
                date,
                label,
                course: course_ref(line),
                home: line["home"].as_str().unwrap_or_default().to_owned(),
                series,
            })
        })
        .collect();
    out.sort_by(|a, b| a.date.cmp(&b.date));
    out
}

/// One study-time reading — a session's hours, dated (§8.6, §19.6).
#[cfg(feature = "tui")]
pub(crate) struct Session {
    pub date: String,
    pub hours: String,
    /// The log it was given to — what you gave the hours *to*.
    pub log: String,
    pub home: String,
}

/// The study-time readings in scope, latest first (§19.6).
///
/// The grade logs are named for their courses (§19.2), so **every other log in scope is
/// study time** — the same rule the `study_hours` figure folds by, read here one line at a
/// time instead of summed.
#[cfg(feature = "tui")]
pub(crate) fn sessions(root: &Path, home: Option<&str>) -> Vec<Session> {
    let courses = spans(root, home)
        .map(|s| course_slugs(&s))
        .unwrap_or_default();
    let mut out: Vec<Session> = series_lines(root, ANNALES, home, None)
        .unwrap_or_default()
        .iter()
        .filter_map(|line| {
            let name = line["series"].as_str()?;
            if courses.contains(name) {
                return None; // a grade log, not a study-time log (§19.2)
            }
            let hours = line["data"]["values"]
                .as_array()
                .and_then(|v| v.first())
                .and_then(Value::as_str)?;
            Some(Session {
                date: line["key"].as_str().map_or_else(String::new, day),
                hours: hours.to_owned(),
                log: name.to_owned(),
                home: line["home"].as_str().unwrap_or_default().to_owned(),
            })
        })
        .collect();
    out.sort_by(|a, b| b.date.cmp(&a.date));
    out
}

/// The people a study life's records point at (§19.6, §8.1).
///
/// **A fold over references, never a directory** — §19.6 says contacts are "the people a
/// course's records point at", so a person is *in* the studies exactly when something in
/// scope references them. Nothing is copied under a course (I3), and Album is read only
/// for the records the edges already name.
#[cfg(feature = "tui")]
pub(crate) fn people(root: &Path, home: Option<&str>) -> Vec<Value> {
    let mut wanted: Vec<String> = Vec::new();
    let mut gather = |rows: Vec<Value>| {
        for row in &rows {
            for slug in row["refs"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .filter_map(|t| t.strip_prefix("album:"))
            {
                if !wanted.iter().any(|s| s == slug) {
                    wanted.push(slug.to_owned());
                }
            }
        }
    };
    // The spans are entities, so `list` is the whole set. The event and log **series** are
    // samples, so `list` would answer with each one's latest line alone — and a professor
    // named on an earlier reading would vanish the day a later one landed (I1).
    gather(spans(root, home).unwrap_or_default());
    gather(series_lines(root, FASTI, home, Some("event")).unwrap_or_default());
    gather(series_lines(root, ANNALES, home, None).unwrap_or_default());
    wanted.sort();
    wanted
        .iter()
        .filter_map(|slug| read(root, ALBUM, &["get", slug]))
        .collect()
}

/// The reflections in scope (§19.6, §8.7): Tabella documents whose `type` is `reflection`.
///
/// Read off `list`'s frontmatter and no further — a fold never reads bodies (§6.1, §8.7).
#[cfg(feature = "tui")]
pub(crate) fn reflections(root: &Path, home: Option<&str>) -> Vec<Value> {
    let mut args = vec!["list"];
    if let Some(home) = home {
        args.extend_from_slice(&["-H", home]);
    }
    read(root, TABELLA, &args)
        .and_then(|v| array(&v))
        .unwrap_or_default()
        .into_iter()
        .filter(|doc| doc["type"].as_str() == Some("reflection"))
        .collect()
}

/// `today` plus `days`, as `YYYYMMDD` — the horizon "the next 28 days" ends at (§19.6).
///
/// The spine's own date crate (§13), for the one piece of arithmetic a calendar cannot be
/// compared its way out of. `None` where the day will not read, which widens the window to
/// everything ahead rather than narrowing it to nothing.
#[cfg(feature = "tui")]
fn plus_days(today: &str, days: i32) -> Option<String> {
    let year: i16 = today.get(..4)?.parse().ok()?;
    let month: i8 = today.get(4..6)?.parse().ok()?;
    let day: i8 = today.get(6..8)?.parse().ok()?;
    let date = jiff::civil::date(year, month, day)
        .checked_add(jiff::Span::new().days(days))
        .ok()?;
    Some(format!(
        "{:04}{:02}{:02}",
        date.year(),
        date.month(),
        date.day()
    ))
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

/// The day part of a series key — `20260315` from `20260315` or `20260315T0900` (§5.4).
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

/// Today as `YYYYMMDD` (§5.4) — the one clock read, for "next exam" alone (§19.4).
fn today_ymd() -> String {
    jiff::Zoned::now().strftime("%Y%m%d").to_string()
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
            ("20260315".to_owned(), "sf1624".to_owned()),
            ("20260901".to_owned(), "sf1626".to_owned()),
            ("20260110".to_owned(), "past".to_owned()),
        ];
        let picked = pick_next(&mut events, "20260401").unwrap();
        assert_eq!(picked, ("20260901".to_owned(), "sf1626".to_owned()));
        // Nothing ahead → nothing (§12).
        let mut only_past = vec![("20250101".to_owned(), "old".to_owned())];
        assert!(pick_next(&mut only_past, "20260401").is_none());
    }
}
