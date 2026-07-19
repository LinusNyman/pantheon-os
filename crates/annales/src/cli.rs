//! `ann` — Annales' CLI (§7). stdout is JSON when piped, a table on a TTY (§7.3).
//!
//! The bin owns only what is Annales' own: its positionals (a reading's values) and
//! the flags its primitive needs (`--series`, `-c`, `-a`, `-r`, `--note`). Everything
//! downstream — reading the hand, confirming a mutation, turning `--at` into a key,
//! *finding* a home and a series, shaping a record into the contract's JSON — is
//! `pantheon::contract`, so every core produces that JSON the same way (I4).

// This module shares the spine's conventional pedantic allows (see pantheon/src/lib.rs).
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::doc_markdown)]
#![allow(clippy::too_many_lines)]
#![allow(clippy::too_many_arguments)]

use std::ffi::OsString;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand, ValueEnum};
use serde_json::{Value, json};

use crate::{Annales, LogReading};

use pantheon::envelope::{Key, Line, Ref};
use pantheon::{
    Checkpoint, Code, Core, Error, RecordChange, Response, Result, SeriesRef, SeriesTarget, Store,
    contract, resolve_root,
};

/// The twelve verbs (§7.3). A closed reserved set: a verb wins over a node code,
/// which is what makes `add` safe to leave implicit (the ambiguity rule, §7.3).
const VERBS: &[&str] = &[
    "add", "edit", "rename", "move", "mv", "rm", "list", "ls", "get", "series", "where", "schema",
    "help", "version",
];

/// What a headless build prints for a bare short (§14, §7.3).
#[cfg(not(feature = "tui"))]
const BARE: &str = "ann — Annales (actio · fact). Built without the `tui` feature; run `ann --help` for the verbs.\n";

#[derive(Parser)]
#[command(
    name = "ann",
    version,
    about = "Annales — the fact tense: what happened, as dated readings in a named log (§8.6).",
    disable_help_subcommand = true
)]
pub(crate) struct Cli {
    /// The tree root; else $PANTHEON_ROOT, else a usage error (§6.2).
    #[arg(short = 'C', long = "root", global = true, value_name = "DIR")]
    root: Option<PathBuf>,
    /// Force output format; default follows the hand (TTY vs pipe, §7.3).
    #[arg(short = 'f', long = "format", global = true, value_enum)]
    format: Option<Format>,
    /// Apply a mutation without prompting (§7.3).
    #[arg(short = 'y', long = "yes", global = true)]
    yes: bool,
    /// Compute and print the change without writing (§7.3).
    #[arg(short = 'n', long = "dry-run", global = true)]
    dry_run: bool,
    /// A plan token from a prior dry-run; honored on apply (§7.3).
    #[arg(short = 'p', long = "plan", global = true, value_name = "TOKEN")]
    plan: Option<String>,
    /// State the home explicitly (§7.3).
    #[arg(short = 'H', long = "home", global = true, value_name = "CODE")]
    home: Option<String>,
    /// Which of the core's tokens (§7.2). Annales declares one: `log`.
    #[arg(short = 'k', long = "kind", global = true, value_name = "K")]
    kind: Option<String>,
    #[command(subcommand)]
    cmd: Option<Cmd>,
}

#[derive(Clone, Copy, ValueEnum)]
enum Format {
    Json,
    Table,
}

