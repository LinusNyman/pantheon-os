//! `rat` — Rationes' CLI (§7). stdout is JSON when piped, a table on a TTY (§7.3).
//!
//! The bin owns only what is Rationes' own: its positionals (`[home] <holding>
//! [amount]`) and the flags its primitive needs (`--currency`, `--expires`,
//! `--note`, `-r`). Everything downstream — reading the hand, confirming a mutation,
//! *finding* a home and a slug, planning the rename cascade, shaping a record into
//! the contract's JSON — is `pantheon`, so every core produces that JSON the same way
//! (I4) and no core reaches into another's records (I5).
//!
//! **Rationes is the hybrid core**: a partitioned register (Album's shape) whose
//! `account` and `asset` holdings each may carry a determined-name balance series
//! (Pensum's write path, over a name slot rather than a bare token). Two things
//! follow, and they are the two things worth knowing before reading further:
//!
//! 1. **The amount is the fork.** `rat crp checking` files a holding; `rat crp
//!    checking 4200` writes a balance reading on it. Arity decides, never content —
//!    a second positional *is* the figure, and one that does not parse as a number is
//!    a usage error rather than a holding quietly named `42oo`.
//! 2. **`rat` proves the determinant before it writes a line.** `Store::write_line`
//!    mints a `Series { named: false }` on first write, because a determined series
//!    is minted by its determinant (§7.3) — and for Pensum the determinant is the
//!    *node*, which the store can see. Rationes' determinant is a holding **entity**,
//!    which the store cannot see: it links no core (I5). So every balance write here
//!    goes through [`holding_for_balance`], which resolves the holding in Rationes'
//!    own bin first and returns **not found (exit `4`)** when there is none. Without
//!    it a typo would mint `crp__balance__nonexistent.jsonl` beside no holding at
//!    all — a stranded series, and a `pan validate` finding nobody asked for (§10.2).

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

use crate::{Balance, Holding, Rationes, Record};

use pantheon::envelope::{Entity, Key, Line, Ref};
use pantheon::validate::{Finding, FindingCode, Severity, findings_json};
use pantheon::{
    Checkpoint, Code, Core, EntityAddr, EntityForm, EntityRef, Error, PresentLine, RecordChange,
    Response, Result, SeriesRef, Store, contract, resolve_root,
};

/// The twelve verbs (§7.3). A closed reserved set: a verb wins over a node code,
/// which is what makes `add` safe to leave implicit (the ambiguity rule, §7.3).
const VERBS: &[&str] = &[
    "add", "edit", "rename", "move", "mv", "rm", "list", "ls", "get", "series", "where", "schema",
    "help", "version",
];

/// What a headless build prints for a bare short (§14, §7.3).
#[cfg(not(feature = "tui"))]
const BARE: &str = "rat — Rationes (res · what). Built without the `tui` feature; run `rat --help` for the verbs.\n";

const ABOUT: &str = "Rationes — res holdings: the accounts, goods, and rights you hold (§8.3).";

#[derive(Parser)]
#[command(name = "rat", version, about = ABOUT, disable_help_subcommand = true)]
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
    /// Which of the core's tokens (§7.2): `account`, `asset`, `claim`, `balance`.
    /// On a write it selects; on a read it filters.
    #[arg(short = 'k', long = "kind", global = true, value_name = "K")]
    kind: Option<String>,
    /// Taken and refused: a balance series is minted by its determinant (§7.3).
    #[arg(short = 'c', long = "create", global = true)]
    create: bool,
    /// The reading's date — a balance keys by the day it was read (§7.3).
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

/// Rationes' own fields, shared by `add` and `edit` (§8.3).
///
/// Each takes an **optional** value: given bare, it names the field the editor form
/// should open (§7.3).
// `Option<Option<String>>` is deliberate, and is the case the lint itself carves out:
// three states genuinely differ here. Absent leaves the record alone (I1), `--note`
// bare is the editor form (§7.3), and `--note TEXT` replaces.
#[allow(clippy::option_option)]
#[derive(clap::Args, Default)]
struct Fields {
    /// The unit its balance is read in — `usd`, `shares` (§8.3).
    #[arg(long = "currency", value_name = "C", num_args = 0..=1)]
    currency: Option<Option<String>>,
    /// The day the holding lapses, `YYMMDD` — a claim's expiry (§8.3).
    #[arg(long = "expires", value_name = "YYMMDD", num_args = 0..=1)]
    expires: Option<Option<String>>,
    /// A hand's remark — on the holding, or on the reading being written.
    #[arg(long = "note", value_name = "TEXT", num_args = 0..=1)]
    note: Option<Option<String>>,
}

