//! Fasti — the placement tense of Actio (§8.4). What sits on the timeline.
//!
//! Two record shapes, Rationes-fashion (§8.3), and so the first core whose primitive
//! brings a second **storage** shape with it (§6.1, §7.1):
//!
//! - **`span`** — a [`Span`], a *partitioned entity*: one `.json` per period or state,
//!   with a `from` and an optionally open `to`. A life, a project's active window, a
//!   career stage, an enrolment, a residence. Long-lived, mutable (close the `to` when
//!   it ends), and heavily referenced, so it earns entity storage; referenced as
//!   `fasti:<slug>` and resolved by filename without opening it (§5.0).
//! - **`event`** — an [`Event`], a *hand-named series* (`Series { named: true }`): one
//!   keyed line per dated occurrence. Being hand-named it is a ref target as a whole
//!   collection (`fasti:standups`, §5.4) and is minted explicitly with `-c`, exactly as
//!   Annales' log is (§7.3) — a typo cannot conjure a timeline.
//!
//! **Neither record stores what it is about.** A span references what it concerns — a
//! career span `refs: ["album:<org>"]`, a residence span `refs: ["mappa:<place>"]`, a
//! friend's residence both — and an event references the *span* it belongs to
//! (`refs: ["fasti:mvp_phase"]`), never a container: a calendar is an arbitrary surface
//! and a span a real period, so an event points at its period (I9). There is no `cal`
//! token, and no `span` field on an event — the edge is the same edge everywhere.
//!
//! **The calendar is a derived view, never stored** — a fold over events by span and
//! date (§8.4). Nothing here keeps one. An event with **no span is legal**: not a
//! validation finding and not a stored flag, surfaced on demand by `fas list
//! --unspanned` so a hand can check the set without anything nagging.
//!
//! **Album's `data.away` overlay is a lens's composition, not Fasti's** (§8.4, §12).
//! A lens reads both cores' JSON off `PATH` and derives the overlap at render; Fasti
//! never reads Album, so where Album is not installed the overlay is simply absent
//! (I4, I5).
//!
//! Build order step 7 — the two shapes in one core, against a contract a screen has
//! now exercised (§16).

// `FastiRecord` is the name §7.1 gives this type — "`enum FastiRecord { Span(..),
// Event(..) }`" — so the prefix is the spec's, not an accident to lint away.
#![allow(clippy::module_name_repetitions)]
// Errors are the exit-code contract (§7.3), documented centrally rather than per-fn —
// the spine's own convention (see pantheon/src/lib.rs).
#![allow(clippy::missing_errors_doc)]

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use pantheon::{Core, Error, Result, Shape};

/// A period or state — the `data` half of a partitioned entity file (§6.1, §8.4).
///
/// Its `refs` ride in the envelope, and its home, core, kind, and slug are the file's
/// location and name (I3), so none of them is stored here. What is left is the period
/// itself: when it opened, when it closed if it has, and a hand's remark.
///
/// `from` is **required** and `to` is **optionally absent** — an absent `to` is an open
/// span, the state you are still in, and closing it is an ordinary `edit` that corrects
/// the one object in place (I1, §6.1). There is no `open` flag beside it: a boolean
/// that merely restates whether a field is present is a second copy of the same fact,
/// free to drift from it (§18).
#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, Default, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Span {
    /// The day the period opens, `YYMMDD` — the same shape a series key wears (§5.4),
    /// so a span and an event sort against each other without a parser.
    pub from: String,
    /// The day it closes; absent while it is still open (§8.4).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to: Option<String>,
    /// A hand's remark on this span.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// One dated occurrence — the `data` half of a series line (§5.4, §8.4).
///
/// The occurrence's key (its date, plus a time where a hand gave one) and its `refs`
/// ride in the envelope, and its home, core, kind, and series name are the file's
/// location and name (I3). What is left is what a hand actually gave.
///
/// **The span it belongs to is a ref, not a field** (§8.4, I9). A `span` field here
/// would be a second reference form for one edge, invisible to the cascade that keeps
/// `core:slug` honest (§5.4) — and it is exactly the in-record kind §6.1 says a series
/// never carries.
///
/// Values stay strings on disk, for Annales' reason (§8.6): a record is read by hand as
/// often as by code (I6, I8), and an occurrence is described in words far more often
/// than it is measured.
#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, Default, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Event {
    /// What the occurrence is, in the order it was given. Empty for a line whose whole
    /// content is its references — a deadline that *is* its `pensum:` ref is still an
    /// occurrence.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub values: Vec<String>,
    /// When it ends — `hhmm`, or `YYMMDDThhmm` where it runs past its own day. The
    /// *start* is the line's key, which is what makes "a meeting 4–5pm" one record
    /// rather than two (§7.3, §8.4). Absent for an occurrence with no duration: a
    /// deadline is a point.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub until: Option<String>,
    /// A hand's remark on this occurrence.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// The `Record` a core with two tokens declares (§7.1) — **a dispatch type, not a disk
