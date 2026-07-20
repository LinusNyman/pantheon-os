//! The verb runner (§7.1): the parts of a core's CLI that are the *same* for every
//! core, so the contract's JSON is produced one way rather than seven (I4).
//!
//! A core's bin owns its own flags and positionals — those follow its primitive
//! (§7.3) — and calls in here for everything downstream of them: how the hand is
//! read (TTY → table, pipe → JSON), how a mutation confirms, how `--at` becomes a
//! key, how a home and a series are *found* rather than invented, and how a record
//! is shaped into the emitted JSON.
//!
//! Step 2 landed the Series-shaped executors (§8.6); step 3 the Partitioned ones
//! (§8.1). The Document path grows here with the core that exercises it (step 5).
//!
//! The two target resolvers stay separate on purpose. A series verb infers a
//! *container that must already exist*, so its grammar is about finding one; a
//! partitioned `add` **is** the record it creates (§18), so its grammar is only
//! `[home] <name>` and arity is the whole of the discipline.

use std::io::{self, IsTerminal, Write};
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::code::{Code, parse_node_dirname};
use crate::core::Core;
use crate::document::Document;
use crate::envelope::{Entity, Frontmatter, Key, Line, Ref};
use crate::name::normalize_token;
use crate::store::{DocumentRef, EntityRef, PresentLine, SeriesRef, Store};
use crate::tree::resolve_code;
use crate::{Error, Result};

// ── the hand: what a command produced, and how it is rendered (§7.3) ─────────

/// What a verb produced. Exit `5` rides here rather than in [`Error`]: a pending
/// change is data the caller shows and re-runs with `-y`, not a failure (§7.3).
pub enum Response {
    /// A contract value rendered per the hand, exit `0`.
    Json(Value),
    /// A contract value rendered per the hand, with a specific exit code (§7.3).
    JsonExit(Value, u8),
    /// Raw text for a shell or a pager to consume, exit `0`.
    Raw(String),
}

/// Whether to emit JSON: an explicit `-f`, else a non-terminal stdout (§7.3).
/// **Format follows the hand** — same data, same code path (I8).
pub fn format_is_json(force: Option<bool>) -> bool {
    force.unwrap_or_else(|| !io::stdout().is_terminal())
}

/// Render a contract value: compact down a pipe, a table for a reader (§7.3).
///
/// The one rendering path in the suite — `pan` and every core reach it here, so a
/// table is written once rather than per bin (I4). The table itself knows nothing
/// about any core: it reads whatever keys the value carries (I5, [`crate::table`]).
pub fn emit(value: &Value, as_json: bool) {
    if as_json {
        println!("{value}");
    } else {
        print!("{}", crate::table::render(value));
    }
}

/// The whole tail of a core's `main`: render what the verb produced and return the
/// process exit code, printing the `{"error":{…}}` envelope to stderr on a failure
/// (§7.3). Every core ends identically.
///
/// This is also where a write's Auspex wake fires (§9.4). The [`Store`] mutators
/// *note* what they wrote; the wake goes out once here, at the end of the process,
/// so a verb writing three lines wakes Auspex once rather than three times. It fires
/// after the output is rendered — the child is detached and inherits no stdio, but
/// waking before printing would still put a subprocess spawn in front of the hand's
/// answer for no reason.
pub fn dispatch(outcome: Result<Response>, as_json: bool) -> std::process::ExitCode {
    let code = match outcome {
        Ok(Response::Json(value)) => {
            emit(&value, as_json);
            std::process::ExitCode::from(0)
        }
        Ok(Response::JsonExit(value, code)) => {
            emit(&value, as_json);
            std::process::ExitCode::from(code)
        }
        Ok(Response::Raw(text)) => {
            print!("{text}");
            std::process::ExitCode::from(0)
        }
        Err(e) => {
            eprintln!("{}", e.to_error_json());
            std::process::ExitCode::from(e.exit_code().as_u8())
        }
    };
    crate::hook::wake_if_noted();
    code
}

// ── confirming a mutation (§7.3) ─────────────────────────────────────────────

/// One computed record-level change awaiting review (§7.3). The structural
/// [`Plan`](crate::plan::Plan) covers node moves; this covers a record's own write,
/// which is what a core mutates.
#[derive(Clone, Debug)]
pub struct RecordChange {
    pub verb: &'static str,
    pub core: String,
    pub home: String,
    pub kind: String,
    /// Which collection the record sits in — `None` for a partitioned core, which
    /// keeps no series to name (§7.1).
    pub series: Option<String>,
    /// The record's identity: a series line's key, or an entity's slug — its *name*
    /// is its key, since a partitioned entity stores none (§5.4, §18).
    pub key: String,
    /// The record as it stands, if it already exists — what an overwrite replaces.
    pub before: Option<Value>,
    /// The record as it would stand; `None` for a removal.
    pub after: Option<Value>,
    /// What a rename would rewrite elsewhere in the tree (§5.4). It rides in the
    /// change — and so in the token — because a review showing three refs must not
    /// be applied against a tree that has since grown a fourth (§7.3).
    pub cascade: Option<Value>,
}