#[derive(Subcommand)]
enum Cmd {
    /// File a holding, or write a balance reading on one (§8.3).
    ///
    /// Tokens are `[home] <holding> [amount]`. Without an amount this files the
    /// holding itself — `rat crp checking -k account`; with one it writes a reading
    /// on a holding that must already exist — `rat crp checking 4200 -a 260718`.
    /// A partitioned entity needs no prior container, and a determined-name series is
    /// minted by its determinant, so **neither form takes `-c`** (§7.3, §18).
    Add {
        tokens: Vec<String>,
        #[command(flatten)]
        fields: Fields,
        /// Attach a reference; repeatable (§5.4). The org an account sits with is a
        /// ref (`-r album:some_bank`), never a home (I3, I9, §8.3).
        #[arg(short = 'r', long = "ref", value_name = "REF")]
        refs: Vec<String>,
    },
    /// Correct a holding in place, by slug (§7.2). What a hand does not give, the
    /// record keeps (I1).
    ///
    /// `-k` changes what the holding fundamentally *is*, which renames the file — a
    /// visible structural act, not a silent field flip (§7.2). A field flag given
    /// bare is the editor form (§7.3): at a TTY that field's value opens in
    /// `$VISUAL`/`$EDITOR`/`vi`; piped, it prints `{"path":…}`.
    Edit {
        slug: String,
        #[command(flatten)]
        fields: Fields,
        #[arg(short = 'r', long = "ref", value_name = "REF")]
        refs: Vec<String>,
    },
    /// Rename a holding and cascade every ref pointing at it (§7.2, §5.4).
    ///
    /// Its balance series follows: the series' name *is* the holding's slug, so a
    /// rename that left it behind would strand it (§7.2, §8.3, §10.2).
    Rename { slug: String, new: String },
    /// Re-home a holding to another node (§7.2), carrying its balance series in the
    /// same planned transaction — that series exists only because the holding does.
    #[command(alias = "mv")]
    Move {
        slug: String,
        #[arg(long = "to", value_name = "CODE")]
        to: String,
    },
    /// Remove a holding and its balance series, or one reading with `-a` (§7.2, §18).
    Rm { slug: String },
    /// Every holding across the subtree, each with its **derived** latest balance
    /// (§7.2, I1). `-k` filters; `--net` folds them to net worth instead.
    #[command(alias = "ls")]
    List {
        /// Net worth: the latest balances summed by currency, never stored (§8.3, I1).
        #[arg(long = "net")]
        net: bool,
    },
    /// One holding by slug, with its derived latest balance (§7.2, I1).
    Get { slug: String },
    /// A holding's balance trend — the whole collection, optionally windowed (§7.2).
    Series {
        tokens: Vec<String>,
        #[arg(long = "from", value_name = "KEY")]
        from: Option<String>,
        #[arg(long = "to", value_name = "KEY")]
        to: Option<String>,
    },
    /// Resolve a slug to its home code, by walking Rationes' own files (§7.3).
    Where { slug: String },
    /// Self-description: name, tokens and shapes, record schema, format version (§7.2).
    Schema,
    /// This tool's name, short, and version, as JSON (§7.3).
    Version,
    /// The verbs, as JSON (§7.3).
    Help,
}

/// Run `rat` exactly as the binary runs it (§7.3) — parse `argv`, dispatch, and
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
///
/// Rationes' own field flags are absent deliberately: each takes an *optional* value,
/// so a scan cannot know whether the next token is its value or the verb. They are
/// per-verb rather than global, so none of them may precede the verb anyway.
const VALUE_FLAGS: &[&str] = &[
    "-C", "--root", "-f", "--format", "-p", "--plan", "-H", "--home", "-k", "--kind", "-a", "--at",
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

    // Taken by clap and refused here, so the refusal wears the contract's error
    // envelope rather than clap's own message (I4, §7.3).
    if cli.create {
        return Err(Error::usage(
            "rationes' balance series is determined by its holding and minted by the \
             holding's own creation, so -c mints nothing — a name that is a slug has \
             nothing to mistype (§7.1, §7.3, §8.3)",
        ));
    }

    match cmd {
        Cmd::Add {
            tokens,
            fields,
            refs,
        } => cmd_add(cli, tokens, fields, refs),
        Cmd::Edit { slug, fields, refs } => cmd_edit(cli, slug, fields, refs),
        Cmd::Rename { slug, new } => cmd_rename(cli, slug, new),
        Cmd::Move { slug, to } => cmd_move(cli, slug, to),
        Cmd::Rm { slug } => cmd_rm(cli, slug),
        Cmd::List { net } => cmd_list(cli, *net),
        Cmd::Get { slug } => cmd_get(cli, slug),
        Cmd::Series { tokens, from, to } => cmd_series(cli, tokens, from.as_deref(), to.as_deref()),
        Cmd::Where { slug } => cmd_where(cli, slug),
        Cmd::Schema => Ok(Response::Json(serde_json::to_value(pantheon::schema::<
            Rationes,
        >(1))?)),
        Cmd::Version => Ok(Response::Json(version_json())),
        Cmd::Help => Ok(Response::Json(help_json())),
    }
}

// ── the verbs ───────────────────────────────────────────────────────────────

/// File a holding, or write a balance reading on one (§8.3).
///
/// **The amount is the fork.** One token past the home names a holding, two name a
/// holding and its reading. Arity decides and content never does: a second positional
/// that does not parse as a figure is a usage error, not a two-word holding name —
/// the same discipline §7.3 keeps for a name, where a quiet join would file the wrong
/// record forever.
fn cmd_add(cli: &Cli, tokens: &[String], fields: &Fields, refs: &[String]) -> Result<Response> {
    refuse_under_rule(cli, "add")?;
    let ctx = Ctx::open(cli)?;
    // Peeled only to count: the entity form hands the *original* tokens back to the
    // spine's own resolver, which does the peel again with the $PWD locus behind it.
    let (home, rest) = contract::peel_home(&ctx.store, cli.home.as_deref(), tokens)?;
    match rest {
        [] => Err(Error::usage(format!(
            "name the {} record (§7.3)",
            Rationes::NAME
        ))),
        [_] if cli.at.is_some() => Err(Error::usage(
            "-a dates a balance reading, and none was given: \
             `rat <home> <holding> <amount> -a <date>` (§7.3, §8.3)",
        )),
        [_] => add_holding(cli, &ctx, tokens, fields, refs),
        [slug, amount] => add_balance(cli, &ctx, home.as_ref(), slug, amount, fields, refs),
        [..] => Err(Error::usage(format!(
            "a holding is one token and its balance one figure, and {} were given \
             (§5.1, §7.3)",
            rest.len()
        ))),
    }
}

