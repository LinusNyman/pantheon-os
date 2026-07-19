//! `fas` — Fasti's CLI (§7). stdout is JSON when piped, a table on a TTY (§7.3).
//!
//! The bin owns only what is Fasti's own: its positionals and the flags its primitive
//! needs (`--from`, `--to`, `--until`, `--series`, `-c`, `-a`, `-r`, `--note`,
//! `--unspanned`). Everything downstream — reading the hand, confirming a mutation,
//! turning `--at` into a key, *finding* a home and a series, planning the rename
//! cascade, shaping a record into the contract's JSON — is `pantheon`, so every core
//! produces that JSON the same way (I4) and no core reaches into another's records (I5).
//!
//! # The one thing Fasti has to decide that a one-shape core does not
//!
//! Fasti declares two tokens in **two shapes** (§7.1), so every verb begins by asking
//! which it means. There are three answers, and each is the spec's own rule rather
//! than a convention invented here:
//!
//! - **On a write, the *form* picks the shape and `-k` never crosses it** (§7.2). Each
//!   shape wears flags only it can wear — `--from`/`--to` are a span's period,
//!   `-a`/`--until`/`--series`/`-c` an event's — so the flags on the line settle it.
//!   Give both sets and it is a usage error; give neither and it is a span, since a
//!   partitioned `add` **is** the record it creates while an event needs a series that
//!   must already exist (§18). `-k` is then *checked* against that form, never
//!   selecting it: `fas add -k event` with nothing to make it a series write is exit
//!   `2`, the same refusal §7.2 spells out for `rat add -k balance`.
//! - **On `edit`/`rm`, the shape is the one that answers** — a span with that slug, or
//!   an event line with that key. Both, and it lists them rather than guessing; neither,
//!   and it is not found. Asked rather than inferred from the key's shape, because a
//!   span may legally be named in digits and `KeyShape` is explicitly best-effort
//!   (§5.4) — the authoritative reading is the owning core's, and here that is a walk.
//! - **On `rename`/`move`/`get`/`where`, the candidates are the two things a *ref* can
//!   point at** (§5.4): a span entity, or an event series *as a collection*. A
//!   date-keyed line inside one is a sample, never a target (I1), so it is not a
//!   candidate here even though `edit` reaches it by key.

// The bin shares the spine's conventional pedantic allows (see pantheon/src/lib.rs).
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::doc_markdown)]
#![allow(clippy::too_many_lines)]
#![allow(clippy::too_many_arguments)]

use std::collections::HashSet;
use std::ffi::OsString;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand, ValueEnum};
use serde_json::{Value, json};

use fasti::{Event, Fasti, FastiRecord, Span};

// The screen rides the `tui` feature; drop it and the core is headless (§14).
#[cfg(feature = "tui")]
mod screen;
use pantheon::envelope::{Entity, Key, Line, Ref};
use pantheon::validate::{Finding, FindingCode, Severity, findings_json};
use pantheon::{
    Checkpoint, Code, Core, EntityAddr, EntityForm, EntityRef, Error, RecordChange, Response,
    Result, SeriesRef, SeriesTarget, Store, contract, resolve_root,
};

/// The twelve verbs (§7.3). A closed reserved set: a verb wins over a node code,
/// which is what makes `add` safe to leave implicit (the ambiguity rule, §7.3).
const VERBS: &[&str] = &[
    "add", "edit", "rename", "move", "mv", "rm", "list", "ls", "get", "series", "where", "schema",
    "help", "version",
];

/// What a headless build prints for a bare short (§14, §7.3).
#[cfg(not(feature = "tui"))]
const BARE: &str = "fas — Fasti (actio · placement). Built without the `tui` feature; run `fas --help` for the verbs.\n";

#[derive(Parser)]
#[command(
    name = "fas",
    version,
    about = "Fasti — the placement tense: spans you are inside of and events on the timeline (§8.4).",
    disable_help_subcommand = true
)]
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
    /// Which of the core's tokens (§7.2): `span` or `event`. On a write it is checked
    /// against the form the flags already picked; on a read it filters.
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

/// Fasti's own fields, shared by `add` and `edit` (§8.4).
///
/// **Each flag belongs to exactly one shape, and that is what picks the form** (§7.2).
/// `--from`/`--to` bound a period, so they can only mean a span; `--until` ends one
/// occurrence and `--series`/`-c`/`-a` name or date a line, so they can only mean an
/// event. `--note` is the one field both wear, and so settles nothing.
// `Option<Option<String>>` on `--note` is deliberate, and is the case the lint itself
// carves out: three states genuinely differ. Absent leaves the record alone (I1),
// `--note` bare is the editor form (§7.3), and `--note TEXT` replaces.
#[allow(clippy::option_option)]
#[derive(clap::Args, Default)]
struct Fields {
    /// The day a span opens, YYMMDD (§8.4). Names the entity form.
    #[arg(long = "from", value_name = "DAY")]
    from: Option<String>,
    /// The day a span closes, YYMMDD (§8.4). Names the entity form; absent on a fresh
    /// span leaves it open, which is the state you are still in.
    #[arg(long = "to", value_name = "DAY")]
    to: Option<String>,
    /// When an occurrence ends — hhmm, or YYMMDDThhmm (§8.4). Names the series form;
    /// the start is the line's own key (§7.3).
    #[arg(long = "until", value_name = "WHEN")]
    until: Option<String>,
    /// A hand's remark. Given bare, it is the editor form (§7.3).
    #[arg(long = "note", value_name = "TEXT", num_args = 0..=1)]
    note: Option<Option<String>>,
}