impl RecordChange {
    fn body(&self) -> Value {
        // Built as a map rather than a literal so a shape that has no series (or no
        // cascade) omits the key entirely rather than carrying a hollow one.
        //
        // **This function's exact bytes are the plan token** (`token()` hashes them),
        // and the snapshots redact that token — so a change here is invisible to
        // every snapshot in the workspace while silently invalidating any token a
        // hand is holding from an earlier `--dry-run`. The one thing that catches it
        // is `units.rs::a_change_body_names_a_series_only_when_there_is_one`, which
        // pins the exact byte string a Series change hashes. If that test fails,
        // the token contract moved: decide that deliberately, and do not simply
        // update the pinned string to match.
        //
        // Adding an `Option` field is safe only because `serde_json` here has no
        // `preserve_order`, so this is a `BTreeMap` and a conditionally-inserted key
        // cannot reorder the rest.
        let mut body = serde_json::Map::new();
        body.insert("verb".into(), json!(self.verb));
        body.insert("core".into(), json!(self.core));
        body.insert("home".into(), json!(self.home));
        body.insert("kind".into(), json!(self.kind));
        if let Some(series) = &self.series {
            body.insert("series".into(), json!(series));
        }
        body.insert("key".into(), json!(self.key));
        body.insert("before".into(), self.before.clone().unwrap_or(Value::Null));
        body.insert("after".into(), self.after.clone().unwrap_or(Value::Null));
        if let Some(cascade) = &self.cascade {
            body.insert("cascade".into(), cascade.clone());
        }
        Value::Object(body)
    }

    /// A hash of the exact computed change (§7.3). Deterministic: serde_json sorts
    /// object keys, so the same change always hashes the same and any edit to it
    /// changes the token.
    pub fn token(&self) -> String {
        let mut hasher = Sha256::new();
        hasher.update(serde_json::to_vec(&self.body()).unwrap_or_default());
        let digest = hasher.finalize();
        let mut out = String::with_capacity(digest.len() * 2);
        for byte in digest {
            use std::fmt::Write as _;
            let _ = write!(out, "{byte:02x}");
        }
        out
    }

    /// The `--dry-run` / exit-`5` contract JSON: the verb, the plan token, the change.
    pub fn to_json(&self) -> Value {
        json!({
            "plan": self.verb,
            "token": self.token(),
            "change": self.body(),
        })
    }

    /// Verify a caller-supplied plan token still matches (§7.3). A mismatch means the
    /// record moved under the review — a validation failure (exit `3`).
    pub fn check_token(&self, supplied: &str) -> Result<()> {
        if self.token() == supplied {
            Ok(())
        } else {
            Err(Error::validation(
                "plan token is stale: the record changed since the dry-run — review again (§7.3)",
            ))
        }
    }
}

/// What the confirm rule decided for one mutation (§7.3).
pub enum Checkpoint {
    /// Go ahead and write.
    Apply,
    /// `--dry-run`: print the change, write nothing, exit `0`.
    DryRun(Value),
    /// Not a terminal and no `-y`: print the change, exit `5` for the caller to review.
    ConfirmRequired(Value),
}

/// The hardcoded confirm rule, one for everyone — there is no autonomy knob (§7.3,
/// §18). A **fresh** `add` never reaches here; a mutation always does.
pub fn checkpoint(
    change: &RecordChange,
    dry_run: bool,
    yes: bool,
    plan: Option<&str>,
) -> Result<Checkpoint> {
    let json = change.to_json();
    if dry_run {
        return Ok(Checkpoint::DryRun(json));
    }
    let applying = yes || (io::stdout().is_terminal() && confirm(&json));
    if !applying {
        // Not a terminal, no `-y`: the structural checkpoint an LLM hand writes through.
        return Ok(Checkpoint::ConfirmRequired(json));
    }
    if let Some(token) = plan {
        change.check_token(token)?;
    }
    Ok(Checkpoint::Apply)
}

fn confirm(change_json: &Value) -> bool {
    eprintln!(
        "{}",
        serde_json::to_string_pretty(change_json).unwrap_or_else(|_| change_json.to_string())
    );
    eprint!("apply this change? [y/N] ");
    let _ = io::stderr().flush();
    let mut line = String::new();
    io::stdin().read_line(&mut line).is_ok() && matches!(line.trim(), "y" | "Y" | "yes")
}

// ── the editor follows the hand too (§7.3, I8) ───────────────────────────────

/// Whether a rule is driving this process (§9.3). Under it every write verb is
/// refused before it computes anything: the one reactive writer is Auspex, and a
/// rule may not borrow a hand's authority (I2).
pub fn under_rule() -> bool {
    std::env::var_os("PANTHEON_RULE").is_some_and(|v| v == "1")
}