#[derive(Subcommand)]
enum Cmd {
    /// Append a reading to an existing log; `-c` mints the log first (§7.3).
    ///
    /// Tokens are `[home] [series] [values…]`, each inferable but only ever found:
    /// `ann ecv weight 78.4` · `ann ecv 78.4` · `ann weight 78.4` · `ann 78.4`.
    Add {
        tokens: Vec<String>,
        /// Mint the series before writing the first reading (§7.3).
        #[arg(short = 'c', long = "create")]
        create: bool,
        /// The reading's date, date and time, or a time today (§7.3).
        #[arg(short = 'a', long = "at", value_name = "WHEN")]
        at: Option<String>,
        /// Attach a reference; repeatable (§5.4).
        #[arg(short = 'r', long = "ref", value_name = "REF")]
        refs: Vec<String>,
        /// Name the series explicitly.
        #[arg(long = "series", value_name = "NAME")]
        series: Option<String>,
        /// A remark on this reading.
        #[arg(long = "note", value_name = "TEXT")]
        note: Option<String>,
    },
    /// Correct a reading in place, by its key (§7.2). A correction rewrites the
    /// keyed line; it never stacks a second (I1).
    ///
    /// Given no new value this is the editor form (§7.3): at a TTY the reading's
    /// values open in `$VISUAL`/`$EDITOR`/`vi`; piped, it prints `{"path":…}`.
    Edit {
        key: String,
        values: Vec<String>,
        #[arg(short = 'r', long = "ref", value_name = "REF")]
        refs: Vec<String>,
        #[arg(long = "series", value_name = "NAME")]
        series: Option<String>,
        #[arg(long = "note", value_name = "TEXT")]
        note: Option<String>,
    },
    /// Remove a reading by its key — irreversible (§7.2, §18).
    Rm {
        key: String,
        #[arg(long = "series", value_name = "NAME")]
        series: Option<String>,
    },
    /// The folded present across the subtree: every log at its latest key (§7.2, I1).
    #[command(alias = "ls")]
    List,
    /// One log's present — the reading at its latest key (§7.2, I1).
    Get { tokens: Vec<String> },
    /// A whole collection: the trend across keys, optionally windowed (§7.2).
    Series {
        tokens: Vec<String>,
        #[arg(long = "from", value_name = "KEY")]
        from: Option<String>,
        #[arg(long = "to", value_name = "KEY")]
        to: Option<String>,
    },
    /// Resolve a log to its home code, by walking Annales' own files (§7.3).
    Where { tokens: Vec<String> },
    /// Self-description: name, tokens and shapes, record schema, format version (§7.2).
    Schema,
    /// Rename a log and cascade its refs (§7.2) — lands with Album (step 3).
    Rename { slug: String, new: String },
    /// Re-home a log (§7.2) — lands with Album (step 3).
    #[command(alias = "mv")]
    Move {
        slug: String,
        #[arg(long = "to", value_name = "CODE")]
        to: String,
    },
    /// This tool's name, short, and version, as JSON (§7.3).
    Version,
    /// The verbs, as JSON (§7.3).
    Help,
}

/// Run `ann` exactly as the binary runs it (§7.3) — parse `argv`, dispatch, and
/// return the process's exit code. The bin is a shell over this and holds nothing of
/// its own.
#[must_use]
pub fn run_cli() -> ExitCode {
    let cli = Cli::parse_from(with_default_verb(std::env::args_os()));
    let as_json = contract::format_is_json(cli.format.map(|f| matches!(f, Format::Json)));
    contract::dispatch(run(&cli, as_json), as_json)
}

/// The flags that take a separate value — what the verb scan must step over to find
/// the first *word* on the line. (`--flag=value` needs no entry: it is one token.)
const VALUE_FLAGS: &[&str] = &[
    "-C", "--root", "-f", "--format", "-p", "--plan", "-H", "--home", "-k", "--kind", "-a", "--at",
    "-r", "--ref", "--series", "--note", "--from", "--to",
];

/// `add` is the default verb (§7.3): where the first word on the line is not one of
/// the reserved verbs, it begins an `add`. Verbs are a closed set and win over any
/// other reading of that token — the ambiguity rule (§7.3).
pub(crate) fn with_default_verb(raw: impl Iterator<Item = OsString>) -> Vec<OsString> {
    let mut argv: Vec<OsString> = raw.collect();
    // Step over leading flags (and their values) to find the first word.
    let mut at = 1;
    while at < argv.len() {
        let token = argv[at].to_string_lossy().into_owned();
        if token == "--" {
            at += 1;
            break;
        }
        if !token.starts_with('-') {
            break;
        }
        at += usize::from(VALUE_FLAGS.contains(&token.as_str())) + 1;
    }
    // Nothing but flags: a bare short (the TUI, §7.3) or `--help`/`--version`.
    let Some(word) = argv.get(at) else {
        return argv;
    };
    if VERBS.contains(&word.to_string_lossy().as_ref()) {
        return argv;
    }
    argv.insert(at, OsString::from("add"));
    argv
}

