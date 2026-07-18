//! The verb runner (§7.1): the parts of a core's CLI that are the *same* for every
//! core, so the contract's JSON is produced one way rather than seven (I4).
//!
//! A core's bin owns its own flags and positionals — those follow its primitive
//! (§7.3) — and calls in here for everything downstream of them: how the hand is
//! read (TTY → table, pipe → JSON), how a mutation confirms, how `--at` becomes a
//! key, how a home and a series are *found* rather than invented, and how a record
//! is shaped into the emitted JSON.
//!
//! Step 2 lands the Series-shaped executors, the shape Annales exercises (§8.6).
//! The Partitioned and Document paths grow here with the cores that exercise them.

use std::io::{self, IsTerminal, Write};
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::code::{Code, parse_node_dirname};
use crate::core::Core;
use crate::envelope::{Key, Line, Ref};
use crate::name::normalize_token;
use crate::store::{PresentLine, SeriesRef, Store};
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

/// Render a contract value: compact down a pipe, pretty for a reader. (A real
/// table is later polish; the data is identical either way.)
pub fn emit(value: &Value, as_json: bool) {
    if as_json {
        println!("{value}");
    } else {
        println!(
            "{}",
            serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string())
        );
    }
}

/// The whole tail of a core's `main`: render what the verb produced and return the
/// process exit code, printing the `{"error":{…}}` envelope to stderr on a failure
/// (§7.3). Every core ends identically.
pub fn dispatch(outcome: Result<Response>, as_json: bool) -> std::process::ExitCode {
    match outcome {
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
    }
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
    pub series: String,
    pub key: String,
    /// The record as it stands, if it already exists — what an overwrite replaces.
    pub before: Option<Value>,
    /// The record as it would stand; `None` for a removal.
    pub after: Option<Value>,
}

impl RecordChange {
    fn body(&self) -> Value {
        json!({
            "verb": self.verb,
            "core": self.core,
            "home": self.home,
            "kind": self.kind,
            "series": self.series,
            "key": self.key,
            "before": self.before,
            "after": self.after,
        })
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
            let mut found = store.find_series(Some(&home), Some(kind), None)?;
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
                        name: series.name.clone(),
                        values,
                        existing: Some(series),
                    })
                }
                _ => Err(Error::usage(format!(
                    "{} holds more than one {} series: {} — name one (§7.3)",
                    home.as_str(),
                    C::NAME,
                    join(found.iter().map(|s| s.name.as_str()))
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
pub fn line_json<T: Serialize>(
    core: &str,
    home: &Code,
    kind: &str,
    series: &str,
    line: &Line<T>,
) -> Result<Value> {
    Ok(json!({
        "core": core,
        "home": home.as_str(),
        "kind": kind,
        "series": series,
        "key": line.key.as_str(),
        "refs": line.refs.iter().map(Ref::to_token).collect::<Vec<_>>(),
        "data": serde_json::to_value(&line.data)?,
    }))
}

/// A series' present, as [`line_json`] (§7.2).
pub fn present_json<T: Serialize>(core: &str, present: &PresentLine<T>) -> Result<Value> {
    line_json(
        core,
        &present.home,
        &present.kind,
        &present.name,
        &present.line,
    )
}

/// A whole collection, in file order (§7.2).
pub fn series_json<T: Serialize>(core: &str, sref: &SeriesRef, lines: &[Line<T>]) -> Result<Value> {
    let rows = lines
        .iter()
        .map(|line| line_json(core, &sref.home, &sref.kind, &sref.name, line))
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