/// The entity half of `add` — the shape Album's `add` wears (§8.1).
fn add_holding(
    cli: &Cli,
    ctx: &Ctx,
    tokens: &[String],
    fields: &Fields,
    refs: &[String],
) -> Result<Response> {
    let kind = ctx.write_kind()?;
    let target = contract::resolve_entity_target(
        &ctx.store,
        &contract::EntityQuery {
            kind: &kind,
            home: cli.home.as_deref(),
            positionals: tokens,
            pwd: None,
        },
    )?;

    // Within a node the check is one `read_dir`, so it is hard: two kinds spell two
    // files but only one ref, which the filesystem permits and the ref namespace does
    // not (§5.4, §18).
    if let Some(held) = &target.existing
        && held.kind != kind
    {
        return Err(Error::validation(format!(
            "{} already holds {:?} as a {}: two kinds spell two files but only one \
             `rationes:{}`, so the ref would be ambiguous (§5.4, §18)",
            target.home.as_str(),
            target.slug,
            held.kind,
            target.slug
        )));
    }

    let holding = build_holding(fields, None);
    let record = Record::Holding(holding);
    Rationes::validate(&record)?;
    let entity = Entity {
        refs: parse_refs(refs)?,
        data: record,
    };
    let addr = EntityAddr {
        home: target.home.clone(),
        kind: kind.clone(),
        slug: target.slug.clone(),
    };
    let after = addr_json(&addr, &entity)?;

    // A fresh `add` runs free; landing on an existing slug is an overwrite — a
    // mutation, shown and confirmed before it commits (§7.3, I1).
    match &target.existing {
        Some(held) => {
            let before = ctx.store.read_entity(held)?;
            let change = entity_change(
                "add",
                &addr,
                Some(contract::entity_json(Rationes::NAME, held, &before)?),
                Some(after.clone()),
                None,
            );
            if let Some(pending) = review(cli, &change)? {
                return Ok(pending);
            }
        }
        // Every write verb takes `--dry-run` (§7.2), fresh or not.
        None if cli.dry_run => {
            let change = entity_change("add", &addr, None, Some(after), None);
            return Ok(Response::Json(change.to_json()));
        }
        None => {}
    }

    let written = ctx
        .store
        .write_entity(&addr, entity.refs.clone(), &entity.data)?;
    // Across nodes the check is a walk, so it stays soft: the record itself goes to
    // stdout, the warning to stderr (§5.4, §18).
    warn_duplicates(ctx, &written)?;
    Ok(Response::Json(contract::entity_json(
        Rationes::NAME,
        &written,
        &entity,
    )?))
}

