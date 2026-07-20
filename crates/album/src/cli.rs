//! `alb` — Album's CLI (§7). stdout is JSON when piped, a table on a TTY (§7.3).
//!
//! The bin owns only what is Album's own: its positionals (`[home] <name>`) and the
//! flags its primitive needs (`--gender`, `--closeness`, `--role`, `--origin`,
//! `--away`, `--note`, `-r`). Everything downstream — reading the hand, confirming a
//! mutation, *finding* a home and a slug, planning the rename cascade, shaping a
//! record into the contract's JSON — is `pantheon`, so every core produces that JSON
//! the same way (I4) and no core reaches into another's records (I5).

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

use crate::{Agent, Album, Away};

use pantheon::envelope::{Entity, Ref};
use pantheon::validate::{Finding, FindingCode, Severity, findings_json};
use pantheon::{
    Checkpoint, Code, Core, EntityAddr, EntityForm, EntityRef, Error, RecordChange, Response,
    Result, Store, contract, resolve_root,
};

/// The twelve verbs (§7.3). A closed reserved set: a verb wins over a node code,
/// which is what makes `add` safe to leave implicit (the ambiguity rule, §7.3).
const VERBS: &[&str] = &[
    "add", "edit", "rename", "move", "mv", "rm", "list", "ls", "get", "series", "where", "schema",
    "help", "version",
];

/// What a headless build prints for a bare short (§14, §7.3).
#[cfg(not(feature = "tui"))]
const BARE: &str = "alb — Album (societas · who). Built without the `tui` feature; run `alb --help` for the verbs.\n";

#[derive(Parser)]
#[command(
    name = "alb",
    version,
    about = "Album — societas agents: the people, organizations, and groups you know (§8.1).",
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
    /// Which of the core's tokens (§7.2): `person`, `organization`, `group`.
    /// On a write it selects; on a read it filters.
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

/// Album's own fields, shared by `add` and `edit` (§8.1).
///
/// Each text field takes an **optional** value: given bare, it names the field the
/// editor form should open (§7.3). `--away` is repeatable, since away periods
/// accumulate in the record rather than becoming a series (§6.1).
// `Option<Option<String>>` is deliberate, and is the case the lint itself carves out:
// three states genuinely differ here. Absent leaves the record alone (I1), `--note`
// bare is the editor form (§7.3), and `--note TEXT` replaces. Collapsing any two
// would cost a verb form the spec names.
#[allow(clippy::option_option)]
#[derive(clap::Args, Default)]
struct Fields {
    /// What a form of address is derived from — never the address itself (§8.1).
    #[arg(long = "gender", value_name = "G", num_args = 0..=1)]
    gender: Option<Option<String>>,
    /// How close the bond is.
    #[arg(long = "closeness", value_name = "C", num_args = 0..=1)]
    closeness: Option<Option<String>>,
    /// What they are to you.
    #[arg(long = "role", value_name = "R", num_args = 0..=1)]
    role: Option<Option<String>>,
    /// Where you met them — provenance, never a home (I3).
    #[arg(long = "origin", value_name = "O", num_args = 0..=1)]
    origin: Option<Option<String>>,
    /// A period away, `FROM` or `FROM..TO` in YYMMDD; repeatable (§6.1).
    #[arg(long = "away", value_name = "PERIOD")]
    away: Vec<String>,
    /// A hand's remark on this agent.
    #[arg(long = "note", value_name = "TEXT", num_args = 0..=1)]
    note: Option<Option<String>>,
}