pub(crate) fn run(cli: &Cli, as_json: bool) -> Result<Response> {
    let Some(cmd) = &cli.cmd else {
        // A bare short opens the TUI at a terminal; piped, it emits help (§7.3) — a
        // screen has nothing to draw down a pipe.
        if as_json {
            return Ok(Response::Json(help_json()));
        }
        #[cfg(feature = "tui")]
        {
            let root = resolve_root(cli.root.as_deref())?;
            crate::screen::open(&root).map_err(|e| Error::runtime(e.to_string()))?;
            return Ok(Response::Raw(String::new()));
        }
        // Headless: there is no screen to open, so help is the whole answer (§14).
        #[cfg(not(feature = "tui"))]
        return Ok(Response::Raw(BARE.to_string()));
    };
    match cmd {
        Cmd::Add {
            tokens,
            create,
            at,
            refs,
            series,
            note,
        } => cmd_add(
            cli,
            tokens,
            *create,
            at.as_deref(),
            refs,
            series.as_deref(),
            note.as_deref(),
        ),
        Cmd::Edit {
            key,
            values,
            refs,
            series,
            note,
        } => cmd_edit(cli, key, values, refs, series.as_deref(), note.as_deref()),
        Cmd::Rm { key, series } => cmd_rm(cli, key, series.as_deref()),
        Cmd::List => cmd_list(cli),
        Cmd::Get { tokens } => cmd_get(cli, tokens),
        Cmd::Series { tokens, from, to } => cmd_series(cli, tokens, from.as_deref(), to.as_deref()),
        Cmd::Where { tokens } => cmd_where(cli, tokens),
        Cmd::Schema => Ok(Response::Json(serde_json::to_value(pantheon::schema::<
            Annales,
        >(1))?)),
        Cmd::Version => Ok(Response::Json(version_json())),
        Cmd::Help => Ok(Response::Json(help_json())),
        Cmd::Rename { slug, new } => cmd_rename(cli, slug, new),
        Cmd::Move { slug, to } => cmd_move(cli, slug, to),
    }
}

// ── the verbs ───────────────────────────────────────────────────────────────

fn cmd_add(
    cli: &Cli,
    tokens: &[String],
    create: bool,
    at: Option<&str>,
    refs: &[String],
    series: Option<&str>,
    note: Option<&str>,
) -> Result<Response> {
    refuse_under_rule(cli, "add")?;
    let ctx = Ctx::open(cli)?;
    let target = ctx.write_target(cli, series, tokens, create)?;

    // `add` fills a container, it never mints one — `-c` does that first (§7.3).
    let sref = match (target.existing.clone(), create) {
        (Some(_), true) => {
            return Err(Error::validation(format!(
                "series {:?} already exists at {} (§7.3)",
                target.name,
                target.home.as_str()
            )));
        }
        (Some(found), false) => found,
        (None, true) => ctx
            .store
            .create_series(&target.home, &target.kind, &target.name)?,
        (None, false) => return Err(missing(&target)),
    };

    // `ann ecv weight -c` mints the log empty (§7.3).
    if create && target.values.is_empty() && note.is_none() && refs.is_empty() {
        return Ok(Response::Json(json!({ "created": identity(&sref) })));
    }

    let key = contract::key_from_at(at)?;
    let record = LogReading {
        values: target.values.clone(),
        note: note.map(str::to_string),
    };
    Annales::validate(&record)?;
    let line = Line {
        key: key.clone(),
        refs: parse_refs(refs)?,
        data: record,
    };
    let after = line_json(&sref, &line)?;

    let existing = ctx.store.read_series(&sref)?;
    let previous = existing.iter().find(|l| l.key == key);

    // A fresh key runs free; landing on one that exists is an overwrite — a
    // mutation, shown and confirmed before it commits (§7.3, I1).
    match previous {
        Some(prev) => {
            let change = change(
                "add",
                &sref,
                &key,
                Some(line_json(&sref, prev)?),
                Some(after.clone()),
            );
            if let Some(pending) = review(cli, &change)? {
                return Ok(pending);
            }
        }
        // Every write verb takes `--dry-run` (§7.2), fresh or not.
        None if cli.dry_run => {
            let change = change("add", &sref, &key, None, Some(after));
            return Ok(Response::Json(change.to_json()));
        }
        None => {}
    }

    ctx.store.write_line(&sref, &line)?;
    Ok(Response::Json(after))
}