/// The series half of `add` — a balance reading on an existing holding (§8.3).
///
/// This is where the determined-series trap is closed; see [`holding_for_balance`].
fn add_balance(
    cli: &Cli,
    ctx: &Ctx,
    scope: Option<&Code>,
    slug: &str,
    amount: &str,
    fields: &Fields,
    refs: &[String],
) -> Result<Response> {
    if fields.currency.is_some() || fields.expires.is_some() {
        return Err(Error::usage(
            "--currency and --expires describe the holding, not a reading of it — set \
             them with `rat edit <holding>` (§8.3, I1)",
        ));
    }
    let amount: f64 = amount.parse().map_err(|_| {
        Error::usage(format!(
            "a balance is one figure, and {amount:?} is not one — a holding takes its \
             name in one token and its reading in the next (§7.3, §8.3)"
        ))
    })?;

    let eref = holding_for_balance(ctx, slug, scope)?;
    let sref = balance_series(ctx, &eref)?;
    let key = contract::key_from_at(cli.at.as_deref())?;

    let balance = Balance {
        amount,
        note: given(fields.note.as_ref(), None),
    };
    let record = Record::Balance(balance);
    Rationes::validate(&record)?;
    let line = Line {
        key: key.clone(),
        refs: parse_refs(refs)?,
        data: record,
    };
    let after = line_json(&sref, &line)?;

    // The series file may not exist yet — the determinant's first reading mints it.
    let existing = if sref.path.exists() {
        ctx.store.read_series(&sref)?
    } else {
        Vec::new()
    };
    let previous = existing.iter().find(|l| l.key == key);

    // A fresh key runs free; landing on one that exists is an overwrite — a mutation,
    // shown and confirmed before it commits, and I1's correction path for a figure
    // read wrong (§7.3).
    match previous {
        Some(prev) => {
            let change = line_change(
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
        // Every write verb takes `--dry-run` (§7.2), fresh or not. Nothing is minted
        // by a dry run either — a plan that left a file behind would not be one.
        None if cli.dry_run => {
            let change = line_change("add", &sref, &key, None, Some(after));
            return Ok(Response::Json(change.to_json()));
        }
        None => {}
    }

    ctx.store.write_line(&sref, &line)?;
    Ok(Response::Json(after))
}

fn cmd_edit(cli: &Cli, slug: &str, fields: &Fields, refs: &[String]) -> Result<Response> {
    refuse_under_rule(cli, "edit")?;
    let ctx = Ctx::open(cli)?;
    if cli.at.is_some() {
        return Err(Error::usage(
            "-a names a balance reading, and a reading is corrected by writing its key \
             again: `rat <home> <holding> <amount> -a <date>` lands on the same key and \
             confirms the overwrite (I1, §6.1, §7.3)",
        ));
    }
    // `-k` on an edit names the holding's *new* kind, so the lookup must not filter by
    // it — you are correcting what it is, not restating what it was.
    let (eref, before) =
        ctx.store
            .get_entity(&normalize_slug(slug)?, None, ctx.scope().as_ref())?;
    before.data.as_holding()?;

    // A field flag given bare names the field the editor should open (§7.3).
    if let Some(field) = fields.bare_field()? {
        return editor_form(&ctx, &eref, &before, field);
    }

    let holding = build_holding(fields, Some(before.data.as_holding()?));
    let record = Record::Holding(holding);
    Rationes::validate(&record)?;
    let entity = Entity {
        refs: if refs.is_empty() {
            before.refs.clone()
        } else {
            parse_refs(refs)?
        },
        data: record,
    };
    // Changing what an entity *is* renames the file (§7.2).
    let kind = match &ctx.kind {
        Some(kind) => {
            if kind == Rationes::BALANCE {
                return Err(balance_is_not_an_entity_kind("edit"));
            }
            kind.clone()
        }
        None => eref.kind.clone(),
    };
    // A holding that already carries a balance may not become a kind that carries
    // none: the series would outlive the reason it exists and strand at the node
    // (§8.3, §10.2). The refusal is a *legality* check within Rationes' own
    // vocabulary, which §6.4 leaves to the owning core.
    if kind != eref.kind
        && !Rationes::carries_balance(&kind)
        && carried_series(&ctx, &eref)?.is_some()
    {
        return Err(Error::validation(format!(
            "{:?} carries a balance series, and a {kind} carries none — remove its \
             readings before changing what it is (§8.3, §10.2)",
            eref.slug
        )));
    }
    let addr = EntityAddr {
        home: eref.home.clone(),
        kind,
        slug: eref.slug.clone(),
    };
    let after = addr_json(&addr, &entity)?;
    let change = entity_change(
        "edit",
        &addr,
        Some(contract::entity_json(Rationes::NAME, &eref, &before)?),
        Some(after),
        None,
    );
    if let Some(pending) = review(cli, &change)? {
        return Ok(pending);
    }

    let eref = if addr.kind == eref.kind {
        eref
    } else {
        ctx.store.relocate_entity(&eref, &addr)?
    };
    ctx.store
        .write_entity(&addr, entity.refs.clone(), &entity.data)?;
    Ok(Response::Json(contract::entity_json(
        Rationes::NAME,
        &eref,
        &entity,
    )?))
}

/// Rename a holding, cascade every ref pointing at it, and carry its balance series
/// along (§7.2, §5.4, §8.3).
fn cmd_rename(cli: &Cli, slug: &str, new: &str) -> Result<Response> {
    refuse_under_rule(cli, "rename")?;
    let ctx = Ctx::open(cli)?;
    let (eref, entity) = ctx.store.get_entity(
        &normalize_slug(slug)?,
        ctx.filter_kind()?,
        ctx.scope().as_ref(),
    )?;
    refuse_entity_as_node(&eref, "rename")?;
    let new = pantheon::name::normalize_token(new, "name")?;

    // The walk that finds the refs is the walk that finds an occupied slug (§5.4).
    // `balance` rides in `own_kinds` with the three entity tokens, so the same walk
    // also refuses a rename onto a *stranded* balance file — the exact path the
    // series' own relocation would then collide with.
    let from = Ref::parse(&format!("{}:{}", Rationes::NAME, eref.slug))?;
    let to = Ref::parse(&format!("{}:{new}", Rationes::NAME))?;
    let own: Vec<&str> = Rationes::kinds().iter().map(|(k, _)| *k).collect();
    let cascade = pantheon::plan_cascade(ctx.store.root(), &own, &from, &to)?;

    let addr = EntityAddr {
        home: eref.home.clone(),
        kind: eref.kind.clone(),
        slug: new.clone(),
    };
    let carried = carried_series(&ctx, &eref)?;
    let change = entity_change(
        "rename",
        &addr,
        Some(contract::entity_json(Rationes::NAME, &eref, &entity)?),
        Some(addr_json(&addr, &entity)?),
        Some(cascade.to_json()),
    );
    if let Some(pending) = review(cli, &change)? {
        return Ok(pending);
    }

    // The record's own file moves first, so a crash mid-cascade leaves refs dangling
    // on the *old* slug — which `pan validate` reports naming exactly the files that
    // still need fixing (§5.4, §10.1). The series follows its determinant, and only
    // then do the refs.
    let moved = ctx.store.relocate_entity(&eref, &addr)?;
    let series = carried
        .map(|sref| ctx.store.relocate_series(&sref, &addr.home, &new))
        .transpose()?;
    cascade.apply(ctx.store.root())?;
    let mut out = json!({
        "renamed": { "from": eref.slug, "to": new },
        "cascade": cascade.to_json(),
        "record": contract::entity_json(Rationes::NAME, &moved, &entity)?,
    });
    if series.is_some() {
        out["series"] = json!({ "from": eref.slug, "to": new });
    }
    Ok(Response::Json(out))
}

/// Re-home a holding (§7.2), carrying its balance series in the same transaction —
/// that series exists only because the holding does, and would otherwise strand at a
/// node its determinant has left (§7.2, §10.2).
fn cmd_move(cli: &Cli, slug: &str, to: &str) -> Result<Response> {
    refuse_under_rule(cli, "move")?;
    let ctx = Ctx::open(cli)?;
    let (eref, entity) = ctx.store.get_entity(
        &normalize_slug(slug)?,
        ctx.filter_kind()?,
        ctx.scope().as_ref(),
    )?;
    refuse_entity_as_node(&eref, "move")?;

    let addr = EntityAddr {
        home: Code::parse(to)?,
        kind: eref.kind.clone(),
        slug: eref.slug.clone(),
    };
    if let Some(held) = ctx.store.slug_taken_at(&addr.home, &addr.slug)? {
        return Err(Error::validation(format!(
            "{} already holds {:?} as a {} (§5.4)",
            addr.home.as_str(),
            addr.slug,
            held.kind
        )));
    }
    // Asked *before* the review, so the refusal is not discovered halfway through a
    // two-file move: `relocate_series` would refuse an occupied path (§7.2), but by
    // then the holding itself has already gone.
    let carried = carried_series(&ctx, &eref)?;
    if carried.is_some()
        && ctx
            .store
            .series_path(&addr.home, Rationes::BALANCE, Some(&addr.slug))?
            .exists()
    {
        return Err(Error::validation(format!(
            "{} already holds a balance series named {:?} (§7.2)",
            addr.home.as_str(),
            addr.slug
        )));
    }

    let change = entity_change(
        "move",
        &addr,
        Some(contract::entity_json(Rationes::NAME, &eref, &entity)?),
        Some(addr_json(&addr, &entity)?),
        None,
    );
    if let Some(pending) = review(cli, &change)? {
        return Ok(pending);
    }
    // No ref changes: a ref carries no path, so it survives a re-home untouched
    // (§5.4). The balance series is not a ref target at all (§7.1) — it simply moves.
    let moved = ctx.store.relocate_entity(&eref, &addr)?;
    let series = carried
        .map(|sref| ctx.store.relocate_series(&sref, &addr.home, &addr.slug))
        .transpose()?;
    let mut out = json!({
        "moved": { "from": eref.home.as_str(), "to": addr.home.as_str() },
        "record": contract::entity_json(Rationes::NAME, &moved, &entity)?,
    });
    if series.is_some() {
        out["series"] = json!({
            "from": eref.home.as_str(),
            "to": addr.home.as_str(),
        });
    }
    Ok(Response::Json(out))
}

/// Remove a holding and its balance series, or one reading with `-a` (§7.2, §18).
fn cmd_rm(cli: &Cli, slug: &str) -> Result<Response> {
    refuse_under_rule(cli, "rm")?;
    let ctx = Ctx::open(cli)?;
    match cli.at.as_deref() {
        Some(at) => rm_reading(cli, &ctx, slug, at),
        None => rm_holding(cli, &ctx, slug),
    }
}

fn rm_holding(cli: &Cli, ctx: &Ctx, slug: &str) -> Result<Response> {
    let (eref, entity) = ctx.store.get_entity(
        &normalize_slug(slug)?,
        ctx.filter_kind()?,
        ctx.scope().as_ref(),
    )?;
    let addr = EntityAddr {
        home: eref.home.clone(),
        kind: eref.kind.clone(),
        slug: eref.slug.clone(),
    };
    let change = entity_change(
        "rm",
        &addr,
        Some(contract::entity_json(Rationes::NAME, &eref, &entity)?),
        None,
        None,
    );
    if let Some(pending) = review(cli, &change)? {
        return Ok(pending);
    }
    ctx.store.remove_entity(&eref)?;
    // The series goes with it, for the reason `move` carries it: it exists only
    // because the holding does, and left behind it is a stranded file — a `pan
    // validate` finding rather than a record (§10.2, §8.3).
    let carried = carried_series(ctx, &eref)?;
    if let Some(sref) = &carried {
        std::fs::remove_file(&sref.path)?;
    }
    let mut out = json!({ "deleted": eref.slug });
    if carried.is_some() {
        out["series"] = json!(eref.slug);
    }
    Ok(Response::Json(out))
}

/// Drop one balance reading (§7.2). The only way a reading leaves the record: an
/// overwrite corrects a figure read wrong, but nothing rewrites a day that never
/// happened away (I1, §6.1).
fn rm_reading(cli: &Cli, ctx: &Ctx, slug: &str, at: &str) -> Result<Response> {
    let eref = holding_for_balance(ctx, slug, ctx.scope().as_ref())?;
    let sref = balance_series(ctx, &eref)?;
    if !sref.path.exists() {
        return Err(no_readings(&eref));
    }
    let key = contract::key_from_at(Some(at))?;
    let lines = ctx.store.read_series(&sref)?;
    let prev = lines.iter().find(|l| l.key == key).ok_or_else(|| {
        Error::not_found(format!("{:?} has no balance keyed {key} (§7.3)", eref.slug))
    })?;

    let change = line_change("rm", &sref, &key, Some(line_json(&sref, prev)?), None);
    if let Some(pending) = review(cli, &change)? {
        return Ok(pending);
    }
    ctx.store.remove_line(&sref, &key)?;
    Ok(Response::Json(
        json!({ "deleted": key.as_str(), "series": eref.slug }),
    ))
}

/// Every holding across the subtree, each carrying its **derived** latest balance
/// (§7.2, I1) — or, with `--net`, those balances folded to net worth.
fn cmd_list(cli: &Cli, net: bool) -> Result<Response> {
    let ctx = Ctx::open(cli)?;
    let locus = ctx.locus();
    let holdings = ctx
        .store
        .fold_entities(locus.as_ref(), ctx.filter_kind()?)?;
    // One walk for every balance series under the locus, each folded to the line at
    // its latest key — which *is* the present (I1). A stranded series answers to no
    // holding and simply joins to nothing here; `pan validate` is what reports it
    // (§10.2).
    let balances = ctx.store.fold(locus.as_ref(), Some(Rationes::BALANCE))?;

    if net {
        return Ok(Response::Json(net_worth_json(&holdings, &balances)?));
    }
    let rows = holdings
        .iter()
        .map(|(eref, entity)| {
            let mut row = contract::entity_json(Rationes::NAME, eref, entity)?;
            attach_balance(&mut row, eref, &balances)?;
            Ok(row)
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(Response::Json(Value::Array(rows)))
}

fn cmd_get(cli: &Cli, slug: &str) -> Result<Response> {
    let ctx = Ctx::open(cli)?;
    let (eref, entity) = ctx.store.get_entity(
        &normalize_slug(slug)?,
        ctx.filter_kind()?,
        ctx.scope().as_ref(),
    )?;
    entity.data.as_holding()?;
    let mut out = contract::entity_json(Rationes::NAME, &eref, &entity)?;
    let balances = ctx.store.fold(Some(&eref.home), Some(Rationes::BALANCE))?;
    attach_balance(&mut out, &eref, &balances)?;
    Ok(Response::Json(out))
}

/// A holding's balance trend — the collection read whole, optionally windowed (§7.2).
///
/// The holding is named, never inferred: §7.3's four inference forms are the
/// *hand-named* series path, and a determined series has no name of its own to have
/// been omitted — it is reached **through** its holding (§5.4, §8.3).
fn cmd_series(
    cli: &Cli,
    tokens: &[String],
    from: Option<&str>,
    to: Option<&str>,
) -> Result<Response> {
    let ctx = Ctx::open(cli)?;
    let [slug] = tokens else {
        return Err(Error::usage(
            "`rat series` takes the holding whose trend you want: `rat series checking` \
             — a balance series is named for its holding and reached through it, so \
             there is no collection to name on its own (§5.4, §7.1, §8.3)",
        ));
    };
    let eref = holding_for_balance(&ctx, slug, ctx.scope().as_ref())?;
    let sref = balance_series(&ctx, &eref)?;
    if !sref.path.exists() {
        return Err(no_readings(&eref));
    }
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
        Rationes::NAME,
        &sref,
        &lines,
    )?))
}

fn cmd_where(cli: &Cli, slug: &str) -> Result<Response> {
    let ctx = Ctx::open(cli)?;
    let eref = ctx.store.locate_entity(
        &normalize_slug(slug)?,
        ctx.filter_kind()?,
        ctx.scope().as_ref(),
    )?;
    let mut out = identity(&eref);
    let rel = eref
        .path
        .strip_prefix(&ctx.root)
        .unwrap_or(&eref.path)
        .to_string_lossy()
        .into_owned();
    out["path"] = Value::String(rel);
    Ok(Response::Json(out))
}

// ── the determinant (§7.3, I5) ──────────────────────────────────────────────

/// Resolve the holding a balance write is *determined by* — the check that closes
/// the trap `Store::write_line` cannot close for itself.
///
/// `write_line` mints any `Shape::Series { named: false }` on first write, because
/// §7.3 says a determined series is minted by its determinant. For Pensum the
/// determinant is the **node**, which the store resolves anyway, so minting on first
/// write is exactly right there. Rationes' determinant is a holding **entity**, and
/// the store links no core and cannot know that (I5) — so it would happily mint
/// `crp__balance__nonexistent.jsonl` beside no holding at all.
///
/// So the check lives here, in Rationes' own bin, over Rationes' own records:
///
/// - **no such holding → exit `4`**, the same not-found §7.3 gives an `add` that
///   would append to a series that does not exist. The determinant *is* the
///   container here, so a typo is a not-found and never a new file.
/// - **a `claim` → exit `3`**, a validation failure: the holding is there and the
///   write is well-formed, but Rationes' own vocabulary says a right you hold carries
///   no balance (§8.3). Legality within a vocabulary is the owning core's check on
///   write (§6.4), and this is Rationes making it.
///
/// The lookup doubles as the home: a balance series lives wherever its holding does,
/// so resolving the determinant answers "which node" at the same time — which is why
/// `rat checking 4200` needs no `-H` and no `$PWD`.
fn holding_for_balance(ctx: &Ctx, slug: &str, scope: Option<&Code>) -> Result<EntityRef> {
    let eref = ctx
        .store
        .locate_entity(&normalize_slug(slug)?, None, scope)?;
    if !Rationes::carries_balance(&eref.kind) {
        return Err(Error::validation(format!(
            "{:?} is a {}, and a {} carries no balance series: an expiry is a field, not \
             a figure sampled over time — set it with `rat edit {} --expires <YYMMDD>` \
             (§8.3, §6.4)",
            eref.slug, eref.kind, eref.kind, eref.slug
        )));
    }
    Ok(eref)
}

/// The [`SeriesRef`] for a holding's balance series, whether or not it exists yet.
/// Its name slot carries the holding's slug — that is the whole of what "determined"
/// means on disk (§7.1, §8.3).
///
/// **The one constructor**, deliberately: a second that skipped the path lookup
/// handed `relocate_series` an empty path, and a `rename` then moved the holding and
/// left its readings behind under the old name — the exact stranding §10.2 is about.
fn balance_series(ctx: &Ctx, eref: &EntityRef) -> Result<SeriesRef> {
    Ok(SeriesRef {
        home: eref.home.clone(),
        kind: Rationes::BALANCE.to_string(),
        name: Some(eref.slug.clone()),
        path: ctx
            .store
            .series_path(&eref.home, Rationes::BALANCE, Some(&eref.slug))?,
    })
}

/// The holding's balance series **if it has one** — what `rename`, `move`, and `rm`
/// carry along, and `None` where the holding has no readings to carry.
fn carried_series(ctx: &Ctx, eref: &EntityRef) -> Result<Option<SeriesRef>> {
    let sref = balance_series(ctx, eref)?;
    Ok(sref.path.exists().then_some(sref))
}

fn no_readings(eref: &EntityRef) -> Error {
    Error::not_found(format!(
        "{:?} holds no balance readings yet — `rat {} {} <amount>` writes the first, \
         which is what mints the series (§7.3, §8.3)",
        eref.slug,
        eref.home.as_str(),
        eref.slug
    ))
}

// ── the derived present (I1) ────────────────────────────────────────────────

/// Attach a holding's latest balance to its emitted record (I1).
///
/// **Absent, not null**, where there is none: a `claim` never has one and an untouched
/// `account` does not have one yet, and a hollow key would read as a figure withheld
/// rather than a figure that does not exist — the same discipline `line_json` keeps
/// for a determined series' name (§7.3).
fn attach_balance(
    row: &mut Value,
    eref: &EntityRef,
    balances: &[PresentLine<Record>],
) -> Result<()> {
    let Some(present) = latest_for(eref, balances) else {
        return Ok(());
    };
    let balance = present.line.data.as_balance()?;
    row["balance"] = json!(balance.amount);
    row["as_of"] = json!(present.line.key.as_str());
    Ok(())
}

fn latest_for<'a>(
    eref: &EntityRef,
    balances: &'a [PresentLine<Record>],
) -> Option<&'a PresentLine<Record>> {
    balances
        .iter()
        .find(|present| present.home == eref.home && present.name.as_deref() == Some(&eref.slug))
}