#[derive(Subcommand)]
enum Cmd {
    /// File an agent — a person, an organization, or a group (§8.1).
    ///
    /// Tokens are `[home] <name>`: `alb csa "John Appleseed"` · `alb john_appleseed`
    /// (homed at $PWD). A partitioned entity needs no prior container — the record
    /// `add` creates *is* the entity (§18).
    Add {
        tokens: Vec<String>,
        #[command(flatten)]
        fields: Fields,
        /// Attach a reference; repeatable (§5.4). A membership is a ref (§8.1).
        #[arg(short = 'r', long = "ref", value_name = "REF")]
        refs: Vec<String>,
        /// Accepted and refused: Album keeps no series to mint (§7.1).
        #[arg(short = 'c', long = "create")]
        create: bool,
        /// Accepted and refused: an entity has no key to date (§7.1).
        #[arg(short = 'a', long = "at", value_name = "WHEN")]
        at: Option<String>,
    },
    /// Correct an agent in place, by slug (§7.2). What a hand does not give, the
    /// record keeps (I1).
    ///
    /// `-k` changes what the agent fundamentally *is*, which renames the file — a
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
    /// Rename an agent and cascade every ref pointing at it (§7.2, §5.4).
    ///
    /// A name and its slug are one thing, so this is its own verb rather than a flag
    /// on `edit`: rewriting refs across the tree is a structural act and reads as one.
    Rename { slug: String, new: String },
    /// Re-home an agent to another node (§7.2). A file `mv` between meta dirs —
    /// refs carry no path, so none of them changes (§5.4).
    #[command(alias = "mv")]
    Move {
        slug: String,
        #[arg(long = "to", value_name = "CODE")]
        to: String,
    },
    /// Remove an agent by slug — irreversible (§7.2, §18).
    Rm { slug: String },
    /// Every agent across the subtree (§7.2). `-k` filters.
    #[command(alias = "ls")]
    List,
    /// One agent by slug (§7.2).
    Get { slug: String },
    /// Accepted and refused: Album's tokens are all partitioned (§7.1).
    Series { tokens: Vec<String> },
    /// Resolve a slug to its home code, by walking Album's own files (§7.3).
    Where { slug: String },
    /// Self-description: name, tokens and shapes, record schema, format version (§7.2).
    Schema,
    /// This tool's name, short, and version, as JSON (§7.3).
    Version,
    /// The verbs, as JSON (§7.3).
    Help,
}

/// Run `alb` exactly as the binary runs it (§7.3) — parse `argv`, dispatch, and
/// return the process's exit code. The bin is a shell over this and holds nothing of
/// its own.
#[must_use]
pub fn run_cli() -> ExitCode {
    let cli = Cli::parse_from(with_default_verb(std::env::args_os()));
    let as_json = contract::format_is_json(cli.format.map(|f| matches!(f, Format::Json)));
    contract::dispatch(run(&cli, as_json), as_json)
}

/// The flags that take a separate value — what the verb scan must step over to find
/// the first *word* on the line. Only the globals appear here: Album's field flags
/// take an optional value, and none of them may precede the verb anyway.
const VALUE_FLAGS: &[&str] = &[
    "-C", "--root", "-f", "--format", "-p", "--plan", "-H", "--home", "-k", "--kind",
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
            fields,
            refs,
            create,
            at,
        } => {
            // Taken by clap and refused here, so the refusal wears the contract's
            // error envelope rather than clap's own message (I4, §7.3).
            if *create {
                return Err(Error::usage(
                    "album keeps no series, so -c mints nothing (§7.1, §7.3)",
                ));
            }
            if at.is_some() {
                return Err(Error::usage(
                    "album keeps no series, so -a keys nothing: an entity is not a sample, \
                     and its name is its key (§5.4, §7.1)",
                ));
            }
            cmd_add(cli, tokens, fields, refs)
        }
        Cmd::Edit { slug, fields, refs } => cmd_edit(cli, slug, fields, refs),
        Cmd::Rename { slug, new } => cmd_rename(cli, slug, new),
        Cmd::Move { slug, to } => cmd_move(cli, slug, to),
        Cmd::Rm { slug } => cmd_rm(cli, slug),
        Cmd::List => cmd_list(cli),
        Cmd::Get { slug } => cmd_get(cli, slug),
        Cmd::Series { .. } => Err(Error::usage(
            "album keeps no series: its tokens are all partitioned, so an agent is read \
             with `get` or `list` (§7.1, §8.1)",
        )),
        Cmd::Where { slug } => cmd_where(cli, slug),
        Cmd::Schema => Ok(Response::Json(serde_json::to_value(pantheon::schema::<
            Album,
        >(1))?)),
        Cmd::Version => Ok(Response::Json(version_json())),
        Cmd::Help => Ok(Response::Json(help_json())),
    }
}