/// format**.
///
/// `untagged`, so each file stores the bare variant payload and **no tag is ever
/// written** (§7.1, §18): the filename's token already says which variant a file holds
/// (§5.2), and a stored tag would be a second copy of a fact the path already carries
/// (I3).
///
/// Untagged serde takes the first variant that parses, so the discrimination has to be
/// made *total* rather than lucky. Two things do it here, and only the first is the
/// general rule: `deny_unknown_fields` on both variants makes their field sets disjoint,
/// which is the precedent Rationes set for a two-shape core and the reason a body that
/// is neither shape is refused outright rather than read as the nearer one. Fasti also
/// happens to carry an independent discriminant — [`Span::from`] is the one required
/// field anywhere here, so a body with it is a span and a body without it falls through
/// to [`Event`], whose every field is optional.
///
/// **The filename stays authoritative regardless** (§5.2). Where a body disagrees with
/// the token it is filed under — a hand-edited file, an older format (I6, §13) — the
/// enum still reads it as *some* variant, so every verb goes through
/// [`FastiRecord::as_span`] / [`as_event`](FastiRecord::as_event), which compare the two
/// and fail with a sentence naming the disagreement (exit `3`, §6.4) rather than
/// emitting a record as the shape it is not. That guard is what the second discriminant
/// buys: a mis-filed record gets a sentence, and only a genuinely unknown key falls back
/// on serde's own refusal.
#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, PartialEq, Eq)]
#[serde(untagged)]
pub enum FastiRecord {
    Span(Span),
    Event(Event),
}

impl FastiRecord {
    /// Read this record as the span its filename says it is (§5.2).
    ///
    /// The token on the file is authoritative; this is the guard for the case where the
    /// bytes disagree with it — a hand-edited file, or one written by an older format
    /// (I6, §13).
    pub fn as_span(&self) -> Result<&Span> {
        match self {
            FastiRecord::Span(span) => Ok(span),
            FastiRecord::Event(_) => Err(Error::validation(
                "this file is filed under `span` but does not read as one: a span needs a \
                 `from` (§5.2, §8.4)",
            )),
        }
    }

    /// Read this record as the event its filename says it is (§5.2).
    pub fn as_event(&self) -> Result<&Event> {
        match self {
            FastiRecord::Event(event) => Ok(event),
            FastiRecord::Span(_) => Err(Error::validation(
                "this line is filed under `event` but reads as a span: an event's period is \
                 the span it references, never a `from` of its own (§8.4, I9)",
            )),
        }
    }
}

/// The core (§7.1): a record type, a name, its tokens, and its `validate`.
/// Everything else — the twelve verbs, storage dispatch, resolution — the spine
/// provides generically.
pub struct Fasti;

impl Fasti {
    /// The partitioned token: a period that endures (§6.1).
    pub const SPAN: &'static str = "span";
    /// The hand-named series token: occurrences sampled onto the timeline (§6.1).
    pub const EVENT: &'static str = "event";

    /// This core's two tokens, in the order `help` and errors should list them.
    pub const KINDS: [&'static str; 2] = [Self::SPAN, Self::EVENT];
}

impl Core for Fasti {
    type Record = FastiRecord;

    const NAME: &'static str = "fasti";

    /// Two tokens, **two shapes** — the declaration §7.1 asks for, and the only place a
    /// shape is named. A span endures, so it is partitioned; an event is sampled onto
    /// the timeline, so it is a series, and hand-named because a timeline is a thing
    /// you point at (`fasti:standups`, §5.4).
    fn kinds() -> &'static [(&'static str, Shape)] {
        &[
            (Self::SPAN, Shape::Partitioned),
            (Self::EVENT, Shape::Series { named: true }),
        ]
    }

    /// Checks beyond the envelope (§7.1), dispatched by variant — which is all the
    /// dispatch type is for.
    fn validate(record: &FastiRecord) -> Result<()> {
        match record {
            FastiRecord::Span(span) => validate_span(span),
            FastiRecord::Event(event) => validate_event(event),
        }
    }
}

/// A span is a real period, so it must open, and it may not close before it opened.
fn validate_span(span: &Span) -> Result<()> {
    check_day(&span.from, "--from")?;
    if let Some(to) = &span.to {
        check_day(to, "--to")?;
        if to < &span.from {
            return Err(Error::validation(format!(
                "a span ends before it starts ({} > {to}) (§8.4)",
                span.from
            )));
        }
    }
    check_remark(span.note.as_deref())
}

/// An occurrence may be empty of values — one carried entirely by its refs is still an
/// occurrence — but a value a hand cannot read is not one.
fn validate_event(event: &Event) -> Result<()> {
    if let Some(i) = event.values.iter().position(|v| v.trim().is_empty()) {
        return Err(Error::validation(format!(
            "event value {i} is blank; a value nobody can read places nothing (§8.4)"
        )));
    }
    if let Some(until) = &event.until {
        check_when(until)?;
    }
    check_remark(event.note.as_deref())
}

/// A day is `YYMMDD`, the shape a series key wears (§5.4) — so a span's bounds and an
/// event's key sort the same way, and a fold can line them up without a parser.
fn check_day(value: &str, which: &str) -> Result<()> {
    if value.len() == 6 && value.bytes().all(|b| b.is_ascii_digit()) {
        Ok(())
    } else {
        Err(Error::validation(format!(
            "{which} is malformed ({value:?}): a day is YYMMDD (§5.4, §8.4)"
        )))
    }
}

/// An end is `hhmm` — the same day — or `YYMMDDThhmm` where the occurrence runs past
/// it. The same two forms `-a` accepts for the start (§7.3), so a hand types one shape.
fn check_when(value: &str) -> Result<()> {
    let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    let ok = match value.split_once('T') {
        Some((day, time)) => digits(day) && day.len() == 6 && digits(time) && time.len() == 4,
        None => digits(value) && value.len() == 4,
    };
    if ok {
        Ok(())
    } else {
        Err(Error::validation(format!(
            "--until is malformed ({value:?}): an end is hhmm or YYMMDDThhmm (§7.3, §8.4)"
        )))
    }
}

fn check_remark(note: Option<&str>) -> Result<()> {
    if note.is_some_and(|n| n.trim().is_empty()) {
        return Err(Error::validation("--note is blank (§8.4)"));
    }
    Ok(())
}