/// Net worth: the latest balance of the kinds that *have* one, summed **by currency**
/// and never stored (I1, §8.3).
///
/// Summed by currency rather than across it, because adding dollars to shares would
/// be a figure that is precise and false. A holding whose currency is unstated folds
/// under `null` — it is one bucket among the others, not an error and not a guess.
/// A `claim` contributes nothing at all: your passport carries no balance, so it
/// never reaches this fold.
fn net_worth_json(
    holdings: &[(EntityRef, Entity<Record>)],
    balances: &[PresentLine<Record>],
) -> Result<Value> {
    let mut totals: Vec<(Option<String>, f64, usize)> = Vec::new();
    for (eref, entity) in holdings {
        if !Rationes::carries_balance(&eref.kind) {
            continue;
        }
        let Some(present) = latest_for(eref, balances) else {
            continue;
        };
        let amount = present.line.data.as_balance()?.amount;
        let currency = entity.data.as_holding()?.currency.clone();
        match totals.iter_mut().find(|(c, _, _)| *c == currency) {
            Some((_, total, count)) => {
                *total += amount;
                *count += 1;
            }
            None => totals.push((currency, amount, 1)),
        }
    }
    // Sorted so the fold is stable across two runs on the same tree, whatever order
    // the filesystem handed the files back in (§7.3).
    totals.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(Value::Array(
        totals
            .into_iter()
            .map(|(currency, total, holdings)| {
                json!({ "currency": currency, "total": total, "holdings": holdings })
            })
            .collect(),
    ))
}

