//! `pen` — Pensum's CLI (§7). stdout is JSON when piped, a table on a TTY (§7.3).
//!
//! The bin owns only what is Pensum's own: its positionals (`[home] <name> [text]`)
//! and the flags its primitive needs (`--done`, `--undone`, `-r`). Everything
//! downstream — reading the hand, confirming a mutation, *finding* a home, shaping a
//! record into the contract's JSON — is `pantheon::contract`, so every core produces
//! that JSON the same way (I4).
//!
//! **Two universal flags mean nothing here and are refused** (exit `2`, §7.3): `-c`,
//! because a determined-name series is minted by its determinant rather than by a
//! hand; and `-a`, because a task keys by its *name* and has no date to key by. The
//! second matters most — it is the flag a due date would arrive through, and a task
//! has none (placement is Fasti's tense, §8.4).
//!
//! Pensum is the first core to answer all twelve verbs: Album refuses `series`,
//! having no collection, and every one of Pensum's records lives in one.

// The bin shares the spine's conventional pedantic allows (see pantheon/src/lib.rs).
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::doc_markdown)]
#![allow(clippy::too_many_lines)]
#![allow(clippy::too_many_arguments)]

use std::ffi::OsString;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand, ValueEnum};
use serde_json::{Value, json};

use pantheon::envelope::{Key, Line, Ref};
use pantheon::validate::{Finding, FindingCode, Severity, findings_json};
use pantheon::{
    Checkpoint, Code, Core, Error, RecordChange, Response, Result, SeriesRef, Store, contract,
    resolve_root,
};
use pensum::{Pensum, Task};

// The screen rides the `tui` feature; drop it and the core is headless (§14).
#[cfg(feature = "tui")]
mod screen;

/// The twelve verbs (§7.3). A closed reserved set: a verb wins over a node code,
/// which is what makes `add` safe to leave implicit (the ambiguity rule, §7.3).
const VERBS: &[&str] = &[
    "add", "edit", "rename", "move", "mv", "rm", "list", "ls", "get", "series", "where", "schema",
    "help", "version",
];

/// What a headless build prints for a bare short: there is no screen to open, so it
/// says where the verbs are (§14, §7.3).
#[cfg(not(feature = "tui"))]
const BARE: &str = "pen — Pensum (actio · intention). Built without the `tui` feature; run `pen --help` for the verbs.\n";

const ABOUT: &str =
    "Pensum — the intention tense: a future doing, as named tasks in a node's register (§8.5).";