// ── the verbs ───────────────────────────────────────────────────────────────

fn cmd_add(cli: &Cli, tokens: &[String], fields: &Fields, refs: &[String]) -> Result<Response> {
    refuse_under_rule(cli, "add")?;
    let ctx = Ctx::open(cli)?;
    let kind = ctx.write_kind();
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
             `album:{}`, so the ref would be ambiguous (§5.4, §18)",
            target.home.as_str(),
            target.slug,
            held.kind,
            target.slug
        )));
    }

    let record = build_record(fields, None)?;
    Album::validate(&record)?;
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
            let change = change(
                "add",
                &addr,
                Some(contract::entity_json(Album::NAME, held, &before)?),
                Some(after.clone()),
                None,
            );
            if let Some(pending) = review(cli, &change)? {
                return Ok(pending);
            }
        }
        // Every write verb takes `--dry-run` (§7.2), fresh or not.
        None if cli.dry_run => {
            let change = change("add", &addr, None, Some(after), None);
            return Ok(Response::Json(change.to_json()));
        }
        None => {}
    }

    let written = ctx
        .store
        .write_entity(&addr, entity.refs.clone(), &entity.data)?;
    // Across nodes the check is a walk, so it stays soft: the record itself goes to
    // stdout, the warning to stderr (§5.4, §18).
    warn_duplicates(&ctx, &written)?;
    Ok(Response::Json(contract::entity_json(
        Album::NAME,
        &written,
        &entity,
    )?))
}

fn cmd_edit(cli: &Cli, slug: &str, fields: &Fields, refs: &[String]) -> Result<Response> {
    refuse_under_rule(cli, "edit")?;
    let ctx = Ctx::open(cli)?;
    // `-k` on an edit names the agent's *new* kind, so the lookup must not filter by
    // it — you are correcting what it is, not restating what it was.
    let (eref, before) =
        ctx.store
            .get_entity(&normalize_slug(slug)?, None, ctx.scope().as_ref())?;

    // A field flag given bare names the field the editor should open (§7.3).
    if let Some(field) = fields.bare_field()? {
        return editor_form(&ctx, &eref, &before, field);
    }

    let record = build_record(fields, Some(&before.data))?;
    Album::validate(&record)?;
    let entity = Entity {
        refs: if refs.is_empty() {
            before.refs.clone()
        } else {
            parse_refs(refs)?
        },
        data: record,
    };
    // Changing what an entity *is* renames the file (§7.2).
    let addr = EntityAddr {
        home: eref.home.clone(),
        kind: ctx.kind.clone().unwrap_or_else(|| eref.kind.clone()),
        slug: eref.slug.clone(),
    };
    let after = addr_json(&addr, &entity)?;
    let change = change(
        "edit",
        &addr,
        Some(contract::entity_json(Album::NAME, &eref, &before)?),
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
        Album::NAME,
        &eref,
        &entity,
    )?))
}