#[derive(Subcommand)]
enum Cmd {
    /// Place something on the timeline — a span, or an occurrence in an event series.
    ///
    /// A **span** is `[home] <name> --from DAY [--to DAY]`: `fas aof mvp_phase --from
    /// 260101`. A partitioned entity needs no prior container — the record `add`
    /// creates *is* the span (§18).
    ///
    /// An **event** is `[home] [series] [values…]` with a date: `fas aof standups
    /// "sprint review" -a 260719T1600`. Each of §7.3's four inference forms works, and
    /// `-c` mints the series first — a hand-named series is never conjured by a typo.
    Add {
        tokens: Vec<String>,
        #[command(flatten)]
        fields: Fields,
        /// Attach a reference; repeatable (§5.4). An event's span is one of these,
        /// never a field (§8.4, I9).
        #[arg(short = 'r', long = "ref", value_name = "REF")]
        refs: Vec<String>,
        /// Mint the event series before writing the first occurrence (§7.3).
        #[arg(short = 'c', long = "create")]
        create: bool,
        /// The occurrence's date, date and time, or a time today (§7.3).
        #[arg(short = 'a', long = "at", value_name = "WHEN")]
        at: Option<String>,
        /// Name the event series explicitly.
        #[arg(long = "series", value_name = "NAME")]
        series: Option<String>,
    },
    /// Correct a record in place — a span by slug, or an occurrence by its key (§7.2).
    /// What a hand does not give, the record keeps (I1).
    ///
    /// Closing an open span is this verb: `fas edit mvp_phase --to 260901`.
    ///
    /// Given no new value it is the editor form (§7.3): at a TTY the value opens in
    /// `$VISUAL`/`$EDITOR`/`vi`; piped, it prints `{"path":…}`.
    Edit {
        key: String,
        values: Vec<String>,
        #[command(flatten)]
        fields: Fields,
        #[arg(short = 'r', long = "ref", value_name = "REF")]
        refs: Vec<String>,
        #[arg(long = "series", value_name = "NAME")]
        series: Option<String>,
    },
    /// Rename a span or an event series, cascading every ref pointing at it (§7.2, §5.4).
    ///
    /// A name and its slug are one thing, so this is its own verb rather than a flag on
    /// `edit`: rewriting refs across the tree is a structural act and reads as one.
    Rename { slug: String, new: String },
    /// Re-home a span or an event series to another node (§7.2). A file `mv` between
    /// meta dirs — refs carry no path, so none of them changes (§5.4).
    #[command(alias = "mv")]
    Move {
        slug: String,
        #[arg(long = "to", value_name = "CODE")]
        to: String,
    },
    /// Remove a span by slug, or one occurrence by its key — irreversible (§7.2, §18).
    Rm {
        key: String,
        #[arg(long = "series", value_name = "NAME")]
        series: Option<String>,
    },
    /// Every span, and every event series' present, across the subtree (§7.2). `-k`
    /// filters to one shape.
    #[command(alias = "ls")]
    List {
        /// Only the events that reference no span (§8.4) — legal, never a finding.
        #[arg(long = "unspanned")]
        unspanned: bool,
    },
    /// One span by slug, or one event series' present — the occurrence at its latest
    /// key (§7.2, I1).
    Get { tokens: Vec<String> },
    /// A whole event series: the occurrences across keys, optionally windowed (§7.2).
    Series {
        tokens: Vec<String>,
        #[arg(long = "from", value_name = "KEY")]
        from: Option<String>,
        #[arg(long = "to", value_name = "KEY")]
        to: Option<String>,
    },
    /// Resolve a span or an event series to its home code, by walking Fasti's own
    /// files (§7.3).
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
const VALUE_FLAGS: &[&str] = &[
    "-C", "--root", "-f", "--format", "-p", "--plan", "-H", "--home", "-k", "--kind", "-a", "--at",
    "-r", "--ref", "--series", "--note", "--from", "--to", "--until",
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
    match cmd {
        Cmd::Add {
            tokens,
            fields,
            refs,
            create,
            at,
            series,
        } => cmd_add(
            cli,
            tokens,
            fields,
            refs,
            *create,
            at.as_deref(),
            series.as_deref(),
        ),
        Cmd::Edit {
            key,
            values,
            fields,
            refs,
            series,
        } => cmd_edit(cli, key, values, fields, refs, series.as_deref()),
        Cmd::Rename { slug, new } => cmd_rename(cli, slug, new),
        Cmd::Move { slug, to } => cmd_move(cli, slug, to),
        Cmd::Rm { key, series } => cmd_rm(cli, key, series.as_deref()),
        Cmd::List { unspanned } => cmd_list(cli, *unspanned),
        Cmd::Get { tokens } => cmd_get(cli, tokens),
        Cmd::Series { tokens, from, to } => cmd_series(cli, tokens, from.as_deref(), to.as_deref()),
        Cmd::Where { tokens } => cmd_where(cli, tokens),
        Cmd::Schema => Ok(Response::Json(serde_json::to_value(pantheon::schema::<
            Fasti,
        >(1))?)),
        Cmd::Version => Ok(Response::Json(version_json())),
        Cmd::Help => Ok(Response::Json(help_json())),
    }
}

// ── which shape a verb means (§7.1, §7.2) ───────────────────────────────────

/// Which of Fasti's two shapes a verb is working in.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Form {
    /// The partitioned `span` (§6.1).
    Span,
    /// The hand-named `event` series (§6.1).
    Event,
}

impl Form {
    fn kind(self) -> &'static str {
        match self {
            Form::Span => Fasti::SPAN,
            Form::Event => Fasti::EVENT,
        }
    }
}

/// The form a **write** means (§7.2): the flags on the line pick the shape, and `-k` is
/// checked against them rather than selecting across them.
///
/// This is §7.2's rule applied to a core whose two tokens are two shapes, so `-k` can
/// only ever confirm what the form already said — which is why `fas add -k event`, with
/// nothing on the line to make it a series write, is the same usage error §7.2 spells
/// out for `rat add -k balance`.
fn write_form(
    kind: Option<&str>,
    fields: &Fields,
    create: bool,
    at: Option<&str>,
    series: Option<&str>,
) -> Result<Form> {
    let span_flags: Vec<&str> = [
        ("--from", fields.from.is_some()),
        ("--to", fields.to.is_some()),
    ]
    .into_iter()
    .filter(|(_, given)| *given)
    .map(|(flag, _)| flag)
    .collect();
    let event_flags: Vec<&str> = [
        ("--until", fields.until.is_some()),
        ("--series", series.is_some()),
        ("-c", create),
        ("-a", at.is_some()),
    ]
    .into_iter()
    .filter(|(_, given)| *given)
    .map(|(flag, _)| flag)
    .collect();

    if !span_flags.is_empty() && !event_flags.is_empty() {
        return Err(Error::usage(format!(
            "{} bound a span and {} date an event, and one write is one shape: a span is a \
             period you are inside of, an event a dated occurrence within one (§7.2, §8.4)",
            span_flags.join("/"),
            event_flags.join("/")
        )));
    }
    // The form the line picked. With neither set given it is a span: a partitioned
    // `add` *is* the record it creates, while an event needs a series that must already
    // exist (§18) — so the shape that can stand alone is the one a bare write means.
    let form = if event_flags.is_empty() {
        Form::Span
    } else {
        Form::Event
    };
    match kind {
        None => Ok(form),
        Some(k) if k == form.kind() => Ok(form),
        Some(k) if k == Fasti::EVENT => Err(Error::usage(
            "the entity form has no `event` token: name the series with `--series`, date the \
             occurrence with `-a`, or mint one with `-c` — `-k` selects within a shape, never \
             across it (§7.2)",
        )),
        Some(_) => Err(Error::usage(format!(
            "the series form has no `span` token: a span is bounded by `--from`/`--to`, not \
             dated into a collection — drop {} to write one (§7.2)",
            event_flags.join("/")
        ))),
    }
}