#[derive(Parser)]
#[command(name = "pen", version, about = ABOUT, disable_help_subcommand = true)]
struct Cli {
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
    /// Which of the core's tokens (§7.2). Pensum declares one: `task`.
    #[arg(short = 'k', long = "kind", global = true, value_name = "K")]
    kind: Option<String>,
    /// Taken and refused: Pensum's series is minted by its determinant (§7.3).
    #[arg(short = 'c', long = "create", global = true)]
    create: bool,
    /// Taken and refused: a task keys by its name, not by a date (§5.4).
    #[arg(short = 'a', long = "at", global = true, value_name = "WHEN")]
    at: Option<String>,
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
    /// File a task at a node, minting the node's register if this is its first (§8.5).
    ///
    /// Tokens are `[home] <name> [text]`: `pen acm reach_out_to_alex` ·
    /// `pen reach_out_to_alex "call re: the contract"` · `pen acm buy_milk "2%"`.
    Add {
        tokens: Vec<String>,
        /// Attach a reference — what the task is *about* (§5.4, §8.5).
        #[arg(short = 'r', long = "ref", value_name = "REF")]
        refs: Vec<String>,
        /// Mark it done, on a date; bare means today (§7.2).
        #[arg(long = "done", value_name = "YYMMDD", num_args = 0..=1, default_missing_value = "")]
        done: Option<String>,
    },
    /// Change a task in place, by its key (§7.2) — mark it done, or rewrite its note.
    ///
    /// Given no new value this is the editor form (§7.3): at a TTY the note opens in
    /// `$VISUAL`/`$EDITOR`/`vi`; piped, it prints `{"path":…}`.
    Edit {
        tokens: Vec<String>,
        #[arg(short = 'r', long = "ref", value_name = "REF")]
        refs: Vec<String>,
        #[arg(long = "done", value_name = "YYMMDD", num_args = 0..=1, default_missing_value = "")]
        done: Option<String>,
        /// Put it back to intended — the clearing form of `--done` (§7.2).
        #[arg(long = "undone", conflicts_with = "done")]
        undone: bool,
    },
    /// Rename a task and cascade every ref pointing at it (§7.2, §5.4).
    Rename { tokens: Vec<String> },
    /// Re-home a task to another node (§7.2).
    #[command(alias = "mv")]
    Move {
        tokens: Vec<String>,
        #[arg(long = "to", value_name = "CODE")]
        to: String,
    },
    /// Remove a task by its key — irreversible (§7.2, §18).
    Rm { tokens: Vec<String> },
    /// Every open task across the subtree (§7.2). `--all` includes the done ones.
    #[command(alias = "ls")]
    List {
        /// Include tasks already done.
        #[arg(long = "all")]
        all: bool,
    },
    /// One task, found by its key anywhere in the tree (§5.4, §7.2).
    Get { tokens: Vec<String> },
    /// One node's whole register, done and open alike, optionally windowed (§7.2).
    Series {
        tokens: Vec<String>,
        #[arg(long = "from", value_name = "KEY")]
        from: Option<String>,
        #[arg(long = "to", value_name = "KEY")]
        to: Option<String>,
    },
    /// Resolve a task to its home code, by walking Pensum's own files (§7.3).
    Where { tokens: Vec<String> },
    /// Self-description: name, tokens and shapes, record schema, format version (§7.2).
    Schema,
    /// This tool's name, short, and version, as JSON (§7.3).
    Version,
    /// The verbs, as JSON (§7.3).
    Help,
}

fn main() -> ExitCode {
    let cli = Cli::parse_from(with_default_verb(std::env::args_os()));
    let as_json = contract::format_is_json(cli.format.map(|f| matches!(f, Format::Json)));
    contract::dispatch(run(&cli, as_json), as_json)
}

/// The flags that take a separate value — what the verb scan must step over to find
/// the first *word* on the line. (`--flag=value` needs no entry: it is one token.)
///
/// `--done` is deliberately absent: it takes an *optional* value, so a scan cannot
/// know whether the next token is its date or the verb, and guessing wrong would
/// swallow a word. It is a per-verb flag rather than a global, so it cannot precede
/// the verb anyway.
const VALUE_FLAGS: &[&str] = &[
    "-C", "--root", "-f", "--format", "-p", "--plan", "-H", "--home", "-k", "--kind", "-a", "--at",
];