fn cmd_edit(
    cli: &Cli,
    key: &str,
    values: &[String],
    refs: &[String],
    series: Option<&str>,
    note: Option<&str>,
) -> Result<Response> {
    refuse_under_rule(cli, "edit")?;
    let ctx = Ctx::open(cli)?;
    let target = ctx.write_target(cli, series, &[], false)?;
    let sref = target.existing.clone().ok_or_else(|| missing(&target))?;
    let key = Key::parse(key)?;

    let lines = ctx.store.read_series(&sref)?;
    let prev = lines
        .iter()
        .find(|l| l.key == key)
        .ok_or_else(|| no_line(&sref, &key))?;

    // An `edit` given no new value is the editor form (§7.3).
    if values.is_empty() && note.is_none() && refs.is_empty() {
        return editor_form(&ctx, &sref, prev, &key);
    }

    // What a hand did not give, the reading keeps (I1: the stored record is truth).
    let record = LogReading {
        values: if values.is_empty() {
            prev.data.values.clone()
        } else {
            values.to_vec()
        },
        note: note.map(str::to_string).or_else(|| prev.data.note.clone()),
    };
    Annales::validate(&record)?;
    let line = Line {
        // The date key is the reading's own identity and never changes on an edit (§5.4).
        key: key.clone(),
        refs: if refs.is_empty() {
            prev.refs.clone()
        } else {
            parse_refs(refs)?
        },
        data: record,
    };

    let after = line_json(&sref, &line)?;
    let change = change(
        "edit",
        &sref,
        &key,
        Some(line_json(&sref, prev)?),
        Some(after.clone()),
    );
    if let Some(pending) = review(cli, &change)? {
        return Ok(pending);
    }
    ctx.store.write_line(&sref, &line)?;
    Ok(Response::Json(after))
}

fn cmd_rm(cli: &Cli, key: &str, series: Option<&str>) -> Result<Response> {
    refuse_under_rule(cli, "rm")?;
    let ctx = Ctx::open(cli)?;
    let target = ctx.write_target(cli, series, &[], false)?;
    let sref = target.existing.clone().ok_or_else(|| missing(&target))?;
    let key = Key::parse(key)?;

    let lines = ctx.store.read_series(&sref)?;
    let prev = lines
        .iter()
        .find(|l| l.key == key)
        .ok_or_else(|| no_line(&sref, &key))?;

    let change = change("rm", &sref, &key, Some(line_json(&sref, prev)?), None);
    if let Some(pending) = review(cli, &change)? {
        return Ok(pending);
    }
    ctx.store.remove_line(&sref, &key)?;
    Ok(Response::Json(json!({ "deleted": key.as_str() })))
}

fn cmd_list(cli: &Cli) -> Result<Response> {
    let ctx = Ctx::open(cli)?;
    // The locus is $PWD (§7.3); outside the tree there is nothing to narrow by.
    let home = match cli.home.as_deref() {
        Some(code) => Some(Code::parse(code)?),
        None => contract::code_at_path(&ctx.root, None).ok(),
    };
    let folded = ctx.store.fold(home.as_ref(), Some(&ctx.kind))?;
    Ok(Response::Json(contract::fold_json(Annales::NAME, &folded)?))
}