fn cmd_rename(cli: &Cli, slug: &str, new: &str) -> Result<Response> {
    refuse_under_rule(cli, "rename")?;
    let ctx = Ctx::open(cli)?;
    let (eref, entity) = ctx.store.get_entity(
        &normalize_slug(slug)?,
        ctx.filter_kind(),
        ctx.scope().as_ref(),
    )?;
    refuse_entity_as_node(&eref, "rename")?;
    let new = pantheon::name::normalize_token(new, "name")?;

    // The walk that finds the refs is the walk that finds an occupied slug (§5.4).
    let from = Ref::parse(&format!("{}:{}", Album::NAME, eref.slug))?;
    let to = Ref::parse(&format!("{}:{new}", Album::NAME))?;
    let cascade = pantheon::plan_cascade(ctx.store.root(), &Album::KINDS, &from, &to)?;

    let addr = EntityAddr {
        home: eref.home.clone(),
        kind: eref.kind.clone(),
        slug: new.clone(),
    };
    let change = change(
        "rename",
        &addr,
        Some(contract::entity_json(Album::NAME, &eref, &entity)?),
        Some(addr_json(&addr, &entity)?),
        Some(cascade.to_json()),
    );
    if let Some(pending) = review(cli, &change)? {
        return Ok(pending);
    }

    // The record's own file moves first, so a crash mid-cascade leaves refs dangling
    // on the *old* slug — which `pan validate` reports naming exactly the files that
    // still need fixing (§5.4, §10.1).
    let moved = ctx.store.relocate_entity(&eref, &addr)?;
    cascade.apply(ctx.store.root())?;
    Ok(Response::Json(json!({
        "renamed": { "from": eref.slug, "to": new },
        "cascade": cascade.to_json(),
        "record": contract::entity_json(Album::NAME, &moved, &entity)?,
    })))
}