/// `add` is the default verb (§7.3): where the first word on the line is not one of
/// the reserved verbs, it begins an `add`. Verbs are a closed set and win over any
/// other reading of that token — the ambiguity rule (§7.3).
fn with_default_verb(raw: impl Iterator<Item = OsString>) -> Vec<OsString> {
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

fn run(cli: &Cli, as_json: bool) -> Result<Response> {
    let Some(cmd) = &cli.cmd else {
        // A bare short opens the TUI at a terminal; piped, it emits help (§7.3) — a
        // screen has nothing to draw down a pipe.
        if as_json {
            return Ok(Response::Json(help_json()));
        }
        #[cfg(feature = "tui")]
        {
            let root = resolve_root(cli.root.as_deref())?;
            screen::open(&root).map_err(|e| Error::runtime(e.to_string()))?;
            return Ok(Response::Raw(String::new()));
        }
        // Headless: there is no screen to open, so help is the whole answer (§14).
        #[cfg(not(feature = "tui"))]
        return Ok(Response::Raw(BARE.to_string()));
    };

    // Taken by clap and refused here, so the refusal wears the contract's error
    // envelope rather than clap's own message (I4, §7.3).
    if cli.create {
        return Err(Error::usage(
            "pensum's register is nameless and minted by the node's first task, so -c \
             mints nothing (§7.1, §7.3)",
        ));
    }
    if cli.at.is_some() {
        return Err(Error::usage(
            "a task keys by its name, not by a date, so -a keys nothing — a deadline is \
             a Fasti event referencing the task (§5.4, §8.4)",
        ));
    }

    match cmd {
        Cmd::Add { tokens, refs, done } => cmd_add(cli, tokens, refs, done.as_deref()),
        Cmd::Edit {
            tokens,
            refs,
            done,
            undone,
        } => cmd_edit(cli, tokens, refs, done.as_deref(), *undone),
        Cmd::Rename { tokens } => cmd_rename(cli, tokens),
        Cmd::Move { tokens, to } => cmd_move(cli, tokens, to),
        Cmd::Rm { tokens } => cmd_rm(cli, tokens),
        Cmd::List { all } => cmd_list(cli, *all),
        Cmd::Get { tokens } => cmd_get(cli, tokens),
        Cmd::Series { tokens, from, to } => cmd_series(cli, tokens, from.as_deref(), to.as_deref()),
        Cmd::Where { tokens } => cmd_where(cli, tokens),
        Cmd::Schema => Ok(Response::Json(serde_json::to_value(pantheon::schema::<
            Pensum,
        >(1))?)),
        Cmd::Version => Ok(Response::Json(version_json())),
        Cmd::Help => Ok(Response::Json(help_json())),
    }
}

// ── the verbs ───────────────────────────────────────────────────────────────

/// File a task (§8.5). Unlike every other `add` in the workspace this one may bring
/// its container with it — a nameless register has nothing to mistype, so it is
/// minted by its determinant, the node's first task, rather than by `-c` (§7.3, §18).
fn cmd_add(cli: &Cli, tokens: &[String], refs: &[String], done: Option<&str>) -> Result<Response> {
    refuse_under_rule(cli, "add")?;
    let ctx = Ctx::open(cli)?;
    let target = contract::resolve_register_target(
        &ctx.store,
        &contract::RegisterQuery {
            kind: &ctx.kind,
            home: cli.home.as_deref(),
            positionals: tokens,
            pwd: None,
        },
    )?;

    // The node's register, whether or not it exists yet: `write_line` mints it.
    let sref = target.existing.clone().unwrap_or_else(|| SeriesRef {
        home: target.home.clone(),
        kind: target.kind.clone(),
        name: None,
        path: ctx.path_at(&target.home),
    });

    let record = Task {
        done: done_value(done)?,
        note: note_from(&target.values),
    };
    Pensum::validate(&record)?;
    let line = Line {
        key: target.key.clone(),
        refs: parse_refs(refs)?,
        data: record,
    };
    let after = line_json(&sref, &line)?;

    let existing = if sref.path.exists() {
        ctx.store.read_series(&sref)?
    } else {
        Vec::new()
    };
    let previous = existing.iter().find(|l| l.key == target.key);

    // A fresh key runs free; landing on one that exists is an overwrite — a
    // mutation, shown and confirmed before it commits (§7.3, I1).
    match previous {
        Some(prev) => {
            let change = change(
                "add",
                &sref,
                &target.key,
                Some(line_json(&sref, prev)?),
                Some(after.clone()),
            );
            if let Some(pending) = review(cli, &change)? {
                return Ok(pending);
            }
        }
        // Every write verb takes `--dry-run` (§7.2), fresh or not. Nothing is minted
        // by a dry run either — a plan that left a file behind would not be one.
        None if cli.dry_run => {
            let change = change("add", &sref, &target.key, None, Some(after));
            return Ok(Response::Json(change.to_json()));
        }
        None => {}
    }

    ctx.store.write_line(&sref, &line)?;
    warn_duplicates(&ctx, &sref, &target.key)?;
    Ok(Response::Json(after))
}

/// Change a task in place (§7.2). A correction rewrites the keyed line; it never
/// stacks a second (I1).
fn cmd_edit(
    cli: &Cli,
    tokens: &[String],
    refs: &[String],
    done: Option<&str>,
    undone: bool,
) -> Result<Response> {
    refuse_under_rule(cli, "edit")?;
    let ctx = Ctx::open(cli)?;
    let (scope, key, values) = ctx.keyed(cli, tokens)?;
    let (sref, prev) = ctx
        .store
        .locate_line(&key, Some(&ctx.kind), scope.as_ref())?;

    // An `edit` given no new value is the editor form (§7.3).
    if values.is_empty() && done.is_none() && !undone && refs.is_empty() {
        return editor_form(&ctx, &sref, &prev, &key);
    }

    // What a hand did not give, the task keeps (I1: the stored record is truth).
    let record = Task {
        done: if undone {
            None
        } else {
            done_value(done)?.or_else(|| prev.data.done.clone())
        },
        note: note_from(&values).or_else(|| prev.data.note.clone()),
    };
    Pensum::validate(&record)?;
    let line = Line {
        // A rename is its own verb, so an edit never moves the key (§5.4, §7.2).
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
        Some(line_json(&sref, &prev)?),
        Some(after.clone()),
    );
    if let Some(pending) = review(cli, &change)? {
        return Ok(pending);
    }
    ctx.store.write_line(&sref, &line)?;
    Ok(Response::Json(after))
}

/// Rename a task and cascade every ref pointing at it (§7.2, §5.4).
///
/// The same operation Album's and Annales' `rename` are, over a third shape. Where
/// those move a *file*, a task's identity is a line's key, so the record's own move
/// is a rewrite inside the file — but the ordering is identical, and for the identical
/// reason (§5.4, §10.1).
fn cmd_rename(cli: &Cli, tokens: &[String]) -> Result<Response> {
    refuse_under_rule(cli, "rename")?;
    let ctx = Ctx::open(cli)?;
    let (scope, key, rest) = ctx.keyed(cli, tokens)?;
    let [new] = rest.as_slice() else {
        return Err(Error::usage(
            "rename takes the task and its new name: `pen rename <key> <new>` (§7.2)",
        ));
    };
    let new = Key::parse(new)?;
    let (sref, _) = ctx
        .store
        .locate_line(&key, Some(&ctx.kind), scope.as_ref())?;

    let from = Ref::parse(&format!("{}:{key}", Pensum::NAME))?;
    let to = Ref::parse(&format!("{}:{new}", Pensum::NAME))?;
    let own: Vec<&str> = Pensum::kinds().iter().map(|(k, _)| *k).collect();
    let cascade = pantheon::plan_cascade(ctx.store.root(), &own, &from, &to)?;

    let change = rename_change("rename", &sref, &key, &new, Some(cascade.to_json()));
    if let Some(pending) = review(cli, &change)? {
        return Ok(pending);
    }
    // The record's own key moves first, so a crash mid-cascade leaves refs dangling
    // on the *old* name — which `pan validate` reports naming the files still to fix
    // (§5.4, §10.1).
    ctx.store.rename_line(&sref, &key, &new)?;
    cascade.apply(ctx.store.root())?;
    Ok(Response::Json(json!({
        "renamed": { "from": key.as_str(), "to": new.as_str() },
        "cascade": cascade.to_json(),
        "record": identity(&sref, &new),
    })))
}

/// Re-home a task (§7.2). A line moved between two registers, touching no refs — a
/// ref carries no path, so it survives a re-home untouched (§5.4).
fn cmd_move(cli: &Cli, tokens: &[String], to: &str) -> Result<Response> {
    refuse_under_rule(cli, "move")?;
    let ctx = Ctx::open(cli)?;
    let (scope, key, _) = ctx.keyed(cli, tokens)?;
    let (sref, _) = ctx
        .store
        .locate_line(&key, Some(&ctx.kind), scope.as_ref())?;
    let home = Code::parse(to)?;

    let mut change = rename_change("move", &sref, &key, &key, None);
    change.home = home.as_str().to_string();
    if let Some(pending) = review(cli, &change)? {
        return Ok(pending);
    }
    let moved = ctx.store.move_line(&sref, &home, &key)?;
    Ok(Response::Json(json!({
        "moved": { "from": sref.home.as_str(), "to": home.as_str() },
        "record": identity(&moved, &key),
    })))
}

fn cmd_rm(cli: &Cli, tokens: &[String]) -> Result<Response> {
    refuse_under_rule(cli, "rm")?;
    let ctx = Ctx::open(cli)?;
    let (scope, key, _) = ctx.keyed(cli, tokens)?;
    let (sref, prev) = ctx
        .store
        .locate_line(&key, Some(&ctx.kind), scope.as_ref())?;

    let change = change("rm", &sref, &key, Some(line_json(&sref, &prev)?), None);
    if let Some(pending) = review(cli, &change)? {
        return Ok(pending);
    }
    ctx.store.remove_line(&sref, &key)?;
    Ok(Response::Json(json!({ "deleted": key.as_str() })))
}

/// The fold across the subtree (§7.2). Every task is its own present — a name-keyed
/// line is a record, not a sample (I1, §5.4) — so this is every one of them, minus
/// the done unless `--all` asks for them: a task list is what is not yet done.
fn cmd_list(cli: &Cli, all: bool) -> Result<Response> {
    let ctx = Ctx::open(cli)?;
    // The locus is $PWD (§7.3); outside the tree there is nothing to narrow by.
    let home = match cli.home.as_deref() {
        Some(code) => Some(Code::parse(code)?),
        None => contract::code_at_path(&ctx.root, None).ok(),
    };
    let mut folded = ctx.store.fold(home.as_ref(), Some(&ctx.kind))?;
    if !all {
        folded.retain(|present| present.line.data.done.is_none());
    }
    Ok(Response::Json(contract::fold_json(Pensum::NAME, &folded)?))
}

fn cmd_get(cli: &Cli, tokens: &[String]) -> Result<Response> {
    let ctx = Ctx::open(cli)?;
    let (scope, key, _) = ctx.keyed(cli, tokens)?;
    let (sref, line) = ctx
        .store
        .locate_line(&key, Some(&ctx.kind), scope.as_ref())?;
    Ok(Response::Json(line_json(&sref, &line)?))
}

/// One node's whole register (§7.2).
///
/// Where `list` spans a *subtree* and folds, this reads one node's file whole — the
/// two differ on the subtree axis, not the fold axis, which for a core with one file
/// per node is easy to miss. `series` shows the done tasks too: a collection read
/// whole is read whole.
fn cmd_series(
    cli: &Cli,
    tokens: &[String],
    from: Option<&str>,
    to: Option<&str>,
) -> Result<Response> {
    let ctx = Ctx::open(cli)?;
    let home = match (cli.home.as_deref(), tokens.len()) {
        (Some(code), 0) => Code::parse(code)?,
        (None, 0) => contract::code_at_path(&ctx.root, None)?,
        (None, 1) => Code::parse(&tokens[0])?,
        _ => {
            return Err(Error::usage(
                "`pen series` takes one node and nothing else: pensum's register is \
                 nameless, so there is no collection to name (§7.1, §7.3)",
            ));
        }
    };
    let path = ctx.path_at(&home);
    if !path.exists() {
        return Err(Error::not_found(format!(
            "no {} register at {} — a node's first task mints one (§7.3, §8.5)",
            Pensum::NAME,
            home.as_str()
        )));
    }
    let sref = SeriesRef {
        home,
        kind: ctx.kind.clone(),
        name: None,
        path,
    };
    let mut lines = ctx.store.read_series(&sref)?;
    // A window is a filter on the collection, never a second verb (§7.2).
    if let Some(from) = from {
        lines.retain(|l| l.key.as_str() >= from);
    }
    if let Some(to) = to {
        lines.retain(|l| l.key.as_str() <= to || l.key.as_str().starts_with(to));
    }
    Ok(Response::Json(contract::series_json(
        Pensum::NAME,
        &sref,
        &lines,
    )?))
}

fn cmd_where(cli: &Cli, tokens: &[String]) -> Result<Response> {
    let ctx = Ctx::open(cli)?;
    let (scope, key, _) = ctx.keyed(cli, tokens)?;
    let (sref, _) = ctx
        .store
        .locate_line(&key, Some(&ctx.kind), scope.as_ref())?;
    let mut out = identity(&sref, &key);
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
    store: Store<Pensum>,
    kind: String,
}

impl Ctx {
    fn open(cli: &Cli) -> Result<Ctx> {
        let root = resolve_root(cli.root.as_deref())?;
        let store = Store::new(root.clone());
        let kind = match cli.kind.as_deref() {
            Some(raw) => {
                let kind = pantheon::name::normalize_token(raw, "kind")?;
                if !Store::<Pensum>::owns_series_kind(&kind) {
                    return Err(Error::usage(format!(
                        "pensum has no {kind:?} token; it declares `task` (§7.1)"
                    )));
                }
                kind
            }
            None => Store::<Pensum>::sole_series_kind()?.to_string(),
        };
        Ok(Ctx { root, store, kind })
    }

    /// Where a node's register sits, whether or not it exists (§5.2).
    fn path_at(&self, home: &Code) -> PathBuf {
        self.store
            .series_path(home, &self.kind, None)
            .unwrap_or_default()
    }

    /// Read `[home] <key> [rest…]` for a verb that reaches an existing task.
    ///
    /// The home it returns is a **scope**, not a locus: a key is unique per *core*,
    /// not per node (§5.4), so with no `-H` the lookup is tree-wide. Narrowing to
    /// `$PWD` would make `pen edit reach_out_to_alex --done` mean different tasks in
    /// different directories, and would break the relay a lens makes with no home at
    /// all (§12).
    fn keyed(&self, cli: &Cli, tokens: &[String]) -> Result<(Option<Code>, Key, Vec<String>)> {
        let (home, rest) = contract::peel_home(&self.store, cli.home.as_deref(), tokens)?;
        let Some((first, tail)) = rest.split_first() else {
            return Err(Error::usage(format!(
                "name the {} record (§7.3)",
                Pensum::NAME
            )));
        };
        Ok((home, Key::parse(first)?, tail.to_vec()))
    }
}

/// The editor form of `edit` (§7.3): an `edit` given no new value opens the value
/// itself, and the session *is* the review — it mints no plan token and needs no
/// `-y`, because the hand is already looking at the thing it is changing.
fn editor_form(ctx: &Ctx, sref: &SeriesRef, prev: &Line<Task>, key: &Key) -> Result<Response> {
    // Piped, it spawns nothing and prints the file's path, by the same law that
    // sends a table to a TTY and JSON down a pipe: the LLM hand gets a path to open
    // with its own tools rather than a blocked process it cannot drive (I8).
    if !contract::stdout_is_terminal() {
        return Ok(Response::Json(
            json!({ "path": sref.path.display().to_string() }),
        ));
    }

    // What opens follows the shape (§6.1): a series line opens a buffer holding only
    // its value — here the note — never the raw JSONL, which is machine-owned and is
    // never handed to a hand raw (I6, §6.6). The task's *name* is not in the buffer:
    // renaming is its own verb, because it cascades (§7.2).
    let initial = format!("{}\n", prev.data.note.clone().unwrap_or_default());
    match contract::edit_text(&initial)? {
        // Text that comes back unchanged writes nothing (§7.3).
        contract::Edited::Unchanged => Ok(Response::Json(line_json(sref, prev)?)),
        contract::Edited::Changed(text) => {
            let record = Task {
                done: prev.data.done.clone(),
                note: note_from(&[text]),
            };
            // Text that comes back invalid exits 3 (§7.3).
            Pensum::validate(&record)?;
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

/// A key held at another node is a *soft* finding: the check is a tree walk, which
/// is the cost the softness exists to avoid (§5.4, §18). The record still goes to
/// stdout; the warning rides stderr in the same shape `pan validate` emits, so a
/// machine hand reads one shape from both surfaces (I4, I8).
fn warn_duplicates(ctx: &Ctx, written: &SeriesRef, key: &Key) -> Result<()> {
    let elsewhere = ctx
        .store
        .duplicate_keys_elsewhere(&written.home, key, Some(&ctx.kind))?;
    if elsewhere.is_empty() {
        return Ok(());
    }
    let findings: Vec<Finding> = elsewhere
        .iter()
        .map(|other| Finding {
            code: FindingCode::DuplicateSlug,
            severity: Severity::Warning,
            rel_path: other
                .path
                .strip_prefix(&ctx.root)
                .unwrap_or(&other.path)
                .to_path_buf(),
            msg: format!(
                "{}:{key} also names a record at {} — a ref meeting both lists them rather \
                 than guessing; a fuller name tells them apart (§5.4, §7.3)",
                Pensum::NAME,
                other.home.as_str()
            ),
        })
        .collect();
    eprintln!("{}", findings_json(&findings));
    Ok(())
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

/// `--done` with no value is today; with one, the date it was done. There is no
/// clock in the value a hand gave, so a bare flag is the only place one enters.
fn done_value(done: Option<&str>) -> Result<Option<String>> {
    match done {
        None => Ok(None),
        Some("") => Ok(Some(contract::key_from_at(None)?.to_string())),
        Some(given) => Ok(Some(contract::key_from_at(Some(given))?.to_string())),
    }
}

/// The trailing positionals as a task's note (§7.3). Several tokens are one note, so
/// `pen acm buy_milk get the 2% one` needs no quotes to say what it means — unlike a
/// *name*, where a quiet join would file the wrong record forever (§5.1).
fn note_from(values: &[String]) -> Option<String> {
    let joined = values.join(" ");
    (!joined.trim().is_empty()).then_some(joined)
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
        core: Pensum::NAME.to_string(),
        home: sref.home.as_str().to_string(),
        kind: sref.kind.clone(),
        // Absent, not null: there is no name to report where a core's series is
        // determined (§9.3, §7.1).
        series: None,
        key: key.to_string(),
        before,
        after,
        cascade: None,
    }
}

/// The change a structural verb reviews. Unlike a line write there is no record body
/// to show — a task's identity *is* what moves — so `before`/`after` carry the
/// identity rather than the task (§7.3).
fn rename_change(
    verb: &'static str,
    sref: &SeriesRef,
    key: &Key,
    new: &Key,
    cascade: Option<Value>,
) -> RecordChange {
    RecordChange {
        verb,
        core: Pensum::NAME.to_string(),
        home: sref.home.as_str().to_string(),
        kind: sref.kind.clone(),
        series: None,
        key: key.to_string(),
        before: Some(identity(sref, key)),
        after: Some(identity(sref, new)),
        cascade,
    }
}

fn line_json(sref: &SeriesRef, line: &Line<Task>) -> Result<Value> {
    contract::line_json(Pensum::NAME, &sref.home, &sref.kind, None, line)
}

fn identity(sref: &SeriesRef, key: &Key) -> Value {
    json!({
        "core": Pensum::NAME,
        "home": sref.home.as_str(),
        "kind": sref.kind,
        "key": key.as_str(),
    })
}

fn parse_refs(refs: &[String]) -> Result<Vec<Ref>> {
    refs.iter().map(|r| Ref::parse(r)).collect()
}

fn version_json() -> Value {
    json!({
        "name": Pensum::NAME,
        "short": "pen",
        "version": env!("CARGO_PKG_VERSION"),
        "format_version": 1,
    })
}

fn help_json() -> Value {
    json!({
        "name": Pensum::NAME,
        "short": "pen",
        "about": "the intention tense: a future doing, as named tasks in a node's register (§8.5)",
        "verbs": VERBS,
        "version": env!("CARGO_PKG_VERSION"),
        "format_version": 1,
    })
}