/// The exit-`6` refusal a write verb gives under `PANTHEON_RULE=1` (§7.3, §9.3).
pub fn refused_under_rule(verb: &str) -> Error {
    Error::write_refused(format!(
        "`{verb}` is a write verb and PANTHEON_RULE=1 refuses it; a rule that wants a value uses \
         `get` or `where` (§9.3)"
    ))
}

/// Whether stdout is a terminal — the one test that decides both the format and
/// whether an `edit` opens an editor or prints a path (§7.3).
pub fn stdout_is_terminal() -> bool {
    io::stdout().is_terminal()
}

/// Whether stdin is a terminal. The counterpart test for a verb that can take its
/// payload off a pipe — `tab add ecv note < draft.md` (§7.3, I8).
pub fn stdin_is_terminal() -> bool {
    io::stdin().is_terminal()
}

/// What came back from an editor session (§7.3).
#[derive(Clone, Debug)]
pub enum Edited {
    /// The text came back unchanged — write nothing, exit `0`.
    Unchanged,
    /// New text, to be folded back into the record.
    Changed(String),
}

/// Open `initial` in the hand's own editor and hand back what it saved (§7.3).
///
/// The editor is the environment's, never Pantheon's: `$VISUAL`, else `$EDITOR`,
/// else `vi`. There is no `PANTHEON_EDITOR`, no per-core variable, and no
/// `--editor` flag — that is a knob where the OS already has one (§18), and the
/// shell already overrides it per command.
///
/// Nothing is locked across the session (§6.4): a session runs for minutes, and any
/// hand may edit the record directly meanwhile regardless (I8). The lock is taken to
/// read and again to write back. An editor exiting non-zero writes nothing (exit `1`).
pub fn edit_text(initial: &str) -> Result<Edited> {
    edit_text_in(&editor_command(), initial)
}

/// [`edit_text`] against a stated editor command rather than the environment's —
/// the seam a test drives, since the environment is the hand's to set (§7.3).
pub fn edit_text_in(command: &str, initial: &str) -> Result<Edited> {
    let scratch = scratch_path();
    std::fs::write(&scratch, initial)?;
    if let Err(e) = spawn_editor(command, &scratch) {
        let _ = std::fs::remove_file(&scratch);
        return Err(e);
    }
    let text = std::fs::read_to_string(&scratch)?;
    let _ = std::fs::remove_file(&scratch);
    if text == initial {
        Ok(Edited::Unchanged)
    } else {
        Ok(Edited::Changed(text))
    }
}

/// Open a file **itself** in the hand's own editor (§7.3).
///
/// What opens follows the shape (§6.1). An entity field or a series line opens a
/// scratch buffer holding only that value, because the JSON record is machine-owned
/// and is never handed to a hand raw (I6, §6.6) — but a **document is opened in
/// place**, since it already *is* the text (§7.3, §8.7). So there is no scratch file
/// here and nothing to fold back.
///
/// That has one consequence worth stating plainly: **the editor commits the file
/// itself**, so §7.3's three outcomes land as — editor exits non-zero: it saved
/// nothing, exit `1`; bytes unchanged: nothing was written, exit `0`; text invalid:
/// the hand's own save stands on disk and the caller reports exit `3`. Invalid is
/// *not* reverted — the editor already wrote it, §18 keeps no prior copy, and
/// restoring one would be the undo layer §18 forbids.
///
/// Nothing is locked across the session (§6.4).
pub fn edit_file(path: &Path) -> Result<Edited> {
    edit_file_in(&editor_command(), path)
}

/// [`edit_file`] against a stated editor command rather than the environment's —
/// the seam a test drives (§7.3).
pub fn edit_file_in(command: &str, path: &Path) -> Result<Edited> {
    let before = std::fs::read_to_string(path)?;
    spawn_editor(command, path)?;
    let after = std::fs::read_to_string(path)?;
    if after == before {
        Ok(Edited::Unchanged)
    } else {
        Ok(Edited::Changed(after))
    }
}

/// Resolve the editor command to an argv and run it against `path`. Shared by both
/// editor forms so they cannot drift on which variable wins or how it is split —
/// `shell-words`, so `code -w` works, and the child is spawned directly, never
/// through `sh -c` (§13).
fn spawn_editor(command: &str, path: &Path) -> Result<()> {
    let words = shell_words::split(command)
        .map_err(|e| Error::usage(format!("$VISUAL/$EDITOR is not a valid command: {e}")))?;
    let (program, args) = words
        .split_first()
        .ok_or_else(|| Error::usage("$VISUAL/$EDITOR is empty (§7.3)"))?;
    let status = std::process::Command::new(program)
        .args(args)
        .arg(path)
        .status()
        .map_err(|e| Error::runtime(format!("could not run {program:?}: {e}")))?;
    if !status.success() {
        return Err(Error::runtime(format!(
            "{program} exited without saving; nothing was written (§7.3)"
        )));
    }
    Ok(())
}