fn cmd_get(cli: &Cli, tokens: &[String]) -> Result<Response> {
    let ctx = Ctx::open(cli)?;
    let target = ctx.read_target(cli, tokens)?;
    if target.existing.is_none() {
        return Err(missing(&target));
    }
    let present = ctx
        .store
        .get(&target.name, Some(&ctx.kind), Some(&target.home))?;
    Ok(Response::Json(contract::present_json(
        Annales::NAME,
        &present,
    )?))
}

fn cmd_series(
    cli: &Cli,
    tokens: &[String],
    from: Option<&str>,
    to: Option<&str>,
) -> Result<Response> {
    let ctx = Ctx::open(cli)?;
    let target = ctx.read_target(cli, tokens)?;
    let sref = target.existing.clone().ok_or_else(|| missing(&target))?;
    let mut lines = ctx.store.read_series(&sref)?;
    // A window is a filter on the collection, never a second verb (§7.2). A `--to`
    // date also admits that day's timed keys (`260703T1400` is within `--to 260703`).
    if let Some(from) = from {
        lines.retain(|l| l.key.as_str() >= from);
    }
    if let Some(to) = to {
        lines.retain(|l| l.key.as_str() <= to || l.key.as_str().starts_with(to));
    }
    Ok(Response::Json(contract::series_json(
        Annales::NAME,
        &sref,
        &lines,
    )?))
}

/// Rename a log and cascade every ref pointing at it (§7.2, §5.4).
///
/// A hand-named series is a ref target by its name (`annales:meetings`, §5.4), so
/// this is the same operation Album's `rename` is, over a different shape — which is
/// the point: the cascade is the spine's, and no core touches another core's records
/// to run it (I5).
fn cmd_rename(cli: &Cli, slug: &str, new: &str) -> Result<Response> {
    refuse_under_rule(cli, "rename")?;
    let ctx = Ctx::open(cli)?;
    let sref = ctx.store.locate(
        &pantheon::name::normalize_token(slug, "series name")?,
        Some(&ctx.kind),
        cli.home.as_deref().map(Code::parse).transpose()?.as_ref(),
    )?;
    let new = pantheon::name::normalize_token(new, "series name")?;

    let from = Ref::parse(&format!("{}:{}", Annales::NAME, sref.label()))?;
    let to = Ref::parse(&format!("{}:{new}", Annales::NAME))?;
    let own: Vec<&str> = Annales::kinds().iter().map(|(k, _)| *k).collect();
    let cascade = pantheon::plan_cascade(ctx.store.root(), &own, &from, &to)?;

    let change = rename_change("rename", &sref, &new, Some(cascade.to_json()));
    if let Some(pending) = review(cli, &change)? {
        return Ok(pending);
    }
    // The series' own file moves first, so a crash mid-cascade leaves refs dangling
    // on the *old* name — which `pan validate` reports naming the files still to fix
    // (§5.4, §10.1).
    let moved = ctx.store.relocate_series(&sref, &sref.home, &new)?;
    cascade.apply(ctx.store.root())?;
    Ok(Response::Json(json!({
        "renamed": { "from": sref.label(), "to": new },
        "cascade": cascade.to_json(),
        "record": identity(&moved),
    })))
}

/// Re-home a log (§7.2). A file `mv` between meta dirs, touching no refs — a ref
/// carries no path, so it survives a re-home untouched (§5.4).
fn cmd_move(cli: &Cli, slug: &str, to: &str) -> Result<Response> {
    refuse_under_rule(cli, "move")?;
    let ctx = Ctx::open(cli)?;
    let sref = ctx.store.locate(
        &pantheon::name::normalize_token(slug, "series name")?,
        Some(&ctx.kind),
        cli.home.as_deref().map(Code::parse).transpose()?.as_ref(),
    )?;
    let home = Code::parse(to)?;

    let mut change = rename_change("move", &sref, sref.label(), None);
    change.home = home.as_str().to_string();
    if let Some(pending) = review(cli, &change)? {
        return Ok(pending);
    }
    let moved = ctx.store.relocate_series(&sref, &home, sref.label())?;
    Ok(Response::Json(json!({
        "moved": { "from": sref.home.as_str(), "to": home.as_str() },
        "record": identity(&moved),
    })))
}