// ── shared plumbing ─────────────────────────────────────────────────────────

struct Ctx {
    root: PathBuf,
    store: Store<Rationes>,
    /// The explicit `-k`, normalized and checked — `None` when none was given. Kept
    /// optional because `-k` means two things: on a write it *selects* the token, on
    /// a read it *filters* by it, and only the write has a default (§7.2).
    kind: Option<String>,
    /// The explicit `-H`, if any.
    home: Option<Code>,
}

impl Ctx {
    fn open(cli: &Cli) -> Result<Ctx> {
        let root = resolve_root(cli.root.as_deref())?;
        let store = Store::new(root.clone());
        let kind = match cli.kind.as_deref() {
            Some(raw) => {
                let kind = pantheon::name::normalize_token(raw, "kind")?;
                let known = Store::<Rationes>::owns_entity_kind(&kind)
                    || Store::<Rationes>::owns_series_kind(&kind);
                if !known {
                    return Err(Error::usage(format!(
                        "rationes has no {kind:?} token; it declares {}, {} (§7.1)",
                        Rationes::KINDS.join(", "),
                        Rationes::BALANCE
                    )));
                }
                Some(kind)
            }
            None => None,
        };
        let home = cli.home.as_deref().map(Code::parse).transpose()?;
        Ok(Ctx {
            root,
            store,
            kind,
            home,
        })
    }