fn editor_command() -> String {
    for key in ["VISUAL", "EDITOR"] {
        if let Ok(value) = std::env::var(key) {
            if !value.trim().is_empty() {
                return value;
            }
        }
    }
    "vi".to_string()
}

static SCRATCH: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

fn scratch_path() -> PathBuf {
    let n = SCRATCH.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    std::env::temp_dir().join(format!("pantheon-edit-{}-{n}.txt", std::process::id()))
}

// ── the key is what you give, never invented (§7.3) ──────────────────────────

/// Turn `--at` into a series key (§7.3): `YYMMDD`, `YYMMDDThhmm`, or `hhmm` for a
/// time today; absent, today's date. The tool never auto-suffixes to dodge a
/// collision — a second reading on one key is an overwrite, and confirms as one.
pub fn key_from_at(at: Option<&str>) -> Result<Key> {
    let today = today();
    let Some(raw) = at else {
        return Key::parse(&today);
    };
    let given = raw.trim();
    let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    match given.split_once('T') {
        Some((date, time))
            if digits(date) && date.len() == 6 && digits(time) && time.len() == 4 =>
        {
            Key::parse(given)
        }
        None if digits(given) && given.len() == 6 => Key::parse(given),
        None if digits(given) && given.len() == 4 => Key::parse(&format!("{today}T{given}")),
        _ => Err(Error::usage(format!(
            "-a takes YYMMDD, YYMMDDThhmm, or hhmm; got {raw:?} (§7.3)"
        ))),
    }
}

fn today() -> String {
    let date = jiff::Zoned::now().date();
    format!(
        "{:02}{:02}{:02}",
        date.year().rem_euclid(100),
        date.month(),
        date.day()
    )
}

// ── home and series are found, never invented (§7.3) ─────────────────────────

/// A resolved write target: which series, at which node, and the positional values
/// left over once the leading tokens were consumed.
pub struct SeriesTarget {
    pub home: Code,
    pub kind: String,
    pub name: String,
    pub values: Vec<String>,
    /// The series file, if it already exists — `None` means nothing to append to.
    pub existing: Option<SeriesRef>,
}

/// What a verb knows before the tree is walked: the tokens a hand typed and the
/// flags that override them (§7.3).
pub struct TargetQuery<'a> {
    /// Which of the core's tokens is meant (§7.2).
    pub kind: &'a str,
    /// `-H`, if given.
    pub home: Option<&'a str>,
    /// `--series`, if given.
    pub series: Option<&'a str>,
    /// The leading positionals, still unclassified.
    pub positionals: &'a [String],
    /// `-c`: mint the series before writing (§7.3).
    pub create: bool,
    /// Whether trailing positionals are the reading's **values** (a write) or there
    /// are none to give (a read verb). This is what tells `ann 78.4` — a reading at
    /// the node `$PWD` sits in — from `ann get weight`, where the lone token can
    /// only be the series' name.
    pub takes_values: bool,
    /// The locus; `None` reads the process's working directory (§7.3).
    pub pwd: Option<&'a Path>,
}

/// The four forms of §7.3 — **both**, **home only**, **series only**, **neither** —
/// over one core's own series. Inference *finds* an existing series; it never
/// creates one, and where more than one answers it lists them and stops rather than
/// guessing.
pub fn resolve_series_target<C: Core>(
    store: &Store<C>,
    query: &TargetQuery<'_>,
) -> Result<SeriesTarget> {
    let &TargetQuery {
        kind, create, pwd, ..
    } = query;
    let (mut home, name, rest) = classify_tokens(store, query)?;

    // `-c` mints, so it is refused on an inference form: a typo must not spawn a
    // junk series (§7.3).
    if create && (home.is_none() || name.is_none()) {
        return Err(Error::usage(
            "-c needs the home and the series both named; it is refused on an inference form (§7.3)",
        ));
    }

    // The locus is $PWD — but only with no home token *and* no series named (§7.3).
    if home.is_none() && name.is_none() {
        home = Some(code_at_path(store.root(), pwd)?);
    }

    let values = rest.to_vec();
    match (home, name) {
        // both — no inference; the series must exist, or `-c`.
        (Some(home), Some(name)) => {
            let existing = store
                .find_series(Some(&home), Some(kind), Some(&name))?
                .into_iter()
                .next();
            Ok(SeriesTarget {
                home,
                kind: kind.to_string(),
                name,
                values,
                existing,
            })
        }
        // home only — infer iff the node holds exactly one of the tool's own series.
        (Some(home), None) => {
            // The four forms are the *hand-named* path (§7.3): a nameless series has
            // no name for a hand to have omitted, so it is not a candidate here.
            let mut found: Vec<SeriesRef> = store
                .find_series(Some(&home), Some(kind), None)?
                .into_iter()
                .filter(|s| s.name.is_some())
                .collect();
            match found.len() {
                0 => Err(Error::not_found(format!(
                    "no {} series at {} to append to — mint one with -c (§7.3)",
                    C::NAME,
                    home.as_str()
                ))),
                1 => {
                    let series = found.pop().expect("one candidate");
                    Ok(SeriesTarget {
                        home,
                        kind: series.kind.clone(),
                        name: series.name.clone().expect("named by the filter above"),
                        values,
                        existing: Some(series),
                    })
                }
                _ => Err(Error::usage(format!(
                    "{} holds more than one {} series: {} — name one (§7.3)",
                    home.as_str(),
                    C::NAME,
                    join(found.iter().map(SeriesRef::label))
                ))),
            }
        }
        // series only — search the whole tree; $PWD never narrows this (§7.3).
        (None, Some(name)) => {
            let mut found = store.find_series(None, Some(kind), Some(&name))?;
            match found.len() {
                0 => Err(Error::not_found(format!(
                    "no {} series named {name:?} — mint it with -c (§7.3)",
                    C::NAME
                ))),
                1 => {
                    let series = found.pop().expect("one candidate");
                    Ok(SeriesTarget {
                        home: series.home.clone(),
                        kind: series.kind.clone(),
                        name,
                        values,
                        existing: Some(series),
                    })
                }
                _ => Err(Error::usage(format!(
                    "series {name:?} is at more than one node: {} — name one with -H (§7.3)",
                    join(found.iter().map(|s| s.home.as_str()))
                ))),
            }
        }
        (None, None) => unreachable!("$PWD supplied the home above"),
    }
}

