//! Where a course sat — the period placement (§19.5).
//!
//! **A span cannot carry a period any more than it can carry a grade** (§8.4, §19.1). It
//! does not need to: the interval already points at every period it overlaps, so the
//! placement is *derived* from the interval against the curriculum's year-less anchors and
//! stored nowhere (I1). That is the whole of §19.5's "a course can point to several
//! periods" — Mekanik's `20250114 → 20250602` covers P3 and P4 and reads as `P3–P4`.
//!
//! **The label is absolute.** With `periods_per_year` periods to a year, a course's label
//! is `(study_year − 1) × periods_per_year + n`, so a year-2 P1 reads as **P6** — the index
//! a study life actually counts in. Study year comes from the span's `from` against the
//! programme span's start, and the year turns where the first period begins: the academic
//! year is the calendar the curriculum declares, not January.
//!
//! Everything here is pure and dated by its arguments — the fold reads the clock in
//! exactly one place (§19.4) and it is not this one.

use crate::curriculum::{Curriculum, Period};

/// Where one enrolment sat (§19.5).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Placement {
    /// The absolute label — `P6`, or `P3–P4` where the interval covers several.
    pub label: String,
    /// The terms it touched, in period order and without repeats (§19.5).
    pub terms: Vec<String>,
}

/// The placement of a span within its programme (§19.5), or `None` where the curriculum
/// declares no calendar, the dates will not read, or the interval overlaps no period.
///
/// An **open** span (`to` is `None`) is placed by where it *started*: an enrolment still
/// running has no end to measure, and reading its placement as "every period from here on"
/// would put a course in periods nobody has sat yet.
#[must_use]
pub fn placement(
    curriculum: &Curriculum,
    programme_from: &str,
    span_from: &str,
    span_to: Option<&str>,
) -> Option<Placement> {
    let periods = curriculum.periods();
    let per_year = curriculum.periods_per_year()?;
    if periods.is_empty() || per_year == 0 {
        return None;
    }
    let from = day(span_from)?;
    let to = span_to.and_then(day).unwrap_or_else(|| from.clone());

    let study_year = study_year(programme_from, span_from)?;
    let base = (study_year - 1).checked_mul(i64::from(per_year))?;

    let mut hit: Vec<&Period> = periods
        .iter()
        .filter(|p| overlaps(p, &from, &to))
        .collect::<Vec<_>>();
    if hit.is_empty() {
        return None;
    }
    hit.sort_by_key(|p| p.n);
    hit.dedup_by_key(|p| p.n);

    let labels: Vec<String> = hit
        .iter()
        .map(|p| format!("P{}", base + i64::from(p.n)))
        .collect();
    let mut terms: Vec<String> = Vec::new();
    for p in &hit {
        if let Some(term) = curriculum.term_of(&p.slug) {
            if !terms.iter().any(|t| t == term) {
                terms.push(term.to_owned());
            }
        }
    }

    // One period reads as itself; several read as the interval they span. Not a list:
    // §19.5 writes it `P3–P4`, and a course covering P3, P4 and P5 sat through all three.
    let label = match (labels.first(), labels.last()) {
        (Some(first), Some(last)) if first != last => format!("{first}–{last}"),
        (Some(only), _) => only.clone(),
        _ => return None,
    };
    Some(Placement { label, terms })
}

/// Which study year a date falls in, counting from the programme's start (§19.5).
///
/// `1` for the programme's own first year. **The programme's start is the anchor**, which
/// is exactly what §19.5 says — the study year turns on the *anniversary of enrolling*,
/// not on some period's anchor. Taking P1's anchor instead looks equivalent and is not: a
/// programme starting 1 August against a P1 opening 26 August would read its own first
/// spring as year two, because the enrolment fell the wrong side of a boundary it defined.
///
/// So a course starting the following January is still year 1, one starting the August
/// after is year 2, and a programme's own dates never argue with the calendar's.
fn study_year(programme_from: &str, span_from: &str) -> Option<i64> {
    let (started, opens) = split(programme_from)?;
    let (year, mmdd) = split(span_from)?;
    let sat = if mmdd >= opens { year } else { year - 1 };
    Some(sat - started + 1)
}

/// Whether a period's window, in any year the span touches, meets the span (§19.5).
///
/// The window is built per calendar year because the anchors are year-less and a period
/// may wrap the new year (KTH's P2 runs from late October into January). Years from the
/// one *before* the span opens, so a wrapped period that began in December still counts.
fn overlaps(period: &Period, from: &(i64, String), to: &(i64, String)) -> bool {
    for year in (from.0 - 1)..=to.0 {
        let opens = (year, period.start.clone());
        // A window whose end reads earlier than its start has wrapped into the next year.
        let closes = if period.end >= period.start {
            (year, period.end.clone())
        } else {
            (year + 1, period.end.clone())
        };
        if opens <= *to && closes >= *from {
            return true;
        }
    }
    false
}