    /// Which token an entity write files under: the explicit `-k`, else `account` —
    /// hardcoded, never a setting (§18).
    ///
    /// `-k balance` is refused here, in §7.2's own words: the entity form has no
    /// `balance` token, and `-k` selects *within* a shape and never across it.
    fn write_kind(&self) -> Result<String> {
        match &self.kind {
            Some(kind) if kind == Rationes::BALANCE => Err(balance_is_not_an_entity_kind("add")),
            Some(kind) => Ok(kind.clone()),
            None => Ok(Rationes::DEFAULT_KIND.to_string()),
        }
    }

    /// Which token a read filters by: `None` means all three holdings (§7.2).
    ///
    /// `balance` is not one of them. A read of Rationes' entities is a read of
    /// holdings, and a balance is reached *through* its holding (§5.4) — `rat series`
    /// is the verb that reads one, so that is where the refusal points.
    fn filter_kind(&self) -> Result<Option<&str>> {
        match self.kind.as_deref() {
            Some(kind) if kind == Rationes::BALANCE => Err(Error::usage(
                "`-k balance` filters no holding: a balance is reached through the \
                 holding that determines it, so its trend is `rat series <holding>` \
                 (§5.4, §7.2, §8.3)",
            )),
            other => Ok(other),
        }
    }

    /// What a slug lookup is scoped to. `-H` narrows it; otherwise the whole tree,
    /// because a slug is unique **per core, not per node** (§5.4) — narrowing to
    /// $PWD would make `rat get checking` mean different accounts in different
    /// directories.
    fn scope(&self) -> Option<Code> {
        self.home.clone()
    }

    /// What a fold is scoped to. Unlike a lookup this *is* the locus: `cd
    /// c_r_res/ && rat ls` lists the holdings filed there (§7.3). Outside the tree
    /// there is nothing to narrow by, so the fold spans the forest.
    fn locus(&self) -> Option<Code> {
        self.home
            .clone()
            .or_else(|| contract::code_at_path(&self.root, None).ok())
    }
}

impl Fields {
    /// Which field, if any, was named without a value — the editor form's target
    /// (§7.3). More than one is a usage error: a buffer holds one value.
    fn bare_field(&self) -> Result<Option<&'static str>> {
        let bare: Vec<&'static str> = [
            ("currency", &self.currency),
            ("expires", &self.expires),
            ("note", &self.note),
        ]
        .into_iter()
        .filter(|(_, v)| matches!(v, Some(None)))
        .map(|(name, _)| name)
        .collect();
        match bare.as_slice() {
            [] => Ok(None),
            [one] => Ok(Some(one)),
            many => Err(Error::usage(format!(
                "the editor form opens one value, and {} fields were named: {} (§7.3)",
                many.len(),
                many.join(", ")
            ))),
        }
    }
}

/// A flag given with a value replaces; a flag absent leaves what the record already
/// holds (I1). A bare flag is the editor form and never reaches here.
fn given(field: Option<&Option<String>>, prev: Option<String>) -> Option<String> {
    match field {
        Some(Some(value)) => Some(value.clone()),
        _ => prev,
    }
}

/// Build the holding a write means. What a hand does not give, the record keeps —
/// the stored record is the truth, not the command line (I1).
fn build_holding(fields: &Fields, prev: Option<&Holding>) -> Holding {
    let prev = prev.cloned().unwrap_or_default();
    Holding {
        currency: given(fields.currency.as_ref(), prev.currency),
        expires: given(fields.expires.as_ref(), prev.expires),
        note: given(fields.note.as_ref(), prev.note),
    }
}

/// The editor form of `edit` (§7.3): a field named with no value opens that value,
/// and the session *is* the review — it mints no plan token and needs no `-y`,
/// because the hand is already looking at the thing it is changing.
fn editor_form(
    ctx: &Ctx,
    eref: &EntityRef,
    before: &Entity<Record>,
    field: &'static str,
) -> Result<Response> {
    // Piped, it spawns nothing and prints the file's path, by the same law that sends
    // a table to a TTY and JSON down a pipe: the LLM hand gets a path to open with
    // its own tools rather than a blocked process it cannot drive (I8).
    if !contract::stdout_is_terminal() {
        return Ok(Response::Json(
            json!({ "path": eref.path.display().to_string() }),
        ));
    }

    // What opens holds **only that value** — never the raw JSON, which is
    // machine-owned and is never handed to a hand raw (I6, §6.6, §7.3).
    let holding = before.data.as_holding()?;
    let initial = format!("{}\n", read_field(holding, field).unwrap_or_default());
    match contract::edit_text(&initial)? {
        // Text that comes back unchanged writes nothing (§7.3).
        contract::Edited::Unchanged => Ok(Response::Json(contract::entity_json(
            Rationes::NAME,
            eref,
            before,
        )?)),
        contract::Edited::Changed(text) => {
            let trimmed = text.trim();
            let value = (!trimmed.is_empty()).then(|| trimmed.to_string());
            let mut holding = holding.clone();
            write_field(&mut holding, field, value);
            let record = Record::Holding(holding);
            // Text that comes back invalid exits 3 (§7.3).
            Rationes::validate(&record)?;
            let entity = Entity {
                refs: before.refs.clone(),
                data: record,
            };
            let addr = EntityAddr {
                home: eref.home.clone(),
                kind: eref.kind.clone(),
                slug: eref.slug.clone(),
            };
            ctx.store
                .write_entity(&addr, entity.refs.clone(), &entity.data)?;
            Ok(Response::Json(contract::entity_json(
                Rationes::NAME,
                eref,
                &entity,
            )?))
        }
    }
}