/// Read the leading positionals into a home and a series name, returning what is
/// left for the reading's values (§7.3). Either may still be `None` — that is what
/// the caller infers.
fn classify_tokens<'a, C: Core>(
    store: &Store<C>,
    query: &TargetQuery<'a>,
) -> Result<(Option<Code>, Option<String>, &'a [String])> {
    let &TargetQuery {
        kind,
        create,
        takes_values,
        ..
    } = query;
    let mut rest = query.positionals;
    let mut home = query.home.map(Code::parse).transpose()?;
    let mut name = query
        .series
        .map(|s| normalize_token(s, "series name"))
        .transpose()?;

    // A lone leading token is classified deterministically: it resolves to a node
    // code → home; otherwise → a series name (§7.3). What follows it decides that
    // second reading — a token with nothing after it is the reading itself, which is
    // what makes `ann 78.4` the *neither* form rather than a hunt for a series
    // named "78.4". A read verb has no values to give, so its lone token is a name.
    if home.is_none() && name.is_none() {
        if let Some((first, tail)) = rest.split_first() {
            if let Some(code) = as_node_code(store.root(), first) {
                home = Some(code);
                rest = tail;
            } else if create || !takes_values || !tail.is_empty() {
                // A token that normalizes to nothing names no series; leave it to be
                // read as the reading it must be (§5.1).
                if let Some(candidate) = crate::name::normalize(first) {
                    name = Some(candidate);
                    rest = tail;
                }
            }
        }
    }

    // With a home in hand, the next token names the series when it names one that
    // exists, when `-c` is minting, or when a value follows it — the last is what
    // makes a typo a not-found rather than a reading on the wrong log: `ann ecv
    // wieght 78.4` names a series that isn't there, while `ann ecv 78.4` is the
    // *home only* form and infers (§7.3).
    if name.is_none() && home.is_some() {
        if let Some((first, tail)) = rest.split_first() {
            if let Some(candidate) = crate::name::normalize(first) {
                let known = !store
                    .find_series(home.as_ref(), Some(kind), Some(&candidate))?
                    .is_empty();
                if create || known || !takes_values || !tail.is_empty() {
                    name = Some(candidate);
                    rest = tail;
                }
            }
        }
    }

    Ok((home, name, rest))
}

// ── the partitioned target: a home and a name, never a value stream (§7.3) ────

/// A resolved entity target: which slug, at which node, under which token.
pub struct EntityTarget {
    pub home: Code,
    pub kind: String,
    pub slug: String,
    /// The entity file, if it already exists — `Some` makes a write an overwrite.
    pub existing: Option<EntityRef>,
}

/// What a partitioned verb knows before the tree is walked (§7.3). Deliberately not
/// a [`TargetQuery`]: a series verb infers a *container that must already exist*,
/// while a partitioned `add` **is** the record it creates (§18), so there is nothing
/// to find and no trailing value stream to separate.
pub struct EntityQuery<'a> {
    /// Which of the core's tokens is meant — for a write, exactly one (§7.2).
    pub kind: &'a str,
    /// `-H`, if given.
    pub home: Option<&'a str>,
    /// The leading positionals, still unclassified.
    pub positionals: &'a [String],
    /// The locus; `None` reads the process's working directory (§7.3).
    pub pwd: Option<&'a Path>,
}