/// A `YYYYMMDD` date as `(year, "MMDD")` — comparable as a pair, so no date arithmetic and
/// no second date crate (§13).
fn split(date: &str) -> Option<(i64, String)> {
    if date.len() != pantheon::DATE_WIDTH || !date.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    // The year is the full four digits now, so a study year spanning 1999→2000 orders
    // correctly where a two-digit year wrapped to a smaller number (§5.4, §19.5).
    let year = date[..4].parse::<i64>().ok()?;
    Some((year, date[4..].to_owned()))
}

fn day(date: &str) -> Option<(i64, String)> {
    split(date.split('T').next().unwrap_or(date))
}

#[cfg(test)]
mod tests {
    use super::*;

    const KTH: &str = r#"
periods_per_year = 5
terms = [ { slug = "ht", periods = ["P1","P2"] }, { slug = "vt", periods = ["P3","P4"] } ]
periods = [
  { n = 1, slug = "P1", term = "ht", start = "0826", end = "1025" },
  { n = 2, slug = "P2", term = "ht", start = "1026", end = "0114" },
  { n = 3, slug = "P3", term = "vt", start = "0115", end = "0315" },
  { n = 4, slug = "P4", term = "vt", start = "0316", end = "0602" },
  { n = 5, slug = "P5", term = "summer", start = "0603", end = "0825" },
]
"#;

    fn kth() -> Curriculum {
        Curriculum::parse(KTH).expect("parses")
    }

    /// §19.5's own example: an interval covering two periods reads as the range, and the
    /// terms it touched come with it.
    #[test]
    fn a_course_spanning_two_periods_reads_as_the_range() {
        let p = placement(&kth(), "20240826", "20250114", Some("20250602")).expect("placed");
        // It opens on P2's *last day*, so P2–P4 — the interval points at every period it
        // overlaps and the fold does not round the edge off.
        assert_eq!(p.label, "P2–P4");
        assert_eq!(p.terms, ["ht", "vt"]);

        // Squarely inside the spring: §19.5's `P3–P4`, in the programme's first year.
        let first = placement(&kth(), "20250826", "20260115", Some("20260602")).expect("placed");
        assert_eq!(first.label, "P3–P4");
        assert_eq!(first.terms, ["vt"]);
    }

    /// **A programme's own dates never argue with the calendar's** (§19.5).
    ///
    /// Enrolling on 1 August against a P1 that opens on the 26th: the following January is
    /// still year *one*. Anchoring the turn on the first period's date instead would put
    /// the programme's own first spring in year two.
    #[test]
    fn a_programme_starting_before_its_first_period_still_counts_from_one() {
        let p = placement(&kth(), "20240801", "20250110", Some("20250601")).expect("placed");
        assert_eq!(p.label, "P2–P4");
    }

    /// **The label is absolute across the programme** (§19.5): a year-2 P1 is P6.
    #[test]
    fn a_second_year_p1_reads_as_p6() {
        let p = placement(&kth(), "20250826", "20260901", Some("20261020")).expect("placed");
        assert_eq!(p.label, "P6");
        assert_eq!(p.terms, ["ht"]);
        // And the year turns at the first period's anchor, not in January: a course
        // starting the preceding May is still year 1.
        let spring = placement(&kth(), "20250826", "20260510", Some("20260520")).expect("placed");
        assert_eq!(spring.label, "P4", "P4 of year one, not year two");
    }

    /// An **open** enrolment is placed by where it started — a course still running has
    /// not sat the periods ahead of it.
    #[test]
    fn an_open_span_is_placed_by_its_start() {
        let p = placement(&kth(), "20250826", "20250901", None).expect("placed");
        assert_eq!(p.label, "P1");
    }

    /// A period wrapping the new year is met from either side of it.
    #[test]
    fn a_period_that_wraps_the_new_year_is_still_met() {
        let p = placement(&kth(), "20250826", "20260105", Some("20260110")).expect("placed");
        assert_eq!(p.label, "P2", "January still sits in the autumn's P2");
    }

    /// No calendar, no placement — the honest absence, never a guessed period (§19.3).
    #[test]
    fn a_curriculum_without_a_calendar_places_nothing() {
        let bare = Curriculum::parse("default_scale = \"af\"\n").unwrap();
        assert!(placement(&bare, "20250826", "20260115", Some("20260602")).is_none());
        // And a date that will not read places nothing either.
        assert!(placement(&kth(), "20250826", "nope", None).is_none());
    }
}