/// The form a **record key** names (§7.2): a span's slug, or an event line's key.
///
/// Asked of the tree rather than inferred from the key's shape. [`KeyShape`] is
/// explicitly best-effort — "a name whose slug is all digits is indistinguishable from
/// a date by shape, so the authoritative reading is the owning core's" (§5.4) — and
/// here that reading is one walk per shape. Both answer, and it lists them; neither
/// does, and it is not found.
///
/// [`KeyShape`]: pantheon::KeyShape
fn record_form(ctx: &Ctx, key: &str) -> Result<Form> {
    if let Some(kind) = ctx.filter_kind() {
        return Ok(if kind == Fasti::SPAN {
            Form::Span
        } else {
            Form::Event
        });
    }
    let scope = ctx.scope();
    let span = ctx.store.find_entities(
        scope.as_ref(),
        Some(Fasti::SPAN),
        Some(&normalize_slug(key)?),
    )?;
    let line = ctx
        .store
        .find_line(&Key::parse(key)?, Some(Fasti::EVENT), scope.as_ref())?;
    match (span.is_empty(), line.is_empty()) {
        (false, true) => Ok(Form::Span),
        (true, false) => Ok(Form::Event),
        (false, false) => Err(Error::usage(format!(
            "{key:?} names both a span and an occurrence in an event series — name one with \
             `-k span` or `-k event` (§7.2)"
        ))),
        (true, true) => Err(Error::not_found(format!(
            "no fasti span named {key:?} and no event occurrence keyed {key} (§7.3)"
        ))),
    }
}

/// The form a **ref target** names (§5.4): a span entity, or an event series *as a
/// collection*.
///
/// Deliberately not [`record_form`]. The two candidates here are the two things a
/// `core:slug` can point at — an entity, and a hand-named series (§5.4) — because these
/// are the verbs that move an identity (`rename`, `move`) or read one whole (`get`,
/// `where`). A **date-keyed line inside** a series is a sample and never a target (I1),
/// so it is not a candidate, even though `edit` reaches it by key.
fn target_form(ctx: &Ctx, name: &str) -> Result<Form> {
    if let Some(kind) = ctx.filter_kind() {
        return Ok(if kind == Fasti::SPAN {
            Form::Span
        } else {
            Form::Event
        });
    }
    let scope = ctx.scope();
    let slug = normalize_slug(name)?;
    let span = ctx
        .store
        .find_entities(scope.as_ref(), Some(Fasti::SPAN), Some(&slug))?;
    let series = ctx
        .store
        .find_series(scope.as_ref(), Some(Fasti::EVENT), Some(&slug))?;
    match (span.is_empty(), series.is_empty()) {
        (false, true) => Ok(Form::Span),
        (true, false) => Ok(Form::Event),
        // Two files, one `fasti:<slug>` — the collision `add` refuses within a node and
        // `pan validate` reports across them (§5.4, §18).
        (false, false) => Err(Error::usage(format!(
            "fasti:{slug} names both a span and an event series — name one with `-k span` or \
             `-k event`, and fix the collision at the source (§5.4)"
        ))),
        (true, true) => Err(Error::not_found(format!(
            "no fasti span or event series named {slug:?} (§7.3)"
        ))),
    }
}

// ── the verbs ───────────────────────────────────────────────────────────────

fn cmd_add(
    cli: &Cli,
    tokens: &[String],
    fields: &Fields,
    refs: &[String],
    create: bool,
    at: Option<&str>,
    series: Option<&str>,
) -> Result<Response> {
    refuse_under_rule(cli, "add")?;
    let ctx = Ctx::open(cli)?;
    match write_form(ctx.filter_kind(), fields, create, at, series)? {
        Form::Span => add_span(cli, &ctx, tokens, fields, refs),
        Form::Event => add_event(cli, &ctx, tokens, fields, refs, create, at, series),
    }
}