/// Read `[home] <name>` into an address (§7.3). Total on arity: one token is a name
/// (or a home with the name missing), two are a home and a name, and three or more
/// is a usage error rather than a silent join — `alb csa john appleseed` must refuse,
/// because a name that quietly became `john` would be the wrong record forever.
pub fn resolve_entity_target<C: Core>(
    store: &Store<C>,
    query: &EntityQuery<'_>,
) -> Result<EntityTarget> {
    let &EntityQuery {
        kind, home, pwd, ..
    } = query;
    let (home, rest) = peel_home(store, home, query.positionals)?;

    let slug = match rest {
        [] => {
            return Err(Error::usage(format!("name the {} record (§7.3)", C::NAME)));
        }
        [one] => normalize_token(one, "name")?,
        [first, ..] => {
            let joined =
                crate::name::normalize(&rest.join("_")).unwrap_or_else(|| (*first).to_string());
            return Err(Error::usage(format!(
                "a name is one token, and {} were given — did you mean {joined:?}? (§5.1, §7.3)",
                rest.len()
            )));
        }
    };

    let home = match home {
        Some(home) => home,
        None => code_at_path(store.root(), pwd)?,
    };
    let existing = store.slug_taken_at(&home, &slug)?;
    Ok(EntityTarget {
        home,
        kind: kind.to_string(),
        slug,
        existing,
    })
}

/// Peel an optional leading home token off the positionals (§7.3). A lone leading
/// token is classified deterministically: it names a node in the tree → home;
/// otherwise it stays where it is, to be read as the record's own name.
///
/// The token is a home only if something **follows** it to be that name — `alb csa`
/// files a person called `csa`, it does not address the node with the name missing.
/// A stated `-H` short-circuits the probe entirely, which is the escape hatch for a
/// name that happens to spell a node code.
pub fn peel_home<'a, C: Core>(
    store: &Store<C>,
    stated: Option<&str>,
    positionals: &'a [String],
) -> Result<(Option<Code>, &'a [String])> {
    if let Some(stated) = stated {
        return Ok((Some(Code::parse(stated)?), positionals));
    }
    if let Some((first, tail)) = positionals.split_first()
        && !tail.is_empty()
        && let Some(code) = as_node_code(store.root(), first)
    {
        return Ok((Some(code), tail));
    }
    Ok((None, positionals))
}

// ── the keyed target: a home, a record's own name, and what follows it ───────

/// A resolved keyed-line target: which record, at which node, and the positionals
/// left over once the home and the key were consumed.
pub struct RegisterTarget {
    pub home: Code,
    pub kind: String,
    /// The record's identity — the name a hand gave, normalized to its slug (§5.4).
    pub key: Key,
    pub values: Vec<String>,
    /// The node's determined series, if it exists yet — `None` at a node whose first
    /// task this is (§7.3, §8.5).
    pub existing: Option<SeriesRef>,
}

/// What a determined-series write knows before the tree is walked (§7.3).
///
/// Deliberately neither a [`TargetQuery`] nor an [`EntityQuery`]. A series verb
/// infers a *container* that must already exist and a partitioned `add` **is** the
/// record it creates; this third form has no container to name — the node implies it
/// — and the record it creates is a *line inside* one, minted by that same write
/// where the file is not there yet (§8.5, §18).
pub struct RegisterQuery<'a> {
    /// Which of the core's tokens is meant — for a write, exactly one (§7.2).
    pub kind: &'a str,
    /// `-H`, if given.
    pub home: Option<&'a str>,
    /// The leading positionals, still unclassified.
    pub positionals: &'a [String],
    /// The locus; `None` reads the process's working directory (§7.3).
    pub pwd: Option<&'a Path>,
}

/// Read `[home] <name> [value…]` into a keyed address (§7.3).
///
/// With no series name to type, §7.3's four inference forms have nowhere to put a
/// collection token and collapse to this one. What is left is `[home] <name>` — the
/// grammar [`resolve_entity_target`] reads — followed by whatever the verb takes,
/// which is what stops this being total on arity the way that one is: at two tokens
/// `pen acm reach_out_to_alex` and `pen reach_out_to_alex "call re: contract"` are
/// the same shape. §7.3's own rule decides between them, and `-H` forces either
/// reading — the probe never runs when a home is stated.
pub fn resolve_register_target<C: Core>(
    store: &Store<C>,
    query: &RegisterQuery<'_>,
) -> Result<RegisterTarget> {
    let &RegisterQuery { kind, pwd, .. } = query;
    let (home, rest) = peel_home(store, query.home, query.positionals)?;
    let Some((first, values)) = rest.split_first() else {
        return Err(Error::usage(format!("name the {} record (§7.3)", C::NAME)));
    };
    let key = Key::parse(first)?;
    let home = match home {
        Some(home) => home,
        None => code_at_path(store.root(), pwd)?,
    };
    // The node's own series, which the write mints if this is its first record.
    // Nothing to search for: a nameless series' location is settled by its home and
    // its token, so this is one `exists` and not the subtree walk `find_series` would
    // make — which would reach *past* this node and answer with a child's file.
    let path = store.series_path(&home, kind, None)?;
    let existing = path.exists().then(|| SeriesRef {
        home: home.clone(),
        kind: kind.to_string(),
        name: None,
        path,
    });
    Ok(RegisterTarget {
        home,
        kind: kind.to_string(),
        key,
        values: values.to_vec(),
        existing,
    })
}

