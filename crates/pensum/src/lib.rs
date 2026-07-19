//! Pensum — the intention tense of Actio (§8.5). A future doing: the task allotted
//! and not yet done.
//!
//! One **nameless** series per node — the node's tasks as keyed lines in
//! `[code]__task.jsonl` — minted by the node's first task rather than by `-c`, and
//! keyed by the task's own name (§5.4, §7.3). That last part is what makes a Pensum
//! task unlike an Annales reading: a date-keyed line is a *sample* and never a ref
//! target (I1), while a name-keyed line is a *record*, reached as
//! `pensum:reach_out_to_alex`, whose rename re-slugs it and cascades its refs (§7.2).
//!
//! A task is homed where the *doing* lives (`acm`) and references what it is *about*
//! (`refs: ["album:alex"]`) — aboutness is an edge, never a nesting (I9, §6.2).
//!
//! Auspex's primary write target (§9), and so the file a detached hook is likeliest
//! to be writing while a hand is mid-edit — which is what the record lock is for
//! (§6.4). Nothing marks a record as Auspex's: a task it wrote and a task you typed
//! are the same task (§9.5).
//!
//! Build order step 4 — the determined-name path Annales' hand-named log never
//! reaches (§16).

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use pantheon::{Core, Error, KeyShape, Result, Shape};

// The CLI and the screen are the lib's, and `main.rs` is the ~30-line clap shell §14
// asks for. They live here rather than in the bin for one reason: an integration test
// links the *lib*, so a screen in the bin is a screen no test can reach — and step 6
// found three defects that only driving a screen caught (P§3, §14).
//
// **What that must not cost is I4.** A core's CLI JSON is the only thing that crosses a
// component boundary, and a verb reachable as a Rust function would be a second door
// into this core. So the verbs stay `pub(crate)`: the only things public here are
// [`run_cli`] — the whole CLI, entered exactly as the binary enters it — and
// [`PensumApp`], which relays through that same CLI. Neither is a way to call a verb
// directly, and nothing else is exposed.
mod cli;
// The screen rides the `tui` feature; drop it and the core is headless (§14).
#[cfg(feature = "tui")]
mod screen;

pub use cli::run_cli;
#[cfg(feature = "tui")]
pub use screen::PensumApp;

/// One task — the `data` half of a series line (§5.4).
///
/// The task's key (its name) and its `refs` ride in the envelope, and its home,
/// core, and kind are the file's location and name (I3), so none of them is stored
/// here. What is left is the two things a hand actually says about a task: whether
/// it is done, and anything it wanted to write past the name.
///
/// There is deliberately **no due date**. Placement is Fasti's tense, not Pensum's
/// (§2, §8.4) — a deadline is a Fasti `event` referencing `pensum:<key>`, which is
/// an edge like every other (I9). A due field here would be the first half of a
/// scheduler, and §18 forbids the second.
#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, Default, PartialEq, Eq)]
pub struct Task {
    /// The date the doing was done (`YYMMDD`), absent while it is still intended.
    ///
    /// A date rather than a flag, because "done" is a thing that happened on a day
    /// and the record may as well say which. Marking it is an ordinary `edit`: the
    /// line is rewritten in place, never stacked with a second (I1, §6.1).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub done: Option<String>,

    /// What a hand typed past the task's name — the editor form's buffer (§7.3).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// The core (§7.1).
pub struct Pensum;

impl Core for Pensum {
    type Record = Task;

    const NAME: &'static str = "pensum";

    /// One token, filed as a nameless series (§8.5). `named: false` is the whole
    /// point of this core's place in the build order: it is the bit the spine reads
    /// to know that this file's name slot carries no identity, and that the records
    /// inside it are reached by their keys instead (§5.4, §7.1).
    fn kinds() -> &'static [(&'static str, Shape)] {
        &[("task", Shape::Series { named: false })]
    }

    fn validate(record: &Task) -> Result<()> {
        if let Some(done) = &record.done {
            // `done` says *when*, so it has to be a date. Reusing the key's own
            // shape rule keeps one reading of what a date looks like (§5.4).
            if !matches!(
                pantheon::Key::parse(done)?.classify(),
                KeyShape::Date | KeyShape::DateTime
            ) {
                return Err(Error::validation(format!(
                    "--done takes the date it was done (YYMMDD), and {done:?} is not \
                     one (§5.4, §7.3)"
                )));
            }
        }
        if record.note.as_ref().is_some_and(|n| n.trim().is_empty()) {
            return Err(Error::validation(
                "the note is blank; a task says what it is in its name, so an empty \
                 note says nothing twice (§8.5)",
            ));
        }
        Ok(())
    }
}

impl Pensum {
    /// The core's sole token. Hardcoded, never configured (§18).
    pub const KIND: &'static str = "task";
}