fn read_field(holding: &Holding, field: &str) -> Option<String> {
    match field {
        "currency" => holding.currency.clone(),
        "expires" => holding.expires.clone(),
        _ => holding.note.clone(),
    }
}

fn write_field(holding: &mut Holding, field: &str, value: Option<String>) {
    match field {
        "currency" => holding.currency = value,
        "expires" => holding.expires = value,
        _ => holding.note = value,
    }
}

/// §7.2's own words: `-k` selects within a shape, never across it, and the entity
/// form has no `balance` token.
fn balance_is_not_an_entity_kind(verb: &str) -> Error {
    Error::usage(format!(
        "`rat {verb} -k balance` names a series token on the entity form: -k selects \
         within a shape and never across it, and a balance is written by giving a \
         holding its figure — `rat <home> <holding> <amount>` (§7.1, §7.2)"
    ))
}

/// Neither `rename` nor `move` may touch an entity-as-node: its slug *is* its node's
/// definition and its home *is* its node (§5.2, I3), so either would be a node
/// operation — which no core may perform (§7.2).
fn refuse_entity_as_node(eref: &EntityRef, verb: &str) -> Result<()> {
    if eref.form != EntityForm::AsNode {
        return Ok(());
    }
    Err(Error::usage(if verb == "rename" {
        format!(
            "{:?} is an entity-as-node: its slug is its node's definition, so renaming it \
             renames the node, which no core may do — use `pan rename {} --def <new>` \
             (§5.5, §7.2)",
            eref.slug,
            eref.home.as_str()
        )
    } else {
        format!(
            "{:?} is an entity-as-node: its home is its own node, so re-homing it is a node \
             move — use `pan mv {} --to <code>`. A file mv would strand the node and \
             everything homed at it (§5.5, §7.2, I3)",
            eref.slug,
            eref.home.as_str()
        )
    }))
}

/// A slug held at another node is a *soft* finding: the check is a tree walk, which
/// is the cost the softness exists to avoid (§5.4, §18). The record still goes to
/// stdout; the warning rides stderr in the same shape `pan validate` emits, so a
/// machine hand reads one shape from both surfaces (I4, I8).
fn warn_duplicates(ctx: &Ctx, written: &EntityRef) -> Result<()> {
    let elsewhere = ctx
        .store
        .duplicate_slugs_elsewhere(&written.home, &written.slug)?;
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
                "{}:{} also names a record at {} — a ref meeting both lists them rather than \
                 guessing; a fuller name tells them apart (§5.4, §7.3)",
                Rationes::NAME,
                written.slug,
                other.home.as_str()
            ),
            // A cross-node duplicate is a genuine choice — which record takes the
            // fuller name is the hand's — so there is no single legal correction (§10.2).
            fix: None,
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

fn entity_change(
    verb: &'static str,
    addr: &EntityAddr,
    before: Option<Value>,
    after: Option<Value>,
    cascade: Option<Value>,
) -> RecordChange {
    RecordChange {
        verb,
        core: Rationes::NAME.to_string(),
        home: addr.home.as_str().to_string(),
        kind: addr.kind.clone(),
        // A holding is partitioned, so the change body names no series (§7.1).
        series: None,
        // A partitioned entity stores no key — its *name* is the key (§5.4, §18).
        key: addr.slug.clone(),
        before,
        after,
        cascade,
    }
}

/// The change a balance write reviews.
///
/// `series` is **present** here, unlike Pensum's, and that is the whole difference
/// between the two determined series: Pensum's name slot is empty, while Rationes'
/// carries its holding's slug — so there *is* a name to report, and a reader of the
/// change who was not told it could not say which account moved (§7.1, §8.3).
fn line_change(
    verb: &'static str,
    sref: &SeriesRef,
    key: &Key,
    before: Option<Value>,
    after: Option<Value>,
) -> RecordChange {
    RecordChange {
        verb,
        core: Rationes::NAME.to_string(),
        home: sref.home.as_str().to_string(),
        kind: sref.kind.clone(),
        series: sref.name.clone(),
        key: key.to_string(),
        before,
        after,
        cascade: None,
    }
}

fn line_json(sref: &SeriesRef, line: &Line<Record>) -> Result<Value> {
    contract::line_json(
        Rationes::NAME,
        &sref.home,
        &sref.kind,
        sref.name.as_deref(),
        line,
    )
}

/// The contract JSON for a record not yet on disk — an `add`'s result, or a
/// `rename`'s destination. The same shape [`contract::entity_json`] emits.
fn addr_json(addr: &EntityAddr, entity: &Entity<Record>) -> Result<Value> {
    Ok(json!({
        "core": Rationes::NAME,
        "home": addr.home.as_str(),
        "kind": addr.kind,
        "slug": addr.slug,
        "refs": entity.refs.iter().map(Ref::to_token).collect::<Vec<_>>(),
        "data": serde_json::to_value(&entity.data)?,
    }))
}

fn identity(eref: &EntityRef) -> Value {
    json!({
        "core": Rationes::NAME,
        "home": eref.home.as_str(),
        "kind": eref.kind,
        "slug": eref.slug,
    })
}

/// A slug given on the command line is a typed token, so it is normalized on the way
/// in — `rat get "Checking Account"` finds `checking_account` (§5.1).
fn normalize_slug(raw: &str) -> Result<String> {
    pantheon::name::normalize_token(raw, "name")
}

fn parse_refs(refs: &[String]) -> Result<Vec<Ref>> {
    refs.iter().map(|r| Ref::parse(r)).collect()
}

fn version_json() -> Value {
    json!({
        "name": Rationes::NAME,
        "short": "rat",
        "version": env!("CARGO_PKG_VERSION"),
        "format_version": 1,
    })
}

fn help_json() -> Value {
    json!({
        "name": Rationes::NAME,
        "short": "rat",
        "about": "res holdings: the accounts, goods, and rights you hold (§8.3)",
        "verbs": VERBS,
        "kinds": Rationes::KINDS,
        "series": Rationes::BALANCE,
        "version": env!("CARGO_PKG_VERSION"),
        "format_version": 1,
    })
}