fn cmd_move(cli: &Cli, slug: &str, to: &str) -> Result<Response> {
    refuse_under_rule(cli, "move")?;
    let ctx = Ctx::open(cli)?;
    let (eref, entity) = ctx.store.get_entity(
        &normalize_slug(slug)?,
        ctx.filter_kind(),
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

    let change = change(
        "move",
        &addr,
        Some(contract::entity_json(Album::NAME, &eref, &entity)?),
        Some(addr_json(&addr, &entity)?),
        None,
    );
    if let Some(pending) = review(cli, &change)? {
        return Ok(pending);
    }
    // No ref changes: a ref carries no path, so it survives a re-home untouched
    // (§5.4). Album determines no series, so nothing travels with the entity (§7.2).
    let moved = ctx.store.relocate_entity(&eref, &addr)?;
    Ok(Response::Json(json!({
        "moved": { "from": eref.home.as_str(), "to": addr.home.as_str() },
        "record": contract::entity_json(Album::NAME, &moved, &entity)?,
    })))
}

fn cmd_rm(cli: &Cli, slug: &str) -> Result<Response> {
    refuse_under_rule(cli, "rm")?;
    let ctx = Ctx::open(cli)?;
    let (eref, entity) = ctx.store.get_entity(
        &normalize_slug(slug)?,
        ctx.filter_kind(),
        ctx.scope().as_ref(),
    )?;
    let addr = EntityAddr {
        home: eref.home.clone(),
        kind: eref.kind.clone(),
        slug: eref.slug.clone(),
    };
    let change = change(
        "rm",
        &addr,
        Some(contract::entity_json(Album::NAME, &eref, &entity)?),
        None,
        None,
    );
    if let Some(pending) = review(cli, &change)? {
        return Ok(pending);
    }
    ctx.store.remove_entity(&eref)?;
    Ok(Response::Json(json!({ "deleted": eref.slug })))
}

fn cmd_list(cli: &Cli) -> Result<Response> {
    let ctx = Ctx::open(cli)?;
    let folded = ctx
        .store
        .fold_entities(ctx.locus().as_ref(), ctx.filter_kind())?;
    Ok(Response::Json(contract::entity_fold_json(
        Album::NAME,
        &folded,
    )?))
}

fn cmd_get(cli: &Cli, slug: &str) -> Result<Response> {
    let ctx = Ctx::open(cli)?;
    let (eref, entity) = ctx.store.get_entity(
        &normalize_slug(slug)?,
        ctx.filter_kind(),
        ctx.scope().as_ref(),
    )?;
    Ok(Response::Json(contract::entity_json(
        Album::NAME,
        &eref,
        &entity,
    )?))
}

fn cmd_where(cli: &Cli, slug: &str) -> Result<Response> {
    let ctx = Ctx::open(cli)?;
    let eref = ctx.store.locate_entity(
        &normalize_slug(slug)?,
        ctx.filter_kind(),
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

// ── shared plumbing ─────────────────────────────────────────────────────────

struct Ctx {
    root: PathBuf,
    store: Store<Album>,
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
                if !Store::<Album>::owns_entity_kind(&kind) {
                    return Err(Error::usage(format!(
                        "album has no {kind:?} token; it declares {} (§7.1)",
                        Album::KINDS.join(", ")
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

    /// Which token a write files under: the explicit `-k`, else `person` — hardcoded,
    /// never a setting (§18).
    fn write_kind(&self) -> String {
        self.kind
            .clone()
            .unwrap_or_else(|| Album::DEFAULT_KIND.to_string())
    }

    /// Which token a read filters by: `None` means all three (§7.2).
    fn filter_kind(&self) -> Option<&str> {
        self.kind.as_deref()
    }

    /// What a slug lookup is scoped to. `-H` narrows it; otherwise the whole tree,
    /// because a slug is unique **per core, not per node** (§5.4) — narrowing to
    /// $PWD would make `alb get mara` mean different people in different directories.
    fn scope(&self) -> Option<Code> {
        self.home.clone()
    }

    /// What a fold is scoped to. Unlike a lookup this *is* the locus: `cd
    /// cs_a_amicitia/ && alb ls` lists the friends filed there (§7.3). Outside the
    /// tree there is nothing to narrow by, so the fold spans the forest.
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
            ("gender", &self.gender),
            ("closeness", &self.closeness),
            ("role", &self.role),
            ("origin", &self.origin),
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

    /// A flag given with a value replaces; a flag absent leaves what the record
    /// already holds (I1). A bare flag is the editor form and never reaches here.
    fn given(field: Option<&Option<String>>, prev: Option<String>) -> Option<String> {
        match field {
            Some(Some(value)) => Some(value.clone()),
            _ => prev,
        }
    }
}

/// Build the record a write means. What a hand does not give, the record keeps —
/// the stored record is the truth, not the command line (I1).
fn build_record(fields: &Fields, prev: Option<&Agent>) -> Result<Agent> {
    let prev = prev.cloned().unwrap_or_default();
    let away = if fields.away.is_empty() {
        prev.away
    } else {
        fields
            .away
            .iter()
            .map(|p| parse_away(p))
            .collect::<Result<Vec<_>>>()?
    };
    Ok(Agent {
        gender: Fields::given(fields.gender.as_ref(), prev.gender),
        closeness: Fields::given(fields.closeness.as_ref(), prev.closeness),
        role: Fields::given(fields.role.as_ref(), prev.role),
        origin: Fields::given(fields.origin.as_ref(), prev.origin),
        away,
        note: Fields::given(fields.note.as_ref(), prev.note),
    })
}

/// `FROM` or `FROM..TO` (§6.1). An open period simply has no `to`.
fn parse_away(raw: &str) -> Result<Away> {
    let (from, to) = match raw.split_once("..") {
        Some((from, "")) => (from, None),
        Some((from, to)) => (from, Some(to.to_string())),
        None => (raw, None),
    };
    if from.is_empty() {
        return Err(Error::usage(
            "an away period needs a start: --away FROM or --away FROM..TO (§8.1)",
        ));
    }
    Ok(Away {
        from: from.to_string(),
        to,
    })
}

/// The editor form of `edit` (§7.3): a field named with no value opens that value,
/// and the session *is* the review — it mints no plan token and needs no `-y`,
/// because the hand is already looking at the thing it is changing.
fn editor_form(
    ctx: &Ctx,
    eref: &EntityRef,
    before: &Entity<Agent>,
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
    let initial = format!("{}\n", read_field(&before.data, field).unwrap_or_default());
    match contract::edit_text(&initial)? {
        // Text that comes back unchanged writes nothing (§7.3).
        contract::Edited::Unchanged => Ok(Response::Json(contract::entity_json(
            Album::NAME,
            eref,
            before,
        )?)),
        contract::Edited::Changed(text) => {
            let trimmed = text.trim();
            let value = (!trimmed.is_empty()).then(|| trimmed.to_string());
            let mut data = before.data.clone();
            write_field(&mut data, field, value);
            // Text that comes back invalid exits 3 (§7.3).
            Album::validate(&data)?;
            let entity = Entity {
                refs: before.refs.clone(),
                data,
            };
            let addr = EntityAddr {
                home: eref.home.clone(),
                kind: eref.kind.clone(),
                slug: eref.slug.clone(),
            };
            ctx.store
                .write_entity(&addr, entity.refs.clone(), &entity.data)?;
            Ok(Response::Json(contract::entity_json(
                Album::NAME,
                eref,
                &entity,
            )?))
        }
    }
}

fn read_field(agent: &Agent, field: &str) -> Option<String> {
    match field {
        "gender" => agent.gender.clone(),
        "closeness" => agent.closeness.clone(),
        "role" => agent.role.clone(),
        "origin" => agent.origin.clone(),
        _ => agent.note.clone(),
    }
}

fn write_field(agent: &mut Agent, field: &str, value: Option<String>) {
    match field {
        "gender" => agent.gender = value,
        "closeness" => agent.closeness = value,
        "role" => agent.role = value,
        "origin" => agent.origin = value,
        _ => agent.note = value,
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
                Album::NAME,
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

fn change(
    verb: &'static str,
    addr: &EntityAddr,
    before: Option<Value>,
    after: Option<Value>,
    cascade: Option<Value>,
) -> RecordChange {
    RecordChange {
        verb,
        core: Album::NAME.to_string(),
        home: addr.home.as_str().to_string(),
        kind: addr.kind.clone(),
        // Album keeps no series, so the change body names none (§7.1).
        series: None,
        // A partitioned entity stores no key — its *name* is the key (§5.4, §18).
        key: addr.slug.clone(),
        before,
        after,
        cascade,
    }
}

/// The contract JSON for a record not yet on disk — an `add`'s result, or a
/// `rename`'s destination. The same shape [`contract::entity_json`] emits.
fn addr_json(addr: &EntityAddr, entity: &Entity<Agent>) -> Result<Value> {
    Ok(json!({
        "core": Album::NAME,
        "home": addr.home.as_str(),
        "kind": addr.kind,
        "slug": addr.slug,
        "refs": entity.refs.iter().map(Ref::to_token).collect::<Vec<_>>(),
        "data": serde_json::to_value(&entity.data)?,
    }))
}

fn identity(eref: &EntityRef) -> Value {
    json!({
        "core": Album::NAME,
        "home": eref.home.as_str(),
        "kind": eref.kind,
        "slug": eref.slug,
    })
}

/// A slug given on the command line is a typed token, so it is normalized on the way
/// in — `alb get "John Appleseed"` finds `john_appleseed` (§5.1).
fn normalize_slug(raw: &str) -> Result<String> {
    pantheon::name::normalize_token(raw, "name")
}

fn parse_refs(refs: &[String]) -> Result<Vec<Ref>> {
    refs.iter().map(|r| Ref::parse(r)).collect()
}

fn version_json() -> Value {
    json!({
        "name": Album::NAME,
        "short": "alb",
        "version": env!("CARGO_PKG_VERSION"),
        "format_version": 1,
    })
}

fn help_json() -> Value {
    json!({
        "name": Album::NAME,
        "short": "alb",
        "about": "societas agents: the people, organizations, and groups you know (§8.1)",
        "verbs": VERBS,
        "kinds": Album::KINDS,
        "version": env!("CARGO_PKG_VERSION"),
    })
}