/// A span is a partitioned entity, so the record `add` creates *is* the span — there is
/// no container to mint and none is conjured (§18).
fn add_span(
    cli: &Cli,
    ctx: &Ctx,
    tokens: &[String],
    fields: &Fields,
    refs: &[String],
) -> Result<Response> {
    let target = contract::resolve_entity_target(
        &ctx.store,
        &contract::EntityQuery {
            kind: Fasti::SPAN,
            home: cli.home.as_deref(),
            positionals: tokens,
            pwd: None,
        },
    )?;

    // Within a node the check is cheap, so it is hard — and for Fasti it runs across
    // *shapes*, not just kinds: `aof__span__standups.json` and
    // `aof__event__standups.jsonl` are two files and one `fasti:standups`, exactly the
    // trap §5.4 describes between two kinds, in the dimension a two-shape core adds.
    // `slug_taken_at` sees only entity files (it is the entity half of the check), so
    // the series half is asked here.
    refuse_series_holding(ctx, &target.home, &target.slug)?;

    let span = build_span(fields, previous_span(ctx, &target)?.as_ref())?;
    let record = FastiRecord::Span(span);
    Fasti::validate(&record)?;
    let entity = Entity {
        refs: parse_refs(refs)?,
        data: record,
    };
    let addr = EntityAddr {
        home: target.home.clone(),
        kind: Fasti::SPAN.to_string(),
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
                Some(contract::entity_json(Fasti::NAME, held, &before)?),
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
    // Across nodes the check is a walk, so it stays soft: the record goes to stdout,
    // the warning to stderr (§5.4, §18).
    warn_duplicates(ctx, &written)?;
    Ok(Response::Json(contract::entity_json(
        Fasti::NAME,
        &written,
        &entity,
    )?))
}

/// An event is a line in a hand-named series, so `add` fills a container it never mints
/// — `-c` does that first (§7.3, §18).
fn add_event(
    cli: &Cli,
    ctx: &Ctx,
    tokens: &[String],
    fields: &Fields,
    refs: &[String],
    create: bool,
    at: Option<&str>,
    series: Option<&str>,
) -> Result<Response> {
    let target = ctx.write_target(cli, series, tokens, create)?;

    let sref = match (target.existing.clone(), create) {
        (Some(_), true) => {
            return Err(Error::validation(format!(
                "series {:?} already exists at {} (§7.3)",
                target.name,
                target.home.as_str()
            )));
        }
        (Some(found), false) => found,
        (None, true) => {
            // The same cross-shape check `add_span` makes, from the other side: a
            // series minted onto a slug a span holds would be two files and one ref.
            refuse_span_holding(ctx, &target.home, &target.name)?;
            ctx.store
                .create_series(&target.home, &target.kind, &target.name)?
        }
        (None, false) => return Err(missing(&target)),
    };

    // `fas aof standups -c` mints the timeline empty (§7.3).
    if create && target.values.is_empty() && fields.is_empty() && refs.is_empty() {
        return Ok(Response::Json(json!({ "created": series_identity(&sref) })));
    }

    let key = contract::key_from_at(at)?;
    let record = FastiRecord::Event(build_event(fields, target.values.clone(), None));
    Fasti::validate(&record)?;
    let line = Line {
        key: key.clone(),
        refs: parse_refs(refs)?,
        data: record,
    };
    let after = line_json(&sref, &line)?;

    let existing = ctx.store.read_series(&sref)?;
    let previous = existing.iter().find(|l| l.key == key);

    // A fresh key runs free; landing on one that exists is an overwrite — a mutation,
    // shown and confirmed before it commits (§7.3, I1).
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
        None if cli.dry_run => {
            let change = line_change("add", &sref, &key, None, Some(after));
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
    fields: &Fields,
    refs: &[String],
    series: Option<&str>,
) -> Result<Response> {
    refuse_under_rule(cli, "edit")?;
    let ctx = Ctx::open(cli)?;
    match record_form(&ctx, key)? {
        Form::Span => edit_span(cli, &ctx, key, values, fields, refs),
        Form::Event => edit_event(cli, &ctx, key, values, fields, refs, series),
    }
}

fn edit_span(
    cli: &Cli,
    ctx: &Ctx,
    slug: &str,
    values: &[String],
    fields: &Fields,
    refs: &[String],
) -> Result<Response> {
    if !values.is_empty() {
        return Err(Error::usage(format!(
            "a span is bounded, not valued: correct it with `--from`/`--to`/`--note`, and \
             {:?} names none of them (§8.4)",
            values.join(" ")
        )));
    }
    let (eref, before) = get_span(ctx, slug)?;
    let previous = before.data.as_span()?.clone();

    // `--note` given bare names the field the editor should open (§7.3).
    if matches!(fields.note, Some(None)) {
        return editor_form_span(ctx, &eref, &before, &previous);
    }

    let record = FastiRecord::Span(build_span(fields, Some(&previous))?);
    Fasti::validate(&record)?;
    let entity = Entity {
        refs: if refs.is_empty() {
            before.refs.clone()
        } else {
            parse_refs(refs)?
        },
        data: record,
    };
    // A span's kind cannot change: Fasti's other token is another *shape*, and `-k`
    // never converts one shape into another — a rename cannot change a file's
    // extension, and the extension is the shape (§5.2, §7.2).
    let addr = EntityAddr {
        home: eref.home.clone(),
        kind: eref.kind.clone(),
        slug: eref.slug.clone(),
    };
    let change = entity_change(
        "edit",
        &addr,
        Some(contract::entity_json(Fasti::NAME, &eref, &before)?),
        Some(addr_json(&addr, &entity)?),
        None,
    );
    if let Some(pending) = review(cli, &change)? {
        return Ok(pending);
    }
    ctx.store
        .write_entity(&addr, entity.refs.clone(), &entity.data)?;
    Ok(Response::Json(contract::entity_json(
        Fasti::NAME,
        &eref,
        &entity,
    )?))
}

fn edit_event(
    cli: &Cli,
    ctx: &Ctx,
    key: &str,
    values: &[String],
    fields: &Fields,
    refs: &[String],
    series: Option<&str>,
) -> Result<Response> {
    let key = Key::parse(key)?;
    let (sref, prev) = find_occurrence(ctx, cli, &key, series)?;
    let previous = prev.data.as_event()?.clone();

    // An `edit` given no new value is the editor form (§7.3).
    if values.is_empty() && fields.is_empty() && refs.is_empty() {
        return editor_form_event(ctx, &sref, &prev, &previous, &key);
    }

    let record = FastiRecord::Event(build_event(fields, values.to_vec(), Some(&previous)));
    Fasti::validate(&record)?;
    let line = Line {
        // The date key is the occurrence's own identity and never changes on an edit
        // (§5.4) — which is also why an event has no `rename`: only its series does.
        key: key.clone(),
        refs: if refs.is_empty() {
            prev.refs.clone()
        } else {
            parse_refs(refs)?
        },
        data: record,
    };
    let after = line_json(&sref, &line)?;
    let change = line_change(
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

/// Rename a span or an event series, cascading every ref pointing at it (§7.2, §5.4).
///
/// One verb over two shapes, and the cascade is the same one either way: it is the
/// spine's, touches only the envelope's `refs` array, and so no core reaches into
/// another core's records to run it (I5). Fasti passes **both** its tokens to
/// `plan_cascade`, which is what makes the occupied-slug refusal see across the shape
/// boundary — a span renamed onto an event series' name is refused, and the reverse.
fn cmd_rename(cli: &Cli, slug: &str, new: &str) -> Result<Response> {
    refuse_under_rule(cli, "rename")?;
    let ctx = Ctx::open(cli)?;
    let new = pantheon::name::normalize_token(new, "name")?;
    let form = target_form(&ctx, slug)?;

    let from = Ref::parse(&format!("{}:{}", Fasti::NAME, normalize_slug(slug)?))?;
    let to = Ref::parse(&format!("{}:{new}", Fasti::NAME))?;

    match form {
        Form::Span => {
            let (eref, entity) = get_span(&ctx, slug)?;
            refuse_entity_as_node(&eref, "rename")?;
            let cascade = pantheon::plan_cascade(ctx.store.root(), &Fasti::KINDS, &from, &to)?;
            let addr = EntityAddr {
                home: eref.home.clone(),
                kind: eref.kind.clone(),
                slug: new.clone(),
            };
            let change = entity_change(
                "rename",
                &addr,
                Some(contract::entity_json(Fasti::NAME, &eref, &entity)?),
                Some(addr_json(&addr, &entity)?),
                Some(cascade.to_json()),
            );
            if let Some(pending) = review(cli, &change)? {
                return Ok(pending);
            }
            // The record's own file moves first, so a crash mid-cascade leaves refs
            // dangling on the *old* slug — which `pan validate` reports naming exactly
            // the files that still need fixing (§5.4, §10.1).
            let moved = ctx.store.relocate_entity(&eref, &addr)?;
            cascade.apply(ctx.store.root())?;
            Ok(Response::Json(json!({
                "renamed": { "from": eref.slug, "to": new },
                "cascade": cascade.to_json(),
                "record": contract::entity_json(Fasti::NAME, &moved, &entity)?,
            })))
        }
        Form::Event => {
            let sref = ctx.store.locate(
                &normalize_slug(slug)?,
                Some(Fasti::EVENT),
                ctx.scope().as_ref(),
            )?;
            let cascade = pantheon::plan_cascade(ctx.store.root(), &Fasti::KINDS, &from, &to)?;
            let change = series_change("rename", &sref, &new, Some(cascade.to_json()));
            if let Some(pending) = review(cli, &change)? {
                return Ok(pending);
            }
            let moved = ctx.store.relocate_series(&sref, &sref.home, &new)?;
            cascade.apply(ctx.store.root())?;
            Ok(Response::Json(json!({
                "renamed": { "from": sref.label(), "to": new },
                "cascade": cascade.to_json(),
                "record": series_identity(&moved),
            })))
        }
    }
}

/// Re-home a span or an event series (§7.2). A file `mv` between meta dirs, touching no
/// refs — a ref carries no path, so it survives a re-home untouched (§5.4).
fn cmd_move(cli: &Cli, slug: &str, to: &str) -> Result<Response> {
    refuse_under_rule(cli, "move")?;
    let ctx = Ctx::open(cli)?;
    let home = Code::parse(to)?;
    match target_form(&ctx, slug)? {
        Form::Span => {
            let (eref, entity) = get_span(&ctx, slug)?;
            refuse_entity_as_node(&eref, "move")?;
            let addr = EntityAddr {
                home,
                kind: eref.kind.clone(),
                slug: eref.slug.clone(),
            };
            refuse_series_holding(&ctx, &addr.home, &addr.slug)?;
            if let Some(held) = ctx.store.slug_taken_at(&addr.home, &addr.slug)? {
                return Err(Error::validation(format!(
                    "{} already holds {:?} as a {} (§5.4)",
                    addr.home.as_str(),
                    addr.slug,
                    held.kind
                )));
            }
            let change = entity_change(
                "move",
                &addr,
                Some(contract::entity_json(Fasti::NAME, &eref, &entity)?),
                Some(addr_json(&addr, &entity)?),
                None,
            );
            if let Some(pending) = review(cli, &change)? {
                return Ok(pending);
            }
            // Fasti determines no series — its `event` is hand-named, not named for a
            // span — so nothing travels with the entity (§7.2, §8.3).
            let moved = ctx.store.relocate_entity(&eref, &addr)?;
            Ok(Response::Json(json!({
                "moved": { "from": eref.home.as_str(), "to": addr.home.as_str() },
                "record": contract::entity_json(Fasti::NAME, &moved, &entity)?,
            })))
        }
        Form::Event => {
            let sref = ctx.store.locate(
                &normalize_slug(slug)?,
                Some(Fasti::EVENT),
                ctx.scope().as_ref(),
            )?;
            refuse_span_holding(&ctx, &home, sref.label())?;
            let mut change = series_change("move", &sref, sref.label(), None);
            change.home = home.as_str().to_string();
            if let Some(pending) = review(cli, &change)? {
                return Ok(pending);
            }
            let moved = ctx.store.relocate_series(&sref, &home, sref.label())?;
            Ok(Response::Json(json!({
                "moved": { "from": sref.home.as_str(), "to": home.as_str() },
                "record": series_identity(&moved),
            })))
        }
    }
}

fn cmd_rm(cli: &Cli, key: &str, series: Option<&str>) -> Result<Response> {
    refuse_under_rule(cli, "rm")?;
    let ctx = Ctx::open(cli)?;
    match record_form(&ctx, key)? {
        Form::Span => {
            let (eref, entity) = get_span(&ctx, key)?;
            let addr = EntityAddr {
                home: eref.home.clone(),
                kind: eref.kind.clone(),
                slug: eref.slug.clone(),
            };
            let change = entity_change(
                "rm",
                &addr,
                Some(contract::entity_json(Fasti::NAME, &eref, &entity)?),
                None,
                None,
            );
            if let Some(pending) = review(cli, &change)? {
                return Ok(pending);
            }
            ctx.store.remove_entity(&eref)?;
            Ok(Response::Json(json!({ "deleted": eref.slug })))
        }
        Form::Event => {
            let key = Key::parse(key)?;
            let (sref, prev) = find_occurrence(&ctx, cli, &key, series)?;
            let change = line_change("rm", &sref, &key, Some(line_json(&sref, &prev)?), None);
            if let Some(pending) = review(cli, &change)? {
                return Ok(pending);
            }
            ctx.store.remove_line(&sref, &key)?;
            Ok(Response::Json(json!({ "deleted": key.as_str() })))
        }
    }
}

/// Every span, and every event series folded to its present (§7.2, I1). `-k` filters to
/// one shape; two shapes in one array is what a two-token core's fold *is*.
///
/// **`--unspanned` widens the fold to every occurrence, not just each series' latest**
/// (§8.4). It is a filter over the same walk, but not over `list`'s present: an event
/// with no span that is not its series' most recent line would be invisible to a
/// present-fold, so the set you asked to check would quietly omit its members — and a
/// check that can lie is worse than none, given §8.4 keeps this off the validator on
/// purpose. Nothing is stored either way; the set is derived on the frame you ask for it.
fn cmd_list(cli: &Cli, unspanned: bool) -> Result<Response> {
    let ctx = Ctx::open(cli)?;
    let locus = ctx.locus();

    if unspanned {
        if ctx.filter_kind() == Some(Fasti::SPAN) {
            return Err(Error::usage(
                "--unspanned asks which events reference no span, so it cannot also filter to \
                 `-k span` (§8.4)",
            ));
        }
        let spans = span_slugs(&ctx)?;
        let mut rows = Vec::new();
        for sref in ctx
            .store
            .find_series(locus.as_ref(), Some(Fasti::EVENT), None)?
        {
            for line in ctx.store.read_series(&sref)? {
                let line = checked_line(line)?;
                if !line
                    .refs
                    .iter()
                    .any(|r| r.core == Fasti::NAME && spans.contains(&r.slug))
                {
                    rows.push(line_json(&sref, &line)?);
                }
            }
        }
        return Ok(Response::Json(Value::Array(rows)));
    }

    let mut rows = Vec::new();
    if ctx.filter_kind() != Some(Fasti::EVENT) {
        let folded = ctx.store.fold_entities(locus.as_ref(), Some(Fasti::SPAN))?;
        for (eref, entity) in &folded {
            entity.data.as_span()?;
            rows.push(contract::entity_json(Fasti::NAME, eref, entity)?);
        }
    }
    if ctx.filter_kind() != Some(Fasti::SPAN) {
        let folded = ctx.store.fold(locus.as_ref(), Some(Fasti::EVENT))?;
        for present in &folded {
            present.line.data.as_event()?;
            rows.push(contract::present_json(Fasti::NAME, present)?);
        }
    }
    Ok(Response::Json(Value::Array(rows)))
}

fn cmd_get(cli: &Cli, tokens: &[String]) -> Result<Response> {
    let ctx = Ctx::open(cli)?;
    let name = sole_token(tokens, "get")?;
    match target_form(&ctx, &name)? {
        Form::Span => {
            let (eref, entity) = get_span(&ctx, &name)?;
            Ok(Response::Json(contract::entity_json(
                Fasti::NAME,
                &eref,
                &entity,
            )?))
        }
        Form::Event => {
            let present = ctx.store.get(
                &normalize_slug(&name)?,
                Some(Fasti::EVENT),
                ctx.scope().as_ref(),
            )?;
            present.line.data.as_event()?;
            Ok(Response::Json(contract::present_json(
                Fasti::NAME,
                &present,
            )?))
        }
    }
}

/// A whole event series: the occurrences across keys, optionally windowed (§7.2).
///
/// A span takes no `series` verb — it is one object, not a collection, so it is read
/// with `get` (§7.1). That refusal is the counterpart of Album's, which has no series
/// at all; Fasti has one, and it is the other token.
fn cmd_series(
    cli: &Cli,
    tokens: &[String],
    from: Option<&str>,
    to: Option<&str>,
) -> Result<Response> {
    let ctx = Ctx::open(cli)?;
    if ctx.filter_kind() == Some(Fasti::SPAN) {
        return Err(Error::usage(
            "a span is one object, not a collection: read it with `get` (§7.1, §7.2)",
        ));
    }
    let target = ctx.read_target(cli, tokens)?;
    let sref = target.existing.clone().ok_or_else(|| missing(&target))?;
    let mut lines = ctx
        .store
        .read_series(&sref)?
        .into_iter()
        .map(checked_line)
        .collect::<Result<Vec<_>>>()?;
    // A window is a filter on the collection, never a second verb (§7.2). A `--to` date
    // also admits that day's timed keys (`260719T1600` is within `--to 260719`).
    if let Some(from) = from {
        lines.retain(|l| l.key.as_str() >= from);
    }
    if let Some(to) = to {
        lines.retain(|l| l.key.as_str() <= to || l.key.as_str().starts_with(to));
    }
    Ok(Response::Json(contract::series_json(
        Fasti::NAME,
        &sref,
        &lines,
    )?))
}

fn cmd_where(cli: &Cli, tokens: &[String]) -> Result<Response> {
    let ctx = Ctx::open(cli)?;
    let name = sole_token(tokens, "where")?;
    let (mut out, path) = match target_form(&ctx, &name)? {
        Form::Span => {
            let eref = ctx.store.locate_entity(
                &normalize_slug(&name)?,
                Some(Fasti::SPAN),
                ctx.scope().as_ref(),
            )?;
            (entity_identity(&eref), eref.path.clone())
        }
        Form::Event => {
            let sref = ctx.store.locate(
                &normalize_slug(&name)?,
                Some(Fasti::EVENT),
                ctx.scope().as_ref(),
            )?;
            (series_identity(&sref), sref.path.clone())
        }
    };
    let rel = path
        .strip_prefix(&ctx.root)
        .unwrap_or(&path)
        .to_string_lossy()
        .into_owned();
    out["path"] = Value::String(rel);
    Ok(Response::Json(out))
}

// ── shared plumbing ─────────────────────────────────────────────────────────

struct Ctx {
    root: PathBuf,
    store: Store<Fasti>,
    /// The explicit `-k`, normalized and checked — `None` when none was given. Kept
    /// optional because `-k` means two things: on a write it is *checked* against the
    /// form the flags picked, on a read it *filters* (§7.2).
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
                // Both halves, since Fasti's two tokens sit in two shapes (§7.1).
                if !Store::<Fasti>::owns_entity_kind(&kind)
                    && !Store::<Fasti>::owns_series_kind(&kind)
                {
                    return Err(Error::usage(format!(
                        "fasti has no {kind:?} token; it declares {} (§7.1)",
                        Fasti::KINDS.join(", ")
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

    /// Which token a read filters by: `None` means both shapes (§7.2).
    fn filter_kind(&self) -> Option<&str> {
        self.kind.as_deref()
    }

    /// What a lookup is scoped to. `-H` narrows it; otherwise the whole tree, because a
    /// slug is unique **per core, not per node** (§5.4) — narrowing to $PWD would make
    /// `fas get mvp_phase` mean different periods in different directories.
    fn scope(&self) -> Option<Code> {
        self.home.clone()
    }

    /// What a fold is scoped to. Unlike a lookup this *is* the locus: `cd a_o_opus/ &&
    /// fas ls` lists what is placed there (§7.3). Outside the tree there is nothing to
    /// narrow by, so the fold spans the forest.
    fn locus(&self) -> Option<Code> {
        self.home
            .clone()
            .or_else(|| contract::code_at_path(&self.root, None).ok())
    }

    /// Resolve the event series a write means: its trailing tokens are the occurrence.
    fn write_target(
        &self,
        cli: &Cli,
        series: Option<&str>,
        tokens: &[String],
        create: bool,
    ) -> Result<SeriesTarget> {
        self.resolve(cli, series, tokens, create, true)
    }

    /// Resolve the event series a read means: it has no values, so a lone token names it.
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
                kind: Fasti::EVENT,
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

impl Fields {
    /// Whether a hand gave any field at all — what tells an `edit` that means the
    /// editor form from one that means a correction (§7.3).
    fn is_empty(&self) -> bool {
        self.from.is_none() && self.to.is_none() && self.until.is_none() && self.note.is_none()
    }

    /// The value a flag carried, or what the record already holds. A flag absent leaves
    /// the record alone — the stored record is the truth, not the command line (I1).
    fn note_or(&self, prev: Option<String>) -> Option<String> {
        match &self.note {
            Some(Some(value)) => Some(value.clone()),
            _ => prev,
        }
    }
}

/// Build the span a write means. What a hand does not give, the record keeps (I1).
fn build_span(fields: &Fields, prev: Option<&Span>) -> Result<Span> {
    let prev = prev.cloned().unwrap_or_default();
    let from = match &fields.from {
        Some(from) => from.clone(),
        None if prev.from.is_empty() => {
            return Err(Error::validation(
                "a span is a period, so it needs a `--from` (§8.4)",
            ));
        }
        None => prev.from,
    };
    Ok(Span {
        from,
        to: fields.to.clone().or(prev.to),
        note: fields.note_or(prev.note),
    })
}

/// Build the event a write means. What a hand does not give, the occurrence keeps (I1).
fn build_event(fields: &Fields, values: Vec<String>, prev: Option<&Event>) -> Event {
    let prev = prev.cloned().unwrap_or_default();
    Event {
        values: if values.is_empty() {
            prev.values
        } else {
            values
        },
        until: fields.until.clone().or(prev.until),
        note: fields.note_or(prev.note),
    }
}

/// The span already at the target address, read back so an overwrite keeps what the
/// hand did not restate (I1).
fn previous_span(ctx: &Ctx, target: &contract::EntityTarget) -> Result<Option<Span>> {
    match &target.existing {
        Some(held) => Ok(Some(ctx.store.read_entity(held)?.data.as_span()?.clone())),
        None => Ok(None),
    }
}

/// `get_entity` with the guard §5.2 requires: the **filename** says `span`, so the body
/// must read as one, and a file where the two disagree is refused rather than emitted
/// as the shape it is not (exit `3`, §6.4, §13).
///
/// Every verb that reads a span goes through here. The untagged enum will happily read
/// an event body out of a `__span__` file — that is what "dispatch type, not a disk
/// format" costs — so the check belongs at the one door rather than at each caller,
/// where the next verb added would forget it.
fn get_span(ctx: &Ctx, slug: &str) -> Result<(EntityRef, Entity<FastiRecord>)> {
    let (eref, entity) = ctx.store.get_entity(
        &normalize_slug(slug)?,
        Some(Fasti::SPAN),
        ctx.scope().as_ref(),
    )?;
    entity.data.as_span()?;
    Ok((eref, entity))
}

/// The same guard for a series line: the filename says `event`, so the body must read
/// as one (§5.2).
fn checked_line(line: Line<FastiRecord>) -> Result<Line<FastiRecord>> {
    line.data.as_event()?;
    Ok(line)
}

/// Find the occurrence a key names, and the series it sits in — `edit`'s lookup and
/// `rm`'s, which are the same lookup and so are written once.
///
/// With `--series` named, §7.3's four forms resolve the collection and the key indexes
/// into it, exactly as Annales does. Without one the key finds its own line: **the one
/// lookup that opens record files** rather than resting on their names (§5.0), listing
/// its candidate homes rather than guessing where more than one answers (§7.3).
fn find_occurrence(
    ctx: &Ctx,
    cli: &Cli,
    key: &Key,
    series: Option<&str>,
) -> Result<(SeriesRef, Line<FastiRecord>)> {
    if series.is_some() {
        let target = ctx.write_target(cli, series, &[], false)?;
        let sref = target.existing.clone().ok_or_else(|| missing(&target))?;
        let line = ctx
            .store
            .read_series(&sref)?
            .into_iter()
            .find(|l| l.key == *key)
            .ok_or_else(|| no_line(&sref, key))
            .and_then(checked_line)?;
        return Ok((sref, line));
    }
    let (sref, line) = ctx
        .store
        .locate_line(key, Some(Fasti::EVENT), ctx.scope().as_ref())?;
    Ok((sref, checked_line(line)?))
}

/// Every span slug in the tree — the set `--unspanned` measures an event's refs against
/// (§8.4).
///
/// Fasti's **own** files, read by filename alone (§5.0). A `fasti:` ref that names an
/// event series rather than a span does not span anything, which is why this is a set of
/// spans and not of every Fasti name.
fn span_slugs(ctx: &Ctx) -> Result<HashSet<String>> {
    Ok(ctx
        .store
        .find_entities(None, Some(Fasti::SPAN), None)?
        .into_iter()
        .map(|eref| eref.slug)
        .collect())
}

/// Refuse a slug this node already holds as an **event series** (§5.4, §18).
///
/// One `exists`, not a walk: a series' location is settled by its home, its token, and
/// its name, so this is the cheap within-node half of §5.4's two-tier uniqueness — the
/// series-shaped counterpart of [`Store::slug_taken_at`], which sees entity files only.
///
/// [`Store::slug_taken_at`]: pantheon::Store::slug_taken_at
fn refuse_series_holding(ctx: &Ctx, home: &Code, slug: &str) -> Result<()> {
    if ctx
        .store
        .series_path(home, Fasti::EVENT, Some(slug))?
        .exists()
    {
        return Err(Error::validation(format!(
            "{} already holds {slug:?} as an event series: two shapes spell two files but only \
             one `fasti:{slug}`, so the ref would be ambiguous (§5.4, §18)",
            home.as_str()
        )));
    }
    Ok(())
}

/// Refuse a name this node already holds as a **span** — the same check from the other
/// side, made where a series is minted or re-homed (§5.4, §18).
fn refuse_span_holding(ctx: &Ctx, home: &Code, name: &str) -> Result<()> {
    if let Some(held) = ctx.store.slug_taken_at(home, name)? {
        return Err(Error::validation(format!(
            "{} already holds {name:?} as a {}: two shapes spell two files but only one \
             `fasti:{name}`, so the ref would be ambiguous (§5.4, §18)",
            home.as_str(),
            held.kind
        )));
    }
    Ok(())
}

/// The editor form of a span's `edit` (§7.3): `--note` given no value opens that value,
/// and the session *is* the review — it mints no plan token and needs no `-y`, because
/// the hand is already looking at the thing it is changing.
fn editor_form_span(
    ctx: &Ctx,
    eref: &EntityRef,
    before: &Entity<FastiRecord>,
    previous: &Span,
) -> Result<Response> {
    // Piped, it spawns nothing and prints the file's path, by the same law that sends a
    // table to a TTY and JSON down a pipe: the LLM hand gets a path to open with its own
    // tools rather than a blocked process it cannot drive (I8).
    if !contract::stdout_is_terminal() {
        return Ok(Response::Json(
            json!({ "path": eref.path.display().to_string() }),
        ));
    }
    // What opens holds **only that value** — never the raw JSON, which is machine-owned
    // and is never handed to a hand raw (I6, §6.6, §7.3).
    let initial = format!("{}\n", previous.note.clone().unwrap_or_default());
    match contract::edit_text(&initial)? {
        contract::Edited::Unchanged => Ok(Response::Json(contract::entity_json(
            Fasti::NAME,
            eref,
            before,
        )?)),
        contract::Edited::Changed(text) => {
            let trimmed = text.trim();
            let mut span = previous.clone();
            span.note = (!trimmed.is_empty()).then(|| trimmed.to_string());
            let record = FastiRecord::Span(span);
            // Text that comes back invalid exits 3 (§7.3).
            Fasti::validate(&record)?;
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
                Fasti::NAME,
                eref,
                &entity,
            )?))
        }
    }
}

/// The editor form of an occurrence's `edit` (§7.3), over its values — Annales' shape,
/// for the same reason: a series line opens a buffer holding only its value, one per
/// line, never the raw JSONL (I6, §6.6).
fn editor_form_event(
    ctx: &Ctx,
    sref: &SeriesRef,
    prev: &Line<FastiRecord>,
    previous: &Event,
    key: &Key,
) -> Result<Response> {
    if !contract::stdout_is_terminal() {
        return Ok(Response::Json(
            json!({ "path": sref.path.display().to_string() }),
        ));
    }
    // An occurrence's values are not typed tokens, so they are kept verbatim rather than
    // normalized (§5.1).
    let initial = format!("{}\n", previous.values.join("\n"));
    match contract::edit_text(&initial)? {
        contract::Edited::Unchanged => Ok(Response::Json(line_json(sref, prev)?)),
        contract::Edited::Changed(text) => {
            let values = text
                .lines()
                .map(str::trim_end)
                .filter(|line| !line.trim().is_empty())
                .map(str::to_string)
                .collect();
            let record = FastiRecord::Event(Event {
                values,
                until: previous.until.clone(),
                note: previous.note.clone(),
            });
            Fasti::validate(&record)?;
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

/// A name held at another node is a *soft* finding: the check is a tree walk, which is
/// the cost the softness exists to avoid (§5.4, §18). The record still goes to stdout;
/// the warning rides stderr in the same shape `pan validate` emits, so a machine hand
/// reads one shape from both surfaces (I4, I8).
///
/// It looks for **both shapes**, because both land in one `fasti:<slug>` namespace: a
/// span elsewhere and an event series elsewhere are the same ambiguity to a ref, so the
/// soft half of the check is symmetric with the hard half.
fn warn_duplicates(ctx: &Ctx, written: &EntityRef) -> Result<()> {
    let mut elsewhere: Vec<(Code, PathBuf)> = ctx
        .store
        .duplicate_slugs_elsewhere(&written.home, &written.slug)?
        .into_iter()
        .map(|other| (other.home, other.path))
        .collect();
    elsewhere.extend(
        ctx.store
            .find_series(None, Some(Fasti::EVENT), Some(&written.slug))?
            .into_iter()
            .filter(|s| s.home.as_str() != written.home.as_str())
            .map(|other| (other.home, other.path)),
    );
    if elsewhere.is_empty() {
        return Ok(());
    }
    let findings: Vec<Finding> = elsewhere
        .iter()
        .map(|(home, path)| Finding {
            code: FindingCode::DuplicateSlug,
            severity: Severity::Warning,
            rel_path: path.strip_prefix(&ctx.root).unwrap_or(path).to_path_buf(),
            msg: format!(
                "{}:{} also names a record at {} — a ref meeting both lists them rather than \
                 guessing; a fuller name tells them apart (§5.4, §7.3)",
                Fasti::NAME,
                written.slug,
                home.as_str()
            ),
        })
        .collect();
    eprintln!("{}", findings_json(&findings));
    Ok(())
}

/// A write verb is refused outright under `PANTHEON_RULE=1` (exit `6`, §9.3): the one
/// reactive writer is Auspex, and a rule may not borrow a hand's authority (I2).
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

/// A span's change. `series` is `None`: a partitioned entity sits in no collection, and
/// a hollow key would read as one withheld (§7.1).
fn entity_change(
    verb: &'static str,
    addr: &EntityAddr,
    before: Option<Value>,
    after: Option<Value>,
    cascade: Option<Value>,
) -> RecordChange {
    RecordChange {
        verb,
        core: Fasti::NAME.to_string(),
        home: addr.home.as_str().to_string(),
        kind: addr.kind.clone(),
        series: None,
        // A partitioned entity stores no key — its *name* is the key (§5.4, §18).
        key: addr.slug.clone(),
        before,
        after,
        cascade,
    }
}

/// One occurrence's change.
fn line_change(
    verb: &'static str,
    sref: &SeriesRef,
    key: &Key,
    before: Option<Value>,
    after: Option<Value>,
) -> RecordChange {
    RecordChange {
        verb,
        core: Fasti::NAME.to_string(),
        home: sref.home.as_str().to_string(),
        kind: sref.kind.clone(),
        series: sref.name.clone(),
        key: key.to_string(),
        before,
        after,
        cascade: None,
    }
}

/// The change a structural verb on a *series* reviews. Unlike a line write there is no
/// record body to show — the series' identity *is* what moves — so `before`/`after`
/// carry the identity rather than an occurrence (§7.3).
fn series_change(
    verb: &'static str,
    sref: &SeriesRef,
    new: &str,
    cascade: Option<Value>,
) -> RecordChange {
    let mut after = series_identity(sref);
    after["series"] = Value::String(new.to_string());
    RecordChange {
        verb,
        core: Fasti::NAME.to_string(),
        home: sref.home.as_str().to_string(),
        kind: sref.kind.clone(),
        series: sref.name.clone(),
        // A named series' key is its name — the thing a ref points at (§5.4).
        key: sref.label().to_string(),
        before: Some(series_identity(sref)),
        after: Some(after),
        cascade,
    }
}

/// The contract JSON for a span not yet on disk — an `add`'s result, or a `rename`'s
/// destination. The same shape [`contract::entity_json`] emits.
fn addr_json(addr: &EntityAddr, entity: &Entity<FastiRecord>) -> Result<Value> {
    Ok(json!({
        "core": Fasti::NAME,
        "home": addr.home.as_str(),
        "kind": addr.kind,
        "slug": addr.slug,
        "refs": entity.refs.iter().map(Ref::to_token).collect::<Vec<_>>(),
        "data": serde_json::to_value(&entity.data)?,
    }))
}

fn line_json(sref: &SeriesRef, line: &Line<FastiRecord>) -> Result<Value> {
    contract::line_json(
        Fasti::NAME,
        &sref.home,
        &sref.kind,
        sref.name.as_deref(),
        line,
    )
}

fn entity_identity(eref: &EntityRef) -> Value {
    json!({
        "core": Fasti::NAME,
        "home": eref.home.as_str(),
        "kind": eref.kind,
        "slug": eref.slug,
    })
}

fn series_identity(sref: &SeriesRef) -> Value {
    json!({
        "core": Fasti::NAME,
        "home": sref.home.as_str(),
        "kind": sref.kind,
        "series": sref.label(),
    })
}

/// A read verb that names one record takes exactly one token — total on arity for
/// [`contract::resolve_entity_target`]'s reason: a name that quietly became its first
/// word would be the wrong record forever (§7.3).
fn sole_token(tokens: &[String], verb: &str) -> Result<String> {
    match tokens {
        [one] => Ok(one.clone()),
        [] => Err(Error::usage(format!(
            "name the span or event series to `{verb}` (§7.3)"
        ))),
        many => Err(Error::usage(format!(
            "a name is one token, and {} were given (§5.1, §7.3)",
            many.len()
        ))),
    }
}

/// A slug given on the command line is a typed token, so it is normalized on the way in
/// — `fas get "MVP Phase"` finds `mvp_phase` (§5.1).
fn normalize_slug(raw: &str) -> Result<String> {
    pantheon::name::normalize_token(raw, "name")
}

fn parse_refs(refs: &[String]) -> Result<Vec<Ref>> {
    refs.iter().map(|r| Ref::parse(r)).collect()
}

fn missing(target: &SeriesTarget) -> Error {
    Error::not_found(format!(
        "no {} event series {:?} at {} — mint it with -c (§7.3)",
        Fasti::NAME,
        target.name,
        target.home.as_str()
    ))
}

fn no_line(sref: &SeriesRef, key: &Key) -> Error {
    Error::not_found(format!(
        "no occurrence keyed {key} in series {:?} at {} (§7.2)",
        sref.label(),
        sref.home.as_str()
    ))
}

fn version_json() -> Value {
    json!({
        "name": Fasti::NAME,
        "short": "fas",
        "version": env!("CARGO_PKG_VERSION"),
        "format_version": 1,
    })
}

fn help_json() -> Value {
    json!({
        "name": Fasti::NAME,
        "short": "fas",
        "about": "the placement tense: spans you are inside of and events on the timeline (§8.4)",
        "verbs": VERBS,
        "kinds": Fasti::KINDS,
        "version": env!("CARGO_PKG_VERSION"),
    })
}