// ── the document target: a home, a slug, and the prose after it ──────────────

/// A resolved document target: which document, at which node, and the positionals
/// left over once the home and the name were consumed.
pub struct DocumentTarget {
    pub home: Code,
    /// The document's identity — the name a hand gave, normalized to its slug (§5.4).
    pub slug: String,
    /// The positionals after the name — the prose, for a verb that takes it. Empty
    /// for a read verb, which has none to give.
    pub body: Vec<String>,
    /// The document already at this node under **any** extension, if there is one.
    /// Any, not just the one asked for: `csa_note.md` and `csa_note.txt` are two
    /// files and one `tabella:note`, so the extension is not part of the identity
    /// this checks (§5.4).
    pub existing: Option<DocumentRef>,
}

/// What a document verb knows before the tree is walked (§7.3).
///
/// Deliberately none of the three queries above. There is no `kind` to carry — a
/// Document core declares none, which is what names it Document (§7.1) — and no
/// series to infer, so §7.3's four inference forms have nothing to range over.
pub struct DocumentQuery<'a> {
    /// `-H`, if given.
    pub home: Option<&'a str>,
    /// The leading positionals, still unclassified.
    pub positionals: &'a [String],
    /// The locus; `None` reads the process's working directory (§7.3).
    pub pwd: Option<&'a Path>,
}

/// Read `[home] <name> [prose…]` into a document address (§7.3).
///
/// The same grammar [`resolve_register_target`] reads, and not total on arity for the
/// same reason: at two tokens `tab ecv trip_idea` and `tab trip_idea "some prose"`
/// are the same shape, so §7.3's rule decides — [`peel_home`] takes a leading token
/// as the home only when something follows it to be the name, and `-H` forces either
/// reading.
pub fn resolve_document_target<C: Core>(
    store: &Store<C>,
    query: &DocumentQuery<'_>,
) -> Result<DocumentTarget> {
    let (home, rest) = peel_home(store, query.home, query.positionals)?;
    let Some((first, body)) = rest.split_first() else {
        return Err(Error::usage(format!(
            "name the {} document (§7.3)",
            C::NAME
        )));
    };
    let slug = crate::name::normalize_token(first, "slug")?;
    let home = match home {
        Some(home) => home,
        None => code_at_path(store.root(), query.pwd)?,
    };
    let existing = store.document_slug_taken_at(&home, &slug)?;
    Ok(DocumentTarget {
        home,
        slug,
        body: body.to_vec(),
        existing,
    })
}

/// Whether a token names a node in the tree — the classification that tells a home
/// token from a series name (§7.3).
fn as_node_code(root: &Path, token: &str) -> Option<Code> {
    let code = Code::parse(token).ok()?;
    resolve_code(root, &code).ok().map(|_| code)
}

/// The locus (§7.3): walk down from the root along `$PWD` to the deepest node it
/// sits in. No stored cursor — the shell tracks location for all three hands (I8).
pub fn code_at_path(root: &Path, pwd: Option<&Path>) -> Result<Code> {
    let pwd: PathBuf = match pwd {
        Some(p) => p.to_path_buf(),
        None => std::env::current_dir()?,
    };
    let pwd = pwd.canonicalize().unwrap_or(pwd);
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let rel = pwd
        .strip_prefix(&root)
        .map_err(|_| Error::usage("$PWD is outside the tree root; name the home with -H (§7.3)"))?;
    let mut here: Option<Code> = None;
    for component in rel.components() {
        let name = component.as_os_str().to_string_lossy();
        if name.ends_with("__") {
            break; // a meta dir is not a node
        }
        match parse_node_dirname(here.as_ref(), &name) {
            Ok(node) => here = Some(node.code),
            Err(_) => break,
        }
    }
    here.ok_or_else(|| Error::usage("no node at $PWD; name the home with -H (§7.3)"))
}

fn join<'a>(items: impl Iterator<Item = &'a str>) -> String {
    items.collect::<Vec<_>>().join(", ")
}

// ── shaping a record into the contract's JSON (§7.2) ─────────────────────────

