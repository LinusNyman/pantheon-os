//! Annales — the fact tense of Actio (§8.6). What happened: the purest expression
//! of I1. One hand-named `log` series per collection, one keyed line per fact.
//!
//! A correction **rewrites the keyed line in place**; it never appends a second
//! (I1, §6.1). The record is the truth, not an audit sink — no line carries its
//! author (§9.5), because a fact is a fact whoever wrote it.
//!
//! Build order step 2 — the simplest core: one token, one shape (§16).

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use pantheon::{Core, Error, Result, Shape};

/// One reading in a log — the `data` half of a series line (§5.4).
///
/// The reading's key (its date) and its `refs` ride in the envelope, and its home,
/// core, kind, and series name are the file's location and name (I3), so none of
/// them is stored here. What is left is what a hand actually gave: the positional
/// values it typed, and an optional note.
///
/// Values stay strings on disk. A weight is `"78.4"`, not a float that a JSON
/// reader might round or reformat — a record is read by hand as often as by code
/// (I6, I8), and Annales logs prose as readily as numbers.
#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, Default, PartialEq, Eq)]
pub struct LogReading {
    /// What was read, in the order it was given. Empty for a line whose whole
    /// content is its references — a "where you've been" fact is its `mappa:` ref.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub values: Vec<String>,
    /// A hand's remark on this reading.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// The core (§7.1): a record type, a name, its tokens, and its `validate`.
/// Everything else — the twelve verbs, storage dispatch, resolution — the spine
/// provides generically.
pub struct Annales;

impl Core for Annales {
    type Record = LogReading;

    const NAME: &'static str = "annales";

    /// One token, one shape. A log is **hand-named**, so it is a ref target
    /// (`annales:meetings`) and is minted explicitly with `-c` (§7.1, §7.3).
    fn kinds() -> &'static [(&'static str, Shape)] {
        &[("log", Shape::Series { named: true })]
    }

    /// Checks beyond the envelope (§7.1). A reading may be empty of values — a fact
    /// carried entirely by its refs is still a fact — but a value a hand cannot read
    /// is not one.
    fn validate(record: &LogReading) -> Result<()> {
        if let Some(i) = record.values.iter().position(|v| v.trim().is_empty()) {
            return Err(Error::validation(format!(
                "reading value {i} is blank; a value nobody can read is not a fact (§8.6)"
            )));
        }
        if record.note.as_ref().is_some_and(|n| n.trim().is_empty()) {
            return Err(Error::validation("--note is blank (§8.6)"));
        }
        Ok(())
    }
}