/// The change a structural verb reviews. Unlike a line write there is no record
/// body to show — a series' identity *is* what moves — so `before`/`after` carry
/// the identity rather than a reading (§7.3).
fn rename_change(
    verb: &'static str,
    sref: &SeriesRef,
    new: &str,
    cascade: Option<Value>,
) -> RecordChange {
    let mut after = identity(sref);
    after["series"] = Value::String(new.to_string());
    RecordChange {
        verb,
        core: Annales::NAME.to_string(),
        home: sref.home.as_str().to_string(),
        kind: sref.kind.clone(),
        series: sref.name.clone(),
        // A named series' key is its name — the thing a ref points at (§5.4).
        key: sref.label().to_string(),
        before: Some(identity(sref)),
        after: Some(after),
        cascade,
    }
}

fn cmd_where(cli: &Cli, tokens: &[String]) -> Result<Response> {
    let ctx = Ctx::open(cli)?;
    let target = ctx.read_target(cli, tokens)?;
    let sref = target.existing.clone().ok_or_else(|| missing(&target))?;
    let mut out = identity(&sref);
    let rel = sref
        .path
        .strip_prefix(&ctx.root)
        .unwrap_or(&sref.path)
        .to_string_lossy()
        .into_owned();
    out["path"] = Value::String(rel);
    Ok(Response::Json(out))
}

// ── shared plumbing ─────────────────────────────────────────────────────────

struct Ctx {
    root: PathBuf,
    store: Store<Annales>,
    kind: String,
}

impl Ctx {
    fn open(cli: &Cli) -> Result<Ctx> {
        let root = resolve_root(cli.root.as_deref())?;
        let store = Store::new(root.clone());
        let kind = match cli.kind.as_deref() {
            Some(raw) => {
                let kind = pantheon::name::normalize_token(raw, "kind")?;
                if !Store::<Annales>::owns_series_kind(&kind) {
                    return Err(Error::usage(format!(
                        "annales has no {kind:?} token; it declares `log` (§7.1)"
                    )));
                }
                kind
            }
            None => Store::<Annales>::sole_series_kind()?.to_string(),
        };
        Ok(Ctx { root, store, kind })
    }

    /// Resolve the series a write means: its trailing tokens are the reading (§7.3).
    fn write_target(
        &self,
        cli: &Cli,
        series: Option<&str>,
        tokens: &[String],
        create: bool,
    ) -> Result<SeriesTarget> {
        self.resolve(cli, series, tokens, create, true)
    }

    /// Resolve the series a read means: it has no values, so a lone token names it.
    fn read_target(&self, cli: &Cli, tokens: &[String]) -> Result<SeriesTarget> {
        self.resolve(cli, None, tokens, false, false)
    }

    fn resolve(
        &self,
        cli: &Cli,
        series: Option<&str>,
        tokens: &[String],
        create: bool,
        takes_values: bool,
    ) -> Result<SeriesTarget> {
        contract::resolve_series_target(
            &self.store,
            &contract::TargetQuery {
                kind: &self.kind,
                home: cli.home.as_deref(),
                series,
                positionals: tokens,
                create,
                takes_values,
                pwd: None,
            },
        )
    }
}