/// One record as the contract emits it. The envelope on disk is `{key,refs,data}`;
/// the home, core, kind, and series come from the file's location and name (I3) and
/// are added here so a reader of the JSON alone knows what it is looking at.
///
/// `series` is **absent** — not null — where the core's series is determined (§9.3):
/// there is no name to report, and a hollow key would read as one withheld. This is
/// the same conditional insert [`RecordChange::body`] makes.
pub fn line_json<T: Serialize>(
    core: &str,
    home: &Code,
    kind: &str,
    series: Option<&str>,
    line: &Line<T>,
) -> Result<Value> {
    let mut out = serde_json::Map::new();
    out.insert("core".into(), json!(core));
    out.insert("home".into(), json!(home.as_str()));
    out.insert("kind".into(), json!(kind));
    if let Some(series) = series {
        out.insert("series".into(), json!(series));
    }
    out.insert("key".into(), json!(line.key.as_str()));
    out.insert(
        "refs".into(),
        json!(line.refs.iter().map(Ref::to_token).collect::<Vec<_>>()),
    );
    out.insert("data".into(), serde_json::to_value(&line.data)?);
    Ok(Value::Object(out))
}

/// A series' present, as [`line_json`] (§7.2).
pub fn present_json<T: Serialize>(core: &str, present: &PresentLine<T>) -> Result<Value> {
    line_json(
        core,
        &present.home,
        &present.kind,
        present.name.as_deref(),
        &present.line,
    )
}

/// A whole collection, in file order (§7.2).
pub fn series_json<T: Serialize>(core: &str, sref: &SeriesRef, lines: &[Line<T>]) -> Result<Value> {
    let rows = lines
        .iter()
        .map(|line| line_json(core, &sref.home, &sref.kind, sref.name.as_deref(), line))
        .collect::<Result<Vec<_>>>()?;
    Ok(Value::Array(rows))
}

/// A fold across a subtree, one row per series (§7.2).
pub fn fold_json<T: Serialize>(core: &str, folded: &[PresentLine<T>]) -> Result<Value> {
    let rows = folded
        .iter()
        .map(|present| present_json(core, present))
        .collect::<Result<Vec<_>>>()?;
    Ok(Value::Array(rows))
}

/// One entity as the contract emits it. The envelope on disk is `{refs,data}`; the
/// home, core, kind, and slug are the file's location and name (I3) and are added
/// here so a reader of the JSON alone knows what it is looking at. There is no
/// `key` — an entity's *name* is its key, and no `series` — there is none (§18).
pub fn entity_json<T: Serialize>(
    core: &str,
    eref: &EntityRef,
    entity: &Entity<T>,
) -> Result<Value> {
    Ok(json!({
        "core": core,
        "home": eref.home.as_str(),
        "kind": eref.kind,
        "slug": eref.slug,
        "refs": entity.refs.iter().map(Ref::to_token).collect::<Vec<_>>(),
        "data": serde_json::to_value(&entity.data)?,
    }))
}

/// A fold across a subtree, one row per entity (§7.2). Unlike a series fold nothing
/// is collapsed: an entity is not a sample, so each file is already its own present.
pub fn entity_fold_json<T: Serialize>(
    core: &str,
    folded: &[(EntityRef, Entity<T>)],
) -> Result<Value> {
    let rows = folded
        .iter()
        .map(|(eref, entity)| entity_json(core, eref, entity))
        .collect::<Result<Vec<_>>>()?;
    Ok(Value::Array(rows))
}

/// One document as the contract emits it (§7.2): the frontmatter's two fields plus
/// the prose, with the home, core, slug, and extension added from the file's location
/// and name (I3) — the same courtesy [`entity_json`] does, so a reader of the JSON
/// alone knows what it is looking at.
///
/// `type` is present as **null** where the frontmatter carries none: a withheld key
/// reads as a different answer than an absent value. There is no `kind` — a Document
/// core declares none (§7.1) — and no `refs`, which is exactly why `-r` is a usage
/// error on this core (§6.1, §7.3).
pub fn document_json(core: &str, dref: &DocumentRef, document: &Document) -> Value {
    json!({
        "core": core,
        "home": dref.home.as_str(),
        "slug": dref.slug,
        "ext": dref.ext.as_str(),
        "type": document.frontmatter.r#type,
        "tags": document.frontmatter.tags,
        "body": document.body,
    })
}

/// A fold across a subtree, one row per document (§7.2) — **frontmatter only**.
///
/// There is no `body` key at all rather than a null one: a fold never reads bodies
/// (§7.1, §8.7), and emitting the key empty would claim it had looked and found
/// nothing. Selecting on `type` or `tags` is the caller's job over this JSON, since
/// Tabella declares no read flags of its own (§8.7, I4).
pub fn document_fold_json(core: &str, folded: &[(DocumentRef, Frontmatter)]) -> Value {
    Value::Array(
        folded
            .iter()
            .map(|(dref, front)| {
                json!({
                    "core": core,
                    "home": dref.home.as_str(),
                    "slug": dref.slug,
                    "ext": dref.ext.as_str(),
                    "type": front.r#type,
                    "tags": front.tags,
                })
            })
            .collect::<Vec<_>>(),
    )
}
