//! Album — societas agents (who) (§8.1). People and the bodies they form, stored as
//! a **partitioned register**: one `.json` object per agent, its kind and slug in the
//! filename (§6.1). Referenced everywhere as `album:<slug>`.
//!
//! Three filename kinds, all partitioned: `person` (an individual), `organization`
//! (a formal body — a company, a school, a state), `group` (an informal set — a
//! family, a friend group, a book club). Homed under Societas by the **nature of the
//! bond**, one agent one file — but not sphere-locked (§6.2, I7).
//!
//! Closeness, role, origin, and gender are **fields, not nodes** (§2). The kind says
//! what an agent *is* and is corrected only by the file-rename `edit -k` (§7.2); a
//! form of address like *Mr/Ms* is **derived** from gender at render time, never
//! stored (I1, §18). Where you met someone is provenance and which context they
//! belong to is an edge; neither is ever their home (I3, I9) — a membership is a
//! reference to the group entity (`refs: ["album:book_club"]`), read from either end
//! and never a nesting.
//!
//! Build order step 3 — the first partitioned register: the second shape, `core:slug`
//! refs, the resolver's filename path, and the rename cascade (§16, §5.4).

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use pantheon::{Core, Error, Result, Shape};

// The CLI and the screen are the lib's, and `main.rs` is the ~30-line clap shell §14
// asks for. They live here rather than in the bin for one reason: an integration test
// links the *lib*, so a screen in the bin is a screen no test can reach — and step 6
// found three defects that only driving a screen caught (P§3, §14).
//
// **What that must not cost is I4.** A core's CLI JSON is the only thing that crosses a
// component boundary, and a verb reachable as a Rust function would be a second door
// into this core. So the verbs stay `pub(crate)`: the only things public here are
// [`run_cli`] — the whole CLI, entered exactly as the binary enters it — and
// [`AlbumApp`], which relays through that same CLI. Neither is a way to call a verb
// directly, and nothing else is exposed.
mod cli;
// The screen rides the `tui` feature; drop it and the core is headless (§14).
#[cfg(feature = "tui")]
mod screen;

pub use cli::run_cli;
#[cfg(feature = "tui")]
pub use screen::AlbumApp;

/// A period an agent is away (§8.1). Keys are `YYMMDD`, the same shape a series key
/// wears (§5.4), and `to` is absent while the period is still open.
///
/// This is the list-valued field §6.1 describes: an entity was never a sample, so
/// the history worth keeping accumulates *inside* the object rather than becoming a
/// series. A lens overlays it on a timeline by reading Album's JSON (§8.4, §12);
/// Album itself knows nothing about timelines.
#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, Default, PartialEq, Eq)]
pub struct Away {
    pub from: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to: Option<String>,
}

/// One agent — the `data` half of an entity file (§6.1).
///
/// Its `refs` ride in the envelope, and its home, core, kind, and slug are the
/// file's location and name (I3), so none of them is stored here. Nor is a display
/// name: a name and its slug are one thing and are never allowed to differ (§5.4),
/// so `rename` is the only way a name changes and there is no second copy to drift.
///
/// One flat struct rather than an enum over the three kinds. §7.1 asks for an enum
/// where a core declares two *shapes*; Album's three tokens are one shape and one
/// primitive. Making it an enum would turn `edit -k person→organization` into a
/// record transformation, when §7.2 says it is a file rename and nothing more — and
/// would have `schemars` emit a tagged union when §18 forbids writing a variant tag.
/// An organization simply leaves `gender` absent.
#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, Default, PartialEq, Eq)]
pub struct Agent {
    /// What a form of address is **derived** from, never the address itself (§8.1).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gender: Option<String>,
    /// How close the bond is — a field, because a person is not filed by it (§2).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub closeness: Option<String>,
    /// What they are to you. A role is a field for the same reason: it changes
    /// without the person becoming someone else.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    /// Where you met them — provenance, which is never a home (I3, §8.1).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin: Option<String>,
    /// Periods away, accumulated in the record (§6.1).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub away: Vec<Away>,
    /// A hand's remark on this agent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// The core (§7.1): a record type, a name, its tokens, and its `validate`.
/// Everything else — the twelve verbs, storage dispatch, resolution — the spine
/// provides generically.
pub struct Album;

impl Album {
    /// The token a bare `add` files under. Hardcoded, never a setting: §18 keeps
    /// per-core defaults out of configuration, and Album's is `person`.
    pub const DEFAULT_KIND: &'static str = "person";

    /// This core's three tokens, in the order `help` and errors should list them.
    pub const KINDS: [&'static str; 3] = ["person", "organization", "group"];
}

impl Core for Album {
    type Record = Agent;

    const NAME: &'static str = "album";

    /// Three tokens, one shape. What an agent *is* is a filename segment, corrected
    /// by `edit -k` — a visible structural act, not a silent field flip (§7.2).
    fn kinds() -> &'static [(&'static str, Shape)] {
        &[
            ("person", Shape::Partitioned),
            ("organization", Shape::Partitioned),
            ("group", Shape::Partitioned),
        ]
    }

    /// Checks beyond the envelope (§7.1).
    ///
    /// Kind-blind by construction: the trait hands `validate` a record and not the
    /// token it was filed under, which is right — the kind is the filename's, and a
    /// field that only makes sense for a person is simply absent on an organization.
    fn validate(record: &Agent) -> Result<()> {
        for (name, value) in [
            ("--gender", &record.gender),
            ("--closeness", &record.closeness),
            ("--role", &record.role),
            ("--origin", &record.origin),
            ("--note", &record.note),
        ] {
            if value.as_ref().is_some_and(|v| v.trim().is_empty()) {
                return Err(Error::validation(format!("{name} is blank (§8.1)")));
            }
        }
        for (i, away) in record.away.iter().enumerate() {
            check_day(&away.from, i, "from")?;
            if let Some(to) = &away.to {
                check_day(to, i, "to")?;
                if to < &away.from {
                    return Err(Error::validation(format!(
                        "away period {i} ends before it starts ({} > {to}) (§8.1)",
                        away.from
                    )));
                }
            }
        }
        Ok(())
    }
}

/// An away bound is a `YYMMDD` day, the same shape a series key wears (§5.4) — so
/// the two sort the same way, and a lens can line them up without a parser.
fn check_day(value: &str, i: usize, which: &str) -> Result<()> {
    if value.len() == 6 && value.bytes().all(|b| b.is_ascii_digit()) {
        Ok(())
    } else {
        Err(Error::validation(format!(
            "away period {i} has a malformed {which} {value:?}: a day is YYMMDD (§5.4)"
        )))
    }
}