/// The editor form of `edit` (§7.3): an `edit` given no new value opens the value
/// itself, and the session *is* the review — it mints no plan token and needs no
/// `-y`, because the hand is already looking at the thing it is changing.
fn editor_form(
    ctx: &Ctx,
    sref: &SeriesRef,
    prev: &Line<LogReading>,
    key: &Key,
) -> Result<Response> {
    // Piped, it spawns nothing and prints the file's path, by the same law that
    // sends a table to a TTY and JSON down a pipe: the LLM hand gets a path to open
    // with its own tools rather than a blocked process it cannot drive (I8).
    if !contract::stdout_is_terminal() {
        return Ok(Response::Json(
            json!({ "path": sref.path.display().to_string() }),
        ));
    }

    // What opens follows the shape (§6.1): a series line opens a buffer holding only
    // its value — one reading value per line — never the raw JSONL, which is
    // machine-owned and is never handed to a hand raw (I6, §6.6). A reading value is
    // not a typed token, so it is kept verbatim rather than normalized (§5.1).
    let initial = format!("{}\n", prev.data.values.join("\n"));
    match contract::edit_text(&initial)? {
        // Text that comes back unchanged writes nothing (§7.3).
        contract::Edited::Unchanged => Ok(Response::Json(line_json(sref, prev)?)),
        contract::Edited::Changed(text) => {
            let values = text
                .lines()
                .map(str::trim_end)
                .filter(|line| !line.trim().is_empty())
                .map(str::to_string)
                .collect();
            let record = LogReading {
                values,
                note: prev.data.note.clone(),
            };
            // Text that comes back invalid exits 3 (§7.3).
            Annales::validate(&record)?;
            let line = Line {
                key: key.clone(),
                refs: prev.refs.clone(),
                data: record,
            };
            ctx.store.write_line(sref, &line)?;
            Ok(Response::Json(line_json(sref, &line)?))
        }
    }
}

/// A write verb is refused outright under `PANTHEON_RULE=1` (exit `6`, §9.3): the
/// one reactive writer is Auspex, and a rule may not borrow a hand's authority (I2).
fn refuse_under_rule(cli: &Cli, verb: &str) -> Result<()> {
    // `--dry-run` computes without writing, so a rule may still plan (§7.3).
    if contract::under_rule() && !cli.dry_run {
        return Err(contract::refused_under_rule(verb));
    }
    Ok(())
}

/// Run one mutation past the confirm rule (§7.3). `Some` is a pending response the
/// caller returns as-is; `None` means go ahead and write.
fn review(cli: &Cli, change: &RecordChange) -> Result<Option<Response>> {
    match contract::checkpoint(change, cli.dry_run, cli.yes, cli.plan.as_deref())? {
        Checkpoint::DryRun(value) => Ok(Some(Response::Json(value))),
        Checkpoint::ConfirmRequired(value) => Ok(Some(Response::JsonExit(value, 5))),
        Checkpoint::Apply => Ok(None),
    }
}

fn change(
    verb: &'static str,
    sref: &SeriesRef,
    key: &Key,
    before: Option<Value>,
    after: Option<Value>,
) -> RecordChange {
    RecordChange {
        verb,
        core: Annales::NAME.to_string(),
        home: sref.home.as_str().to_string(),
        kind: sref.kind.clone(),
        series: sref.name.clone(),
        key: key.to_string(),
        before,
        after,
        cascade: None,
    }
}

fn line_json(sref: &SeriesRef, line: &Line<LogReading>) -> Result<Value> {
    contract::line_json(
        Annales::NAME,
        &sref.home,
        &sref.kind,
        sref.name.as_deref(),
        line,
    )
}

fn identity(sref: &SeriesRef) -> Value {
    json!({
        "core": Annales::NAME,
        "home": sref.home.as_str(),
        "kind": sref.kind,
        "series": sref.label(),
    })
}

fn parse_refs(refs: &[String]) -> Result<Vec<Ref>> {
    refs.iter().map(|r| Ref::parse(r)).collect()
}

fn missing(target: &SeriesTarget) -> Error {
    Error::not_found(format!(
        "no {} series {:?} at {} — mint it with -c (§7.3)",
        Annales::NAME,
        target.name,
        target.home.as_str()
    ))
}

fn no_line(sref: &SeriesRef, key: &Key) -> Error {
    Error::not_found(format!(
        "no line keyed {key} in series {:?} at {} (§7.2)",
        sref.label(),
        sref.home.as_str()
    ))
}

fn version_json() -> Value {
    json!({
        "name": Annales::NAME,
        "short": "ann",
        "version": env!("CARGO_PKG_VERSION"),
        "format_version": 1,
    })
}

fn help_json() -> Value {
    json!({
        "name": Annales::NAME,
        "short": "ann",
        "about": "the fact tense: what happened, as dated readings in a named log (§8.6)",
        "verbs": VERBS,
        "version": env!("CARGO_PKG_VERSION"),
    })
}
